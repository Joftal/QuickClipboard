use std::{fs, path::{Path, PathBuf}};
use serde::Serialize;

use crate::services::{get_data_directory, get_settings, update_settings};
use crate::services::settings::storage::SettingsStorage;
use crate::services::database::{init_database};
use crate::services::database::connection::{close_database, with_connection};
use crate::services::system::hotkey::reload_from_settings;

#[derive(Debug, Clone, Serialize)]
pub struct TargetDataInfo {
    pub has_data: bool,
    pub has_database: bool,
    pub has_images: bool,
    pub has_image_library: bool,
    pub database_size: u64,
    pub images_count: usize,
    pub images_size: u64,
    pub image_library_count: usize,
    pub image_library_size: u64,
}

pub fn check_target_has_data(target_dir: &Path) -> Result<TargetDataInfo, String> {
    let db_path = target_dir.join("quickclipboard.db");
    let images_dir = target_dir.join("clipboard_images");
    let image_library_dir = target_dir.join("image_library");
    
    let has_database = db_path.exists();
    let database_size = if has_database {
        fs::metadata(&db_path).map(|m| m.len()).unwrap_or(0)
    } else {
        0
    };
    
    let (has_images, images_count, images_size) = if images_dir.exists() {
        let mut count = 0usize;
        let mut size = 0u64;
        if let Ok(entries) = fs::read_dir(&images_dir) {
            for entry in entries.flatten() {
                if let Ok(meta) = entry.metadata() {
                    if meta.is_file() {
                        count += 1;
                        size += meta.len();
                    }
                }
            }
        }
        (count > 0, count, size)
    } else {
        (false, 0, 0)
    };
    
    let (has_image_library, image_library_count, image_library_size) = if image_library_dir.exists() {
        let mut count = 0usize;
        let mut size = 0u64;
        if let Ok(entries) = fs::read_dir(&image_library_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    if let Ok(sub_entries) = fs::read_dir(&path) {
                        for sub_entry in sub_entries.flatten() {
                            if let Ok(meta) = sub_entry.metadata() {
                                if meta.is_file() {
                                    count += 1;
                                    size += meta.len();
                                }
                            }
                        }
                    }
                } else if let Ok(meta) = entry.metadata() {
                    if meta.is_file() {
                        count += 1;
                        size += meta.len();
                    }
                }
            }
        }
        (count > 0, count, size)
    } else {
        (false, 0, 0)
    };
    
    Ok(TargetDataInfo {
        has_data: has_database || has_images || has_image_library,
        has_database,
        has_images,
        has_image_library,
        database_size,
        images_count,
        images_size,
        image_library_count,
        image_library_size,
    })
}

fn copy_dir_all(src: &Path, dst: &Path) -> Result<(), String> {
    if !dst.exists() {
        fs::create_dir_all(dst).map_err(|e| format!("创建目录失败: {}", e))?;
    }
    for entry in fs::read_dir(src).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let file_type = entry.file_type().map_err(|e| e.to_string())?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_all(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path).map_err(|e| format!("复制文件失败: {}", e))?;
        }
    }
    Ok(())
}

pub fn reset_all_data() -> Result<String, String> {
    let current_dir = get_current_storage_dir()?;
    let default_dir = get_default_data_dir()?;

    let _ = crate::services::database::connection::with_connection(|conn| {
        conn.execute_batch("PRAGMA wal_checkpoint(FULL); PRAGMA wal_checkpoint(TRUNCATE);")
    });
    close_database();

    fn clean_dir(dir: &Path) -> Result<(), String> {
        let images = dir.join("clipboard_images");
        if images.exists() { let _ = fs::remove_dir_all(&images); }
        let image_library = dir.join("image_library");
        if image_library.exists() { let _ = fs::remove_dir_all(&image_library); }
        let app_icons = dir.join("app_icons");
        if app_icons.exists() { let _ = fs::remove_dir_all(&app_icons); }
        for name in ["quickclipboard.db", "quickclipboard.db-shm", "quickclipboard.db-wal"] {
            let p = dir.join(name);
            if p.exists() { let _ = fs::remove_file(&p); }
        }
        Ok(())
    }

    clean_dir(&current_dir)?;
    if current_dir != default_dir { clean_dir(&default_dir)?; }

    let defaults = crate::services::AppSettings {
        use_custom_storage: false,
        custom_storage_path: None,
        ..crate::services::AppSettings::default()
    };
    update_settings(defaults.clone())?;

    let db_path = default_dir.join("quickclipboard.db");
    init_database(db_path.to_str().ok_or("数据库路径无效")?)?;
    let _ = crate::services::database::connection::with_connection(|conn| {
        conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
    });

    let _ = reload_from_settings();

    Ok(default_dir.to_string_lossy().to_string())
}

pub fn get_default_data_dir() -> Result<PathBuf, String> {
    let settings_path = SettingsStorage::get_settings_path()?;
    settings_path.parent().map(|p| p.to_path_buf()).ok_or("无法获取默认数据目录".to_string())
}

pub fn get_current_storage_dir() -> Result<PathBuf, String> {
    get_data_directory()
}

// mode: "source_only" | "target_only" | "merge"
pub fn change_storage_dir(new_dir: PathBuf, mode: &str) -> Result<PathBuf, String> {
    if crate::services::is_portable_build() || std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.join("portable.txt").exists())).unwrap_or(false) {
        return Err("便携版不支持更改存储路径".into());
    }
    if !new_dir.exists() { fs::create_dir_all(&new_dir).map_err(|e| e.to_string())?; }

    let current_dir = get_current_storage_dir()?;
    if new_dir == current_dir {
        return Err("新位置与当前存储位置相同，无需迁移".to_string());
    }

    change_storage_dir_internal(&current_dir, &new_dir, mode)?;

    let mut settings = get_settings();
    settings.use_custom_storage = true;
    settings.custom_storage_path = Some(new_dir.to_string_lossy().to_string());
    update_settings(settings.clone())?;

    let db_path = new_dir.join("quickclipboard.db");
    init_database(db_path.to_str().ok_or("数据库路径无效")?)?;
    let _ = crate::services::database::connection::with_connection(|conn| {
        conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
    });

    Ok(new_dir)
}

fn safe_move_item(src: &Path, dst: &Path) -> Result<(), String> {
    if !src.exists() { return Ok(()); }
    if fs::rename(src, dst).is_err() {
        if src.is_dir() {
            copy_dir_all(src, dst)?;
            fs::remove_dir_all(src).map_err(|e| format!("删除源目录失败: {}", e))?;
        } else {
            if let Some(parent) = dst.parent() { fs::create_dir_all(parent).map_err(|e| e.to_string())?; }
            fs::copy(src, dst).map_err(|e| format!("复制文件失败: {}", e))?;
            fs::remove_file(src).map_err(|e| format!("删除源文件失败: {}", e))?;
        }
    }
    Ok(())
}

fn merge_dir_no_overwrite(src: &Path, dst: &Path) -> Result<(), String> {
    for entry in fs::read_dir(src).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            if !dst_path.exists() { fs::create_dir_all(&dst_path).map_err(|e| e.to_string())?; }
            merge_dir_no_overwrite(&src_path, &dst_path)?;
        } else if !dst_path.exists() {
            if let Some(parent) = dst_path.parent() { fs::create_dir_all(parent).map_err(|e| e.to_string())?; }
            fs::copy(&src_path, &dst_path).map_err(|e| format!("复制文件失败: {}", e))?;
        }
    }
    Ok(())
}

fn merge_database(src_db: &Path) -> Result<(), String> {
    with_connection(|conn| {
        let import_path = src_db.to_str().ok_or(rusqlite::Error::InvalidPath("bad path".into()))?;
        conn.execute("ATTACH DATABASE ?1 AS importdb", [import_path])?;
        let _ = conn.execute(
            "INSERT OR IGNORE INTO groups (name, icon, color, order_index, created_at, updated_at)
             SELECT name, icon, color, order_index, created_at, updated_at FROM importdb.groups",
            [],
        );

        let _ = conn.execute(
            "INSERT OR IGNORE INTO favorites (id, title, content, html_content, content_type, image_id, group_name, item_order, created_at, updated_at)
             SELECT id, title, content, html_content, content_type, image_id, group_name, item_order, created_at, updated_at FROM importdb.favorites",
            [],
        );
        let _ = conn.execute(
            "INSERT INTO clipboard (content, html_content, content_type, image_id, created_at, updated_at)
             SELECT content, html_content, content_type, image_id, created_at, updated_at FROM importdb.clipboard",
            [],
        );

        let _ = conn.execute("DETACH DATABASE importdb", []);
        reorder_clipboard_by_time(conn);
        
        Ok(())
    })?;
    Ok(())
}

fn reorder_clipboard_by_time(conn: &rusqlite::Connection) {
    if let Ok(mut stmt) = conn.prepare(
        "SELECT id FROM clipboard ORDER BY is_pinned DESC, created_at DESC"
    ) {
        let ids: Vec<i64> = stmt.query_map([], |row| row.get(0))
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default();
        let count = ids.len() as i64;
        for (i, id) in ids.iter().enumerate() {
            conn.execute("UPDATE clipboard SET item_order = ? WHERE id = ?",
                rusqlite::params![count - i as i64, id]).ok();
        }
    }
}

pub fn reset_storage_dir_to_default(mode: &str) -> Result<PathBuf, String> {
    if crate::services::is_portable_build() || std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.join("portable.txt").exists())).unwrap_or(false) {
        return Err("便携版不支持重置存储路径".into());
    }
    let default_dir = get_default_data_dir()?;
    let current_dir = get_current_storage_dir()?;

    if current_dir == default_dir {
        return Err("当前已在默认存储位置".to_string());
    }

    change_storage_dir_internal(&current_dir, &default_dir, mode)?;

    let mut settings = get_settings();
    settings.use_custom_storage = false;
    settings.custom_storage_path = None;
    update_settings(settings.clone())?;

    let db_path = default_dir.join("quickclipboard.db");
    init_database(db_path.to_str().ok_or("数据库路径无效")?)?;
    let _ = crate::services::database::connection::with_connection(|conn| {
        conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
    });

    Ok(default_dir)
}

fn change_storage_dir_internal(src_dir: &Path, dst_dir: &Path, mode: &str) -> Result<(), String> {
    let _ = crate::services::database::connection::with_connection(|conn| {
        conn.execute_batch("PRAGMA wal_checkpoint(FULL); PRAGMA wal_checkpoint(TRUNCATE);")
    });
    close_database();

    let src_images = src_dir.join("clipboard_images");
    let dst_images = dst_dir.join("clipboard_images");
    let src_pin_images = src_dir.join("pin_images");
    let dst_pin_images = dst_dir.join("pin_images");
    let src_image_library = src_dir.join("image_library");
    let dst_image_library = dst_dir.join("image_library");
    let src_app_icons = src_dir.join("app_icons");
    let dst_app_icons = dst_dir.join("app_icons");
    let src_db = src_dir.join("quickclipboard.db");
    let dst_db = dst_dir.join("quickclipboard.db");

    match mode {
        "source_only" => {
            if dst_images.exists() {
                fs::remove_dir_all(&dst_images).map_err(|e| format!("删除目标图片目录失败: {}", e))?;
            }
            if dst_pin_images.exists() {
                fs::remove_dir_all(&dst_pin_images).map_err(|e| format!("删除目标贴图目录失败: {}", e))?;
            }
            if dst_image_library.exists() {
                fs::remove_dir_all(&dst_image_library).map_err(|e| format!("删除目标图库目录失败: {}", e))?;
            }
            if dst_app_icons.exists() {
                fs::remove_dir_all(&dst_app_icons).map_err(|e| format!("删除目标图标目录失败: {}", e))?;
            }
            if dst_db.exists() {
                fs::remove_file(&dst_db).map_err(|e| format!("删除目标数据库失败: {}", e))?;
            }
            if src_images.exists() {
                safe_move_item(&src_images, &dst_images)?;
            }
            if src_pin_images.exists() {
                safe_move_item(&src_pin_images, &dst_pin_images)?;
            }
            if src_image_library.exists() {
                safe_move_item(&src_image_library, &dst_image_library)?;
            }
            if src_app_icons.exists() {
                safe_move_item(&src_app_icons, &dst_app_icons)?;
            }
            if src_db.exists() {
                safe_move_item(&src_db, &dst_db)?;
            }
        }
        "target_only" => {
            if src_images.exists() {
                fs::remove_dir_all(&src_images).map_err(|e| format!("删除源图片目录失败: {}", e))?;
            }
            if src_pin_images.exists() {
                fs::remove_dir_all(&src_pin_images).map_err(|e| format!("删除源贴图目录失败: {}", e))?;
            }
            if src_image_library.exists() {
                fs::remove_dir_all(&src_image_library).map_err(|e| format!("删除源图库目录失败: {}", e))?;
            }
            if src_app_icons.exists() {
                fs::remove_dir_all(&src_app_icons).map_err(|e| format!("删除源图标目录失败: {}", e))?;
            }
            if src_db.exists() {
                fs::remove_file(&src_db).map_err(|e| format!("删除源数据库失败: {}", e))?;
            }
        }
        "merge" => {
            // 源数据优先：先把目标数据合并到源，再移动源到目标
            if src_images.exists() {
                if !dst_images.exists() { fs::create_dir_all(&dst_images).map_err(|e| e.to_string())?; }
                if dst_images.exists() { merge_dir_no_overwrite(&dst_images, &src_images)?; }
                if dst_images.exists() { fs::remove_dir_all(&dst_images).map_err(|e| format!("删除目标图片目录失败: {}", e))?; }
                safe_move_item(&src_images, &dst_images)?;
            }
            if src_pin_images.exists() {
                if !dst_pin_images.exists() { fs::create_dir_all(&dst_pin_images).map_err(|e| e.to_string())?; }
                if dst_pin_images.exists() { merge_dir_no_overwrite(&dst_pin_images, &src_pin_images)?; }
                if dst_pin_images.exists() { fs::remove_dir_all(&dst_pin_images).map_err(|e| format!("删除目标贴图目录失败: {}", e))?; }
                safe_move_item(&src_pin_images, &dst_pin_images)?;
            }
            if src_image_library.exists() {
                if !dst_image_library.exists() { fs::create_dir_all(&dst_image_library).map_err(|e| e.to_string())?; }
                if dst_image_library.exists() { merge_dir_no_overwrite(&dst_image_library, &src_image_library)?; }
                if dst_image_library.exists() { fs::remove_dir_all(&dst_image_library).map_err(|e| format!("删除目标图库目录失败: {}", e))?; }
                safe_move_item(&src_image_library, &dst_image_library)?;
            }
            if src_app_icons.exists() {
                if !dst_app_icons.exists() { fs::create_dir_all(&dst_app_icons).map_err(|e| e.to_string())?; }
                if dst_app_icons.exists() { merge_dir_no_overwrite(&dst_app_icons, &src_app_icons)?; }
                if dst_app_icons.exists() { fs::remove_dir_all(&dst_app_icons).map_err(|e| format!("删除目标图标目录失败: {}", e))?; }
                safe_move_item(&src_app_icons, &dst_app_icons)?;
            }
            if src_db.exists() {
                if dst_db.exists() {
                    init_database(src_db.to_str().ok_or("数据库路径无效")?)?;
                    merge_database(&dst_db)?;
                    close_database();
                    fs::remove_file(&dst_db).map_err(|e| format!("删除目标数据库失败: {}", e))?;
                }
                safe_move_item(&src_db, &dst_db)?;
            }
        }
        _ => {
            return Err(format!("不支持的迁移模式: {}", mode));
        }
    }

    for name in ["quickclipboard.db-shm", "quickclipboard.db-wal"] {
        let p = dst_dir.join(name);
        if p.exists() { let _ = fs::remove_file(&p); }
        let sp = src_dir.join(name);
        if sp.exists() { let _ = fs::remove_file(&sp); }
    }

    Ok(())
}




