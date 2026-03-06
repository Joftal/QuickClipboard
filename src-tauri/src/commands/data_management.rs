use serde::Deserialize;

#[derive(Deserialize)]
pub struct ChangePathPayload {
    #[serde(alias = "new_path", alias = "newPath")]
    new_path: String,
    #[serde(default = "default_source_only")]
    mode: String,
}

#[derive(Deserialize)]
pub struct ResetPathPayload {
    #[serde(default = "default_source_only")]
    mode: String,
}

#[derive(Deserialize)]
pub struct CheckTargetPayload {
    #[serde(alias = "target_path", alias = "targetPath")]
    target_path: String,
}

fn default_source_only() -> String {
    "source_only".to_string()
}

#[tauri::command]
pub fn dm_get_current_storage_path() -> Result<String, String> {
    let path = crate::services::data_management::get_current_storage_dir()?;
    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
pub fn dm_get_default_storage_path() -> Result<String, String> {
    let path = crate::services::data_management::get_default_data_dir()?;
    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
pub fn dm_check_target_has_data(payload: CheckTargetPayload) -> Result<crate::services::data_management::TargetDataInfo, String> {
    let path = std::path::PathBuf::from(payload.target_path);
    crate::services::data_management::check_target_has_data(&path)
}

#[tauri::command]
pub fn dm_change_storage_path(_app: tauri::AppHandle, payload: ChangePathPayload) -> Result<String, String> {
    let path = std::path::PathBuf::from(payload.new_path);
    let new_dir = crate::services::data_management::change_storage_dir(path, &payload.mode)?;

    Ok(new_dir.to_string_lossy().to_string())
}

#[tauri::command]
pub fn dm_reset_storage_path_to_default(_app: tauri::AppHandle, payload: ResetPathPayload) -> Result<String, String> {
    let dir = crate::services::data_management::reset_storage_dir_to_default(&payload.mode)?;

    Ok(dir.to_string_lossy().to_string())
}

#[tauri::command]
pub fn dm_reset_all_data(_app: tauri::AppHandle) -> Result<String, String> {
    let path = crate::services::data_management::reset_all_data()?;
    Ok(path)
}
