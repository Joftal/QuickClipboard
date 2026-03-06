import '@shared/styles/tabler-icons-woff2.css';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { useInputFocus } from '@shared/hooks/useInputFocus';
import ImageLibraryTab from './emoji/ImageLibraryTab';

function ImagesTab({ imageCategory }) {
  const { t } = useTranslation();
  const searchInputRef = useInputFocus();
  const [searchQuery, setSearchQuery] = useState('');

  return (
    <div className="h-full flex flex-col overflow-hidden">
      <div className="flex-shrink-0 p-2 border-b border-gray-200 dark:border-gray-700/50">
        <div className="relative">
          <i className="ti ti-search absolute left-2.5 top-1/2 -translate-y-1/2 text-gray-400 text-sm"></i>
          <input
            ref={searchInputRef}
            type="text"
            value={searchQuery}
            onChange={event => setSearchQuery(event.target.value)}
            placeholder={t('emoji.searchImagePlaceholder') || '搜索文件名...'}
            className="w-full h-8 pl-8 pr-8 text-sm bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-lg focus:outline-none focus:ring-1 focus:ring-blue-500 text-gray-900 dark:text-gray-100 placeholder-gray-400 dark:placeholder-gray-500"
          />
          {searchQuery && (
            <button
              type="button"
              onClick={() => setSearchQuery('')}
              className="absolute right-2 top-1/2 -translate-y-1/2 text-gray-400 hover:text-gray-600"
            >
              <i className="ti ti-x text-sm"></i>
            </button>
          )}
        </div>
      </div>

      <div className="flex-1 overflow-hidden">
        <ImageLibraryTab imageCategory={imageCategory} searchQuery={searchQuery} />
      </div>
    </div>
  );
}

export default ImagesTab;
