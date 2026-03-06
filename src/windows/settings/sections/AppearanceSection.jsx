import '@tabler/icons-webfont/dist/tabler-icons.min.css';
import { useTranslation } from 'react-i18next';
import { useSnapshot } from 'valtio';
import { settingsStore } from '@shared/store/settingsStore';
import SettingsSection from '../components/SettingsSection';
import SettingItem from '../components/SettingItem';
import Toggle from '@shared/components/ui/Toggle';
import ThemeOption from '../components/ThemeOption';
function AppearanceSection({
  settings,
  onSettingChange
}) {
  const {
    t
  } = useTranslation();
  const {
    theme,
    darkThemeStyle
  } = useSnapshot(settingsStore);
  const themeOptions = [{
    id: 'light',
    label: t('settings.appearance.themeLight'),
    preview: 'linear-gradient(135deg, #f5f7fa 0%, #c3cfe2 100%)'
  }, {
    id: 'dark',
    label: t('settings.appearance.themeDark'),
    preview: 'linear-gradient(135deg, #2c3e50 0%, #000000 100%)'
  }];
  return <SettingsSection title={t('settings.appearance.title')} description={t('settings.appearance.description')}>
      <div className="space-y-6">
        <div>
          <label className="block text-sm font-medium text-gray-900 dark:text-white mb-3">
            {t('settings.appearance.themeSelect')}
          </label>
          <p className="text-xs text-gray-500 dark:text-gray-400 mb-4">
            {t('settings.appearance.themeSelectDesc')}
          </p>
          
          <div className="grid w-full max-w-[440px] grid-cols-2 gap-3 mx-auto">
            {themeOptions.map(option => <ThemeOption key={option.id} option={option} isActive={theme === option.id} onClick={() => settingsStore.setTheme(option.id)} />)}
          </div>
        </div>

        {theme === 'dark' && <div className="animate-slide-in-left-fast">
            <label className="block text-sm font-medium text-gray-900 dark:text-white mb-3">
              {t('settings.appearance.darkThemeStyle') || '暗色风格'}
            </label>
            <p className="text-xs text-gray-500 dark:text-gray-400 mb-4">
              {t('settings.appearance.darkThemeStyleDesc') || '选择暗色主题的显示风格'}
            </p>
            
            <div className="grid grid-cols-2 gap-3">
              <button onClick={() => onSettingChange('darkThemeStyle', 'modern')} className={`
                  flex flex-col items-start gap-2 p-4 rounded-lg border-2 
                  transition-all duration-300 
                  focus:outline-none active:scale-95
                  ${darkThemeStyle === 'modern' ? 'border-blue-500 bg-blue-50 dark:bg-blue-900/20 scale-102 shadow-lg shadow-blue-500/20' : 'border-gray-200 dark:border-gray-700 hover:border-gray-300 dark:hover:border-gray-600 hover:scale-101 hover:shadow-md'}
                `}>
                <div className="w-full">
                  <div className="text-sm font-semibold text-gray-900 dark:text-white mb-1">
                    {t('settings.appearance.darkThemeModern') || '现代风格'}
                  </div>
                  <div className="text-xs text-gray-600 dark:text-gray-400">
                    {t('settings.appearance.darkThemeModernDesc') || '色彩丰富的现代暗色主题'}
                  </div>
                </div>
              </button>

              <button onClick={() => onSettingChange('darkThemeStyle', 'classic')} className={`
                  flex flex-col items-start gap-2 p-4 rounded-lg border-2 
                  transition-all duration-300 
                  focus:outline-none active:scale-95
                  ${darkThemeStyle === 'classic' ? 'border-blue-500 bg-blue-50 dark:bg-blue-900/20 scale-102 shadow-lg shadow-blue-500/20' : 'border-gray-200 dark:border-gray-700 hover:border-gray-300 dark:hover:border-gray-600 hover:scale-101 hover:shadow-md'}
                `}>
                <div className="w-full">
                  <div className="text-sm font-semibold text-gray-900 dark:text-white mb-1">
                    {t('settings.appearance.darkThemeClassic') || '经典风格'}
                  </div>
                  <div className="text-xs text-gray-600 dark:text-gray-400">
                    {t('settings.appearance.darkThemeClassicDesc') || '低调优雅的灰色暗色主题'}
                  </div>
                </div>
              </button>
            </div>
          </div>}

        <div>
          <SettingItem label={t('settings.appearance.clipboardAnimation')} description={t('settings.appearance.clipboardAnimationDesc')}>
            <Toggle checked={settings.clipboardAnimationEnabled} onChange={checked => onSettingChange('clipboardAnimationEnabled', checked)} />
          </SettingItem>

          <SettingItem label={t('settings.appearance.uiAnimation')} description={t('settings.appearance.uiAnimationDesc')}>
            <Toggle checked={settings.uiAnimationEnabled} onChange={checked => onSettingChange('uiAnimationEnabled', checked)} />
          </SettingItem>
        </div>
      </div>
    </SettingsSection>;
}
export default AppearanceSection;
