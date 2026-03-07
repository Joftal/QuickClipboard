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

fn get_attached_table_columns(
    conn: &rusqlite::Connection,
    schema: &str,
    table: &str,
) -> Result<std::collections::HashSet<String>, String> {
    let pragma_sql = format!("PRAGMA {}.table_info({})", schema, table);
    let mut stmt = conn.prepare(&pragma_sql)
        .map_err(|e| format!("读取 {}.{} 表结构失败: {}", schema, table, e))?;

    let columns = stmt.query_map([], |row| row.get::<_, String>(1))
        .map_err(|e| format!("读取 {}.{} 字段列表失败: {}", schema, table, e))?
        .collect::<Result<std::collections::HashSet<_>, _>>()
        .map_err(|e| format!("解析 {}.{} 字段列表失败: {}", schema, table, e))?;

    Ok(columns)
}

fn ensure_required_columns(
    columns: &std::collections::HashSet<String>,
    table: &str,
    required: &[&str],
) -> Result<(), String> {
    let missing: Vec<&str> = required
        .iter()
        .copied()
        .filter(|column| !columns.contains(*column))
        .collect();

    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "导入数据库中的 {} 表缺少必需字段: {}",
            table,
            missing.join(", ")
        ))
    }
}

fn import_column_expr(
    columns: &std::collections::HashSet<String>,
    column: &str,
    default_expr: &str,
) -> String {
    if columns.contains(column) {
        format!("src.{}", column)
    } else {
        format!("{} AS {}", default_expr, column)
    }
}

fn merge_database(src_db: &Path) -> Result<(), String> {
    with_connection(|conn| {
        let import_path = src_db.to_str().ok_or(rusqlite::Error::InvalidPath("bad path".into()))?;
        conn.execute("ATTACH DATABASE ?1 AS importdb", [import_path])?;

        let merge_result = (|| -> Result<(), String> {
            let group_columns = get_attached_table_columns(conn, "importdb", "groups")?;
            ensure_required_columns(
                &group_columns,
                "groups",
                &["name", "icon", "order_index", "created_at", "updated_at"],
            )?;

            let favorite_columns = get_attached_table_columns(conn, "importdb", "favorites")?;
            ensure_required_columns(
                &favorite_columns,
                "favorites",
                &[
                    "id",
                    "title",
                    "content",
                    "html_content",
                    "content_type",
                    "image_id",
                    "group_name",
                    "created_at",
                    "updated_at",
                ],
            )?;

            let clipboard_columns = get_attached_table_columns(conn, "importdb", "clipboard")?;
            ensure_required_columns(
                &clipboard_columns,
                "clipboard",
                &["content", "html_content", "content_type", "image_id", "created_at", "updated_at"],
            )?;

            let groups_sql = format!(
                "INSERT OR IGNORE INTO groups (name, icon, color, order_index, created_at, updated_at)
                 SELECT src.name, src.icon, {}, src.order_index, src.created_at, src.updated_at
                 FROM importdb.groups AS src",
                import_column_expr(&group_columns, "color", "'#dc2626'"),
            );
            conn.execute(&groups_sql, [])
                .map_err(|e| format!("合并分组数据失败: {}", e))?;

            let favorites_sql = format!(
                "INSERT OR IGNORE INTO favorites (
                    id, title, content, html_content, content_type, image_id,
                    group_name, item_order, paste_count, char_count, created_at, updated_at
                 )
                 SELECT
                    src.id,
                    src.title,
                    src.content,
                    src.html_content,
                    src.content_type,
                    src.image_id,
                    src.group_name,
                    {},
                    {},
                    {},
                    src.created_at,
                    src.updated_at
                 FROM importdb.favorites AS src",
                import_column_expr(&favorite_columns, "item_order", "0"),
                import_column_expr(&favorite_columns, "paste_count", "0"),
                import_column_expr(&favorite_columns, "char_count", "NULL"),
            );
            conn.execute(&favorites_sql, [])
                .map_err(|e| format!("合并收藏数据失败: {}", e))?;

            let clipboard_sql = format!(
                "INSERT INTO clipboard (
                    content, html_content, content_type, image_id,
                    item_order, is_pinned, paste_count, source_app, source_icon_hash, char_count,
                    created_at, updated_at
                 )
                 SELECT
                    src.content,
                    src.html_content,
                    src.content_type,
                    src.image_id,
                    {},
                    {},
                    {},
                    {},
                    {},
                    {},
                    src.created_at,
                    src.updated_at
                 FROM importdb.clipboard AS src",
                import_column_expr(&clipboard_columns, "item_order", "0"),
                import_column_expr(&clipboard_columns, "is_pinned", "0"),
                import_column_expr(&clipboard_columns, "paste_count", "0"),
                import_column_expr(&clipboard_columns, "source_app", "NULL"),
                import_column_expr(&clipboard_columns, "source_icon_hash", "NULL"),
                import_column_expr(&clipboard_columns, "char_count", "NULL"),
            );
            conn.execute(&clipboard_sql, [])
                .map_err(|e| format!("合并剪贴板数据失败: {}", e))?;

            reorder_clipboard_by_time(conn);
            Ok(())
        })();

        let detach_result = conn.execute("DETACH DATABASE importdb", []);

        merge_result.map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::other(e))))?;
        detach_result.map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::other(format!("分离导入数据库失败: {}", e)))))?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::database::connection::close_database;
    use crate::services::database::init_database;
    use once_cell::sync::Lazy;
    use rusqlite::params;
    use std::sync::Mutex;
    use uuid::Uuid;

    static TEST_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

    fn with_merge_test_databases(test: impl FnOnce(PathBuf, PathBuf)) {
        let _guard = TEST_MUTEX.lock().expect("test mutex poisoned");
        let base_dir = std::env::temp_dir().join(format!(
            "quickclipboard-merge-test-{}",
            Uuid::new_v4()
        ));
        let source_db = base_dir.join("source.db");
        let target_db = base_dir.join("target.db");

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            fs::create_dir_all(&base_dir).expect("create test dir failed");
            test(source_db.clone(), target_db.clone());
        }));

        close_database();
        let _ = fs::remove_dir_all(&base_dir);

        if let Err(panic) = result {
            std::panic::resume_unwind(panic);
        }
    }

    #[test]
    fn merge_database_preserves_metadata_from_source_db() {
        with_merge_test_databases(|source_db, target_db| {
            init_database(source_db.to_string_lossy().as_ref()).expect("init source db failed");
            with_connection(|conn| {
                conn.execute(
                    "INSERT INTO groups (name, icon, color, order_index, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params!["工作", "ti ti-briefcase", "#00ff88", 1_i64, 100_i64, 200_i64],
                )?;
                conn.execute(
                    "INSERT INTO favorites (
                        id, title, content, html_content, content_type, image_id,
                        group_name, item_order, paste_count, char_count, created_at, updated_at
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                    params![
                        "fav-1",
                        "标题",
                        "收藏内容",
                        "<p>收藏内容</p>",
                        "rich_text",
                        "img-fav",
                        "工作",
                        9_i64,
                        7_i64,
                        123_i64,
                        300_i64,
                        400_i64,
                    ],
                )?;
                conn.execute(
                    "INSERT INTO clipboard (
                        content, html_content, content_type, image_id,
                        item_order, is_pinned, paste_count, source_app, source_icon_hash, char_count,
                        created_at, updated_at
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                    params![
                        "剪贴板内容",
                        "<p>剪贴板内容</p>",
                        "rich_text",
                        "img-clip",
                        12_i64,
                        1_i64,
                        5_i64,
                        "Code.exe",
                        "icon-code",
                        456_i64,
                        500_i64,
                        600_i64,
                    ],
                )?;
                Ok(())
            }).expect("seed source data failed");
            close_database();

            init_database(target_db.to_string_lossy().as_ref()).expect("init target db failed");
            merge_database(&source_db).expect("merge database failed");

            with_connection(|conn| {
                let group_color: String = conn.query_row(
                    "SELECT color FROM groups WHERE name = ?1",
                    params!["工作"],
                    |row| row.get(0),
                )?;
                assert_eq!(group_color, "#00ff88");

                let favorite: (i64, Option<i64>) = conn.query_row(
                    "SELECT paste_count, char_count FROM favorites WHERE id = ?1",
                    params!["fav-1"],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )?;
                assert_eq!(favorite.0, 7);
                assert_eq!(favorite.1, Some(123));

                let clipboard: (i64, i64, Option<String>, Option<String>, Option<i64>) = conn.query_row(
                    "SELECT is_pinned, paste_count, source_app, source_icon_hash, char_count
                     FROM clipboard WHERE content = ?1",
                    params!["剪贴板内容"],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
                )?;
                assert_eq!(clipboard.0, 1);
                assert_eq!(clipboard.1, 5);
                assert_eq!(clipboard.2.as_deref(), Some("Code.exe"));
                assert_eq!(clipboard.3.as_deref(), Some("icon-code"));
                assert_eq!(clipboard.4, Some(456));

                Ok(())
            }).expect("verify merged metadata failed");
        });
    }

    #[test]
    fn merge_database_supports_legacy_source_schema() {
        with_merge_test_databases(|source_db, target_db| {
            let source_conn = rusqlite::Connection::open(&source_db).expect("open legacy source db failed");
            source_conn.execute_batch(
                "CREATE TABLE groups (
                    name TEXT PRIMARY KEY,
                    icon TEXT NOT NULL DEFAULT 'ti ti-folder',
                    order_index INTEGER NOT NULL DEFAULT 0,
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL
                );
                CREATE TABLE favorites (
                    id TEXT PRIMARY KEY,
                    title TEXT NOT NULL,
                    content TEXT NOT NULL,
                    html_content TEXT,
                    content_type TEXT NOT NULL DEFAULT 'text',
                    image_id TEXT,
                    group_name TEXT NOT NULL DEFAULT '全部',
                    item_order INTEGER NOT NULL DEFAULT 0,
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL
                );
                CREATE TABLE clipboard (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    content TEXT NOT NULL,
                    html_content TEXT,
                    content_type TEXT NOT NULL DEFAULT 'text',
                    image_id TEXT,
                    item_order INTEGER NOT NULL DEFAULT 0,
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL
                );"
            ).expect("create legacy schema failed");
            source_conn.execute(
                "INSERT INTO groups (name, icon, order_index, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params!["旧分组", "ti ti-folder", 1_i64, 10_i64, 20_i64],
            ).expect("insert legacy group failed");
            source_conn.execute(
                "INSERT INTO favorites (
                    id, title, content, html_content, content_type, image_id,
                    group_name, item_order, created_at, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    "legacy-fav",
                    "旧收藏",
                    "旧内容",
                    "<p>旧内容</p>",
                    "rich_text",
                    "legacy-img",
                    "旧分组",
                    2_i64,
                    30_i64,
                    40_i64,
                ],
            ).expect("insert legacy favorite failed");
            source_conn.execute(
                "INSERT INTO clipboard (content, html_content, content_type, image_id, item_order, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params!["旧剪贴板", "<p>旧剪贴板</p>", "rich_text", "legacy-clip-img", 3_i64, 50_i64, 60_i64],
            ).expect("insert legacy clipboard failed");
            drop(source_conn);

            init_database(target_db.to_string_lossy().as_ref()).expect("init target db failed");
            merge_database(&source_db).expect("merge legacy database failed");

            with_connection(|conn| {
                let group_color: String = conn.query_row(
                    "SELECT color FROM groups WHERE name = ?1",
                    params!["旧分组"],
                    |row| row.get(0),
                )?;
                assert_eq!(group_color, "#dc2626");

                let favorite: (i64, Option<i64>) = conn.query_row(
                    "SELECT paste_count, char_count FROM favorites WHERE id = ?1",
                    params!["legacy-fav"],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )?;
                assert_eq!(favorite.0, 0);
                assert_eq!(favorite.1, None);

                let clipboard: (i64, i64, Option<String>, Option<String>, Option<i64>) = conn.query_row(
                    "SELECT is_pinned, paste_count, source_app, source_icon_hash, char_count
                     FROM clipboard WHERE content = ?1",
                    params!["旧剪贴板"],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
                )?;
                assert_eq!(clipboard.0, 0);
                assert_eq!(clipboard.1, 0);
                assert_eq!(clipboard.2, None);
                assert_eq!(clipboard.3, None);
                assert_eq!(clipboard.4, None);

                Ok(())
            }).expect("verify legacy merge failed");
        });
    }
}




