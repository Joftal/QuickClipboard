use super::processor::ProcessedContent;
use crate::services::database::clipboard::limit_clipboard_history;
use crate::services::database::connection::with_connection;
use crate::services::settings::get_settings;
use chrono;
use rusqlite::params;
use serde_json::Value;
use std::collections::HashSet;

enum StoreClipboardOutcome {
    Inserted(i64),
    Updated {
        id: i64,
        images_to_delete: Vec<String>,
    },
}

struct DuplicateMatchResult {
    id: i64,
    images_to_delete: Vec<String>,
}

// 计算文本字符数
fn calculate_char_count(content: &str, content_type: &str) -> Option<i64> {
    if content_type.contains("text") || content_type.contains("rich_text") {
        let count = content.chars().count() as i64;
        if count > 0 {
            Some(count)
        } else {
            None
        }
    } else {
        None
    }
}

pub fn store_clipboard_item(content: ProcessedContent) -> Result<i64, String> {
    let settings = get_settings();

    if !settings.save_images && is_image_type(&content.content_type) {
        return Err("已禁止保存图片".to_string());
    }

    let result = with_connection(|conn| {
        let now = chrono::Local::now().timestamp();

        match check_and_handle_duplicate(&content, conn, now) {
            Ok(Some(duplicate)) => {
                return Ok(StoreClipboardOutcome::Updated {
                    id: duplicate.id,
                    images_to_delete: duplicate.images_to_delete,
                });
            }
            Ok(None) => {}
            Err(error) => {
                eprintln!("检查重复内容失败: {}", error);
            }
        }

        let max_order: i64 = conn
            .query_row(
                "SELECT COALESCE(MAX(item_order), 0) FROM clipboard",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);
        let new_order = max_order + 1;
        let char_count = calculate_char_count(&content.content, &content.content_type);

        conn.execute(
            "INSERT INTO clipboard (content, html_content, content_type, image_id, item_order, source_app, source_icon_hash, char_count, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                content.content,
                content.html_content,
                content.content_type,
                content.image_id,
                new_order,
                content.source_app,
                content.source_icon_hash,
                char_count,
                now,
                now
            ],
        )?;

        Ok(StoreClipboardOutcome::Inserted(conn.last_insert_rowid()))
    });

    match result {
        Ok(StoreClipboardOutcome::Inserted(id)) => {
            let _ = limit_clipboard_history(settings.history_limit);
            Ok(id)
        }
        Ok(StoreClipboardOutcome::Updated {
            id,
            images_to_delete,
        }) => {
            if let Err(error) = delete_image_files(images_to_delete) {
                eprintln!("清理重复项遗留图片失败: {}", error);
            }
            let _ = limit_clipboard_history(settings.history_limit);
            Ok(id)
        }
        Err(error) => Err(error),
    }
}

// 智能去重：命中重复项时复用原记录，避免丢失元数据，同时清理旧图片引用
fn check_and_handle_duplicate(
    content: &ProcessedContent,
    conn: &rusqlite::Connection,
    now: i64,
) -> Result<Option<DuplicateMatchResult>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT id, content, content_type, is_pinned, image_id
         FROM clipboard
         ORDER BY created_at DESC
         LIMIT 100",
    )?;

    let recent_items = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, i64>(3)?,
            row.get::<_, Option<String>>(4)?,
        ))
    })?;

    for item in recent_items {
        let (db_id, db_content, db_type, is_pinned, previous_image_ids) = item?;

        let is_same = if is_text_type(&content.content_type) && is_text_type(&db_type) {
            content.content == db_content
        } else if is_file_type(&content.content_type) && is_file_type(&db_type) {
            compare_file_contents(&content.content, &db_content)
        } else {
            false
        };

        if !is_same {
            continue;
        }

        let max_order: i64 = conn
            .query_row(
                if is_pinned != 0 {
                    "SELECT COALESCE(MAX(item_order), 0) FROM clipboard WHERE is_pinned = 1"
                } else {
                    "SELECT COALESCE(MAX(item_order), 0) FROM clipboard WHERE is_pinned = 0"
                },
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let char_count = calculate_char_count(&content.content, &content.content_type);

        conn.execute(
            "UPDATE clipboard
             SET content = ?1,
                 html_content = ?2,
                 content_type = ?3,
                 image_id = ?4,
                 item_order = ?5,
                 source_app = ?6,
                 source_icon_hash = ?7,
                 char_count = ?8,
                 updated_at = ?9
             WHERE id = ?10",
            params![
                &content.content,
                &content.html_content,
                &content.content_type,
                &content.image_id,
                max_order + 1,
                &content.source_app,
                &content.source_icon_hash,
                char_count,
                now,
                db_id,
            ],
        )?;

        let images_to_delete = collect_deleted_image_ids_after_update(conn, previous_image_ids.as_deref())?;

        return Ok(Some(DuplicateMatchResult {
            id: db_id,
            images_to_delete,
        }));
    }

    Ok(None)
}

fn is_text_type(content_type: &str) -> bool {
    content_type.starts_with("text") || content_type.contains("rich_text") || content_type.contains("link")
}

fn is_file_type(content_type: &str) -> bool {
    content_type.contains("image") || content_type.contains("file")
}

fn is_image_type(content_type: &str) -> bool {
    content_type.contains("image")
}

fn split_image_ids(s: &str) -> Vec<String> {
    s.split(',')
        .map(|x| x.trim())
        .filter(|x| !x.is_empty())
        .map(|x| x.to_string())
        .collect()
}

fn is_image_id_referenced(
    conn: &rusqlite::Connection,
    image_id: &str,
) -> Result<bool, rusqlite::Error> {
    let exact = image_id;
    let p1 = format!("{},%", image_id);
    let p2 = format!("%,{},%", image_id);
    let p3 = format!("%,{}", image_id);

    let exists_in = |table: &str| -> Result<bool, rusqlite::Error> {
        let sql = format!(
            "SELECT EXISTS(SELECT 1 FROM {} WHERE image_id = ?1 OR image_id LIKE ?2 OR image_id LIKE ?3 OR image_id LIKE ?4)",
            table
        );
        let exists: i64 = conn.query_row(&sql, params![exact, p1, p2, p3], |row| row.get(0))?;
        Ok(exists != 0)
    };

    Ok(exists_in("clipboard")? || exists_in("favorites")?)
}

fn collect_deleted_image_ids_after_update(
    conn: &rusqlite::Connection,
    previous_image_ids: Option<&str>,
) -> Result<Vec<String>, rusqlite::Error> {
    let Some(previous_image_ids) = previous_image_ids else {
        return Ok(Vec::new());
    };

    let mut seen = HashSet::new();
    let mut to_delete = Vec::new();

    for image_id in split_image_ids(previous_image_ids) {
        if !seen.insert(image_id.clone()) {
            continue;
        }

        if !is_image_id_referenced(conn, &image_id)? {
            to_delete.push(image_id);
        }
    }

    Ok(to_delete)
}

fn delete_image_files(image_ids: Vec<String>) -> Result<(), String> {
    crate::services::delete_clipboard_image_files(&image_ids)
}

// 比较文件内容
fn compare_file_contents(content1: &str, content2: &str) -> bool {
    if !content1.starts_with("files:") || !content2.starts_with("files:") {
        return content1 == content2;
    }

    let Ok(json1) = serde_json::from_str::<Value>(&content1[6..]) else {
        return false;
    };
    let Ok(json2) = serde_json::from_str::<Value>(&content2[6..]) else {
        return false;
    };

    extract_file_paths(&json1) == extract_file_paths(&json2)
}

// 从 JSON 提取并排序文件路径
fn extract_file_paths(json: &Value) -> Vec<String> {
    let mut paths: Vec<String> = json["files"]
        .as_array()
        .into_iter()
        .flat_map(|files| files.iter())
        .filter_map(|file| file["path"].as_str().map(String::from))
        .collect();

    paths.sort();
    paths
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::database::connection::close_database;
    use crate::services::database::{
        get_clipboard_count, get_clipboard_item_by_id, increment_paste_count, init_database,
        toggle_pin_clipboard_item,
    };
    use crate::services::settings::{get_settings, replace_settings, AppSettings};
    use crate::services::test_support::lock_global_test_state;
    use std::fs;
    use std::path::PathBuf;
    use uuid::Uuid;

    fn rich_text_content(
        content: &str,
        html: &str,
        image_id: &str,
        source_app: &str,
    ) -> ProcessedContent {
        ProcessedContent {
            content: content.to_string(),
            html_content: Some(html.to_string()),
            content_type: "rich_text".to_string(),
            image_id: Some(image_id.to_string()),
            source_app: Some(source_app.to_string()),
            source_icon_hash: Some(format!("icon-{}", source_app)),
        }
    }

    fn with_test_database(test: impl FnOnce(PathBuf)) {
        let _guard = lock_global_test_state();
        let original_settings = get_settings();
        let data_dir = std::env::temp_dir().join(format!(
            "quickclipboard-storage-test-{}",
            Uuid::new_v4()
        ));

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            close_database();

            let settings = AppSettings {
                use_custom_storage: true,
                custom_storage_path: Some(data_dir.to_string_lossy().to_string()),
                ..AppSettings::default()
            };
            replace_settings(settings);

            fs::create_dir_all(data_dir.join("clipboard_images")).unwrap();
            let db_path = data_dir.join("quickclipboard.db");
            init_database(db_path.to_string_lossy().as_ref()).unwrap();

            test(data_dir.clone());
        }));

        close_database();
        replace_settings(original_settings);
        let _ = fs::remove_dir_all(&data_dir);

        if let Err(panic) = result {
            std::panic::resume_unwind(panic);
        }
    }

    #[test]
    fn duplicate_update_preserves_metadata_and_removes_stale_images() {
        with_test_database(|data_dir| {
            let images_dir = data_dir.join("clipboard_images");
            let old_image_path = images_dir.join("old.png");
            let new_image_path = images_dir.join("new.png");
            fs::write(&old_image_path, b"old").unwrap();
            fs::write(&new_image_path, b"new").unwrap();

            let first_id =
                store_clipboard_item(rich_text_content("same text", "<p>old</p>", "old", "alpha"))
                    .unwrap();
            increment_paste_count(first_id).unwrap();
            assert!(toggle_pin_clipboard_item(first_id).unwrap());

            let before = get_clipboard_item_by_id(first_id).unwrap().unwrap();

            let updated_id =
                store_clipboard_item(rich_text_content("same text", "<p>new</p>", "new", "beta"))
                    .unwrap();

            assert_eq!(updated_id, first_id);
            assert_eq!(get_clipboard_count().unwrap(), 1);

            let after = get_clipboard_item_by_id(first_id).unwrap().unwrap();
            assert_eq!(after.id, first_id);
            assert_eq!(after.created_at, before.created_at);
            assert_eq!(after.paste_count, before.paste_count);
            assert!(after.is_pinned);
            assert_eq!(after.html_content.as_deref(), Some("<p>new</p>"));
            assert_eq!(after.image_id.as_deref(), Some("new"));
            assert_eq!(after.source_app.as_deref(), Some("beta"));
            assert_eq!(after.source_icon_hash.as_deref(), Some("icon-beta"));
            assert!(!old_image_path.exists());
            assert!(new_image_path.exists());
        });
    }

    #[test]
    fn duplicate_update_keeps_shared_images_when_still_referenced() {
        with_test_database(|data_dir| {
            let images_dir = data_dir.join("clipboard_images");
            let old_image_path = images_dir.join("shared.png");
            let new_image_path = images_dir.join("fresh.png");
            fs::write(&old_image_path, b"shared").unwrap();
            fs::write(&new_image_path, b"fresh").unwrap();

            let first_id = store_clipboard_item(rich_text_content(
                "same text",
                "<p>old</p>",
                "shared",
                "alpha",
            ))
            .unwrap();
            let _second_id = store_clipboard_item(rich_text_content(
                "other text",
                "<p>other</p>",
                "shared",
                "gamma",
            ))
            .unwrap();

            let updated_id = store_clipboard_item(rich_text_content(
                "same text",
                "<p>new</p>",
                "fresh",
                "beta",
            ))
            .unwrap();

            assert_eq!(updated_id, first_id);
            assert_eq!(get_clipboard_count().unwrap(), 2);
            assert!(old_image_path.exists());
            assert!(new_image_path.exists());
        });
    }
}
