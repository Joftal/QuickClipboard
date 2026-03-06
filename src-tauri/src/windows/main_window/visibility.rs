use super::state::{set_window_state, WindowState};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Manager, WebviewWindow};

static VISIBILITY_REQUEST_ID: AtomicU64 = AtomicU64::new(0);

fn next_visibility_request_id() -> u64 {
    VISIBILITY_REQUEST_ID.fetch_add(1, Ordering::SeqCst) + 1
}

fn is_visibility_request_current(request_id: u64) -> bool {
    VISIBILITY_REQUEST_ID.load(Ordering::SeqCst) == request_id
}

fn schedule_restore_always_on_top(window: &WebviewWindow, request_id: u64) {
    let app = window.app_handle().clone();
    let label = window.label().to_string();

    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(10)).await;

        if !is_visibility_request_current(request_id) {
            return;
        }

        if let Some(window) = app.get_webview_window(&label) {
            let _ = window.set_always_on_top(true);
        }
    });
}

fn finalize_hide_normal_window(window: WebviewWindow, request_id: u64) {
    if !is_visibility_request_current(request_id) {
        return;
    }

    if let Some(position) = window.outer_position().ok().filter(|_| crate::get_settings().window_position_mode == "remember") {
        let mut settings = crate::get_settings();
        settings.saved_window_position = Some((position.x, position.y));

        if settings.remember_window_size {
            if let Ok(size) = window.outer_size() {
                settings.saved_window_size = Some((size.width, size.height));
            }
        }

        let _ = crate::services::update_settings(settings);
    }

    if !super::state::is_pinned() {
        let _ = window.set_always_on_top(false);
    }

    let _ = window.hide();
    set_window_state(WindowState::Hidden);

    crate::input_monitor::disable_mouse_monitoring();
    crate::input_monitor::disable_navigation_keys();
}

fn schedule_finalize_hide(window: &WebviewWindow, delay_ms: u64, request_id: u64) {
    let app = window.app_handle().clone();
    let label = window.label().to_string();

    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(delay_ms)).await;

        if !is_visibility_request_current(request_id) {
            return;
        }

        if let Some(window) = app.get_webview_window(&label) {
            finalize_hide_normal_window(window, request_id);
        }
    });
}

// 显示主窗口
pub fn show_main_window(window: &WebviewWindow) {
    let request_id = next_visibility_request_id();

    if crate::services::system::is_front_app_globally_disabled_from_settings() {
        return;
    }

    let state = super::state::get_window_state();

    if state.is_snapped && state.is_hidden {
        let _ = super::show_snapped_window(window);
        return;
    }

    if state.is_snapped && !state.is_hidden {
        let _ = super::restore_from_snap(window);
    }

    show_normal_window(window);
    let _ = window.set_always_on_top(false);
    schedule_restore_always_on_top(window, request_id);
}

// 隐藏主窗口
pub fn hide_main_window(window: &WebviewWindow) {
    if crate::is_context_menu_visible() {
        return;
    }

    let state = super::state::get_window_state();

    if state.is_snapped {
        if !state.is_hidden {
            let _ = super::hide_snapped_window(window);
        }
        return;
    }

    hide_normal_window(window);
}

pub fn toggle_main_window_visibility(app: &AppHandle) {
    if crate::services::low_memory::is_low_memory_mode() {
        if let Err(e) = crate::services::low_memory::exit_low_memory_mode(app) {
            eprintln!("退出低占用模式失败: {}", e);
            return;
        }
        if let Some(window) = super::get_main_window(app) {
            show_main_window(&window);
        }
        return;
    }

    if let Some(window) = super::get_main_window(app) {
        if crate::services::system::is_front_app_globally_disabled_from_settings() {
            return;
        }

        let state = super::state::get_window_state();

        let should_show =
            state.is_snapped && state.is_hidden || state.state != WindowState::Visible;

        if should_show {
            show_main_window(&window);
        } else {
            hide_main_window(&window);
        }
    }
}

fn show_normal_window(window: &WebviewWindow) {
    let state = super::state::get_window_state();
    let was_visible = state.state == WindowState::Visible;

    // 根据配置定位窗口
    let settings = crate::get_settings();
    match settings.window_position_mode.as_str() {
        "remember" => {
            if let Some((x, y)) = settings.saved_window_position {
                let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
            } else {
                let _ = crate::utils::positioning::position_at_cursor(window);
            }
        }
        "center" => {
            let _ = crate::utils::positioning::center_window(window);
        }
        _ => {
            let _ = crate::utils::positioning::position_at_cursor(window);
        }
    }

    // 恢复窗口大小
    if settings.remember_window_size {
        if let Some((w, h)) = settings.saved_window_size {
            let _ = window.set_size(tauri::PhysicalSize::new(w, h));
        }
    }

    let _ = window.show();

    if !was_visible {
        use tauri::Emitter;
        let _ = window.emit("window-show-animation", ());
    }

    set_window_state(WindowState::Visible);

    crate::input_monitor::enable_mouse_monitoring();
    crate::input_monitor::enable_navigation_keys();
}

fn hide_normal_window(window: &WebviewWindow) {
    use tauri::Emitter;

    let request_id = next_visibility_request_id();

    let _ = crate::windows::pin_image_window::close_image_preview(window.app_handle().clone());
    
    let _ = window.emit("window-hide-animation", ());

    set_window_state(WindowState::Hidden);
    crate::input_monitor::disable_mouse_monitoring();
    crate::input_monitor::disable_navigation_keys();

    let settings = crate::get_settings();
    if settings.clipboard_animation_enabled {
        schedule_finalize_hide(window, 200, request_id);
        return;
    }

    finalize_hide_normal_window(window.clone(), request_id);
}
