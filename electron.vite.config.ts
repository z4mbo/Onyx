import { resolve } from 'path'
import { defineConfig, externalizeDepsPlugin } from 'electron-vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'

export default defineConfig({
  main: {
    plugins: [
      externalizeDepsPlugin({
        exclude: ['electron-store', 'ajv', 'ajv-formats']
      })
    ],
    resolve: {
      alias: {
        'ajv/dist/runtime/equal': resolve('node_modules/ajv/dist/runtime/equal.js'),
        'ajv/dist/runtime/ucs2length': resolve('node_modules/ajv/dist/runtime/ucs2length.js'),
        'ajv/dist/runtime/uri': resolve('node_modules/ajv/dist/runtime/uri.js'),
        'ajv/dist/runtime/validation_error': resolve('node_modules/ajv/dist/runtime/validation_error.js'),
        'ajv-formats/dist/formats': resolve('node_modules/ajv-formats/dist/formats.js')
      }
    },
    build: {
      rollupOptions: {
        external: ['node-pty', 'ws']
      }
    }
  },
  preload: {
    plugins: [externalizeDepsPlugin()]
  },
  renderer: {
    resolve: {
      alias: {
        '@': resolve('src/renderer')
      }
    },
    plugins: [react(), tailwindcss()]
  }
})
