use rusqlite::Connection;
use parking_lot::Mutex;
use once_cell::sync::Lazy;

pub const MAX_CONTENT_LENGTH: usize = 1600;

// 数据库连接
static DB_CONNECTION: Lazy<Mutex<Option<Connection>>> = 
    Lazy::new(|| Mutex::new(None));

// 初始化数据库连接
pub fn init_database(db_path: &str) -> Result<(), String> {
    let conn = Connection::open(db_path)
        .map_err(|e| format!("打开数据库失败: {}", e))?;
    
    // 创建表结构
    create_tables(&conn)?;

    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;
         PRAGMA cache_size = 10000;
         PRAGMA temp_store = MEMORY;"
    ).map_err(|e| format!("设置数据库参数失败: {}", e))?;
    
    let mut db_conn = DB_CONNECTION.lock();
    *db_conn = Some(conn);
    
    Ok(())
}

// 关闭数据库连接
pub fn close_database() {
    let mut db_conn = DB_CONNECTION.lock();
    if db_conn.is_some() {
        *db_conn = None;
    }
}

// 创建数据库表
fn create_tables(conn: &Connection) -> Result<(), String> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS clipboard (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            content TEXT NOT NULL,
            html_content TEXT,
            content_type TEXT NOT NULL DEFAULT 'text',
            image_id TEXT,
            item_order INTEGER NOT NULL DEFAULT 0,
            is_pinned INTEGER NOT NULL DEFAULT 0,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        )",
        [],
    ).map_err(|e| format!("创建剪贴板表失败: {}", e))?;

    let pinned_exists = has_column(conn, "clipboard", "is_pinned")?;
    
    if !pinned_exists {
        conn.execute(
            "ALTER TABLE clipboard ADD COLUMN is_pinned INTEGER NOT NULL DEFAULT 0",
            [],
        ).map_err(|e| format!("添加置顶字段失败: {}", e))?;
    }

    let paste_count_exists = has_column(conn, "clipboard", "paste_count")?;
    
    if !paste_count_exists {
        conn.execute(
            "ALTER TABLE clipboard ADD COLUMN paste_count INTEGER NOT NULL DEFAULT 0",
            [],
        ).map_err(|e| format!("添加粘贴次数字段失败: {}", e))?;
    }
    
    migrate_clipboard_order(conn)?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS favorites (
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
        )",
        [],
    ).map_err(|e| format!("创建收藏表失败: {}", e))?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS groups (
            name TEXT PRIMARY KEY,
            icon TEXT NOT NULL DEFAULT 'ti ti-folder',
            color TEXT NOT NULL DEFAULT '#dc2626',
            order_index INTEGER NOT NULL DEFAULT 0,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        )",
        [],
    ).map_err(|e| format!("创建分组表失败: {}", e))?;

    let color_exists = has_column(conn, "groups", "color")?;
    
    if !color_exists {
        conn.execute(
            "ALTER TABLE groups ADD COLUMN color TEXT NOT NULL DEFAULT '#dc2626'",
            [],
        ).map_err(|e| format!("添加颜色字段失败: {}", e))?;
    }

    let fav_paste_count_exists = has_column(conn, "favorites", "paste_count")?;
    
    if !fav_paste_count_exists {
        conn.execute(
            "ALTER TABLE favorites ADD COLUMN paste_count INTEGER NOT NULL DEFAULT 0",
            [],
        ).map_err(|e| format!("添加收藏粘贴次数字段失败: {}", e))?;
    }

    let source_app_exists = has_column(conn, "clipboard", "source_app")?;
    
    if !source_app_exists {
        conn.execute("ALTER TABLE clipboard ADD COLUMN source_app TEXT", [])
            .map_err(|e| format!("添加来源应用字段失败: {}", e))?;
    }

    let source_icon_hash_exists = has_column(conn, "clipboard", "source_icon_hash")?;
    
    if !source_icon_hash_exists {
        conn.execute("ALTER TABLE clipboard ADD COLUMN source_icon_hash TEXT", [])
            .map_err(|e| format!("添加来源图标哈希字段失败: {}", e))?;
    }

    let clip_char_count_exists = has_column(conn, "clipboard", "char_count")?;
    
    if !clip_char_count_exists {
        conn.execute("ALTER TABLE clipboard ADD COLUMN char_count INTEGER", [])
            .map_err(|e| format!("添加剪贴板字符数量字段失败: {}", e))?;
    }

    let fav_char_count_exists = has_column(conn, "favorites", "char_count")?;
    
    if !fav_char_count_exists {
        conn.execute("ALTER TABLE favorites ADD COLUMN char_count INTEGER", [])
            .map_err(|e| format!("添加收藏字符数量字段失败: {}", e))?;
    }

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_clipboard_order ON clipboard(is_pinned DESC, item_order DESC, updated_at DESC)",
        [],
    ).map_err(|e| format!("创建剪贴板排序索引失败: {}", e))?;
    
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_clipboard_content_type ON clipboard(content_type)",
        [],
    ).map_err(|e| format!("创建内容类型索引失败: {}", e))?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_favorites_group ON favorites(group_name, item_order)",
        [],
    ).map_err(|e| format!("创建收藏索引失败: {}", e))?;
    migrate_favorites_auto_titles(conn)?;

    Ok(())
}

fn list_table_columns(conn: &Connection, table: &str) -> Result<Vec<String>, String> {
    let pragma = format!("PRAGMA table_info({})", table);
    let mut stmt = conn.prepare(&pragma)
        .map_err(|e| format!("读取 {} 表结构失败: {}", table, e))?;
    let columns = stmt.query_map([], |row| row.get::<_, String>(1))
        .map_err(|e| format!("查询 {} 表字段失败: {}", table, e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("解析 {} 表字段失败: {}", table, e))?;
    Ok(columns)
}

fn has_column(conn: &Connection, table: &str, column: &str) -> Result<bool, String> {
    Ok(list_table_columns(conn, table)?.iter().any(|name| name == column))
}

// 迁移 item_order（ASC → DESC）
pub fn migrate_clipboard_order(conn: &Connection) -> Result<(), String> {
    let need_migrate: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM clipboard WHERE item_order < 0) 
         OR (SELECT MAX(item_order) FROM clipboard) < (SELECT COUNT(*) FROM clipboard)",
        [], |row| row.get(0)
    ).map_err(|e| format!("检查剪贴板排序迁移状态失败: {}", e))?;
    
    if need_migrate {
        let mut stmt = conn.prepare(
            "SELECT id FROM clipboard ORDER BY is_pinned DESC, item_order ASC, updated_at DESC"
        ).map_err(|e| format!("读取剪贴板排序迁移数据失败: {}", e))?;
        let ids: Vec<i64> = stmt.query_map([], |row| row.get(0))
            .map_err(|e| format!("查询剪贴板排序迁移数据失败: {}", e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("解析剪贴板排序迁移数据失败: {}", e))?;
        let count = ids.len() as i64;
        for (i, id) in ids.iter().enumerate() {
            conn.execute("UPDATE clipboard SET item_order = ? WHERE id = ?",
                rusqlite::params![count - i as i64, id])
                .map_err(|e| format!("更新剪贴板排序失败: {}", e))?;
        }
    }
    
    // 收藏迁移：按分组独立处理
    let mut groups_stmt = conn.prepare("SELECT DISTINCT group_name FROM favorites")
        .map_err(|e| format!("读取收藏分组失败: {}", e))?;
    let groups: Vec<String> = groups_stmt.query_map([], |r| r.get::<_, String>(0))
        .map_err(|e| format!("查询收藏分组失败: {}", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("解析收藏分组失败: {}", e))?;

    for group in groups {
        let need: bool = conn.query_row(
            "SELECT (SELECT MAX(item_order) FROM favorites WHERE group_name = ?1) 
                  < (SELECT COUNT(*) FROM favorites WHERE group_name = ?1)",
            [&group], |row| row.get(0)
        ).map_err(|e| format!("检查收藏分组排序迁移状态失败: {}", e))?;
        
        if need {
            let mut stmt = conn.prepare(
                "SELECT id FROM favorites WHERE group_name = ? ORDER BY item_order ASC, updated_at DESC"
            ).map_err(|e| format!("读取收藏排序迁移数据失败: {}", e))?;
            let ids: Vec<String> = stmt.query_map([&group], |row| row.get(0))
                .map_err(|e| format!("查询收藏排序迁移数据失败: {}", e))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| format!("解析收藏排序迁移数据失败: {}", e))?;
            let count = ids.len() as i64;
            for (i, id) in ids.iter().enumerate() {
                conn.execute("UPDATE favorites SET item_order = ? WHERE id = ?",
                    rusqlite::params![count - i as i64, id])
                    .map_err(|e| format!("更新收藏排序失败: {}", e))?;
            }
        }
    }

    Ok(())
}

// 获取数据库连接
pub fn with_connection<F, R>(f: F) -> Result<R, String>
where
    F: FnOnce(&Connection) -> Result<R, rusqlite::Error>,
{
    let conn_guard = DB_CONNECTION.lock();
    let conn = conn_guard.as_ref()
        .ok_or("数据库未初始化")?;
    f(conn).map_err(|e| format!("数据库操作失败: {}", e))
}


// 清理文件和图片类型收藏项的自动生成标题
fn migrate_favorites_auto_titles(conn: &Connection) -> Result<(), String> {
    let mut stmt = conn.prepare(
        "SELECT id, title, content FROM favorites WHERE content_type LIKE '%file%' OR content_type LIKE '%image%'"
    ).map_err(|e| format!("读取收藏自动标题迁移数据失败: {}", e))?;
    let items: Vec<(String, String, String)> = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    }).map_err(|e| format!("查询收藏自动标题迁移数据失败: {}", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("解析收藏自动标题迁移数据失败: {}", e))?;
    
    for (id, title, content) in items {
        let content_chars: Vec<char> = content.chars().collect();
        let expected_title = if content_chars.len() > 50 {
            format!("{}...", content_chars[..50].iter().collect::<String>())
        } else {
            content_chars.iter().collect::<String>()
        };
        
        if title == expected_title {
            conn.execute("UPDATE favorites SET title = '' WHERE id = ?", [&id])
                .map_err(|e| format!("清理收藏自动标题失败: {}", e))?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_table_columns_returns_error_for_invalid_table_identifier() {
        let conn = Connection::open_in_memory().expect("open in-memory db failed");
        let result = list_table_columns(&conn, "clipboard)");
        assert!(result.is_err());
    }

    #[test]
    fn migrate_clipboard_order_returns_error_when_favorites_schema_is_invalid() {
        let conn = Connection::open_in_memory().expect("open in-memory db failed");
        conn.execute_batch(
            "CREATE TABLE clipboard (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                item_order INTEGER NOT NULL DEFAULT 0,
                is_pinned INTEGER NOT NULL DEFAULT 0,
                updated_at INTEGER NOT NULL
            );
            CREATE TABLE favorites (
                id TEXT PRIMARY KEY,
                item_order INTEGER NOT NULL DEFAULT 0,
                updated_at INTEGER NOT NULL
            );"
        ).expect("create malformed schema failed");

        let result = migrate_clipboard_order(&conn);
        assert!(result.is_err());
        let message = result.err().unwrap_or_default();
        assert!(message.contains("收藏分组") || message.contains("排序迁移"), "unexpected error: {}", message);
    }
}
