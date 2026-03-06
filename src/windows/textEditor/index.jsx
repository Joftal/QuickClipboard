import { createRoot } from 'react-dom/client';
import App from './App';
import '@shared/i18n';
import '@shared/styles/uno';
import '@unocss/reset/tailwind.css';
import '@shared/styles/index.css';
createRoot(document.getElementById('root')).render(<App />);
