import {
  setWindowPinned,
  openSettingsWindow
} from '@shared/api'
import { settingsStore } from '@shared/store/settingsStore'

// 不持久化的临时工具状态
const temporaryTools = ['pin-button']

// 临时工具的内存状态
const temporaryStates = {}

// 配置文件工具
const configFileTools = {
  'format-toggle-button': { key: 'pasteWithFormat', default: true }
}

// 获取工具状态
export function getToolState(toolId) {
  if (temporaryTools.includes(toolId)) {
    return temporaryStates[toolId] ?? false
  }
  
  try {
    const saved = localStorage.getItem(`tool-state-${toolId}`)
    if (saved !== null) {
      return JSON.parse(saved)
    }
  } catch (error) {
    console.error(`读取工具状态失败 ${toolId}:`, error)
  }
  
  // 返回默认值
  if (configFileTools[toolId]) {
    return configFileTools[toolId].default
  }
  return false
}

// 设置工具状态到 localStorage
export function setToolState(toolId, state) {
  if (temporaryTools.includes(toolId)) {
    temporaryStates[toolId] = state
    return
  }
  
  try {
    localStorage.setItem(`tool-state-${toolId}`, JSON.stringify(state))
  } catch (error) {
    console.error(`保存工具状态失败 ${toolId}:`, error)
  }
}

// 初始化工具状态：从配置文件同步到 localStorage 缓存
export async function initializeToolStates(settingsStore) {
  temporaryTools.forEach(toolId => {
    try {
      localStorage.removeItem(`tool-state-${toolId}`)
    } catch (error) {
      console.error(`清理临时工具状态失败 ${toolId}:`, error)
    }
  })
  
  // 从配置文件同步工具状态到 localStorage 缓存
  for (const [toolId, config] of Object.entries(configFileTools)) {
    try {
      const value = settingsStore[config.key]
      localStorage.setItem(`tool-state-${toolId}`, JSON.stringify(value))
    } catch (error) {
      console.error(`初始化工具状态失败 ${toolId}:`, error)
    }
  }
}

// 工具操作处理器
export const toolActions = {
  // 窗口固定
  'pin-button': async () => {
    const currentState = getToolState('pin-button')
    const newState = !currentState
    
    try {
      await setWindowPinned(newState)
      setToolState('pin-button', newState)
      return newState
    } catch (error) {
      console.error('设置窗口固定状态失败:', error)
      throw error
    }
  },
  
  // 打开设置
  'settings-button': async () => {
    try {
      await openSettingsWindow()
    } catch (error) {
      console.error('打开设置窗口失败:', error)
      throw error
    }
  },
  
  // 一次性粘贴
  'one-time-paste-button': async () => {
    const currentState = getToolState('one-time-paste-button')
    const newState = !currentState
    setToolState('one-time-paste-button', newState)

    window.dispatchEvent(new CustomEvent('one-time-paste-changed', {
      detail: { enabled: newState }
    }))
    
    return newState
  },
  
  // 格式切换
  'format-toggle-button': async () => {
    const currentState = getToolState('format-toggle-button')
    const newState = !currentState
    
    try {
      // 保存到配置文件
      await settingsStore.saveSetting('pasteWithFormat', newState, { showToast: false })
      
      // 同步到 localStorage 缓存
      setToolState('format-toggle-button', newState)
      
      // 触发自定义事件
      window.dispatchEvent(new CustomEvent('format-mode-changed', {
        detail: { withFormat: newState }
      }))
      
      return newState
    } catch (error) {
      console.error('切换格式模式失败:', error)
      throw error
    }
  }
}

// 执行工具操作
export async function executeToolAction(toolId) {
  const action = toolActions[toolId]
  if (!action) {
    console.warn(`工具 ${toolId} 没有对应的操作`)
    return null
  }
  
  try {
    return await action()
  } catch (error) {
    console.error(`执行工具操作失败 ${toolId}:`, error)
    throw error
  }
}

