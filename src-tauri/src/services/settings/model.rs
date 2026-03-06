use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AppSettings {
    // 基础设置
    pub auto_start: bool,
    pub run_as_admin: bool,
    pub start_hidden: bool,
    #[serde(alias = "history_limit")]
    pub history_limit: u64,
    pub language: String,
    pub theme: String,
    pub dark_theme_style: String,
    pub opacity: f64,
    pub toggle_shortcut: String,
    pub number_shortcuts: bool,
    pub number_shortcuts_modifier: String,
    pub clipboard_monitor: bool,
    pub ignore_duplicates: bool,
    pub save_images: bool,
    pub image_preview: bool,
    pub text_preview: bool,

    // 图片显示限制
    pub image_max_size_mb: u32,
    pub image_max_width: u32,
    pub image_max_height: u32,

    // 预览窗口设置
    pub quickpaste_enabled: bool,
    pub quickpaste_shortcut: String,
    pub quickpaste_paste_on_modifier_release: bool,
    pub quickpaste_window_width: u32,
    pub quickpaste_window_height: u32,

    // 鼠标设置
    pub mouse_middle_button_enabled: bool,
    pub mouse_middle_button_modifier: String,
    pub mouse_middle_button_trigger: String,
    pub mouse_middle_button_long_press_ms: u32,

    // 动画设置
    pub clipboard_animation_enabled: bool,
    pub ui_animation_enabled: bool,

    // 显示行为
    pub auto_scroll_to_top_on_show: bool,
    pub auto_clear_search: bool,

    // 应用过滤设置
    pub app_filter_enabled: bool,
    pub app_filter_mode: String,
    pub app_filter_list: Vec<String>,
    pub app_filter_effect: String,

    // 窗口设置
    pub window_position_mode: String,
    pub remember_window_size: bool,
    pub saved_window_position: Option<(i32, i32)>,
    pub saved_window_size: Option<(u32, u32)>,

    // 贴边隐藏设置
    pub edge_hide_enabled: bool,
    pub edge_snap_position: Option<(i32, i32)>,
    pub edge_hide_offset: i32,

    // 窗口行为设置
    pub auto_focus_search: bool,

    // 标题栏设置
    pub title_bar_position: String,

    // 格式设置
    pub paste_with_format: bool,
    pub paste_shortcut_mode: String,
    
    pub paste_to_top: bool,
    pub show_badges: bool,
    pub show_source_icon: bool,

    // 快捷键设置
    pub hotkeys_enabled: bool,
    pub navigate_up_shortcut: String,
    pub navigate_down_shortcut: String,
    pub tab_left_shortcut: String,
    pub tab_right_shortcut: String,
    pub focus_search_shortcut: String,
    pub hide_window_shortcut: String,
    pub execute_item_shortcut: String,
    pub previous_group_shortcut: String,
    pub next_group_shortcut: String,
    pub toggle_pin_shortcut: String,
    pub toggle_clipboard_monitor_shortcut: String,
    pub toggle_paste_with_format_shortcut: String,
    pub paste_plain_text_shortcut: String,

    // 数据存储设置
    #[serde(alias = "custom_storage_path")]
    pub custom_storage_path: Option<String>,
    #[serde(alias = "use_custom_storage")]
    pub use_custom_storage: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            auto_start: false,
            run_as_admin: false,
            start_hidden: true,
            history_limit: 100,
            language: "zh-CN".to_string(),
            theme: "light".to_string(),
            dark_theme_style: "classic".to_string(),
            opacity: 0.9,
            toggle_shortcut: "Shift+Space".to_string(),
            number_shortcuts: true,
            number_shortcuts_modifier: "Ctrl".to_string(),
            clipboard_monitor: true,
            ignore_duplicates: true,
            save_images: true,
            image_preview: false,
            text_preview: false,

            image_max_size_mb: 15,
            image_max_width: 4096,
            image_max_height: 4096,

            quickpaste_enabled: true,
            quickpaste_shortcut: "Ctrl+`".to_string(),
            quickpaste_paste_on_modifier_release: false,
            quickpaste_window_width: 300,
            quickpaste_window_height: 400,

            mouse_middle_button_enabled: false,
            mouse_middle_button_modifier: "None".to_string(),
            mouse_middle_button_trigger: "short_press".to_string(),
            mouse_middle_button_long_press_ms: 300,

            clipboard_animation_enabled: true,
            ui_animation_enabled: true,

            auto_scroll_to_top_on_show: false,
            auto_clear_search: false,

            app_filter_enabled: false,
            app_filter_mode: "blacklist".to_string(),
            app_filter_list: vec![],
            app_filter_effect: "clipboard_only".to_string(),

            window_position_mode: "smart".to_string(),
            remember_window_size: false,
            saved_window_position: None,
            saved_window_size: None,

            edge_hide_enabled: true,
            edge_snap_position: None,
            edge_hide_offset: 3,

            auto_focus_search: false,

            title_bar_position: "top".to_string(),

            paste_with_format: true,
            paste_shortcut_mode: "ctrl_v".to_string(),
            paste_to_top: false,
            show_badges: true,
            show_source_icon: true,

            hotkeys_enabled: true,
            navigate_up_shortcut: "ArrowUp".to_string(),
            navigate_down_shortcut: "ArrowDown".to_string(),
            tab_left_shortcut: "ArrowLeft".to_string(),
            tab_right_shortcut: "ArrowRight".to_string(),
            focus_search_shortcut: "Tab".to_string(),
            hide_window_shortcut: "Escape".to_string(),
            execute_item_shortcut: "Ctrl+Enter".to_string(),
            previous_group_shortcut: "Ctrl+ArrowUp".to_string(),
            next_group_shortcut: "Ctrl+ArrowDown".to_string(),
            toggle_pin_shortcut: "Ctrl+P".to_string(),
            toggle_clipboard_monitor_shortcut: "Ctrl+Shift+Z".to_string(),
            toggle_paste_with_format_shortcut: "Ctrl+Shift+X".to_string(),
            paste_plain_text_shortcut: String::new(),

            custom_storage_path: None,
            use_custom_storage: false,
        }
    }
}

