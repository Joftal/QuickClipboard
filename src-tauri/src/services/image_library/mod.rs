use std::{collections::HashMap, fs, path::{Component, Path, PathBuf}};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde::{Serialize, Deserialize};
use crate::services::get_data_directory;

const IMAGE_LIBRARY_DIR: &str = "image_library";
const IMAGES_SUBDIR: &str = "images";
const GIFS_SUBDIR: &str = "gifs";

static IMAGE_SAVE_SEQUENCE: AtomicU64 = AtomicU64::new(0);
static IMAGE_LIST_CACHE: Lazy<Mutex<HashMap<String, CachedImageList>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));
static OCR_IN_FLIGHT: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageInfo {
    pub id: String,
    pub filename: String,
    pub path: String,
    pub size: u64,
    pub created_at: u64,
    pub category: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImageListResult {
    pub total: usize,
    pub items: Vec<ImageInfo>,
}

#[derive(Debug, Clone)]
struct CachedImageList {
    dir_stamp_ms: Option<u64>,
    items: Vec<ImageInfo>,
}

struct OcrInFlightGuard;

impl OcrInFlightGuard {
    fn try_acquire() -> Option<Self> {
        OCR_IN_FLIGHT
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()
            .map(|_| OcrInFlightGuard)
    }
}

impl Drop for OcrInFlightGuard {
    fn drop(&mut self) {
        OCR_IN_FLIGHT.store(false, Ordering::Release);
    }
}

// 获取图片库目录路径
pub fn get_image_library_dir() -> Result<PathBuf, String> {
    let data_dir = get_data_directory()?;
    Ok(data_dir.join(IMAGE_LIBRARY_DIR))
}

// 获取图片子目录路径
pub fn get_images_dir() -> Result<PathBuf, String> {
    Ok(get_image_library_dir()?.join(IMAGES_SUBDIR))
}

// 获取 GIF 子目录路径
pub fn get_gifs_dir() -> Result<PathBuf, String> {
    Ok(get_image_library_dir()?.join(GIFS_SUBDIR))
}

// 初始化图片库目录结构
pub fn init_image_library() -> Result<(), String> {
    let images_dir = get_images_dir()?;
    let gifs_dir = get_gifs_dir()?;
    
    if !images_dir.exists() {
        fs::create_dir_all(&images_dir)
            .map_err(|e| format!("创建图片目录失败: {}", e))?;
    }
    
    if !gifs_dir.exists() {
        fs::create_dir_all(&gifs_dir)
            .map_err(|e| format!("创建 GIF 目录失败: {}", e))?;
    }
    
    Ok(())
}

fn image_category_key(category: &str) -> &'static str {
    match category {
        "gifs" => "gifs",
        _ => "images",
    }
}

fn allowed_image_extension(path: &Path) -> bool {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();

    matches!(
        ext.as_str(),
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "avif" | "svg" | "ico" | "tiff" | "tif" | "heic" | "heif" | "jfif"
    )
}

fn get_dir_stamp_ms(dir: &Path) -> Result<Option<u64>, String> {
    if !dir.exists() {
        return Ok(None);
    }

    let modified = fs::metadata(dir)
        .map_err(|e| format!("读取图库目录元数据失败: {}", e))?
        .modified()
        .map_err(|e| format!("读取图库目录修改时间失败: {}", e))?;

    Ok(modified
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_millis() as u64))
}

fn build_cached_image_list(dir: &Path, category: &str) -> Result<CachedImageList, String> {
    let dir_stamp_ms = get_dir_stamp_ms(dir)?;

    if !dir.exists() {
        return Ok(CachedImageList {
            dir_stamp_ms,
            items: Vec::new(),
        });
    }

    let mut items = Vec::new();
    for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if !path.is_file() || !allowed_image_extension(&path) {
            continue;
        }

        let metadata = entry.metadata().ok();
        let created_at = metadata.as_ref()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let size = metadata.map(|m| m.len()).unwrap_or(0);
        let filename = entry.file_name().to_string_lossy().to_string();

        items.push(ImageInfo {
            id: filename.clone(),
            filename,
            path: path.to_string_lossy().to_string(),
            size,
            created_at,
            category: category.to_string(),
        });
    }

    items.sort_by(|a, b| {
        b.created_at
            .cmp(&a.created_at)
            .then_with(|| a.filename.cmp(&b.filename))
    });

    Ok(CachedImageList { dir_stamp_ms, items })
}

fn get_cached_image_list(dir: &Path, category: &str) -> Result<CachedImageList, String> {
    let cache_key = image_category_key(category).to_string();
    let dir_stamp_ms = get_dir_stamp_ms(dir)?;

    if let Some(cached) = IMAGE_LIST_CACHE.lock().get(&cache_key).cloned() {
        if cached.dir_stamp_ms == dir_stamp_ms {
            return Ok(cached);
        }
    }

    let rebuilt = build_cached_image_list(dir, category)?;
    IMAGE_LIST_CACHE.lock().insert(cache_key, rebuilt.clone());
    Ok(rebuilt)
}

// 通过文件头魔数判断是否为 GIF
fn is_gif_by_magic(data: &[u8]) -> bool {
    if data.len() < 6 {
        return false;
    }
    &data[0..6] == b"GIF87a" || &data[0..6] == b"GIF89a"
}

// 通过文件头判断是否为 WebP
fn is_webp_by_magic(data: &[u8]) -> bool {
    if data.len() < 12 {
        return false;
    }
    &data[0..4] == b"RIFF" && &data[8..12] == b"WEBP"
}

// 检测 WebP 是否为动态图
fn is_animated_webp(data: &[u8]) -> bool {
    if !is_webp_by_magic(data) || data.len() < 30 {
        return false;
    }
    
    if data.len() >= 21 && &data[12..16] == b"VP8X" {
        let flags = data[20];
        return (flags & 0x02) != 0;
    }
    false
}

// 将静态 WebP 转换为 JPG
fn convert_webp_to_jpg(data: &[u8]) -> Result<Vec<u8>, String> {
    use image::ImageReader;
    use std::io::Cursor;
    
    let reader = ImageReader::new(Cursor::new(data))
        .with_guessed_format()
        .map_err(|e| format!("读取 WebP 失败: {}", e))?;
    
    let img = reader.decode()
        .map_err(|e| format!("解码 WebP 失败: {}", e))?;
    
    let mut buffer = Vec::new();
    let mut cursor = Cursor::new(&mut buffer);
    img.write_to(&mut cursor, image::ImageFormat::Jpeg)
        .map_err(|e| format!("编码 JPG 失败: {}", e))?;
    
    Ok(buffer)
}

// 提取 GIF 第一帧为 PNG 数据
fn extract_gif_first_frame(data: &[u8]) -> Option<Vec<u8>> {
    use image::codecs::gif::GifDecoder;
    use image::AnimationDecoder;
    use std::io::Cursor;
    
    let decoder = GifDecoder::new(Cursor::new(data)).ok()?;
    let first_frame = decoder.into_frames().next()?.ok()?;
    
    let mut buffer = Vec::new();
    let mut cursor = Cursor::new(&mut buffer);
    first_frame.buffer().write_to(&mut cursor, image::ImageFormat::Png).ok()?;
    
    Some(buffer)
}

fn run_ocr_task_with_timeout<T, F>(task: F, timeout: Duration) -> Option<T>
where
    T: Send + 'static,
    F: FnOnce() -> Option<T> + Send + 'static,
{
    use std::sync::mpsc;
    use std::thread;

    let guard = OcrInFlightGuard::try_acquire()?;
    let (tx, rx) = mpsc::channel();

    let spawn_result = thread::Builder::new()
        .name("il_ocr".to_string())
        .spawn(move || {
            let _guard = guard;
            let _ = tx.send(task());
        });

    if spawn_result.is_err() {
        return None;
    }

    rx.recv_timeout(timeout).unwrap_or_default()
}

// 使用 OCR 识别图片文字
fn ocr_image_text(data: &[u8]) -> Option<String> {
    use qcocr::recognize_from_bytes;

    let data = data.to_vec();
    let text = run_ocr_task_with_timeout(
        move || recognize_from_bytes(&data, None).ok().map(|r| r.text),
        Duration::from_secs(2),
    )?;

    let text = text.trim();
    
    if text.is_empty() {
        return None;
    }
    
    let cleaned: String = text
        .chars()
        .filter(|c| c.is_alphanumeric())
        .take(50)
        .collect();
    
    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned)
    }
}

fn build_library_filename(timestamp: u128, sequence: u64, extension: &str, ocr_text: Option<&str>) -> String {
    match ocr_text {
        Some(text) if !text.is_empty() => format!("{}_{}_{}.{}", timestamp, sequence, text, extension),
        _ => format!("{}_{}.{}", timestamp, sequence, extension),
    }
}

fn allocate_library_filename(target_dir: &Path, timestamp: u128, extension: &str, ocr_text: Option<&str>) -> Result<(String, PathBuf), String> {
    for _ in 0..1024 {
        let sequence = IMAGE_SAVE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let filename = build_library_filename(timestamp, sequence, extension, ocr_text);
        let file_path = target_dir.join(&filename);
        if !file_path.exists() {
            return Ok((filename, file_path));
        }
    }

    Err("生成唯一图片文件名失败，请稍后重试".to_string())
}

// 保存图片到图片库
pub fn save_image(filename: &str, data: &[u8]) -> Result<ImageInfo, String> {
    init_image_library()?;
    
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_millis();
    
    let (final_data, extension, category): (Vec<u8>, &str, &str) = if is_gif_by_magic(data) {
        // GIF 文件直接保存
        (data.to_vec(), "gif", "gifs")
    } else if is_webp_by_magic(data) {
        if is_animated_webp(data) {
            // 动态 WebP 直接保存
            (data.to_vec(), "webp", "gifs")
        } else {
            // 静态 WebP 转 JPG
            let jpg_data = convert_webp_to_jpg(data)?;
            (jpg_data, "jpg", "images")
        }
    } else {
        // 其他格式保留原扩展名
        let ext = std::path::Path::new(filename)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("png");
        (data.to_vec(), ext, "images")
    };
    
    let ocr_text = if is_gif_by_magic(data) {
        extract_gif_first_frame(data).and_then(|frame| ocr_image_text(&frame))
    } else if category != "gifs" {
        ocr_image_text(&final_data)
    } else {
        None
    };
    
    let target_dir = if category == "gifs" { get_gifs_dir()? } else { get_images_dir()? };
    let (new_filename, file_path) = allocate_library_filename(&target_dir, timestamp, extension, ocr_text.as_deref())?;
    
    fs::write(&file_path, &final_data)
        .map_err(|e| format!("保存图片失败: {}", e))?;
    
    Ok(ImageInfo {
        id: new_filename.clone(),
        filename: new_filename,
        path: file_path.to_string_lossy().to_string(),
        size: final_data.len() as u64,
        created_at: timestamp as u64,
        category: category.to_string(),
    })
}

pub fn save_image_from_path(source_path: &str) -> Result<ImageInfo, String> {
    let path = Path::new(source_path);
    if !path.exists() {
        return Err(format!("源文件不存在: {}", source_path));
    }

    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("源文件名无效: {}", source_path))?
        .to_string();

    let data = fs::read(path).map_err(|error| format!("读取源文件失败: {}", error))?;
    save_image(&filename, &data)
}

// 获取图片列表
pub fn get_image_list(category: &str, offset: usize, limit: usize) -> Result<ImageListResult, String> {
    init_image_library()?;
    
    let (dir, cat_str) = match category {
        "gifs" => (get_gifs_dir()?, "gifs"),
        _ => (get_images_dir()?, "images"),
    };

    let cached = get_cached_image_list(&dir, cat_str)?;
    let total = cached.items.len();
    let items = cached.items.into_iter().skip(offset).take(limit).collect();

    Ok(ImageListResult { total, items })
}

// 获取图片总数
pub fn get_image_count(category: &str) -> Result<usize, String> {
    init_image_library()?;
    
    let (dir, cat_str) = match category {
        "gifs" => (get_gifs_dir()?, "gifs"),
        _ => (get_images_dir()?, "images"),
    };

    Ok(get_cached_image_list(&dir, cat_str)?.items.len())
}

fn sanitize_library_filename(filename: &str, field_name: &str) -> Result<String, String> {
    let trimmed = filename.trim();
    if trimmed.is_empty() {
        return Err(format!("{}不能为空", field_name));
    }

    let mut components = Path::new(trimmed).components();
    match (components.next(), components.next()) {
        (Some(Component::Normal(part)), None) => part
            .to_str()
            .map(|value| value.to_string())
            .ok_or_else(|| format!("{}包含无效字符", field_name)),
        _ => Err(format!("{}包含非法路径", field_name)),
    }
}

fn build_library_file_path(dir: &Path, filename: &str, field_name: &str) -> Result<(String, PathBuf), String> {
    let sanitized = sanitize_library_filename(filename, field_name)?;
    let path = dir.join(&sanitized);

    match path.parent() {
        Some(parent) if parent == dir => Ok((sanitized, path)),
        _ => Err(format!("{}超出图片库目录范围", field_name)),
    }
}

// 删除图片
pub fn delete_image(category: &str, filename: &str) -> Result<(), String> {
    let dir = match category {
        "gifs" => get_gifs_dir()?,
        _ => get_images_dir()?,
    };
    
    let (_, file_path) = build_library_file_path(&dir, filename, "文件名")?;
    
    if file_path.exists() {
        fs::remove_file(&file_path)
            .map_err(|e| format!("删除图片失败: {}", e))?;
    }
    
    Ok(())
}

// 重命名图片
pub fn rename_image(category: &str, old_filename: &str, new_filename: &str) -> Result<ImageInfo, String> {
    let dir = match category {
        "gifs" => get_gifs_dir()?,
        _ => get_images_dir()?,
    };
    
    let (old_name, old_path) = build_library_file_path(&dir, old_filename, "原文件名")?;
    if !old_path.exists() {
        return Err(format!("文件不存在: {}", old_name));
    }
    
    let old_ext = std::path::Path::new(&old_name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("png");

    let sanitized_new_name = sanitize_library_filename(new_filename, "新文件名")?;
    
    let new_name_with_ext = if sanitized_new_name.contains('.') {
        sanitized_new_name
    } else {
        format!("{}.{}", sanitized_new_name, old_ext)
    };
    
    let (new_name_with_ext, new_path) = build_library_file_path(&dir, &new_name_with_ext, "新文件名")?;
    
    if new_path.exists() {
        return Err("目标文件名已存在".to_string());
    }
    
    fs::rename(&old_path, &new_path)
        .map_err(|e| format!("重命名失败: {}", e))?;
    
    let metadata = fs::metadata(&new_path).ok();
    let size = metadata.as_ref().map(|m| m.len()).unwrap_or(0);
    let created_at = metadata
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    
    Ok(ImageInfo {
        id: new_name_with_ext.clone(),
        filename: new_name_with_ext,
        path: new_path.to_string_lossy().to_string(),
        size,
        created_at,
        category: category.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::settings::{get_settings, replace_settings, AppSettings};
    use crate::services::test_support::lock_global_test_state;
    use uuid::Uuid;

    fn with_test_image_library(test: impl FnOnce(PathBuf)) {
        let _guard = lock_global_test_state();
        let original_settings = get_settings();
        let data_dir = std::env::temp_dir().join(format!("quickclipboard-image-library-test-{}", Uuid::new_v4()));

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let settings = AppSettings {
                use_custom_storage: true,
                custom_storage_path: Some(data_dir.to_string_lossy().to_string()),
                ..AppSettings::default()
            };
            replace_settings(settings);
            init_image_library().expect("init image library failed");
            test(data_dir.clone());
        }));

        replace_settings(original_settings);
        let _ = fs::remove_dir_all(&data_dir);

        if let Err(panic) = result {
            std::panic::resume_unwind(panic);
        }
    }

    #[test]
    fn sanitize_filename_rejects_path_traversal() {
        assert!(sanitize_library_filename("../../outside.png", "文件名").is_err());
        assert!(sanitize_library_filename("nested/file.png", "文件名").is_err());
        assert!(sanitize_library_filename("", "文件名").is_err());
        assert_eq!(sanitize_library_filename("safe.png", "文件名").unwrap(), "safe.png");
    }

    #[test]
    fn delete_image_rejects_out_of_dir_path() {
        with_test_image_library(|data_dir| {
            let outside_path = data_dir.join("outside.png");
            fs::write(&outside_path, b"outside").expect("write outside file failed");

            let result = delete_image("images", "../../outside.png");
            assert!(result.is_err());
            assert!(outside_path.exists());
        });
    }

    #[test]
    fn rename_image_rejects_out_of_dir_target() {
        with_test_image_library(|data_dir| {
            let images_dir = get_images_dir().expect("get images dir failed");
            let source_path = images_dir.join("source.png");
            let escaped_path = data_dir.join("escaped.png");
            fs::write(&source_path, b"source").expect("write source file failed");

            let result = rename_image("images", "source.png", "../../escaped");
            assert!(result.is_err());
            assert!(source_path.exists());
            assert!(!escaped_path.exists());
        });
    }

    #[test]
    fn build_library_filename_uses_sequence_to_avoid_same_timestamp_collision() {
        let first = build_library_filename(123456789, 1, "png", Some("Hello"));
        let second = build_library_filename(123456789, 2, "png", Some("Hello"));
        assert_ne!(first, second);
        assert_eq!(first, "123456789_1_Hello.png");
        assert_eq!(second, "123456789_2_Hello.png");
    }

    #[test]
    fn allocate_library_filename_skips_existing_file() {
        with_test_image_library(|_data_dir| {
            let images_dir = get_images_dir().expect("get images dir failed");
            let existing = images_dir.join("999_0_same.png");
            fs::write(&existing, b"old").expect("write existing file failed");

            IMAGE_SAVE_SEQUENCE.store(0, Ordering::Relaxed);
            let (filename, file_path) = allocate_library_filename(&images_dir, 999, "png", Some("same"))
                .expect("allocate unique filename failed");

            assert_eq!(filename, "999_1_same.png");
            assert_eq!(file_path, images_dir.join("999_1_same.png"));
            assert!(existing.exists());
        });
    }

    #[test]
    fn get_image_count_refreshes_after_external_file_change() {
        with_test_image_library(|_data_dir| {
            let images_dir = get_images_dir().expect("get images dir failed");

            let initial = get_image_count("images").expect("get initial count failed");
            assert_eq!(initial, 0);

            std::thread::sleep(Duration::from_millis(5));
            fs::write(images_dir.join("manual.png"), b"manual").expect("write manual file failed");

            let refreshed = get_image_count("images").expect("get refreshed count failed");
            assert_eq!(refreshed, 1);
        });
    }

    #[test]
    fn get_image_list_refreshes_after_external_file_change() {
        with_test_image_library(|_data_dir| {
            let images_dir = get_images_dir().expect("get images dir failed");
            fs::write(images_dir.join("200_0_b.png"), b"b").expect("write first image failed");

            let first = get_image_list("images", 0, 10).expect("get first list failed");
            assert_eq!(first.total, 1);
            assert_eq!(first.items.len(), 1);

            std::thread::sleep(Duration::from_millis(5));
            fs::write(images_dir.join("300_0_c.png"), b"c").expect("write second image failed");

            let second = get_image_list("images", 0, 10).expect("get second list failed");
            assert_eq!(second.total, 2);
            assert_eq!(second.items.len(), 2);
        });
    }

    #[test]
    fn ocr_task_rejects_concurrent_requests_until_running_task_finishes() {
        let _guard = lock_global_test_state();

        let first = run_ocr_task_with_timeout(
            || {
                std::thread::sleep(Duration::from_millis(50));
                Some("first".to_string())
            },
            Duration::from_millis(5),
        );
        assert!(first.is_none(), "expected first OCR task to time out");

        let second = run_ocr_task_with_timeout(
            || Some("second".to_string()),
            Duration::from_millis(5),
        );
        assert!(second.is_none(), "expected concurrent OCR task to be rejected while first is still running");

        std::thread::sleep(Duration::from_millis(60));

        let third = run_ocr_task_with_timeout(
            || Some("third".to_string()),
            Duration::from_millis(20),
        );
        assert_eq!(third.as_deref(), Some("third"));
    }
}
