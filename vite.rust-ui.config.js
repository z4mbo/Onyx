import { defineConfig } from "vite"
import tailwindcss from "@tailwindcss/vite"

export default defineConfig({
  base: "/static/",
  publicDir: false,
  plugins: [tailwindcss()],
  build: {
    outDir: "frontend-rs/static",
    emptyOutDir: true,
    minify: true,
    rollupOptions: {
      input: {
        styles: "frontend-rs/styles-entry.css",
        runtime: "frontend-rs/runtime.js",
      },
      output: {
        entryFileNames: "[name].js",
        assetFileNames: (asset) =>
          asset.name?.endsWith(".css")
            ? "onyx.css"
            : "assets/[name]-[hash][extname]",
      },
    },
  },
})
