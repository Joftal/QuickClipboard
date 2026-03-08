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

fn remove_dir_if_exists(path: &Path, label: &str) -> Result<(), String> {
    if path.exists() {
        fs::remove_dir_all(path).map_err(|e| format!("删除{}失败: {}", label, e))?;
    }
    Ok(())
}

fn remove_file_if_exists(path: &Path, label: &str) -> Result<(), String> {
    if path.exists() {
        fs::remove_file(path).map_err(|e| format!("删除{}失败: {}", label, e))?;
    }
    Ok(())
}

fn clean_storage_artifacts(dir: &Path) -> Result<(), String> {
    remove_dir_if_exists(&dir.join("clipboard_images"), "图片目录")?;
    remove_dir_if_exists(&dir.join("pin_images"), "贴图目录")?;
    remove_dir_if_exists(&dir.join("image_library"), "图库目录")?;
    remove_dir_if_exists(&dir.join("app_icons"), "图标目录")?;

    remove_file_if_exists(&dir.join("quickclipboard.db"), "数据库文件")?;
    remove_file_if_exists(&dir.join("quickclipboard.db-shm"), "数据库共享内存文件")?;
    remove_file_if_exists(&dir.join("quickclipboard.db-wal"), "数据库 WAL 文件")?;

    Ok(())
}

fn normalize_path_for_comparison(path: &Path) -> Result<String, String> {
    let canonical = path
        .canonicalize()
        .map_err(|e| format!("规范化路径失败 [{}]: {}", path.to_string_lossy(), e))?;

    #[cfg(windows)]
    {
        let canonical = canonical.to_string_lossy();
        let normalized = if let Some(path) = canonical.strip_prefix(r"\\?\UNC\") {
            format!(r"\\{}", path)
        } else if let Some(path) = canonical.strip_prefix(r"\\?\") {
            path.to_string()
        } else {
            canonical.to_string()
        };

        Ok(normalized.replace('/', "\\").to_ascii_lowercase())
    }

    #[cfg(not(windows))]
    {
        Ok(canonical.to_string_lossy().to_string())
    }
}

fn paths_refer_to_same_location(left: &Path, right: &Path) -> Result<bool, String> {
    Ok(normalize_path_for_comparison(left)? == normalize_path_for_comparison(right)?)
}

pub fn reset_all_data() -> Result<String, String> {
    let current_dir = get_current_storage_dir()?;
    let default_dir = get_default_data_dir()?;

    let _ = crate::services::database::connection::with_connection(|conn| {
        conn.execute_batch("PRAGMA wal_checkpoint(FULL); PRAGMA wal_checkpoint(TRUNCATE);")
    });
    close_database();

    clean_storage_artifacts(&current_dir)?;
    if !paths_refer_to_same_location(&current_dir, &default_dir)? {
        clean_storage_artifacts(&default_dir)?;
    }

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
    if !new_dir.exists() { fs::create_dir_all(&new_dir).map_err(|e| e.to_string())?; }

    let current_dir = get_current_storage_dir()?;
    if paths_refer_to_same_location(&new_dir, &current_dir)? {
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
            let tx = conn.unchecked_transaction()
                .map_err(|e| format!("开始数据库合并事务失败: {}", e))?;

            let group_columns = get_attached_table_columns(&tx, "importdb", "groups")?;
            ensure_required_columns(
                &group_columns,
                "groups",
                &["name", "icon", "order_index", "created_at", "updated_at"],
            )?;

            let favorite_columns = get_attached_table_columns(&tx, "importdb", "favorites")?;
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

            let clipboard_columns = get_attached_table_columns(&tx, "importdb", "clipboard")?;
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
            tx.execute(&groups_sql, [])
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
            tx.execute(&favorites_sql, [])
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
            tx.execute(&clipboard_sql, [])
                .map_err(|e| format!("合并剪贴板数据失败: {}", e))?;

            reorder_clipboard_by_time(&tx);
            tx.commit()
                .map_err(|e| format!("提交数据库合并事务失败: {}", e))?;
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
    let default_dir = get_default_data_dir()?;
    let current_dir = get_current_storage_dir()?;

    if paths_refer_to_same_location(&current_dir, &default_dir)? {
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
    use crate::services::test_support::lock_global_test_state;
    use rusqlite::params;
    use uuid::Uuid;

    fn with_merge_test_databases(test: impl FnOnce(PathBuf, PathBuf)) {
        let _guard = lock_global_test_state();
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

    fn with_temp_dir(prefix: &str, test: impl FnOnce(PathBuf)) {
        let _guard = lock_global_test_state();
        let base_dir = std::env::temp_dir().join(format!(
            "{}-{}",
            prefix,
            Uuid::new_v4()
        ));

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            fs::create_dir_all(&base_dir).expect("create test dir failed");
            test(base_dir.clone());
        }));

        let _ = fs::remove_dir_all(&base_dir);

        if let Err(panic) = result {
            std::panic::resume_unwind(panic);
        }
    }

    #[test]
    fn paths_refer_to_same_location_treats_dot_suffix_as_same_dir() {
        with_temp_dir("quickclipboard-path-compare", |base_dir| {
            let dotted = base_dir.join(".");
            let same = paths_refer_to_same_location(&base_dir, &dotted)
                .expect("compare dotted path failed");
            assert!(same);
        });
    }

    #[cfg(windows)]
    #[test]
    fn paths_refer_to_same_location_is_case_insensitive_on_windows() {
        with_temp_dir("QuickClipboard-Case-Compare", |base_dir| {
            let upper = PathBuf::from(base_dir.to_string_lossy().to_uppercase());
            let lower = PathBuf::from(base_dir.to_string_lossy().to_lowercase());
            let same = paths_refer_to_same_location(&upper, &lower)
                .expect("compare case-insensitive path failed");
            assert!(same);
        });
    }

    #[test]
    fn paths_refer_to_same_location_distinguishes_different_dirs() {
        with_temp_dir("quickclipboard-path-diff", |base_dir| {
            let sibling = base_dir.join("..")
                .join(format!("{}-other", base_dir.file_name().and_then(|v| v.to_str()).unwrap_or("dir")));
            fs::create_dir_all(&sibling).expect("create sibling dir failed");

            let same = paths_refer_to_same_location(&base_dir, &sibling)
                .expect("compare different dirs failed");
            assert!(!same);

            fs::remove_dir_all(&sibling).expect("remove sibling dir failed");
        });
    }

    #[cfg(windows)]
    #[test]
    fn clean_storage_artifacts_returns_error_when_db_file_cannot_be_removed() {
        with_temp_dir("quickclipboard-clean-test", |base_dir| {
            let db_path = base_dir.join("quickclipboard.db");
            fs::create_dir_all(&db_path).expect("create fake db dir failed");

            let result = clean_storage_artifacts(&base_dir);
            assert!(result.is_err(), "expected cleanup to fail for invalid db path type");
            let message = result.err().unwrap_or_default();
            assert!(message.contains("数据库文件"), "unexpected error message: {}", message);

            fs::remove_dir_all(&db_path).expect("remove fake db dir failed");
        });
    }

    #[test]
    fn clean_storage_artifacts_removes_known_files_and_dirs() {
        with_temp_dir("quickclipboard-clean-success", |base_dir| {
            let clipboard_images = base_dir.join("clipboard_images");
            let pin_images = base_dir.join("pin_images");
            let image_library = base_dir.join("image_library");
            let app_icons = base_dir.join("app_icons");
            fs::create_dir_all(&clipboard_images).expect("create clipboard_images failed");
            fs::create_dir_all(&pin_images).expect("create pin_images failed");
            fs::create_dir_all(&image_library).expect("create image_library failed");
            fs::create_dir_all(&app_icons).expect("create app_icons failed");
            fs::write(clipboard_images.join("a.txt"), b"x").expect("seed clipboard_images failed");
            fs::write(pin_images.join("p.txt"), b"x").expect("seed pin_images failed");
            fs::write(image_library.join("b.txt"), b"x").expect("seed image_library failed");
            fs::write(app_icons.join("c.txt"), b"x").expect("seed app_icons failed");
            fs::write(base_dir.join("quickclipboard.db"), b"db").expect("seed db failed");
            fs::write(base_dir.join("quickclipboard.db-shm"), b"shm").expect("seed shm failed");
            fs::write(base_dir.join("quickclipboard.db-wal"), b"wal").expect("seed wal failed");

            clean_storage_artifacts(&base_dir).expect("cleanup should succeed");

            assert!(!clipboard_images.exists());
            assert!(!pin_images.exists());
            assert!(!image_library.exists());
            assert!(!app_icons.exists());
            assert!(!base_dir.join("quickclipboard.db").exists());
            assert!(!base_dir.join("quickclipboard.db-shm").exists());
            assert!(!base_dir.join("quickclipboard.db-wal").exists());
        });
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

    #[test]
    fn merge_database_rolls_back_when_clipboard_import_fails() {
        with_merge_test_databases(|source_db, target_db| {
            let source_conn = rusqlite::Connection::open(&source_db).expect("open source db failed");
            source_conn.execute_batch(
                "CREATE TABLE groups (
                    name TEXT PRIMARY KEY,
                    icon TEXT NOT NULL,
                    color TEXT NOT NULL DEFAULT '#dc2626',
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
                    paste_count INTEGER NOT NULL DEFAULT 0,
                    char_count INTEGER,
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL
                );
                CREATE TABLE clipboard (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    content TEXT,
                    html_content TEXT,
                    content_type TEXT NOT NULL DEFAULT 'text',
                    image_id TEXT,
                    item_order INTEGER NOT NULL DEFAULT 0,
                    is_pinned INTEGER NOT NULL DEFAULT 0,
                    paste_count INTEGER NOT NULL DEFAULT 0,
                    source_app TEXT,
                    source_icon_hash TEXT,
                    char_count INTEGER,
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL
                );"
            ).expect("create source schema failed");
            source_conn.execute(
                "INSERT INTO groups (name, icon, color, order_index, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params!["事务分组", "ti ti-folder", "#123456", 1_i64, 10_i64, 20_i64],
            ).expect("insert group failed");
            source_conn.execute(
                "INSERT INTO favorites (
                    id, title, content, html_content, content_type, image_id,
                    group_name, item_order, paste_count, char_count, created_at, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    "tx-fav",
                    "事务收藏",
                    "事务内容",
                    Option::<String>::None,
                    "text",
                    Option::<String>::None,
                    "事务分组",
                    1_i64,
                    0_i64,
                    Option::<i64>::None,
                    30_i64,
                    40_i64,
                ],
            ).expect("insert favorite failed");
            source_conn.execute(
                "INSERT INTO clipboard (
                    content, html_content, content_type, image_id, item_order, is_pinned,
                    paste_count, source_app, source_icon_hash, char_count, created_at, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    Option::<String>::None,
                    Option::<String>::None,
                    "text",
                    Option::<String>::None,
                    1_i64,
                    0_i64,
                    0_i64,
                    Option::<String>::None,
                    Option::<String>::None,
                    Option::<i64>::None,
                    50_i64,
                    60_i64,
                ],
            ).expect("insert invalid clipboard row failed");
            drop(source_conn);

            init_database(target_db.to_string_lossy().as_ref()).expect("init target db failed");
            let result = merge_database(&source_db);
            assert!(result.is_err(), "expected merge to fail");

            with_connection(|conn| {
                let groups_count: i64 = conn.query_row("SELECT COUNT(*) FROM groups", [], |row| row.get(0))?;
                let favorites_count: i64 = conn.query_row("SELECT COUNT(*) FROM favorites", [], |row| row.get(0))?;
                let clipboard_count: i64 = conn.query_row("SELECT COUNT(*) FROM clipboard", [], |row| row.get(0))?;

                assert_eq!(groups_count, 0, "groups should be rolled back");
                assert_eq!(favorites_count, 0, "favorites should be rolled back");
                assert_eq!(clipboard_count, 0, "clipboard should be rolled back");

                Ok(())
            }).expect("verify rollback failed");
        });
    }
}




