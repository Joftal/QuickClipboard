import { proxy } from 'valtio'
import { listen } from '@tauri-apps/api/event'
import i18n from '@shared/i18n'
import { groupsStore } from './groupsStore'
import { getToolState } from '@shared/services/toolActions'
import {
  getFavoritesHistory,
  getFavoritesTotalCount,
  deleteFavorite as apiDeleteFavorite,
  pasteFavorite as apiPasteFavorite
} from '@shared/api/favorites'
import { showConfirm } from '@shared/utils/dialog'

let favoritesStoreListenerDispose = null
let favoritesStoreListenerInitPromise = null

export async function initFavoritesStoreListeners() {
  if (favoritesStoreListenerDispose) {
    return
  }

  if (favoritesStoreListenerInitPromise) {
    return favoritesStoreListenerInitPromise
  }

  favoritesStoreListenerInitPromise = listen('favorite-paste-count-updated', (event) => {
    const id = event.payload
    for (const key of Object.keys(favoritesStore.items)) {
      const item = favoritesStore.items[key]
      if (item && item.id === id) {
        favoritesStore.items[key] = { ...item, paste_count: (item.paste_count || 0) + 1 }
        break
      }
    }
  }).then(unlisten => {
    favoritesStoreListenerDispose = unlisten
  }).catch(error => {
    console.error('初始化收藏 store 监听失败:', error)
    throw error
  }).finally(() => {
    favoritesStoreListenerInitPromise = null
  })

  return favoritesStoreListenerInitPromise
}

export function disposeFavoritesStoreListeners() {
  if (!favoritesStoreListenerDispose) {
    return
  }

  try {
    favoritesStoreListenerDispose()
  } catch (error) {
    console.error('释放收藏 store 监听失败:', error)
  } finally {
    favoritesStoreListenerDispose = null
  }
}

const CACHE_WINDOW_SIZE = 200  
const CACHE_BUFFER = 100     

// 收藏 Store
export const favoritesStore = proxy({
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
  
  // 获取指定索引的项
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
      invalidateFavoritesCollection()
    }
  },
  
  setContentType(value) {
    if (this.applyViewState({ contentType: value })) {
      invalidateFavoritesCollection()
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
    invalidateFavoritesCollection({ resetTotalCount: true })
    this.selectedIds = new Set()
    this.currentViewRange = { start: 0, end: 50 }
  },
  
  // 记录正在加载的范围
  addLoadingRange(start, end) {
    this.loadingRanges.add(`${start}-${end}`)
  },
  
  // 移除加载中的范围
  removeLoadingRange(start, end) {
    this.loadingRanges.delete(`${start}-${end}`)
  },
  
  // 检查范围是否正在加载
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

function resolveFavoritesGroupName(groupName = null) {
  return groupName || groupsStore.currentGroup
}

function invalidateFavoritesCollection({ resetTotalCount = false } = {}) {
  favoritesStore.requestGeneration += 1
  favoritesStore.invalidateItemsCache()
  favoritesStore.loadingRanges = new Set()

  if (resetTotalCount) {
    favoritesStore.totalCount = 0
  }
}

function createFavoritesRequestContext(groupName = null) {
  return {
    generation: favoritesStore.requestGeneration,
    filter: favoritesStore.filter,
    contentType: favoritesStore.contentType,
    groupName: resolveFavoritesGroupName(groupName)
  }
}

function isFavoritesRequestCurrent(context) {
  return (
    context.generation === favoritesStore.requestGeneration &&
    context.filter === favoritesStore.filter &&
    context.contentType === favoritesStore.contentType &&
    context.groupName === resolveFavoritesGroupName()
  )
}

function beginFavoritesRequestCycle(groupName = null) {
  invalidateFavoritesCollection()
  return createFavoritesRequestContext(groupName)
}

export async function updateFavoritesView({
  filter = favoritesStore.filter,
  contentType = favoritesStore.contentType,
  groupName = null,
} = {}) {
  const changed = favoritesStore.applyViewState({ filter, contentType })
  if (!changed) {
    return false
  }

  await refreshFavorites(groupName)
  return true
}

export async function handleFavoritesDataChanged(groupName = null) {
  return await refreshFavorites(groupName)
}

// 加载指定范围的数据
export async function loadFavoritesRange(startIndex, endIndex, groupName = null) {
  if (favoritesStore.isRangeLoading(startIndex, endIndex) || 
      favoritesStore.hasOverlappingLoadingRange(startIndex, endIndex)) {
    return
  }
  
  // 检查是否所有数据都已加载
  let allLoaded = true
  for (let i = startIndex; i <= endIndex; i++) {
    if (!favoritesStore.hasItem(i)) {
      allLoaded = false
      break
    }
  }
  
  if (allLoaded) {
    return
  }
  
  favoritesStore.addLoadingRange(startIndex, endIndex)
  const resolvedGroupName = resolveFavoritesGroupName(groupName)
  const requestContext = createFavoritesRequestContext(resolvedGroupName)
  
  try {
    const limit = endIndex - startIndex + 1
    const result = await getFavoritesHistory({
      offset: startIndex,
      limit,
      groupName: resolvedGroupName,
      contentType: favoritesStore.contentType !== 'all' ? favoritesStore.contentType : undefined,
      search: favoritesStore.filter || undefined
    })

    if (!isFavoritesRequestCurrent(requestContext)) {
      return
    }
    
    // 将数据按索引存储
    favoritesStore.setItemsInRange(startIndex, result.items)
    
    // 更新总数
    if (result.total_count !== undefined) {
      favoritesStore.totalCount = result.total_count
    }
  } catch (err) {
    console.error(`加载范围 ${startIndex}-${endIndex} 失败:`, err)
    if (isFavoritesRequestCurrent(requestContext)) {
      favoritesStore.error = err.message || '加载失败'
    }
  } finally {
    favoritesStore.removeLoadingRange(startIndex, endIndex)
  }
}

// 初始化加载
export async function initFavorites(groupName = null) {
  const resolvedGroupName = resolveFavoritesGroupName(groupName)
  const requestContext = beginFavoritesRequestCycle(resolvedGroupName)
  favoritesStore.loading = true
  favoritesStore.error = null
  
  try {
    if (favoritesStore.contentType !== 'all' || favoritesStore.filter) {
      const result = await getFavoritesHistory({
        offset: 0,
        limit: 50,
        groupName: resolvedGroupName,
        contentType: favoritesStore.contentType !== 'all' ? favoritesStore.contentType : undefined,
        search: favoritesStore.filter || undefined
      })

      if (!isFavoritesRequestCurrent(requestContext)) {
        return
      }
      
      favoritesStore.totalCount = result.total_count
      favoritesStore.setItemsInRange(0, result.items)
    } else {
      const totalCount = await getFavoritesTotalCount(resolvedGroupName)

      if (!isFavoritesRequestCurrent(requestContext)) {
        return
      }

      favoritesStore.totalCount = totalCount
      
      if (totalCount > 0) {
        const endIndex = Math.min(49, totalCount - 1)
        await loadFavoritesRange(0, endIndex, resolvedGroupName)
      }
    }
  } catch (err) {
    console.error('初始化收藏列表失败:', err)
    if (isFavoritesRequestCurrent(requestContext)) {
      favoritesStore.error = err.message || '加载失败'
    }
  } finally {
    if (isFavoritesRequestCurrent(requestContext)) {
      favoritesStore.loading = false
    }
  }
}

// 刷新收藏列表
export async function refreshFavorites(groupName = null) {
  return await initFavorites(groupName)
}

// 删除收藏项
export async function deleteFavorite(id) {
  try {
    const confirmed = await showConfirm(
      i18n.t('favorites.confirmDelete'),
      i18n.t('favorites.confirmDeleteTitle')
    )
    if (!confirmed) return false

    await apiDeleteFavorite(id)
    await handleFavoritesDataChanged()
    return true
  } catch (err) {
    console.error('删除收藏项失败:', err)
    throw err
  }
}

// 粘贴收藏项
export async function pasteFavorite(id, format = null) {
  try {
    await apiPasteFavorite(id, format)

    if (getToolState('one-time-paste-button')) {
      try {
        await apiDeleteFavorite(id)
        setTimeout(() => {
          refreshFavorites().catch(error => {
            console.error('一次性粘贴：刷新收藏列表失败:', error)
          })
        }, 200)
      } catch (deleteError) {
        console.error('一次性粘贴：删除收藏项失败', deleteError)
      }
    }

    return true
  } catch (err) {
    console.error('粘贴收藏项失败:', err)
    throw err
  }
}

