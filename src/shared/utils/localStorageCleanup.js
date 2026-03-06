import { LAYOUT_STORAGE_KEY } from '@shared/config/tools'

const SETTINGS_RESET_KEYS = [
  'fontSize',
  'rowHeight',
  'fileDisplayMode',
  'listStyle',
  'footerLeftRatio',
  'tool-state-format-toggle-button'
]

const APP_RESET_KEYS = [
  ...SETTINGS_RESET_KEYS,
  LAYOUT_STORAGE_KEY,
  'pinImageSettings'
]

const APP_RESET_PREFIXES = [
  'tool-state-'
]

function resolveStorage(storage) {
  return storage ?? globalThis?.localStorage ?? null
}

function collectKeysByPrefixes(storage, prefixes) {
  const matchedKeys = []

  for (let index = 0; index < storage.length; index += 1) {
    const key = storage.key(index)
    if (!key) {
      continue
    }

    if (prefixes.some(prefix => key.startsWith(prefix))) {
      matchedKeys.push(key)
    }
  }

  return matchedKeys
}

function removeLocalStorageEntries({ storage, keys = [], prefixes = [] }) {
  const targetStorage = resolveStorage(storage)
  if (!targetStorage) {
    return
  }

  const keySet = new Set(keys)
  collectKeysByPrefixes(targetStorage, prefixes).forEach(key => keySet.add(key))
  keySet.forEach(key => targetStorage.removeItem(key))
}

export function clearLocalStorageForSettingsReset(storage) {
  removeLocalStorageEntries({ storage, keys: SETTINGS_RESET_KEYS })
}

export function clearLocalStorageForAppReset(storage) {
  removeLocalStorageEntries({
    storage,
    keys: APP_RESET_KEYS,
    prefixes: APP_RESET_PREFIXES
  })
}

