use super::capture::ClipboardContent;
use super::processor::process_content;
use super::storage::store_clipboard_item;
use clipboard_rs::{
    ClipboardHandler, ClipboardWatcher, ClipboardWatcherContext, WatcherShutdown,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use parking_lot::Mutex;
use once_cell::sync::Lazy;
use std::thread;

static IS_RUNNING: AtomicBool = AtomicBool::new(false);

static GENERATION: AtomicU64 = AtomicU64::new(0);
static MONITOR_LIFECYCLE_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

// 监听器状态
struct MonitorState {
    watcher_handle: Option<thread::JoinHandle<()>>,
    watcher_shutdown: Option<WatcherShutdown>,
    worker_handle: Option<thread::JoinHandle<()>>,
    work_sender: Option<Sender<ClipboardWorkItem>>,
}

static MONITOR_STATE: Lazy<Arc<Mutex<MonitorState>>> = Lazy::new(|| {
    Arc::new(Mutex::new(MonitorState {
        watcher_handle: None,
        watcher_shutdown: None,
        worker_handle: None,
        work_sender: None,
    }))
});

// 上一次捕获的内容哈希集合（用于去重）
static LAST_CONTENT_HASHES: Lazy<Arc<Mutex<Vec<String>>>> = Lazy::new(|| {
    Arc::new(Mutex::new(Vec::new()))
});

// 清除上一次内容缓存（用于删除剪贴板项后允许重新添加相同内容）
pub fn clear_last_content_cache() {
    let mut last_hashes = LAST_CONTENT_HASHES.lock();
    last_hashes.clear();
}

// 剪贴板监听管理器
struct ClipboardMonitorManager {
    generation: u64,
}

struct ClipboardWorkItem {
    contents: Vec<ClipboardContent>,
    hashes: Vec<String>,
}

impl ClipboardMonitorManager {
    pub fn new(generation: u64) -> Result<Self, String> {
        Ok(ClipboardMonitorManager { generation })
    }
}

impl ClipboardHandler for ClipboardMonitorManager {
    fn on_clipboard_change(&mut self) {
        if !IS_RUNNING.load(Ordering::Relaxed) {
            return;
        }
        
        if self.generation != GENERATION.load(Ordering::Relaxed) {
            return;
        }
        
        if let Err(e) = handle_clipboard_change() {
            if !e.contains("重复内容") {
                eprintln!("处理剪贴板内容失败: {}", e);
            }
        }
    }
}

pub fn start_clipboard_monitor() -> Result<(), String> {
    let _lifecycle_guard = MONITOR_LIFECYCLE_LOCK.lock();

    if IS_RUNNING.load(Ordering::SeqCst) {
        return Ok(());
    }

    stop_clipboard_monitor_inner();

    let new_generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;

    let (mut watcher, watcher_shutdown) = create_clipboard_watcher(new_generation)?;
    let (work_sender, worker_handle) = start_clipboard_worker();

    IS_RUNNING.store(true, Ordering::SeqCst);

    #[cfg(target_os = "windows")]
    crate::services::system::start_clipboard_source_monitor();

    let handle = thread::spawn(move || {
        watcher.start_watch();
        IS_RUNNING.store(false, Ordering::SeqCst);
    });

    let mut state = MONITOR_STATE.lock();
    state.watcher_handle = Some(handle);
    state.watcher_shutdown = Some(watcher_shutdown);
    state.worker_handle = Some(worker_handle);
    state.work_sender = Some(work_sender);

    Ok(())
}

pub fn stop_clipboard_monitor() -> Result<(), String> {
    let _lifecycle_guard = MONITOR_LIFECYCLE_LOCK.lock();
    stop_clipboard_monitor_inner();
    Ok(())
}

pub fn is_monitor_running() -> bool {
    IS_RUNNING.load(Ordering::Relaxed)
}

fn create_clipboard_watcher(
    generation: u64,
) -> Result<(ClipboardWatcherContext<ClipboardMonitorManager>, WatcherShutdown), String> {
    let manager = ClipboardMonitorManager::new(generation)?;
    let mut watcher = ClipboardWatcherContext::new()
        .map_err(|e| format!("创建剪贴板监听器失败: {}", e))?;
    watcher.add_handler(manager);
    let watcher_shutdown = watcher.get_shutdown_channel();
    Ok((watcher, watcher_shutdown))
}

fn start_clipboard_worker() -> (Sender<ClipboardWorkItem>, thread::JoinHandle<()>) {
    let (sender, receiver) = mpsc::channel();
    let handle = thread::spawn(move || run_clipboard_worker(receiver));
    (sender, handle)
}

fn run_clipboard_worker(receiver: Receiver<ClipboardWorkItem>) {
    while let Ok(work_item) = receiver.recv() {
        process_clipboard_contents(work_item);
    }
}

fn process_clipboard_contents(work_item: ClipboardWorkItem) {
    let mut any_stored = false;
    let mut retained_hashes = Vec::with_capacity(work_item.hashes.len());

    for content in work_item.contents {
        let content_hash = content.calculate_hash();

        match process_content(content) {
            Ok(processed) => match store_clipboard_item(processed) {
                Ok(_) => {
                    any_stored = true;
                    retained_hashes.push(content_hash);
                }
                Err(e) if e.contains("重复内容") || e.contains("已禁止保存图片") => {
                    retained_hashes.push(content_hash);
                }
                Err(e) => eprintln!("存储剪贴板内容失败: {}", e),
            },
            Err(e) => eprintln!("处理剪贴板内容失败: {}", e),
        }
    }

    reconcile_last_content_hashes(&work_item.hashes, retained_hashes);

    if any_stored {
        let _ = emit_clipboard_updated();
    }
}

fn enqueue_clipboard_contents(contents: Vec<ClipboardContent>, hashes: Vec<String>) -> Result<(), String> {
    let sender = {
        let state = MONITOR_STATE.lock();
        state.work_sender.clone()
    };

    let Some(sender) = sender else {
        return Err("剪贴板工作线程未启动".to_string());
    };

    sender
        .send(ClipboardWorkItem {
            contents,
            hashes: hashes.clone(),
        })
        .map_err(|_| "剪贴板工作线程已停止".to_string())?;

    set_last_content_hashes(hashes);

    Ok(())
}

fn stop_clipboard_monitor_inner() {
    IS_RUNNING.store(false, Ordering::SeqCst);

    #[cfg(target_os = "windows")]
    crate::services::system::stop_clipboard_source_monitor();

    let (watcher_shutdown, watcher_handle, work_sender, worker_handle) = {
        let mut state = MONITOR_STATE.lock();
        (
            state.watcher_shutdown.take(),
            state.watcher_handle.take(),
            state.work_sender.take(),
            state.worker_handle.take(),
        )
    };

    if let Some(shutdown) = watcher_shutdown {
        shutdown.stop();
    }

    if let Some(handle) = watcher_handle {
        if let Err(error) = handle.join() {
            eprintln!("剪贴板监听线程退出失败: {:?}", error);
        }
    }

    drop(work_sender);

    if let Some(handle) = worker_handle {
        if let Err(error) = handle.join() {
            eprintln!("剪贴板工作线程退出失败: {:?}", error);
        }
    }
}

fn handle_clipboard_change() -> Result<(), String> {
    // 检查应用过滤
    let settings = crate::services::get_settings();

    if crate::services::system::is_front_app_globally_disabled(
        settings.app_filter_enabled,
        &settings.app_filter_mode,
        &settings.app_filter_list,
        &settings.app_filter_effect,
    ) {
        return Ok(());
    }

    if !crate::services::system::is_current_app_allowed(
        settings.app_filter_enabled,
        &settings.app_filter_mode,
        &settings.app_filter_list,
    ) {
        return Ok(());
    }
    
    let contents = ClipboardContent::capture()?;
    if contents.is_empty() {
        return Ok(());
    }
    
    // 计算所有内容的哈希
    let current_hashes: Vec<String> = contents.iter().map(|c| c.calculate_hash()).collect();
    
    // 检查是否与上次完全相同
    {
        let last_hashes = LAST_CONTENT_HASHES.lock();
        if *last_hashes == current_hashes {
            return Ok(());
        }
    }

    // 过滤出新内容
    let new_contents: Vec<_> = {
        let last_hashes = LAST_CONTENT_HASHES.lock();
        contents
            .into_iter()
            .filter(|content| !last_hashes.contains(&content.calculate_hash()))
            .collect()
    };

    if new_contents.is_empty() {
        return Ok(());
    }

    enqueue_clipboard_contents(new_contents, current_hashes)?;

    Ok(())
}

fn set_last_content_hashes(hashes: Vec<String>) {
    let mut last_hashes = LAST_CONTENT_HASHES.lock();
    *last_hashes = hashes;
}

fn reconcile_last_content_hashes(expected_hashes: &[String], retained_hashes: Vec<String>) {
    let mut last_hashes = LAST_CONTENT_HASHES.lock();

    if last_hashes.as_slice() != expected_hashes {
        return;
    }

    *last_hashes = retained_hashes;
}

static APP_HANDLE: Lazy<Arc<Mutex<Option<tauri::AppHandle>>>> = Lazy::new(|| {
    Arc::new(Mutex::new(None))
});

pub fn set_app_handle(handle: tauri::AppHandle) {
    *APP_HANDLE.lock() = Some(handle);
}

pub fn get_app_handle() -> Option<tauri::AppHandle> {
    APP_HANDLE.lock().clone()
}

fn emit_clipboard_updated() -> Result<(), String> {
    let app_handle = APP_HANDLE.lock();
    let handle = app_handle.as_ref().ok_or("应用未初始化")?;

    use tauri::Emitter;
    handle.emit("clipboard-updated", ()).map_err(|e| e.to_string())
}

// 预设哈希缓存（文本类型）
pub fn set_last_hash_text(text: &str) {
    use sha2::{Sha256, Digest};
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    let hash = format!("{:x}", hasher.finalize());
    
    set_last_content_hashes(vec![hash]);
}

// 预设哈希缓存（文件类型）
pub fn set_last_hash_files(content: &str) {
    use sha2::{Sha256, Digest};
    
    if let Some(json_str) = content.strip_prefix("files:") {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(json_str) {
            let mut hasher = Sha256::new();
            
            if let Some(files) = json["files"].as_array() {
                for file in files {
                    if let Some(path) = file["path"].as_str() {
                        let normalized = crate::services::normalize_path_for_hash(path);
                        hasher.update(normalized.as_bytes());
                    }
                }
            }
            
            let hash = format!("{:x}", hasher.finalize());
            set_last_content_hashes(vec![hash]);
        }
    }
}

// 预设哈希缓存（单文件路径）
pub fn set_last_hash_file(file_path: &str) {
    use sha2::{Sha256, Digest};
    
    let mut hasher = Sha256::new();
    let normalized = crate::services::normalize_path_for_hash(file_path);
    hasher.update(normalized.as_bytes());
    
    let hash = format!("{:x}", hasher.finalize());
    set_last_content_hashes(vec![hash]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::clipboard::capture::ContentType;

    fn text_content(text: &str) -> ClipboardContent {
        ClipboardContent {
            content_type: ContentType::Text,
            text: Some(text.to_string()),
            html: None,
            files: None,
        }
    }

    #[test]
    fn reconcile_removes_failed_hashes_when_snapshot_matches() {
        clear_last_content_cache();

        let first = text_content("first");
        let second = text_content("second");
        let expected_hashes = vec![first.calculate_hash(), second.calculate_hash()];

        set_last_content_hashes(expected_hashes.clone());
        reconcile_last_content_hashes(&expected_hashes, vec![expected_hashes[0].clone()]);

        let last_hashes = LAST_CONTENT_HASHES.lock().clone();
        assert_eq!(last_hashes, vec![expected_hashes[0].clone()]);

        clear_last_content_cache();
    }

    #[test]
    fn reconcile_does_not_override_newer_snapshot() {
        clear_last_content_cache();

        let original = vec![text_content("old").calculate_hash()];
        let newer = vec![text_content("new").calculate_hash()];

        set_last_content_hashes(newer.clone());
        reconcile_last_content_hashes(&original, Vec::new());

        let last_hashes = LAST_CONTENT_HASHES.lock().clone();
        assert_eq!(last_hashes, newer);

        clear_last_content_cache();
    }
}

