import {
  reloadSettings,
  saveSettings as saveSettingsApi,
  setEdgeHideEnabled as setEdgeHideEnabledApi,
  getAllWindowsInfo,
  getAppVersion as getAppVersionApi,
  isPortableMode as isPortableModeApi
} from '@shared/api'
import { emit } from '@tauri-apps/api/event'
import { toast } from '@shared/store/toastStore'
import i18n from '@shared/i18n'

export const defaultSettings = {
  // 常规设置
  autoStart: false,
  runAsAdmin: false,
  startHidden: false,
  historyLimit: 100,
  language: 'zh-CN',
  
  // 外观设置
  theme: 'light',
  darkThemeStyle: 'classic',
  opacity: 0.9,
  clipboardAnimationEnabled: true,
  uiAnimationEnabled: true,
  
  // 快捷键设置
  toggleShortcut: 'Shift+Space',
  quickpasteShortcut: 'Ctrl+`',
  numberShortcuts: true,
  numberShortcutsModifier: 'Ctrl',
  
  // 剪贴板窗口快捷键
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
  
  // 剪贴板设置
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

  // 图片显示限制
  imageMaxSizeMb: 15,
  imageMaxWidth: 4096,
  imageMaxHeight: 4096,
  
  // 便捷粘贴设置
  quickpasteEnabled: true,
  quickpastePasteOnModifierRelease: false,
  
  // 鼠标设置
  mouseMiddleButtonEnabled: false,
  mouseMiddleButtonModifier: 'None',
  mouseMiddleButtonTrigger: 'short_press',
  mouseMiddleButtonLongPressMs: 300,
  
  // 应用过滤
  appFilterEnabled: false,
  appFilterMode: 'blacklist',
  appFilterList: [],
  appFilterEffect: 'clipboard_only',
  
  // 保存的窗口状态
  savedWindowPosition: null,
  savedWindowSize: null,
  
  // 数据存储设置
  customStoragePath: null,
  useCustomStorage: false
}

// 加载设置
export async function loadSettingsFromBackend() {
  try {
    const savedSettings = await reloadSettings()
    return { ...defaultSettings, ...savedSettings }
  } catch (error) {
    console.error('加载设置失败:', error)
    return { ...defaultSettings }
  }
}

// 保存设置
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

// 保存单个设置项
export async function saveSingleSetting(key, value, allSettings) {
  const updatedSettings = { ...allSettings, [key]: value }
  return await saveSettingsToBackend(updatedSettings)
}

// 获取应用版本
export async function getAppVersion() {
  try {
    const versionInfo = await getAppVersionApi()
    return versionInfo
  } catch (error) {
    console.error('获取版本信息失败:', error)
    return { version: '未知' }
  }
}


// 检查是否为便携版模式
export async function isPortableMode() {
  try {
    return await isPortableModeApi()
  } catch (error) {
    console.error('检查便携版模式失败:', error)
    return false
  }
}

// 设置贴边隐藏
export async function setEdgeHideEnabled(enabled) {
  try {
    await setEdgeHideEnabledApi(enabled)
    return { success: true }
  } catch (error) {
    console.error('更新贴边隐藏设置失败:', error)
    return { success: false, error: error.message }
  }
}

// 获取所有窗口信息（用于应用过滤）
export async function getAllWindowsInfoService() {
  try {
    return await getAllWindowsInfo()
  } catch (error) {
    console.error('获取应用列表失败:', error)
    return []
  }
}

