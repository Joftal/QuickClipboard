use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use tauri::{Manager, WebviewWindow};

static MAIN_WINDOW: Mutex<Option<WebviewWindow>> = Mutex::new(None);
static MONITORING_ACTIVE: AtomicBool = AtomicBool::new(false);
static MONITOR_RUNTIME: Mutex<Option<EdgeMonitorRuntime>> = Mutex::new(None);

struct EdgeMonitorRuntime {
    stop_tx: Sender<()>,
    handle: JoinHandle<()>,
}

pub fn init_edge_monitor(window: WebviewWindow) {
    *MAIN_WINDOW.lock() = Some(window);
}

pub fn start_edge_monitoring() {
    let mut runtime_guard = MONITOR_RUNTIME.lock();
    if MONITORING_ACTIVE.load(Ordering::Relaxed) && runtime_guard.is_some() {
        return;
    }

    if let Some(runtime) = runtime_guard.take() {
        drop(runtime_guard);
        stop_runtime(runtime);
        runtime_guard = MONITOR_RUNTIME.lock();
    }

    MONITORING_ACTIVE.store(true, Ordering::Relaxed);

    let (stop_tx, stop_rx) = mpsc::channel();
    let handle = thread::spawn(move || monitor_loop(stop_rx));
    *runtime_guard = Some(EdgeMonitorRuntime { stop_tx, handle });
}

pub fn stop_edge_monitoring() {
    MONITORING_ACTIVE.store(false, Ordering::Relaxed);

    if let Some(runtime) = MONITOR_RUNTIME.lock().take() {
        stop_runtime(runtime);
    }
}

fn stop_runtime(runtime: EdgeMonitorRuntime) {
    let _ = runtime.stop_tx.send(());
    if let Err(error) = runtime.handle.join() {
        eprintln!("贴边监控线程退出失败: {:?}", error);
    }
}

fn monitor_loop(stop_rx: Receiver<()>) {
    if sleep_or_stopped(&stop_rx, Duration::from_millis(200)) {
        MONITORING_ACTIVE.store(false, Ordering::Relaxed);
        return;
    }

    let mut last_near_state = false;
    let mut last_hidden_state = false;

    loop {
        let window = match MAIN_WINDOW.lock().clone() {
            Some(window) => window,
            None => {
                if sleep_or_stopped(&stop_rx, Duration::from_millis(100)) {
                    break;
                }
                continue;
            }
        };

        let state = crate::get_window_state();

        if !state.is_snapped || state.is_dragging {
            if sleep_or_stopped(&stop_rx, Duration::from_millis(100)) {
                break;
            }
            continue;
        }

        if last_hidden_state != state.is_hidden {
            last_hidden_state = state.is_hidden;
            if let Ok(is_near) = check_mouse_near_edge(&window, &state) {
                last_near_state = is_near;
            }
            if sleep_or_stopped(&stop_rx, Duration::from_millis(50)) {
                break;
            }
            continue;
        }

        let is_near = match check_mouse_near_edge(&window, &state) {
            Ok(near) => near,
            Err(_) => {
                if sleep_or_stopped(&stop_rx, Duration::from_millis(100)) {
                    break;
                }
                continue;
            }
        };

        if is_near == last_near_state {
            if sleep_or_stopped(&stop_rx, Duration::from_millis(50)) {
                break;
            }
            continue;
        }

        if is_near && state.is_hidden {
            if !crate::services::system::is_front_app_globally_disabled_from_settings() {
                let _ = crate::show_snapped_window(&window);
            }
        } else if !is_near && !state.is_hidden && !state.is_pinned {
            let _ = crate::hide_snapped_window(&window);
        }

        last_near_state = is_near;
        if sleep_or_stopped(&stop_rx, Duration::from_millis(50)) {
            break;
        }
    }

    MONITORING_ACTIVE.store(false, Ordering::Relaxed);
}

fn sleep_or_stopped(stop_rx: &Receiver<()>, duration: Duration) -> bool {
    match stop_rx.recv_timeout(duration) {
        Ok(_) | Err(RecvTimeoutError::Disconnected) => true,
        Err(RecvTimeoutError::Timeout) => false,
    }
}

const CONTENT_INSET_LOGICAL: f64 = 5.0;

fn check_mouse_near_edge(
    window: &WebviewWindow,
    state: &super::state::MainWindowState,
) -> Result<bool, String> {
    let (cursor_x, cursor_y) = crate::mouse::get_cursor_position();
    let (win_x, win_y, win_width, win_height) = crate::get_window_bounds(window)?;

    let (monitor_x, monitor_y, monitor_w, monitor_h) =
        crate::utils::screen::ScreenUtils::get_monitor_at_point(window.app_handle(), win_x, win_y)?;
    let monitor_right = monitor_x + monitor_w;
    let monitor_bottom = monitor_y + monitor_h;

    let scale_factor = crate::utils::screen::ScreenUtils::get_scale_factor_at_point(
        window.app_handle(),
        win_x,
        win_y,
    );

    let settings = crate::get_settings();
    let base_trigger = if settings.edge_hide_offset >= 10 {
        settings.edge_hide_offset
    } else {
        10
    };

    let mouse_in_window = cursor_x >= win_x
        && cursor_x <= win_x + win_width as i32
        && cursor_y >= win_y
        && cursor_y <= win_y + win_height as i32;

    let content_inset = (CONTENT_INSET_LOGICAL * scale_factor) as i32;
    let trigger_distance = base_trigger + content_inset;

    let is_near = match state.snap_edge {
        super::state::SnapEdge::Left => {
            cursor_x <= monitor_x + trigger_distance
                && cursor_y >= win_y
                && cursor_y <= win_y + win_height as i32
        }
        super::state::SnapEdge::Right => {
            cursor_x >= monitor_right - trigger_distance
                && cursor_y >= win_y
                && cursor_y <= win_y + win_height as i32
        }
        super::state::SnapEdge::Top => {
            cursor_y <= monitor_y + trigger_distance
                && cursor_x >= win_x
                && cursor_x <= win_x + win_width as i32
        }
        super::state::SnapEdge::Bottom => {
            cursor_y >= monitor_bottom - trigger_distance
                && cursor_x >= win_x
                && cursor_x <= win_x + win_width as i32
        }
        super::state::SnapEdge::None => false,
    };

    Ok(is_near || mouse_in_window)
}
