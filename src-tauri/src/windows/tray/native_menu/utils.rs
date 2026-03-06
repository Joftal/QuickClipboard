// 工具函数

use super::state::MAX_LABEL_LENGTH;

// 计算字符显示宽度
fn char_width(c: char) -> usize {
    if c.is_ascii() { 1 } else { 2 }
}

// 计算字符串显示宽度
fn str_display_width(s: &str) -> usize {
    s.chars().map(char_width).sum()
}

// 规范化文本
fn normalize_text(text: &str) -> String {
    let mut result = String::new();
    let mut last_was_space = false;
    
    for c in text.chars() {
        if c.is_whitespace() {
            if !last_was_space {
                result.push(' ');
                last_was_space = true;
            }
        } else {
            result.push(c);
            last_was_space = false;
        }
    }
    
    result.trim().to_string()
}

// 截断内容到指定宽度
fn truncate_content(text: &str, max_width: usize) -> String {
    let text = normalize_text(text);
    
    if str_display_width(&text) <= max_width {
        return text;
    }
    
    let target_width = max_width.saturating_sub(3);
    let mut result = String::new();
    let mut current_width = 0;
    
    for c in text.chars() {
        let w = char_width(c);
        if current_width + w > target_width {
            break;
        }
        result.push(c);
        current_width += w;
    }
    
    format!("{}...", result)
}

fn parse_files_content(content: &str) -> Option<Vec<String>> {
    if !content.starts_with("files:") {
        return None;
    }
    
    let json_str = &content[6..];
    let parsed: serde_json::Value = serde_json::from_str(json_str).ok()?;
    let files = parsed.get("files")?.as_array()?;
    
    let names: Vec<String> = files
        .iter()
        .filter_map(|f| f.get("name").and_then(|n| n.as_str()).map(|s| s.to_string()))
        .collect();
    
    if names.is_empty() { None } else { Some(names) }
}

// 格式化剪贴板项标签
pub fn format_item_label(item: &crate::services::database::ClipboardItem) -> String {
    let pin_mark = if item.is_pinned { "📌 " } else { "    " };

    let (content, type_label) = match item.content_type.as_str() {
        "text" => (
            truncate_content(&item.content, MAX_LABEL_LENGTH - 14),
            "[文本]"
        ),
        "link" => (
            truncate_content(&item.content, MAX_LABEL_LENGTH - 14),
            "[链接]"
        ),
        "rich_text" => (
            truncate_content(&item.content, MAX_LABEL_LENGTH - 16),
            "[富文本]"
        ),
        "image" => {
            let content = if let Some(names) = parse_files_content(&item.content) {
                if names.len() == 1 {
                    truncate_content(&names[0], MAX_LABEL_LENGTH - 14)
                } else {
                    format!("{} 等{}张", truncate_content(&names[0], MAX_LABEL_LENGTH - 20), names.len())
                }
            } else {
                "图片".to_string()
            };
            (content, "[图片]")
        },
        "file" => {
            let content = if let Some(names) = parse_files_content(&item.content) {
                if names.len() == 1 {
                    truncate_content(&names[0], MAX_LABEL_LENGTH - 14)
                } else {
                    format!("{} 等{}个", truncate_content(&names[0], MAX_LABEL_LENGTH - 20), names.len())
                }
            } else {
                let filename = item.content
                    .split(['/', '\\'])
                    .next_back()
                    .unwrap_or("文件");
                truncate_content(filename, MAX_LABEL_LENGTH - 14)
            };
            (content, "[文件]")
        },
        _ => (
            truncate_content(&item.content, MAX_LABEL_LENGTH - 14),
            "[其他]"
        ),
    };
    
    format!("{}{} {}", pin_mark, content, type_label)
}
