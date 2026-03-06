// 右键菜单工具函数
import { showContextMenuFromEvent, createMenuItem, createSeparator } from '@/plugins/context_menu/index.js'
import { openPath } from '@tauri-apps/plugin-opener'
import i18n from '@shared/i18n'
import { getPrimaryType } from './contentType'
import { settingsStore } from '@shared/store/settingsStore'
import { toast, toastStore, TOAST_SIZES, TOAST_POSITIONS } from '@shared/store/toastStore'
import {
  addClipboardToFavorites,
  pinImageToScreen,
  clearClipboardHistory,
  moveFavoriteToGroup,
  deleteFavorite,
  saveImageFromPath,
  copyTextToClipboard,
  recognizeImageOcr,
  moveClipboardItemToTop,
  copyClipboardItem
} from '@shared/api'
import { copyFavoriteItem } from '@shared/api/favorites'
import { openEditorForClipboard, openEditorForFavorite } from '@shared/api/textEditor'
import { getToolState } from '@shared/services/toolActions'
import { clipboardStore, deleteClipboardItem, loadClipboardItems, pasteClipboardItem } from '@shared/store/clipboardStore'
import { refreshFavorites, pasteFavorite } from '@shared/store/favoritesStore'
import { groupsStore } from '@shared/store/groupsStore'
import { showConfirm } from '@shared/utils/dialog'

const TOAST_CONFIG = {
  size: TOAST_SIZES.EXTRA_SMALL,
  position: TOAST_POSITIONS.BOTTOM_RIGHT
}

// 创建粘贴菜单项
function createPasteMenuItem(contentType, hasHtmlContent) {
  const isRichText = contentType.includes('rich_text')

  if (isRichText && hasHtmlContent) {
    const pasteMenuItem = createMenuItem('paste', i18n.t('contextMenu.paste'), { icon: 'ti ti-clipboard' })
    pasteMenuItem.children = [
      createMenuItem('paste-formatted', i18n.t('contextMenu.pasteWithFormat'), { icon: 'ti ti-typography' }),
      createMenuItem('paste-plain', i18n.t('contextMenu.pastePlainText'), { icon: 'ti ti-text-size' })
    ]
    return pasteMenuItem
  }

  return createMenuItem('paste', i18n.t('contextMenu.paste'), { icon: 'ti ti-clipboard' })
}

// 创建内容类型特定菜单项
function createContentTypeMenuItems(contentType) {
  if (contentType.includes('image')) {
    return [
      createMenuItem('pin-image', i18n.t('contextMenu.pinToScreen'), { icon: 'ti ti-window-maximize' }),
      createMenuItem('save-image', i18n.t('contextMenu.saveImage'), { icon: 'ti ti-download' }),
      createMenuItem('extract-text', i18n.t('contextMenu.extractText'), { icon: 'ti ti-text-scan-2' })
    ]
  }

  if (contentType.includes('file')) {
    return [
      createMenuItem('open-file', i18n.t('contextMenu.openWithDefault'), { icon: 'ti ti-external-link' }),
      createMenuItem('open-location', i18n.t('contextMenu.openLocation'), { icon: 'ti ti-folder-open' }),
      createMenuItem('copy-path', i18n.t('contextMenu.copyPath'), { icon: 'ti ti-copy' })
    ]
  }

  const isRichText = contentType.includes('rich_text')
  return [
    createMenuItem('edit-text', isRichText ? i18n.t('contextMenu.editPlainText') : i18n.t('contextMenu.editText'), { icon: 'ti ti-edit' })
  ]
}

// 处理粘贴操作
async function handlePasteActions(result, item, isClipboard = true, index = undefined) {
  const pasteActions = {
    'paste': null,
    'paste-formatted': 'formatted',
    'paste-plain': 'plain'
  }
  
  if (!(result in pasteActions)) return false
  
  const pasteFunc = isClipboard ? pasteClipboardItem : pasteFavorite
  await pasteFunc(item.id, pasteActions[result])

  // 粘贴后置顶
  if (isClipboard) {
    const oneTimeEnabled = getToolState('one-time-paste-button')
    if (settingsStore.pasteToTop && !oneTimeEnabled && item.id && !item.is_pinned) {
      try {
        await moveClipboardItemToTop(item.id)
      } finally {
        clipboardStore.items = {}
      }
    }
  }
  return true
}

// 处理内容类型操作
async function handleContentTypeActions(result, item, index) {
  if (result === 'edit-text') {
    await openEditorForClipboard(item, index)
    return true
  }
  if (result === 'edit-item') {
    await openEditorForFavorite(item)
    return true
  }

  const contentType = item.content_type || 'text'
  if (!contentType.includes('file') && !contentType.includes('image')) return false

  if (typeof item.content !== 'string' || !item.content.startsWith('files:')) return false

  let filePath = null
  try {
    const filesData = JSON.parse(item.content.substring(6))
    const storedPath = filesData?.files?.[0]?.path || null
    if (storedPath) {
      const { invoke } = await import('@tauri-apps/api/core')
      filePath = await invoke('resolve_image_path', { storedPath })
    }
  } catch (error) {
    console.warn('解析图片文件路径失败:', error)
    filePath = null
  }
  if (!filePath) return false

  const dirPath = filePath.substring(0, Math.max(filePath.lastIndexOf('\\'), filePath.lastIndexOf('/')))

  const actions = {
    'pin-image': async () => {
      await pinImageToScreen(filePath)
      toast.success(i18n.t('contextMenu.imagePinned'), TOAST_CONFIG)
    },
    'save-image': async () => {
      await saveImageFromPath(filePath)
      toast.success(i18n.t('contextMenu.imageSaved'), TOAST_CONFIG)
    },
    'extract-text': async () => {
      const loadingToastId = toast.info(i18n.t('contextMenu.extractingText'), { duration: 0, ...TOAST_CONFIG })
      try {
        const result = await recognizeImageOcr(filePath)
        toastStore.removeToast(loadingToastId)
        
        if (result.text && result.text.trim()) {
          await copyTextToClipboard(result.text)
          toast.success(i18n.t('contextMenu.textExtracted'), TOAST_CONFIG)
        } else {
          toast.error(i18n.t('contextMenu.extractTextFailed'), TOAST_CONFIG)
        }
      } catch (error) {
        console.error('OCR识别失败:', error)
        toastStore.removeToast(loadingToastId)
        toast.error(i18n.t('contextMenu.extractTextFailed'), TOAST_CONFIG)
      }
    },
    'open-file': async () => {
      try {
        await openPath(filePath)
        toast.success(i18n.t('contextMenu.fileOpened'), TOAST_CONFIG)
      } catch (error) {
        console.error('打开文件失败:', error)
        toast.error(i18n.t('common.operationFailed'), TOAST_CONFIG)
      }
    },
    'open-location': async () => {
      try {
        await openPath(dirPath)
        toast.success(i18n.t('contextMenu.locationOpened'), TOAST_CONFIG)
      } catch (error) {
        console.error('打开文件位置失败:', error)
        toast.error(i18n.t('common.operationFailed'), TOAST_CONFIG)
      }
    },
    'copy-path': async () => {
      try {
        await copyTextToClipboard(filePath)
        toast.success(i18n.t('contextMenu.pathCopied'), TOAST_CONFIG)
      } catch (error) {
        console.error('复制文件路径失败:', error)
        toast.error(i18n.t('common.operationFailed'), TOAST_CONFIG)
      }
    }
  }
  
  if (actions[result]) {
    await actions[result]()
    return true
  }
  return false
}

// 显示剪贴板项的右键菜单
export async function showClipboardItemContextMenu(event, item, index) {
  const menuItems = []
  const contentType = item.content_type || 'text'

  const pasteMenuItem = createPasteMenuItem(contentType, !!item.html_content)
  menuItems.push(pasteMenuItem)
  menuItems.push(createMenuItem('copy-item', i18n.t('contextMenu.copy'), { icon: 'ti ti-copy' }))
  menuItems.push(createSeparator())

  const contentMenuItems = createContentTypeMenuItems(contentType)
  if (contentMenuItems.length > 0) {
    menuItems.push(...contentMenuItems)
  }

  // 添加分隔线
  if (menuItems.length > 0 && !menuItems[menuItems.length - 1].separator) {
    menuItems.push(createSeparator())
  }

  // 添加"添加到收藏"菜单
  const groups = groupsStore.groups || []

  const addToFavoritesItem = createMenuItem('add-to-favorites', i18n.t('contextMenu.addToFavorites'), { icon: 'ti ti-star' })

  if (groups.length > 0) {
    addToFavoritesItem.children = groups.map(group =>
      createMenuItem(`add-to-group-${group.name}`, group.name, {
        icon: group.icon || 'ti ti-folder',
        iconColor: group.name === '全部' ? null : (group.color || '#dc2626')
      })
    )
  }

  // 添加通用菜单项
  menuItems.push(
    addToFavoritesItem,
    createMenuItem('delete-item', i18n.t('contextMenu.deleteItem'), { icon: 'ti ti-trash' }),
    createSeparator(),
    createMenuItem('clear-all', i18n.t('contextMenu.clearAll'), { icon: 'ti ti-trash-x' })
  )

  // 显示菜单并处理结果
  const result = await showContextMenuFromEvent(event, menuItems, { theme: settingsStore.theme })
  if (!result) return

  try {
    // 处理复制操作
    if (result === 'copy-item') {
      await copyClipboardItem(item.id)
      toast.success(i18n.t('contextMenu.copied'), TOAST_CONFIG)
      return
    }

    // 处理粘贴操作
    if (await handlePasteActions(result, item, true, index)) return

    // 处理添加到收藏
    if (result.startsWith('add-to-group-')) {
      const groupName = result.substring(13)
      await addClipboardToFavorites(item.id, groupName)
      toast.success(i18n.t('contextMenu.addedToFavorites'), TOAST_CONFIG)
      return
    }

    if (result === 'add-to-favorites') {
      await addClipboardToFavorites(item.id)
      toast.success(i18n.t('contextMenu.addedToFavorites'), TOAST_CONFIG)
      return
    }

    // 处理内容类型操作
    if (await handleContentTypeActions(result, item, index)) return

    // 处理其他操作
    switch (result) {
      case 'delete-item':
        await deleteClipboardItem(item.id)
        toast.success(i18n.t('common.deleted'), TOAST_CONFIG)
        break

      case 'clear-all':
        const confirmed = await showConfirm(
          i18n.t('contextMenu.clearAllConfirm'),
          i18n.t('contextMenu.clearAllConfirmTitle')
        )
        if (confirmed) {
          await clearClipboardHistory()
          await loadClipboardItems()
          toast.success(i18n.t('contextMenu.allCleared'), TOAST_CONFIG)
        }
        break
    }
  } catch (error) {
    console.error('处理菜单操作失败:', error)
    toast.error(i18n.t('common.operationFailed'), TOAST_CONFIG)
  }
}

// 显示收藏项的右键菜单
export async function showFavoriteItemContextMenu(event, item, index) {
  const menuItems = []
  const contentType = item.content_type || 'text'

  const pasteMenuItem = createPasteMenuItem(contentType, !!item.html_content)
  menuItems.push(pasteMenuItem)
  menuItems.push(createMenuItem('copy-item', i18n.t('contextMenu.copy'), { icon: 'ti ti-copy' }))
  menuItems.push(createSeparator())


  // 添加内容类型特定菜单项（图片、文件等）
  const contentMenuItems = createContentTypeMenuItems(contentType)
  if (contentMenuItems.length > 0) {
    menuItems.push(...contentMenuItems, createSeparator())
  }

  // 添加"移动到分组"菜单
  const groups = groupsStore.groups || []

  const moveToGroupItem = createMenuItem('move-to-group', i18n.t('contextMenu.moveToGroup'), { icon: 'ti ti-folder' })

  if (groups.length > 0) {
    moveToGroupItem.children = groups
      .filter(group => group.name !== item.group_name)
      .map(group =>
        createMenuItem(`move-to-group-${group.name}`, group.name, {
          icon: group.icon || 'ti ti-folder',
          iconColor: group.name === '全部' ? null : (group.color || '#dc2626')
        })
      )
  }

  // 添加通用菜单项
  menuItems.push(
    moveToGroupItem,
    createSeparator(),
    createMenuItem('delete-item', i18n.t('contextMenu.delete'), { icon: 'ti ti-trash' })
  )

  const result = await showContextMenuFromEvent(event, menuItems, { theme: settingsStore.theme })
  if (!result) return

  try {
    // 处理复制操作
    if (result === 'copy-item') {
      await copyFavoriteItem(item.id)
      toast.success(i18n.t('contextMenu.copied'), TOAST_CONFIG)
      return
    }

    // 处理粘贴操作
    if (await handlePasteActions(result, item, false, index)) return

    if (result === 'edit-text') {
      await openEditorForFavorite(item)
      return
    }

    // 处理移动到分组
    if (result.startsWith('move-to-group-')) {
      const groupName = result.substring(14)
      await moveFavoriteToGroup(item.id, groupName)
      await refreshFavorites()
      toast.success(i18n.t('contextMenu.movedToGroup'), TOAST_CONFIG)
      return
    }

    // 处理内容类型操作
    if (await handleContentTypeActions(result, item, index)) return

    // 处理删除操作
    if (result === 'delete-item') {
      const confirmed = await showConfirm(
        i18n.t('favorites.confirmDelete'),
        i18n.t('favorites.confirmDeleteTitle')
      )
      if (confirmed) {
        await deleteFavorite(item.id)
        await refreshFavorites()
        toast.success(i18n.t('common.deleted'), TOAST_CONFIG)
      }
    }
  } catch (error) {
    console.error('处理菜单操作失败:', error)
    toast.error(i18n.t('common.operationFailed'), TOAST_CONFIG)
  }
}
