import React from 'react';
import ReactDOM from 'react-dom/client';
import '@shared/styles/uno';
import '@unocss/reset/tailwind.css';
import '@shared/styles/index.css';
import '@shared/i18n';
import { disposeStores, initStores } from '@shared/store';
import { initClipboardItems } from '@shared/store/clipboardStore';
import { initFavorites } from '@shared/store/favoritesStore';
import { loadGroups } from '@shared/store/groupsStore';

import App from './App';
initStores().then(() => {
  ReactDOM.createRoot(document.getElementById('root')).render(<React.StrictMode>
      <App />
    </React.StrictMode>);
  return Promise.all([initClipboardItems(), loadGroups().then(() => initFavorites())]);
});

window.addEventListener('beforeunload', disposeStores);

if (import.meta.hot) {
  import.meta.hot.dispose(() => {
    disposeStores();
  });
}
