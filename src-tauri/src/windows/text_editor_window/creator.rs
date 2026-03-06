use tauri::AppHandle;
use std::time::{SystemTime, UNIX_EPOCH};

fn encode_query_value(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());

    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            _ => encoded.push_str(&format!("%{:02X}", byte)),
        }
    }

    encoded
}

fn append_query_param(url: &mut String, key: &str, value: &str) {
    let separator = if url.contains('?') { '&' } else { '?' };
    url.push(separator);
    url.push_str(key);
    url.push('=');
    url.push_str(&encode_query_value(value));
}

pub fn create_text_editor_window(
    app: &AppHandle,
    item_id: &str,
    item_type: &str,
    item_index: Option<i32>,
    group_name: Option<String>,
) -> Result<String, String> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    let window_label = format!("text-editor-{}-{}", item_type, timestamp);
    
    let mut url = "windows/textEditor/index.html".to_string();
    append_query_param(&mut url, "id", item_id);
    append_query_param(&mut url, "type", item_type);

    if let Some(index) = item_index {
        append_query_param(&mut url, "index", &index.to_string());
    }

    if let Some(group) = group_name {
        append_query_param(&mut url, "group", &group);
    }
    
    let _editor_window = tauri::WebviewWindowBuilder::new(
        app,
        &window_label,
        tauri::WebviewUrl::App(url.into()),
    )
    .title("文本编辑器 - 快速剪贴板")
    .inner_size(900.0, 700.0)
    .min_inner_size(600.0, 400.0)
    .center()
    .resizable(true)
    .maximizable(true)
    .decorations(false)
    .transparent(false)
    .skip_taskbar(false)
    .visible(true)
    .focused(true)
    .build()
    .map_err(|e| format!("创建文本编辑器窗口失败: {}", e))?;

    Ok(window_label)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_query_value_handles_reserved_and_unicode_characters() {
        assert_eq!(
            encode_query_value("默认 分组&A?#测试"),
            "%E9%BB%98%E8%AE%A4%20%E5%88%86%E7%BB%84%26A%3F%23%E6%B5%8B%E8%AF%95"
        );
    }

    #[test]
    fn append_query_param_uses_correct_separator() {
        let mut url = "windows/textEditor/index.html".to_string();
        append_query_param(&mut url, "id", "1");
        append_query_param(&mut url, "group", "A&B");

        assert_eq!(url, "windows/textEditor/index.html?id=1&group=A%26B");
    }
}

