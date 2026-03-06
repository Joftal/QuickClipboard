import { proxy } from 'valtio'
import { listen } from '@tauri-apps/api/event'
import { getToolState } from '@shared/services/toolActions'
import { 
  getClipboardHistory, 
  getClipboardTotalCount,
  deleteClipboardItem as apiDeleteItem,
  clearClipboardHistory as apiClearHistory,
  pasteClipboardItem as apiPasteClipboardItem
} from '@shared/api'

let clipboardStoreListenerDispose = null
let clipboardStoreListenerInitPromise = null

export async function initClipboardStoreListeners() {
  if (clipboardStoreListenerDispose) {
    return
  }

  if (clipboardStoreListenerInitPromise) {
    return clipboardStoreListenerInitPromise
  }

  clipboardStoreListenerInitPromise = listen('paste-count-updated', (event) => {
    const id = event.payload
    for (const key of Object.keys(clipboardStore.items)) {
      const item = clipboardStore.items[key]
      if (item && item.id === id) {
        clipboardStore.items[key] = { ...item, paste_count: (item.paste_count || 0) + 1 }
        break
      }
    }
  }).then(unlisten => {
    clipboardStoreListenerDispose = unlisten
  }).catch(error => {
    console.error('初始化剪贴板 store 监听失败:', error)
    throw error
  }).finally(() => {
    clipboardStoreListenerInitPromise = null
  })

  return clipboardStoreListenerInitPromise
}

export function disposeClipboardStoreListeners() {
  if (!clipboardStoreListenerDispose) {
    return
  }

  try {
    clipboardStoreListenerDispose()
  } catch (error) {
    console.error('释放剪贴板 store 监听失败:', error)
  } finally {
    clipboardStoreListenerDispose = null
  }
}

const CACHE_WINDOW_SIZE = 200 
const CACHE_BUFFER = 100      

// 剪贴板 Store
export const clipboardStore = proxy({
  items: {}, 
  totalCount: 0,
  filter: '',
  contentType: 'all',
  requestGeneration: 0,
  selectedIds: new Set(),
  loading: false,
  error: null,
  loadingRanges: new Set(),
  currentViewRange: { start: 0, end: 50 }, 
  
  // 设置指定范围的数据
  setItemsInRange(startIndex, items) {
    items.forEach((item, offset) => {
      this.items[startIndex + offset] = item
    })
  },
  
  updateViewRange(startIndex, endIndex) {
    const prev = this.currentViewRange
    if (Math.abs(prev.start - startIndex) > 30 || Math.abs(prev.end - endIndex) > 30) {
      this.currentViewRange = { start: startIndex, end: endIndex }
      this.trimCache()
    }
  },
  
  trimCache() {
    const itemCount = Object.keys(this.items).length
    if (itemCount <= CACHE_WINDOW_SIZE) return
    
    const { start, end } = this.currentViewRange
    const center = Math.floor((start + end) / 2)
    const keepStart = Math.max(0, center - CACHE_BUFFER)
    const keepEnd = Math.min(this.totalCount - 1, center + CACHE_BUFFER)
    
    for (const key of Object.keys(this.items)) {
      const index = parseInt(key, 10)
      if (index < keepStart || index > keepEnd) {
        delete this.items[key]
      }
    }
  },

  getItem(index) {
    return this.items[index]
  },
  
  // 检查指定索引是否已加载
  hasItem(index) {
    return index in this.items
  },

  invalidateItemsCache() {
    this.items = {}
  },

  applyViewState({ filter = this.filter, contentType = this.contentType } = {}) {
    let changed = false

    if (this.filter !== filter) {
      this.filter = filter
      changed = true
    }

    if (this.contentType !== contentType) {
      this.contentType = contentType
      changed = true
    }

    return changed
  },
  
  setFilter(value) {
    if (this.applyViewState({ filter: value })) {
      invalidateClipboardCollection()
    }
  },
  
  setContentType(value) {
    if (this.applyViewState({ contentType: value })) {
      invalidateClipboardCollection()
    }
  },
  
  toggleSelect(id) {
    if (this.selectedIds.has(id)) {
      this.selectedIds.delete(id)
    } else {
      this.selectedIds.add(id)
    }
  },
  
  clearSelection() {
    this.selectedIds.clear()
  },
  
  clearAll() {
    invalidateClipboardCollection({ resetTotalCount: true })
    this.selectedIds = new Set()
    this.currentViewRange = { start: 0, end: 50 }
  },
  
  // 记录正在加载的范围
  addLoadingRange(start, end) {
    this.loadingRanges.add(`${start}-${end}`)
  },
  
  removeLoadingRange(start, end) {
    this.loadingRanges.delete(`${start}-${end}`)
  },
  
  isRangeLoading(start, end) {
    return this.loadingRanges.has(`${start}-${end}`)
  },

  hasOverlappingLoadingRange(start, end) {
    for (const range of this.loadingRanges) {
      const [loadStart, loadEnd] = range.split('-').map(Number);
      if (start <= loadEnd && end >= loadStart) {
        return true;
      }
    }
    return false;
  }
})

function createClipboardRequestContext() {
  return {
    generation: clipboardStore.requestGeneration,
    filter: clipboardStore.filter,
    contentType: clipboardStore.contentType
  }
}

function invalidateClipboardCollection({ resetTotalCount = false } = {}) {
  clipboardStore.requestGeneration += 1
  clipboardStore.invalidateItemsCache()
  clipboardStore.loadingRanges = new Set()

  if (resetTotalCount) {
    clipboardStore.totalCount = 0
  }
}

function isClipboardRequestCurrent(context) {
  return (
    context.generation === clipboardStore.requestGeneration &&
    context.filter === clipboardStore.filter &&
    context.contentType === clipboardStore.contentType
  )
}

function beginClipboardRequestCycle() {
  invalidateClipboardCollection()
  return createClipboardRequestContext()
}

// 加载指定范围的数据
export async function loadClipboardRange(startIndex, endIndex) {
  // 避免重复加载
  if (clipboardStore.isRangeLoading(startIndex, endIndex) || 
      clipboardStore.hasOverlappingLoadingRange(startIndex, endIndex)) {
    return
  }
  
  // 检查是否所有数据都已加载
  let allLoaded = true
  for (let i = startIndex; i <= endIndex; i++) {
    if (!clipboardStore.hasItem(i)) {
      allLoaded = false
      break
    }
  }
  
  if (allLoaded) {
    return
  }
  
  clipboardStore.addLoadingRange(startIndex, endIndex)
  const requestContext = createClipboardRequestContext()
  
  try {
    const limit = endIndex - startIndex + 1
    const result = await getClipboardHistory({
      offset: startIndex,
      limit,
      contentType: clipboardStore.contentType !== 'all' ? clipboardStore.contentType : undefined,
      search: clipboardStore.filter || undefined
    })

    if (!isClipboardRequestCurrent(requestContext)) {
      return
    }
    
    // 将数据按索引存储
    clipboardStore.setItemsInRange(startIndex, result.items)
    
    // 更新总数
    if (result.total_count !== undefined) {
      clipboardStore.totalCount = result.total_count
    }
  } catch (err) {
    console.error(`加载范围 ${startIndex}-${endIndex} 失败:`, err)
    if (isClipboardRequestCurrent(requestContext)) {
      clipboardStore.error = err.message || '加载失败'
    }
  } finally {
    clipboardStore.removeLoadingRange(startIndex, endIndex)
  }
}

export async function loadClipboardItems() {
  return await handleClipboardDataChanged()
}

export async function updateClipboardView({ filter = clipboardStore.filter, contentType = clipboardStore.contentType } = {}) {
  const changed = clipboardStore.applyViewState({ filter, contentType })
  if (!changed) {
    return false
  }

  await refreshClipboardHistory()
  return true
}

// 初始化加载
export async function initClipboardItems() {
  const requestContext = beginClipboardRequestCycle()
  clipboardStore.loading = true
  clipboardStore.error = null
  
  try {
    if (clipboardStore.contentType !== 'all' || clipboardStore.filter) {
      const result = await getClipboardHistory({
        offset: 0,
        limit: 100,
        contentType: clipboardStore.contentType !== 'all' ? clipboardStore.contentType : undefined,
        search: clipboardStore.filter || undefined
      })

      if (!isClipboardRequestCurrent(requestContext)) {
        return
      }
      
      clipboardStore.totalCount = result.total_count
      clipboardStore.setItemsInRange(0, result.items)
    } else {
      const totalCount = await getClipboardTotalCount()

      if (!isClipboardRequestCurrent(requestContext)) {
        return
      }

      clipboardStore.totalCount = totalCount
      
      if (totalCount > 0) {
        const endIndex = Math.min(99, totalCount - 1)
        await loadClipboardRange(0, endIndex)
      }
    }
  } catch (err) {
    console.error('初始化剪贴板失败:', err)
    if (isClipboardRequestCurrent(requestContext)) {
      clipboardStore.error = err.message || '加载失败'
    }
  } finally {
    if (isClipboardRequestCurrent(requestContext)) {
      clipboardStore.loading = false
    }
  }
}

// 刷新剪贴板历史
export async function refreshClipboardHistory() {
  return await initClipboardItems()
}

export async function handleClipboardDataChanged() {
  return await refreshClipboardHistory()
}

// 删除剪贴板项
export async function deleteClipboardItem(id) {
  try {
    await apiDeleteItem(id)
    await handleClipboardDataChanged()
    return true
  } catch (err) {
    console.error('删除剪贴板项失败:', err)
    throw err
  }
}

// 清空剪贴板历史
export async function clearClipboardHistory() {
  try {
    await apiClearHistory()
    clipboardStore.clearAll()
    return true
  } catch (err) {
    console.error('清空剪贴板历史失败:', err)
    throw err
  }
}

// 粘贴剪贴板项
export async function pasteClipboardItem(id, format = null) {
  try {
    await apiPasteClipboardItem(id, format)

    if (getToolState('one-time-paste-button')) {
      try {
        await apiDeleteItem(id)
        setTimeout(() => {
          refreshClipboardHistory().catch(error => {
            console.error('一次性粘贴：刷新剪贴板列表失败:', error)
          })
        }, 200)
      } catch (deleteError) {
        console.error('一次性粘贴：删除失败', deleteError)
      }
    }

    return true
  } catch (err) {
    console.error('粘贴剪贴板项失败:', err)
    throw err
  }
}

