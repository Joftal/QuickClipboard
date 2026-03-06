// 导入
import { initSettings as initSettingsFunc } from './settingsStore'
import { initToolsStore as initToolsStoreFunc } from './toolsStore'
import {
  initClipboardStoreListeners,
  disposeClipboardStoreListeners,
} from './clipboardStore'
import {
  initFavoritesStoreListeners,
  disposeFavoritesStoreListeners,
} from './favoritesStore'

// 导出所有 stores
export { 
  clipboardStore, 
  initClipboardStoreListeners,
  disposeClipboardStoreListeners,
  loadClipboardItems, 
  updateClipboardView,
  handleClipboardDataChanged,
  refreshClipboardHistory,
  deleteClipboardItem,
  clearClipboardHistory 
} from './clipboardStore'
export { settingsStore, initSettings } from './settingsStore'
export {
  favoritesStore,
  initFavoritesStoreListeners,
  disposeFavoritesStoreListeners,
  initFavorites,
  updateFavoritesView,
  handleFavoritesDataChanged,
  loadFavoritesRange,
  refreshFavorites,
  deleteFavorite,
  pasteFavorite
} from './favoritesStore'
export {
  groupsStore,
  loadGroups,
  addGroup,
  updateGroup,
  deleteGroup
} from './groupsStore'
export { toolsStore, initToolsStore } from './toolsStore'
export { toastStore, toast, TOAST_POSITIONS } from './toastStore'
export { navigationStore } from './navigationStore'

// 初始化所有 stores
export async function initStores() {
  await Promise.all([
    initClipboardStoreListeners(),
    initFavoritesStoreListeners(),
  ])
  await initSettingsFunc()
  initToolsStoreFunc()
}

export function disposeStores() {
  disposeClipboardStoreListeners()
  disposeFavoritesStoreListeners()
}

