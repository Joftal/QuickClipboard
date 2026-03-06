import { invoke } from '@tauri-apps/api/core'

// 设置窗口置顶状态
export async function setWindowPinned(pinned) {
  return await invoke('set_window_pinned', { pinned })
}

// 隐藏主窗口
export async function hideMainWindow() {
  return await invoke('hide_main_window')
}

// 聚焦剪贴板窗口
export async function focusClipboardWindow() {
  return await invoke('focus_clipboard_window')
}

// 仅保存当前焦点（不切换焦点）
export async function saveCurrentFocus() {
  return await invoke('save_current_focus')
}

// 恢复上次焦点窗口
export async function restoreLastFocus() {
  return await invoke('restore_last_focus')
}

// 开始自定义拖拽
export async function startCustomDrag(mouseScreenX, mouseScreenY) {
  return await invoke('start_custom_drag', { mouseScreenX, mouseScreenY })
}

// 打开设置窗口
export async function openSettingsWindow() {
  return await invoke('open_settings_window')
}

// 刷新所有窗口
export async function reloadAllWindows() {
  return await invoke('reload_all_windows')
}

