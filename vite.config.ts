import { defineConfig } from "vite";

export default defineConfig({
  clearScreen: false,
  build: {
    minify: "terser",
    sourcemap: false,
    terserOptions: {
      compress: {
        passes: 2,
        drop_debugger: true,
        pure_funcs: ["console.log", "console.debug", "console.info"]
      },
      mangle: true,
      toplevel: true,
      format: {
        comments: false
      }
    },
    // Evita convertir cientos de banderas e insignias pequeñas a Base64 dentro
    // del JavaScript de standings. Como recursos separados solo se decodifican
    // las imágenes que realmente aparecen en pantalla.
    assetsInlineLimit: 0,
    rollupOptions: {
      input: {
        control: "index.html",
        browser: "browser.html",
        composite: "composite.html",
        delta: "delta.html",
        timing: "timing.html",
        driving: "driving.html",
        tires: "tires.html",
        damage: "damage.html",
        standings: "standings.html",
        relative: "relative.html",
        fuel: "fuel.html",
        pitstop: "pitstop.html",
        flags: "flags.html",
        rejoin: "rejoin.html",
        trackmap: "trackmap.html",
        forecast: "forecast.html",
        conditions: "conditions.html"
      }
    }
  },
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      ignored: ["**/src-tauri/**"]
    }
  }
});
