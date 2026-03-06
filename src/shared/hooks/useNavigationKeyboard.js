import { useEffect } from 'react'
import { listen } from '@tauri-apps/api/event'
import { hideMainWindow } from '@shared/api'

// 全局键盘导航Hook
export function useNavigationKeyboard({
  onNavigateUp = null,
  onNavigateDown = null,
  onExecuteItem = null,
  onTabLeft = null,
  onTabRight = null,
  onFocusSearch = null,
  onTogglePin = null,
  onPreviousGroup = null,
  onNextGroup = null,
  enabled = true
}) {
  useEffect(() => {
    if (!enabled) return
    
    let disposed = false

    const unlistenNavigationActionPromise = listen('navigation-action', (event) => {
      const action = event.payload.action
      
      switch (action) {
        case 'navigate-up':
          if (onNavigateUp) onNavigateUp()
          break
        case 'navigate-down':
          if (onNavigateDown) onNavigateDown()
          break
        case 'execute-item':
          if (onExecuteItem) onExecuteItem()
          break
        case 'tab-left':
          if (onTabLeft) onTabLeft()
          break
        case 'tab-right':
          if (onTabRight) onTabRight()
          break
        case 'focus-search':
          if (onFocusSearch) onFocusSearch()
          break
        case 'hide-window':
          hideMainWindow().catch(err => {
            console.error('隐藏窗口失败:', err)
          })
          break
        case 'toggle-pin':
          if (onTogglePin) {
            onTogglePin()
          }
          break
        case 'previous-group':
          if (onPreviousGroup) onPreviousGroup()
          break
        case 'next-group':
          if (onNextGroup) onNextGroup()
          break
        default:
          break
      }
    }).then(unlisten => {
      if (disposed) {
        unlisten()
        return null
      }

      return unlisten
    }).catch(error => {
      console.error('设置导航监听器失败:', error)
      return null
    })
    
    // 清理
    return () => {
      disposed = true

      unlistenNavigationActionPromise.then(unlistenNavigationAction => {
        if (unlistenNavigationAction) {
          unlistenNavigationAction()
        }
      })
    }
  }, [
    enabled,
    onNavigateUp,
    onNavigateDown,
    onExecuteItem,
    onTabLeft,
    onTabRight,
    onFocusSearch,
    onTogglePin,
    onPreviousGroup,
    onNextGroup
  ])
}

