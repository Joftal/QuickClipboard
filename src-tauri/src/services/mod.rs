pub mod clipboard;
pub mod database;
pub mod data_management;
pub mod settings;
pub mod system;
pub mod paste;
pub mod image_library;
pub mod memory;

pub use settings::{AppSettings, get_settings, update_settings, get_data_directory};
pub use system::hotkey;

use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};

#[cfg(test)]
pub(crate) mod test_support {
    use once_cell::sync::Lazy;
    use parking_lot::{Mutex, MutexGuard};

    static GLOBAL_TEST_STATE_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

    pub(crate) fn lock_global_test_state() -> MutexGuard<'static, ()> {
        GLOBAL_TEST_STATE_LOCK.lock()
    }
}

pub fn normalize_path_for_hash(path: &str) -> String {
    let normalized = path.replace("\\", "/");
    for prefix in ["clipboard_images/", "pin_images/"] {
        if let Some(idx) = normalized.find(prefix) {
            return normalized[idx..].to_string();
        }
    }
    normalized
}

// 解析存储的路径为实际绝对路径
pub fn resolve_stored_path(stored_path: &str) -> String {
    let normalized_input = stored_path.replace("/", "\\");
    
    if normalized_input.starts_with("clipboard_images\\") 
        || normalized_input.starts_with("pin_images\\")
        || normalized_input.starts_with("image_library\\") {
        if let Ok(data_dir) = get_data_directory() {
            return data_dir.join(&normalized_input).to_string_lossy().to_string();
        }
    }
    
    let search_path = stored_path.replace("\\", "/");
    for prefix in ["clipboard_images/", "pin_images/", "image_library/"] {
        if let Some(idx) = search_path.find(prefix) {
            if let Ok(data_dir) = get_data_directory() {
                let relative = search_path[idx..].replace("/", "\\");
                let new_path = data_dir.join(&relative);
                if new_path.exists() {
                    return new_path.to_string_lossy().to_string();
                }
            }
        }
    }
    
    stored_path.to_string()
}

fn is_single_normal_path(path: &Path) -> bool {
    let mut components = path.components();
    matches!(
        (components.next(), components.next()),
        (Some(Component::Normal(_)), None)
    )
}

fn resolve_clipboard_image_paths(data_dir: &Path, image_ref: &str) -> Vec<PathBuf> {
    let normalized = image_ref.trim().replace('\\', "/");
    if normalized.is_empty() {
        return Vec::new();
    }

    if let Some(relative) = normalized.strip_prefix("clipboard_images/") {
        let relative_path = Path::new(relative);
        if !is_single_normal_path(relative_path) {
            return Vec::new();
        }

        return vec![data_dir.join("clipboard_images").join(relative_path)];
    }

    let file_name = Path::new(&normalized)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(image_ref)
        .trim();

    if file_name.is_empty() {
        return Vec::new();
    }

    let file_name_path = Path::new(file_name);
    if !is_single_normal_path(file_name_path) {
        return Vec::new();
    }

    if Path::new(file_name).extension().is_some() {
        return vec![data_dir.join("clipboard_images").join(file_name)];
    }

    vec![data_dir.join("clipboard_images").join(format!("{}.png", file_name))]
}

pub fn delete_clipboard_image_files(image_refs: &[String]) -> Result<(), String> {
    if image_refs.is_empty() {
        return Ok(());
    }

    let data_dir = get_data_directory()?;
    let mut seen_paths = HashSet::new();

    for image_ref in image_refs {
        for path in resolve_clipboard_image_paths(&data_dir, image_ref) {
            if !seen_paths.insert(path.clone()) || !path.exists() {
                continue;
            }

            std::fs::remove_file(&path)
                .map_err(|e| format!("删除图片文件失败 [{}]: {}", path.display(), e))?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{delete_clipboard_image_files, resolve_clipboard_image_paths};
    use crate::services::settings::{get_settings, replace_settings, AppSettings};
    use crate::services::test_support::lock_global_test_state;
    use std::path::Path;
    use std::{fs, path::PathBuf};
    use uuid::Uuid;

    fn with_test_data_dir(test: impl FnOnce(PathBuf)) {
        let _guard = lock_global_test_state();
        let original_settings = get_settings();
        let data_dir = std::env::temp_dir().join(format!(
            "quickclipboard-services-test-{}",
            Uuid::new_v4()
        ));

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            fs::create_dir_all(data_dir.join("clipboard_images"))
                .expect("create clipboard_images failed");
            replace_settings(AppSettings {
                use_custom_storage: true,
                custom_storage_path: Some(data_dir.to_string_lossy().to_string()),
                ..AppSettings::default()
            });
            test(data_dir.clone());
        }));

        replace_settings(original_settings);
        let _ = fs::remove_dir_all(&data_dir);

        if let Err(panic) = result {
            std::panic::resume_unwind(panic);
        }
    }

    #[test]
    fn resolve_clipboard_image_paths_supports_bare_id() {
        let data_dir = Path::new("C:/data");
        let paths = resolve_clipboard_image_paths(data_dir, "abc123");

        assert_eq!(paths, vec![data_dir.join("clipboard_images").join("abc123.png")]);
    }

    #[test]
    fn resolve_clipboard_image_paths_supports_stored_relative_path() {
        let data_dir = Path::new("C:/data");
        let paths = resolve_clipboard_image_paths(data_dir, "clipboard_images/demo.png");

        assert_eq!(paths, vec![data_dir.join("clipboard_images").join("demo.png")]);
    }

    #[test]
    fn resolve_clipboard_image_paths_supports_filename_with_extension() {
        let data_dir = Path::new("C:/data");
        let paths = resolve_clipboard_image_paths(data_dir, "demo.png");

        assert_eq!(paths, vec![data_dir.join("clipboard_images").join("demo.png")]);
    }

    #[test]
    fn resolve_clipboard_image_paths_rejects_prefixed_parent_dir_escape() {
        let data_dir = Path::new("C:/data");
        let paths = resolve_clipboard_image_paths(data_dir, "clipboard_images/../../outside.txt");

        assert!(paths.is_empty());
    }

    #[test]
    fn delete_clipboard_image_files_does_not_delete_outside_target_dir() {
        with_test_data_dir(|data_dir| {
            let outside_file = data_dir.join("outside.txt");
            fs::write(&outside_file, b"outside").expect("write outside file failed");

            delete_clipboard_image_files(&["clipboard_images/../../outside.txt".to_string()])
                .expect("delete should ignore unsafe refs");

            assert!(outside_file.exists());
        });
    }
}
