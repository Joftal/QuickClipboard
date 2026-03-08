import { invoke } from '@tauri-apps/api/core'

export async function getAppVersion() {
  return await invoke('get_app_version')
}

export async function copyTextToClipboard(text) {
  return await invoke('copy_text_to_clipboard', { text })
}

export async function recognizeImageOcr(filePath, language = null) {
  return await invoke('recognize_file_ocr', { filePath, language })
}

export async function promptDisableWinVHotkeyIfNeeded() {
  return await invoke('prompt_disable_win_v_hotkey_if_needed')
}

export async function promptEnableWinVHotkey() {
  return await invoke('prompt_enable_win_v_hotkey')
}
