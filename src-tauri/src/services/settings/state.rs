use super::{AppSettings, storage::SettingsStorage};
use once_cell::sync::Lazy;
use parking_lot::RwLock;

fn resolve_initial_settings(load_result: Result<AppSettings, String>) -> (AppSettings, Option<String>) {
    match load_result {
        Ok(settings) => (settings, None),
        Err(error) => {
            eprintln!("设置加载失败，将阻止依赖配置的关键路径继续运行: {}", error);
            (AppSettings::default(), Some(error))
        }
    }
}

fn ensure_settings_loaded(load_error: &Option<String>) -> Result<(), String> {
    if let Some(error) = load_error {
        return Err(format!("设置加载失败，请修复 settings.json 后重试: {}", error));
    }

    Ok(())
}

static INITIAL_SETTINGS_STATE: Lazy<(AppSettings, Option<String>)> = Lazy::new(|| {
    resolve_initial_settings(SettingsStorage::load())
});

static SETTINGS: Lazy<RwLock<AppSettings>> = Lazy::new(|| {
    RwLock::new(INITIAL_SETTINGS_STATE.0.clone())
});

static SETTINGS_LOAD_ERROR: Lazy<RwLock<Option<String>>> = Lazy::new(|| {
    RwLock::new(INITIAL_SETTINGS_STATE.1.clone())
});

pub fn get_settings() -> AppSettings {
    SETTINGS.read().clone()
}

pub fn replace_settings(settings: AppSettings) {
    *SETTINGS.write() = settings;
    *SETTINGS_LOAD_ERROR.write() = None;
}

pub fn update_settings(settings: AppSettings) -> Result<(), String> {
    let mut current_settings = SETTINGS.write();
    SettingsStorage::save(&settings)?;
    *current_settings = settings;
    *SETTINGS_LOAD_ERROR.write() = None;
    Ok(())
}

pub fn update_with<F>(mutator: F) -> Result<(), String>
where
    F: FnOnce(&mut AppSettings),
{
    let mut current_settings = SETTINGS.write();
    let mut next_settings = current_settings.clone();
    mutator(&mut next_settings);
    SettingsStorage::save(&next_settings)?;
    *current_settings = next_settings;
    *SETTINGS_LOAD_ERROR.write() = None;
    Ok(())
}

pub fn get_data_directory() -> Result<std::path::PathBuf, String> {
    ensure_settings_loaded(&SETTINGS_LOAD_ERROR.read())?;
    let settings = SETTINGS.read();
    SettingsStorage::get_data_directory(&settings)
}

#[cfg(test)]
mod tests {
    use super::{ensure_settings_loaded, resolve_initial_settings};
    use crate::services::AppSettings;

    #[test]
    fn resolve_initial_settings_keeps_loaded_settings() {
        let expected = AppSettings::default();
        let (settings, load_error) = resolve_initial_settings(Ok(expected.clone()));

        assert_eq!(settings.language, expected.language);
        assert!(load_error.is_none());
    }

    #[test]
    fn resolve_initial_settings_preserves_error_instead_of_silent_success() {
        let (settings, load_error) = resolve_initial_settings(Err("bad json".to_string()));
        let defaults = AppSettings::default();

        assert_eq!(settings.language, defaults.language);
        assert_eq!(settings.use_custom_storage, defaults.use_custom_storage);
        assert_eq!(settings.custom_storage_path, defaults.custom_storage_path);
        assert_eq!(load_error.as_deref(), Some("bad json"));
    }

    #[test]
    fn ensure_settings_loaded_returns_explicit_error() {
        let result = ensure_settings_loaded(&Some("bad json".to_string()));

        assert!(result.is_err());
        let message = result.err().unwrap_or_default();
        assert!(message.contains("设置加载失败"));
        assert!(message.contains("bad json"));
    }
}
