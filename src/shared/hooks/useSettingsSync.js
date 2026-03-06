import { useEffect } from 'react'
import { listen } from '@tauri-apps/api/event'
import { settingsStore } from '@shared/store/settingsStore'
import { toolsStore } from '@shared/store/toolsStore'
import i18n from '@shared/i18n'

// 监听设置变更事件并同步到当前窗口（跨窗口设置同步）
export function useSettingsSync() {
  useEffect(() => {
    let disposed = false

    const unlistenPromise = listen('settings-changed', (event) => {
      // 批量更新设置
      if (event.payload && typeof event.payload === 'object') {
        settingsStore.updateSettings(event.payload)
        
        if (event.payload.language !== undefined && event.payload.language !== i18n.language) {
          i18n.changeLanguage(event.payload.language)
        }
        if (event.payload.pasteWithFormat !== undefined) {
          const value = event.payload.pasteWithFormat
          localStorage.setItem('tool-state-format-toggle-button', JSON.stringify(value))
          toolsStore.states['format-toggle-button'] = value
        }
      }
    }).then(unlisten => {
      if (disposed) {
        unlisten()
        return null
      }

      return unlisten
    }).catch(error => {
      console.error('设置监听器启动失败:', error)
      return null
    })

    return () => {
      disposed = true

      unlistenPromise.then(unlisten => {
        if (unlisten) {
          unlisten()
        }
      })
    }
  }, [])
}

