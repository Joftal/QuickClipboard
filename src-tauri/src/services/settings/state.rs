use super::{AppSettings, storage::SettingsStorage};
use once_cell::sync::Lazy;
use parking_lot::RwLock;

static SETTINGS: Lazy<RwLock<AppSettings>> = Lazy::new(|| {
    RwLock::new(SettingsStorage::load().unwrap_or_default())
});

pub fn get_settings() -> AppSettings {
    SETTINGS.read().clone()
}

pub fn replace_settings(settings: AppSettings) {
    *SETTINGS.write() = settings;
}

pub fn update_settings(settings: AppSettings) -> Result<(), String> {
    let mut current_settings = SETTINGS.write();
    SettingsStorage::save(&settings)?;
    *current_settings = settings;
    Ok(())
}

pub fn update_with<F>(updater: F) -> Result<(), String>
where
    F: FnOnce(&mut AppSettings),
{
    let mut current_settings = SETTINGS.write();
    let mut next_settings = current_settings.clone();
    updater(&mut next_settings);
    SettingsStorage::save(&next_settings)?;
    *current_settings = next_settings;
    Ok(())
}

pub fn get_data_directory() -> Result<std::path::PathBuf, String> {
    let settings = SETTINGS.read();
    SettingsStorage::get_data_directory(&settings)
}
