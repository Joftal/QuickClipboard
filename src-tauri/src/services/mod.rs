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
use std::path::{Path, PathBuf};

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

pub fn is_portable_build() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.file_name().and_then(|s| s.to_str()).map(|s| s.to_ascii_lowercase()))
        .map(|name| name.contains("portable"))
        .unwrap_or(false)
}

fn resolve_clipboard_image_paths(data_dir: &Path, image_ref: &str) -> Vec<PathBuf> {
    let normalized = image_ref.trim().replace('\\', "/");
    if normalized.is_empty() {
        return Vec::new();
    }

    if let Some(relative) = normalized.strip_prefix("clipboard_images/") {
        return vec![data_dir.join("clipboard_images").join(relative)];
    }

    let file_name = Path::new(&normalized)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(image_ref)
        .trim();

    if file_name.is_empty() {
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
    use super::resolve_clipboard_image_paths;
    use std::path::Path;

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
}
