import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { AtlasWorkspace } from './components/AtlasWorkspace';
createRoot(document.getElementById('root')!).render(<StrictMode><AtlasWorkspace /></StrictMode>);
