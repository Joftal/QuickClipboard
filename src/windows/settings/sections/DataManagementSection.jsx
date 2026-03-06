import '@shared/styles/tabler-icons-woff2.css';
import { useTranslation } from 'react-i18next';
import { useEffect, useState } from 'react';
import { createPortal } from 'react-dom';
import SettingsSection from '../components/SettingsSection';
import SettingItem from '../components/SettingItem';
import Button from '@shared/components/ui/Button';
import { open } from '@tauri-apps/plugin-dialog';
import { openPath } from '@tauri-apps/plugin-opener';
import { getCurrentStoragePath, getDefaultStoragePath, changeStoragePath, resetStoragePathToDefault, resetAllData, checkTargetHasData } from '@shared/api/dataManagement';
import { showError, showMessage, showConfirm } from '@shared/utils/dialog';
import { reloadAllWindows } from '@shared/api/window';
import { resetSettingsToDefault } from '@shared/api/settings';
import { isPortableMode } from '@shared/api/system';
import { clearClipboardHistory } from '@shared/api/clipboard';
import { clearLocalStorageForAppReset, clearLocalStorageForSettingsReset } from '@shared/utils/localStorageCleanup';
function DataManagementSection() {
  const {
    t
  } = useTranslation();
  const [storagePath, setStoragePath] = useState(t('common.loading'));
  const [portable, setPortable] = useState(false);
  const [busy, setBusy] = useState(false);
  const [busyText, setBusyText] = useState('');
  const [migrationDialog, setMigrationDialog] = useState(null); // { type: 'change' | 'reset', targetPath?: string, targetInfo?: object }

  const warnNonCriticalError = (action, error) => {
    console.warn(`${action}失败:`, error)
  }

  const reloadAllWindowsSafely = async (action) => {
    try {
      await reloadAllWindows()
    } catch (error) {
      warnNonCriticalError(action, error)
    }
  }

  const runLocalStorageCleanupSafely = (action, cleanup) => {
    try {
      cleanup(window.localStorage)
    } catch (error) {
      warnNonCriticalError(action, error)
    }
  }

  const formatSize = (bytes) => {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  };

  useEffect(() => {
    (async () => {
      try {
        const path = await getCurrentStoragePath();
        setStoragePath(path);
      } catch (e) {
        setStoragePath(t('common.loadError'));
      }
      try {
        const p = await isPortableMode();
        setPortable(!!p);
      } catch (error) {
        warnNonCriticalError('检查便携模式', error)
      }
    })();
  }, []);

  const handleOpenStorageFolder = async () => {
    try {
      if (storagePath && typeof storagePath === 'string') {
        await openPath(storagePath);
      }
    } catch (e) {
      await showError(t('settings.dataManagement.openFolderFailed', { defaultValue: `打开目录失败: ${e?.message || e}` }))
    }
  };

  const handleChangeStorageLocation = async () => {
    try {
      const dir = await open({ directory: true, multiple: false });
      if (!dir) return;

      // 检测目标位置是否有数据
      const targetInfo = await checkTargetHasData(dir);

      if (targetInfo.has_data) {
        setMigrationDialog({ type: 'change', targetPath: dir, targetInfo });
        return;
      }

      setBusyText(t('settings.dataManagement.overlayMigrating'));
      setBusy(true);
      await changeStoragePath(dir, 'source_only');
      const latest = await getCurrentStoragePath();
      setStoragePath(latest);
      await showMessage(t('settings.dataManagement.updateSuccess'));
      await reloadAllWindowsSafely('更改存储位置后刷新窗口')
    } catch (e) {
      await showError(t('settings.dataManagement.changeFailed', { message: e?.message || e }));
    }
    finally {
      setBusy(false);
      setBusyText('');
    }
  };

  const handleMigrationModeSelect = async (mode) => {
    const dialog = migrationDialog;
    setMigrationDialog(null);

    if (!dialog) return;

    try {
      setBusyText(t('settings.dataManagement.overlayMigrating'));
      setBusy(true);

      if (dialog.type === 'change') {
        await changeStoragePath(dialog.targetPath, mode);
      } else if (dialog.type === 'reset') {
        await resetStoragePathToDefault(mode);
      }

      const latest = await getCurrentStoragePath();
      setStoragePath(latest);
      await showMessage(dialog.type === 'change'
        ? t('settings.dataManagement.updateSuccess')
        : t('settings.dataManagement.resetSuccess'));
      await reloadAllWindowsSafely('迁移存储位置后刷新窗口')
    } catch (e) {
      const errorKey = dialog.type === 'change'
        ? 'settings.dataManagement.changeFailed'
        : 'settings.dataManagement.resetFailed';
      await showError(t(errorKey, { message: e?.message || e }));
    } finally {
      setBusy(false);
      setBusyText('');
    }
  };

  const handleResetStorageLocation = async () => {
    try {
      const defaultPath = await getDefaultStoragePath();
      const currentPath = await getCurrentStoragePath();

      if (currentPath === defaultPath) {
        await showMessage(t('settings.dataManagement.alreadyDefault'));
        return;
      }

      const targetInfo = await checkTargetHasData(defaultPath);

      if (targetInfo.has_data) {
        setMigrationDialog({ type: 'reset', targetPath: defaultPath, targetInfo });
        return;
      }

      setBusyText(t('settings.dataManagement.overlayMigrating'));
      setBusy(true);
      await resetStoragePathToDefault('source_only');
      const latest = await getCurrentStoragePath();
      setStoragePath(latest);
      await showMessage(t('settings.dataManagement.resetSuccess'));
      await reloadAllWindowsSafely('重置存储位置后刷新窗口')
    } catch (e) {
      await showError(t('settings.dataManagement.resetFailed', { message: e?.message || e }));
    }
    finally {
      setBusy(false);
      setBusyText('');
    }
  };

  const handleClearHistory = async () => {
    const ok = await showConfirm(t('settings.dataManagement.clearConfirm'));
    if (!ok) return;
    try {
      setBusyText(t('settings.dataManagement.overlayCleaning') || t('settings.dataManagement.overlayMigrating'));
      setBusy(true);
      await clearClipboardHistory();
      await showMessage(t('settings.dataManagement.clearSuccess') || t('common.success'));
      await reloadAllWindowsSafely('清空历史后刷新窗口')
    } catch (e) {
      await showError(t('settings.dataManagement.clearFailed', { message: e?.message || e }) || String(e));
    } finally {
      setBusy(false);
      setBusyText('');
    }
  };

  const handleResetSettings = async () => {
    const ok = await showConfirm(t('settings.dataManagement.resetConfirm'));
    if (!ok) return;
    try {
      setBusyText(t('settings.dataManagement.overlayResetSettings') || t('settings.dataManagement.overlayMigrating'));
      setBusy(true);
      await resetSettingsToDefault();
      runLocalStorageCleanupSafely('重置设置时清理本地缓存', clearLocalStorageForSettingsReset)
      await showMessage(t('settings.dataManagement.resetSettingsSuccess') || t('common.success'));
      await reloadAllWindowsSafely('重置设置后刷新窗口')
    } catch (e) {
      await showError(t('settings.dataManagement.resetSettingsFailed', { message: e?.message || e }) || String(e));
    } finally {
      setBusy(false);
      setBusyText('');
    }
  };

  const handleResetAllData = async () => {
    const ok = await showConfirm(t('settings.dataManagement.resetAllConfirm'));
    if (!ok) return;
    try {
      setBusyText(t('settings.dataManagement.overlayResetAll') || t('settings.dataManagement.overlayMigrating'));
      setBusy(true);
      const dir = await resetAllData();
      runLocalStorageCleanupSafely('重置全部数据时清理本地缓存', clearLocalStorageForAppReset)
      const latest = await getCurrentStoragePath();
      setStoragePath(latest);
      await showMessage(t('settings.dataManagement.resetAllSuccess', { path: dir }) || t('common.success'));
      await reloadAllWindowsSafely('重置全部数据后刷新窗口')
    } catch (e) {
      await showError(t('settings.dataManagement.resetAllFailed', { message: e?.message || e }) || String(e));
    } finally {
      setBusy(false);
      setBusyText('');
    }
  };

  return (
    <>
      {/* 数据存储位置 */}
      <SettingsSection title={t('settings.dataManagement.storageTitle')} description={t('settings.dataManagement.storageDesc')}>
        <SettingItem label={t('settings.dataManagement.currentPath')} description={storagePath}>
          <Button onClick={handleOpenStorageFolder} disabled={busy} variant="secondary" icon={<i className="ti ti-folder-open"></i>}>
            {t('settings.dataManagement.openFolder')}
          </Button>
        </SettingItem>

        <SettingItem label={t('settings.dataManagement.changePath')} description={t('settings.dataManagement.changePathDesc')}>
          <Button onClick={handleChangeStorageLocation} disabled={busy || portable} variant="primary" icon={<i className="ti ti-folder-plus"></i>}>
            {t('settings.dataManagement.selectNewPath')}
          </Button>
        </SettingItem>

        <SettingItem label={t('settings.dataManagement.resetPath')} description={t('settings.dataManagement.resetPathDesc')}>
          <Button onClick={handleResetStorageLocation} disabled={busy || portable} variant="secondary" icon={<i className="ti ti-home"></i>}>
            {t('settings.dataManagement.resetPathButton')}
          </Button>
        </SettingItem>
      </SettingsSection>

      {/* 数据清理 */}
      <SettingsSection title={t('settings.dataManagement.cleanupTitle')} description={t('settings.dataManagement.cleanupDesc')}>
        <SettingItem label={t('settings.dataManagement.clearHistory')} description={t('settings.dataManagement.clearHistoryDesc')}>
          <Button onClick={handleClearHistory} variant="danger" icon={<i className="ti ti-trash"></i>}>
            {t('settings.dataManagement.clearButton')}
          </Button>
        </SettingItem>

        <SettingItem label={t('settings.dataManagement.resetSettings')} description={t('settings.dataManagement.resetSettingsDesc')}>
          <Button onClick={handleResetSettings} variant="danger" icon={<i className="ti ti-restore"></i>}>
            {t('settings.dataManagement.resetButton')}
          </Button>
        </SettingItem>

        <SettingItem label={t('settings.dataManagement.resetAll')} description={t('settings.dataManagement.resetAllDesc')}>
          <Button onClick={handleResetAllData} variant="danger" icon={<i className="ti ti-refresh"></i>}>
            {t('settings.dataManagement.resetAllButton')}
          </Button>
        </SettingItem>
      </SettingsSection>

      {busy && createPortal(
        <div className="fixed inset-0 z-[9999] bg-black/40 backdrop-blur-sm flex items-center justify-center">
          <div className="bg-white dark:bg-gray-800 rounded-xl p-6 shadow-xl flex items-center gap-3">
            <div className="w-5 h-5 border-2 border-blue-500 border-t-transparent rounded-full animate-spin" />
            <div className="text-sm text-gray-700 dark:text-gray-200">{busyText || t('settings.dataManagement.overlayMigrating')}</div>
          </div>
        </div>,
        document.body
      )}

      {/* 迁移模式选择对话框 */}
      {migrationDialog && createPortal(
        <div className="fixed inset-0 z-[9999] bg-black/40 backdrop-blur-sm flex items-center justify-center">
          <div className="bg-white dark:bg-gray-800 rounded-xl p-6 shadow-xl max-w-md w-full mx-4">
            <div className="flex items-center gap-3 mb-4">
              <div className="w-10 h-10 rounded-full bg-amber-100 dark:bg-amber-900/30 flex items-center justify-center">
                <i className="ti ti-alert-triangle text-amber-600 dark:text-amber-400 text-xl"></i>
              </div>
              <div>
                <h3 className="text-lg font-semibold text-gray-900 dark:text-white">
                  {t('settings.dataManagement.migrationConflictTitle')}
                </h3>
              </div>
            </div>
            
            <p className="text-sm text-gray-600 dark:text-gray-400 mb-4">
              {t('settings.dataManagement.migrationConflictDesc')}
            </p>
            
            {migrationDialog.targetInfo && (
              <div className="bg-gray-100 dark:bg-gray-700/50 rounded-lg p-3 mb-4 text-sm">
                <div className="flex items-center gap-2 text-gray-700 dark:text-gray-300">
                  <i className="ti ti-database"></i>
                  <span>{t('settings.dataManagement.targetHasDatabase')}: {migrationDialog.targetInfo.has_database ? t('common.confirm') : '-'}</span>
                  {migrationDialog.targetInfo.has_database && (
                    <span className="text-gray-500">({formatSize(migrationDialog.targetInfo.database_size)})</span>
                  )}
                </div>
                <div className="flex items-center gap-2 text-gray-700 dark:text-gray-300 mt-1">
                  <i className="ti ti-photo"></i>
                  <span>{t('settings.dataManagement.targetHasImages')}: {migrationDialog.targetInfo.images_count} {t('settings.dataManagement.imagesCount')}</span>
                  {migrationDialog.targetInfo.images_count > 0 && (
                    <span className="text-gray-500">({formatSize(migrationDialog.targetInfo.images_size)})</span>
                  )}
                </div>
                {(migrationDialog.targetInfo.has_image_library || migrationDialog.targetInfo.image_library_count > 0) && (
                  <div className="flex items-center gap-2 text-gray-700 dark:text-gray-300 mt-1">
                    <i className="ti ti-photo-star"></i>
                    <span>{t('settings.dataManagement.targetHasImageLibrary')}: {migrationDialog.targetInfo.image_library_count} {t('settings.dataManagement.imagesCount')}</span>
                    {migrationDialog.targetInfo.image_library_count > 0 && (
                      <span className="text-gray-500">({formatSize(migrationDialog.targetInfo.image_library_size)})</span>
                    )}
                  </div>
                )}
              </div>
            )}
            
            <div className="flex flex-col gap-2">
              <button
                onClick={() => handleMigrationModeSelect('source_only')}
                className="flex items-start gap-3 p-3 border border-gray-200 dark:border-gray-600 rounded-lg hover:bg-gray-50 dark:hover:bg-gray-700 transition-colors text-left"
              >
                <i className="ti ti-replace text-blue-500 mt-0.5"></i>
                <div className="flex-1">
                  <div className="font-medium text-gray-900 dark:text-white">
                    {t('settings.dataManagement.migrationSourceOnly')}
                  </div>
                  <div className="text-xs text-gray-500 dark:text-gray-400 mt-0.5">
                    {t('settings.dataManagement.migrationSourceOnlyDesc')}
                  </div>
                </div>
              </button>
              
              <button
                onClick={() => handleMigrationModeSelect('target_only')}
                className="flex items-start gap-3 p-3 border border-gray-200 dark:border-gray-600 rounded-lg hover:bg-gray-50 dark:hover:bg-gray-700 transition-colors text-left"
              >
                <i className="ti ti-file-check text-green-500 mt-0.5"></i>
                <div className="flex-1">
                  <div className="font-medium text-gray-900 dark:text-white">
                    {t('settings.dataManagement.migrationTargetOnly')}
                  </div>
                  <div className="text-xs text-gray-500 dark:text-gray-400 mt-0.5">
                    {t('settings.dataManagement.migrationTargetOnlyDesc')}
                  </div>
                </div>
              </button>
              
              <button
                onClick={() => handleMigrationModeSelect('merge')}
                className="flex items-start gap-3 p-3 border border-gray-200 dark:border-gray-600 rounded-lg hover:bg-gray-50 dark:hover:bg-gray-700 transition-colors text-left"
              >
                <i className="ti ti-git-merge text-purple-500 mt-0.5"></i>
                <div className="flex-1">
                  <div className="font-medium text-gray-900 dark:text-white">
                    {t('settings.dataManagement.migrationMerge')}
                  </div>
                  <div className="text-xs text-gray-500 dark:text-gray-400 mt-0.5">
                    {t('settings.dataManagement.migrationMergeDesc')}
                  </div>
                </div>
              </button>
            </div>
            
            <div className="mt-4 pt-4 border-t border-gray-200 dark:border-gray-700 flex justify-end">
              <button
                onClick={() => setMigrationDialog(null)}
                className="px-4 py-2 text-sm text-gray-600 dark:text-gray-400 hover:text-gray-900 dark:hover:text-white transition-colors"
              >
                {t('common.cancel')}
              </button>
            </div>
          </div>
        </div>,
        document.body
      )}

    </>
  );
}

export default DataManagementSection;
