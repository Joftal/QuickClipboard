import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import UnoCSS from 'unocss/vite'
import { resolve } from 'path'
import wasm from 'vite-plugin-wasm'

const isDev = process.env.NODE_ENV === 'development'
const isTauriDebug = process.env.TAURI_DEBUG === 'true'

function resolveVendorChunk(id) {
  const normalizedId = id.replace(/\\/g, '/')

  if (normalizedId.includes('/node_modules/@codemirror/')) return 'vendor-editor'
  if (normalizedId.includes('/node_modules/@dnd-kit/')) return 'vendor-dnd'
  if (normalizedId.includes('/node_modules/react-virtuoso/')) return 'vendor-list'
  if (normalizedId.includes('/node_modules/i18next/') ||
      normalizedId.includes('/node_modules/react-i18next/') ||
      normalizedId.includes('/node_modules/i18next-browser-languagedetector/')) return 'vendor-i18n'
  if (normalizedId.includes('/node_modules/react/') ||
      normalizedId.includes('/node_modules/react-dom/')) return 'vendor-react'
  if (normalizedId.includes('/node_modules/valtio/')) return 'vendor-state'
  if (normalizedId.includes('/node_modules/@tauri-apps/')) return 'vendor-tauri'
  if (normalizedId.includes('/node_modules/dompurify/')) return 'vendor-sanitize'

  return 'vendor'
}

export default defineConfig({
  root: 'src',
  clearScreen: false,

  server: {
    port: 1421,
    strictPort: true,
    fs: {
      allow: [
        resolve(__dirname, '.'),
        resolve(__dirname, 'node_modules'),
        resolve(__dirname, 'src'),
      ],
    },
  },

  envPrefix: ['VITE_', 'TAURI_'],

  resolve: {
    alias: {
      '@': resolve(__dirname, 'src'),
      '@shared': resolve(__dirname, 'src/shared'),
      '@windows': resolve(__dirname, 'src/windows'),
      'uno.css': 'virtual:uno.css',
    },
  },

  plugins: [
    UnoCSS({
      mode: 'global',
      inspector: false,
    }),
    react({
      babel: {
        plugins: [
          ['babel-plugin-react-compiler', {}],
        ],
      },
    }),
    wasm(),
  ],

  build: {
    outDir: '../dist',
    emptyOutDir: true,
    target: process.env.TAURI_PLATFORM === 'windows'
      ? 'chrome105'
      : 'safari16',

    minify: isDev || isTauriDebug ? false : 'esbuild',

    esbuild: isDev || isTauriDebug
      ? {}
      : {
          drop: ['debugger'],
          pure: ['console.log', 'console.info', 'console.debug'],
        },

    sourcemap: isDev || isTauriDebug,
    cssCodeSplit: true,

    rollupOptions: {
      input: (() => {
        return {
          main: resolve(__dirname, 'src/windows/main/index.html'),
          settings: resolve(__dirname, 'src/windows/settings/index.html'),
          quickpaste: resolve(__dirname, 'src/windows/quickpaste/index.html'),
          textEditor: resolve(__dirname, 'src/windows/textEditor/index.html'),
          contextMenu: resolve(__dirname, 'src/plugins/context_menu/contextMenu.html'),
          inputDialog: resolve(__dirname, 'src/plugins/input_dialog/inputDialog.html'),
          pinImage: resolve(__dirname, 'src/windows/pinImage/pinImage.html'),
        }
      })(),

      output: {
        assetFileNames: 'assets/[name]-[hash][extname]',
        chunkFileNames: 'js/[name]-[hash].js',
        entryFileNames: 'js/[name]-[hash].js',

        manualChunks(id) {
          if (id.includes('node_modules')) return resolveVendorChunk(id)
          if (id.includes('/shared/') || id.includes('\\shared\\')) return 'shared'
          return undefined
        },
      },
    },
  },
})
