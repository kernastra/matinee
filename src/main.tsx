import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import '@fontsource-variable/fraunces/wght.css';
import '@fontsource-variable/manrope/wght.css';
import '@fontsource/ibm-plex-mono/latin-400.css';
import '@fontsource/ibm-plex-mono/latin-500.css';
import '@fontsource/ibm-plex-mono/latin-600.css';
import '@fontsource/ibm-plex-mono/latin-700.css';
import App from './App';
import AppErrorBoundary from './components/AppErrorBoundary';
import { CustomPosterProvider } from './components/CustomPosterProvider';
import './styles.css';

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <AppErrorBoundary>
      <CustomPosterProvider>
        <App />
      </CustomPosterProvider>
    </AppErrorBoundary>
  </StrictMode>,
);
