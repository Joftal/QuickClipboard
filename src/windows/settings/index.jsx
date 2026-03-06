import ReactDOM from 'react-dom/client';
import '@shared/styles/uno';
import '@unocss/reset/tailwind.css';
import '@shared/styles/index.css';
import '@shared/i18n';
import { disposeStores, initStores } from '@shared/store';
import App from './App';
const FIRST_LOAD_KEY = 'app_first_load_done';
const isFirstLoad = !sessionStorage.getItem(FIRST_LOAD_KEY);
initStores().then(() => {
  if (import.meta.env.DEV && isFirstLoad) {
    sessionStorage.setItem(FIRST_LOAD_KEY, 'true');
    window.location.reload();
  }
  ReactDOM.createRoot(document.getElementById('root')).render(<App />);
});

window.addEventListener('beforeunload', disposeStores);

if (import.meta.hot) {
  import.meta.hot.dispose(() => {
    disposeStores();
  });
}
