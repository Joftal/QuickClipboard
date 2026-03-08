use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use tauri::{Manager, WebviewWindow};

static LAST_FOCUS_HWND: Mutex<Option<isize>> = Mutex::new(None);
static LISTENER_RUNNING: AtomicBool = AtomicBool::new(false);

static LAST_FOREGROUND_CACHE: Mutex<Option<(isize, ForegroundAppInfo)>> = Mutex::new(None);

#[derive(Debug, Clone)]
pub struct ForegroundAppInfo {
    pub process_name: String,
    pub process_path: String,
    pub window_title: String,
}

#[cfg(windows)]
static EXCLUDED_HWNDS: Mutex<Vec<isize>> = Mutex::new(Vec::new());
#[cfg(windows)]
static LISTENER_THREAD: Mutex<Option<std::thread::JoinHandle<()>>> = Mutex::new(None);
#[cfg(windows)]
static LISTENER_THREAD_ID: AtomicU32 = AtomicU32::new(0);
#[cfg(windows)]
static LISTENER_LIFECYCLE_LOCK: Mutex<()> = Mutex::new(());

// 启动焦点变化监听器
pub fn start_focus_listener(app_handle: tauri::AppHandle) {
    #[cfg(windows)]
    {
        let _lifecycle_guard = LISTENER_LIFECYCLE_LOCK.lock();

        if LISTENER_RUNNING.load(Ordering::SeqCst) {
            return;
        }

        stop_focus_listener_inner();
        LISTENER_RUNNING.store(true, Ordering::SeqCst);
        
        let mut excluded = Vec::new();
        for label in ["main", "context-menu", "settings", "preview"] {
            if let Some(win) = app_handle.get_webview_window(label) {
                if let Ok(hwnd) = win.hwnd() {
                    excluded.push(hwnd.0 as isize);
                }
            }
        }
        *EXCLUDED_HWNDS.lock() = excluded;

        crate::services::system::hotkey::sync_hotkeys_for_foreground();

        let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
        let handle = std::thread::spawn(move || {
            start_win_event_hook(ready_tx);
        });

        *LISTENER_THREAD.lock() = Some(handle);
        let _ = ready_rx.recv();
    }
    
    #[cfg(not(windows))]
    {
        let _ = app_handle;
    }
}

// 停止焦点变化监听器
pub fn stop_focus_listener() {
    #[cfg(windows)]
    {
        let _lifecycle_guard = LISTENER_LIFECYCLE_LOCK.lock();
        stop_focus_listener_inner();
    }

    #[cfg(not(windows))]
    {
        LISTENER_RUNNING.store(false, Ordering::SeqCst);
    }
}

#[cfg(windows)]
fn stop_focus_listener_inner() {
    LISTENER_RUNNING.store(false, Ordering::SeqCst);
    signal_focus_listener_exit();

    let handle = LISTENER_THREAD.lock().take();
    if let Some(handle) = handle {
        if let Err(error) = handle.join() {
            eprintln!("焦点监听线程退出失败: {:?}", error);
        }
    }

    LISTENER_THREAD_ID.store(0, Ordering::SeqCst);
}

#[cfg(windows)]
fn signal_focus_listener_exit() {
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{PostThreadMessageW, WM_QUIT};

    let thread_id = LISTENER_THREAD_ID.load(Ordering::SeqCst);
    if thread_id == 0 {
        return;
    }

    if let Err(error) = unsafe { PostThreadMessageW(thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) } {
        eprintln!("发送焦点监听退出信号失败: {:?}", error);
    }
}

#[cfg(windows)]
pub fn add_excluded_hwnd(hwnd: isize) {
    let mut excluded = EXCLUDED_HWNDS.lock();
    if !excluded.contains(&hwnd) {
        excluded.push(hwnd);
    }
}

// 聚焦剪贴板窗口
pub fn focus_clipboard_window(window: WebviewWindow) -> Result<(), String> {
    window.set_focus().map_err(|e| format!("设置窗口焦点失败: {}", e))
}

// 仅保存当前焦点（手动）
pub fn save_current_focus(_app_handle: tauri::AppHandle) -> Result<(), String> {
    Ok(())
}

// 恢复上次焦点窗口
pub fn restore_last_focus() -> Result<(), String> {
    #[cfg(windows)]
    {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow;
        use std::ffi::c_void;
        
        if let Some(hwnd_val) = *LAST_FOCUS_HWND.lock() {
            unsafe {
                let _ = SetForegroundWindow(HWND(hwnd_val as *mut c_void));
            }
        }
        Ok(())
    }
    
    #[cfg(not(windows))]
    {
        Ok(())
    }
}

// 获取当前记录的焦点窗口句柄
pub fn get_last_focus_hwnd() -> Option<isize> {
    *LAST_FOCUS_HWND.lock()
}

pub fn get_foreground_app_info() -> Option<ForegroundAppInfo> {
    #[cfg(windows)]
    {
        use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId};

        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.0.is_null() {
                return None;
            }

            let hwnd_val = hwnd.0 as isize;
            if let Some((cached_hwnd, cached_info)) = LAST_FOREGROUND_CACHE.lock().clone() {
                if cached_hwnd == hwnd_val {
                    return Some(cached_info);
                }
            }

            let mut pid: u32 = 0;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            if pid == 0 {
                return None;
            }

            let mut title_buf = [0u16; 512];
            let title_len = GetWindowTextW(hwnd, &mut title_buf);
            let window_title = if title_len > 0 {
                String::from_utf16_lossy(&title_buf[..title_len as usize])
            } else {
                String::new()
            };

            let Some((process_path, process_name)) =
                crate::services::system::process::query_process_path_and_name(pid)
            else {
                return None;
            };

            let info = ForegroundAppInfo {
                process_name,
                process_path,
                window_title,
            };

            *LAST_FOREGROUND_CACHE.lock() = Some((hwnd_val, info.clone()));
            Some(info)
        }
    }

    #[cfg(not(windows))]
    {
        None
    }
}

#[cfg(windows)]
fn start_win_event_hook(ready_tx: std::sync::mpsc::SyncSender<()>) {
    use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent};
    use windows::Win32::System::Threading::GetCurrentThreadId;
    use windows::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, GetMessageW, PeekMessageW, TranslateMessage, MSG, PM_NOREMOVE,
        EVENT_SYSTEM_FOREGROUND, WINEVENT_OUTOFCONTEXT,
    };
    
    unsafe {
        let thread_id = GetCurrentThreadId();
        let mut message = MSG::default();
        let _ = PeekMessageW(&mut message, None, 0, 0, PM_NOREMOVE);
        LISTENER_THREAD_ID.store(thread_id, Ordering::SeqCst);
        let _ = ready_tx.send(());

        let hook = SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            None,
            Some(focus_callback),
            0,
            0,
            WINEVENT_OUTOFCONTEXT,
        );
        
        if hook.0.is_null() {
            LISTENER_THREAD_ID.store(0, Ordering::SeqCst);
            LISTENER_RUNNING.store(false, Ordering::SeqCst);
            return;
        }
        
        let mut msg = MSG::default();
        while LISTENER_RUNNING.load(Ordering::SeqCst) {
            let message_result = GetMessageW(&mut msg, None, 0, 0);
            if message_result.0 == -1 || !message_result.as_bool() {
                break;
            }

            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        
        let _ = UnhookWinEvent(hook);
        LISTENER_THREAD_ID.store(0, Ordering::SeqCst);
        LISTENER_RUNNING.store(false, Ordering::SeqCst);
    }
}

#[cfg(windows)]
unsafe extern "system" fn focus_callback(
    _hook: windows::Win32::UI::Accessibility::HWINEVENTHOOK,
    _event: u32,
    _hwnd: windows::Win32::Foundation::HWND,
    _id_object: i32,
    _id_child: i32,
    _id_event_thread: u32,
    _dwms_event_time: u32,
) {
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetClassNameW, GetWindowTextW};

    let hwnd = GetForegroundWindow();
    if hwnd.0.is_null() {
        return;
    }
    
    let hwnd_val = hwnd.0 as isize;

    if EXCLUDED_HWNDS.lock().contains(&hwnd_val) {
        return;
    }
 
    let mut class_buf = [0u16; 256];
    let mut name_buf = [0u16; 256];
    let class_len = GetClassNameW(hwnd, &mut class_buf);
    let name_len = GetWindowTextW(hwnd, &mut name_buf);
    let class_name = String::from_utf16_lossy(&class_buf[..class_len as usize]);
    let name = String::from_utf16_lossy(&name_buf[..name_len as usize]);
    
    // 过滤窗口
    if class_name == "Shell_TrayWnd" 
        || class_name == "Shell_SecondaryTrayWnd"
        || class_name == "NotifyIconOverflowWindow"
        || class_name == "TopLevelWindowForOverflowXamlIsland"
        || class_name == "tray_icon_app"
        || class_name.starts_with("Windows.UI.")
        || class_name == "#32768"
        || class_name == "DropDown"
        || class_name == "Xaml_WindowedPopupClass"
        || name == "快速剪贴板"
        || name == "菜单" {
        return;
    }
    
    *LAST_FOCUS_HWND.lock() = Some(hwnd_val);

    crate::services::system::hotkey::sync_hotkeys_for_foreground();
}
