// 持久化存储服务
// 封装 tauri-plugin-store，供 Rust 代码使用

use parking_lot::Mutex;
use std::path::PathBuf;
use once_cell::sync::Lazy;
use tauri::{AppHandle, Manager};
use tauri_plugin_store::StoreExt;

// 默认存储文件名
#[allow(dead_code)]
const DEFAULT_STORE_FILE: &str = "app-store.json";

// 全局 AppHandle 引用
static APP_HANDLE: Lazy<Mutex<Option<AppHandle>>> = Lazy::new(|| Mutex::new(None));

fn get_app_handle() -> Option<AppHandle> {
    APP_HANDLE.lock().clone()
}

fn require_app_handle() -> Result<AppHandle, String> {
    get_app_handle().ok_or("AppHandle 未初始化".to_string())
}

// 初始化存储服务
pub fn init(app: &AppHandle) {
    *APP_HANDLE.lock() = Some(app.clone());
}

// 获取存储路径
#[allow(dead_code)]
fn get_store_path(app: &AppHandle) -> PathBuf {
    app.path().app_data_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(DEFAULT_STORE_FILE)
}

// 获取值
#[allow(dead_code)]
pub fn get<T: serde::de::DeserializeOwned>(key: &str) -> Option<T> {
    let app = get_app_handle()?;
    
    let store_path = get_store_path(&app);
    let store = app.store(store_path).ok()?;
    
    store.get(key).and_then(|v| serde_json::from_value(v).ok())
}

// 设置值
#[allow(dead_code)]
pub fn set<T: serde::Serialize>(key: &str, value: &T) -> Result<(), String> {
    let app = require_app_handle()?;
    
    let store_path = get_store_path(&app);
    let store = app.store(store_path).map_err(|e| e.to_string())?;
    
    let json_value = serde_json::to_value(value).map_err(|e| e.to_string())?;
    store.set(key, json_value);
    store.save().map_err(|e| e.to_string())?;
    
    Ok(())
}

// 删除值
#[allow(dead_code)]
pub fn delete(key: &str) -> Result<(), String> {
    let app = require_app_handle()?;
    
    let store_path = get_store_path(&app);
    let store = app.store(store_path).map_err(|e| e.to_string())?;
    
    store.delete(key);
    store.save().map_err(|e| e.to_string())?;
    
    Ok(())
}

// 检查键是否存在
#[allow(dead_code)]
pub fn has(key: &str) -> bool {
    let Some(app) = get_app_handle() else { return false };
    
    let store_path = get_store_path(&app);
    let Ok(store) = app.store(store_path) else { return false };
    
    store.has(key)
}

// 获取所有键
#[allow(dead_code)]
pub fn keys() -> Vec<String> {
    let Some(app) = get_app_handle() else { return vec![] };
    
    let store_path = get_store_path(&app);
    let Ok(store) = app.store(store_path) else { return vec![] };
    
    store.keys().into_iter().map(|s| s.to_string()).collect()
}
