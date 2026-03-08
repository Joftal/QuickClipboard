import {
  reloadSettings,
  saveSettings as saveSettingsApi,
  getAppVersion as getAppVersionApi
} from '@shared/api'
import { emit } from '@tauri-apps/api/event'
import { toast } from '@shared/store/toastStore'
import i18n from '@shared/i18n'

export const defaultSettings = {
  autoStart: false,
  runAsAdmin: false,
  startHidden: false,
  historyLimit: 100,
  language: 'zh-CN',

  theme: 'light',
  darkThemeStyle: 'classic',
  opacity: 0.9,
  clipboardAnimationEnabled: true,
  uiAnimationEnabled: true,

  toggleShortcut: 'Shift+Space',
  quickpasteShortcut: 'Ctrl+`',
  numberShortcuts: true,
  numberShortcutsModifier: 'Ctrl',

  navigateUpShortcut: 'ArrowUp',
  navigateDownShortcut: 'ArrowDown',
  tabLeftShortcut: 'ArrowLeft',
  tabRightShortcut: 'ArrowRight',
  focusSearchShortcut: 'Tab',
  hideWindowShortcut: 'Escape',
  executeItemShortcut: 'Ctrl+Enter',
  previousGroupShortcut: 'Ctrl+ArrowUp',
  nextGroupShortcut: 'Ctrl+ArrowDown',
  togglePinShortcut: 'Ctrl+P',
  toggleClipboardMonitorShortcut: 'Ctrl+Shift+Z',
  togglePasteWithFormatShortcut: 'Ctrl+Shift+X',
  pastePlainTextShortcut: '',

  clipboardMonitor: true,
  ignoreDuplicates: true,
  saveImages: true,
  imagePreview: false,
  textPreview: false,
  autoScrollToTopOnShow: false,
  autoClearSearch: false,
  windowPositionMode: 'smart',
  rememberWindowSize: false,
  titleBarPosition: 'top',
  edgeHideEnabled: true,
  edgeSnapPosition: null,
  edgeHideOffset: 3,
  autoFocusSearch: false,
  pasteWithFormat: true,
  pasteShortcutMode: 'ctrl_v',
  pasteToTop: false,
  showBadges: true,
  showSourceIcon: true,

  imageMaxSizeMb: 15,
  imageMaxWidth: 4096,
  imageMaxHeight: 4096,

  quickpasteEnabled: true,
  quickpastePasteOnModifierRelease: false,

  mouseMiddleButtonEnabled: false,
  mouseMiddleButtonModifier: 'None',
  mouseMiddleButtonTrigger: 'short_press',
  mouseMiddleButtonLongPressMs: 300,

  appFilterEnabled: false,
  appFilterMode: 'blacklist',
  appFilterList: [],
  appFilterEffect: 'clipboard_only',

  savedWindowPosition: null,
  savedWindowSize: null,

  customStoragePath: null,
  useCustomStorage: false
}

export async function loadSettingsFromBackend() {
  try {
    const savedSettings = await reloadSettings()
    return { ...defaultSettings, ...savedSettings }
  } catch (error) {
    console.error('加载设置失败:', error)
    return { ...defaultSettings }
  }
}

export async function saveSettingsToBackend(settings, options = {}) {
  const { showToast = true } = options

  try {
    await saveSettingsApi(settings)
    await emit('settings-changed', settings)

    if (showToast) {
      toast.success(i18n.t('settings.saved'))
    }
    return { success: true }
  } catch (error) {
    console.error('保存设置失败:', error)
    if (showToast) {
      toast.error(i18n.t('settings.saveFailed'))
    }
    return { success: false, error: error.message }
  }
}

export async function getAppVersion() {
  try {
    return await getAppVersionApi()
  } catch (error) {
    console.error('获取版本信息失败:', error)
    return { version: '未知' }
  }
}
