import { createReadStream, statSync } from "node:fs";
import { extname, resolve, sep } from "node:path";
import react from "@vitejs/plugin-react";
import type { Plugin, ProxyOptions } from "vite";
import { defineConfig } from "vitest/config";

/**
 * Dev server for the web client. `/api`, `/media`, `/out` and `/healthz` are proxied to the Rust
 * content server (docs/api.md), `ENCARTA_SERVER` (default http://127.0.0.1:8080).
 *
 * `ENCARTA_DEV_OUT=1` instead serves the converter output `../out` under `/out/` from Vite itself,
 * so the tours dev page works without the Rust server. Nothing from `out/` is bundled.
 */
const outDir = resolve(__dirname, "../out");
const server = process.env.ENCARTA_SERVER ?? "http://127.0.0.1:8080";
const serveOutLocally = process.env.ENCARTA_DEV_OUT === "1";

const mimeTypes: Record<string, string> = {
  ".json": "application/json",
  ".glb": "model/gltf-binary",
  ".bin": "application/octet-stream",
  ".png": "image/png",
  ".jpg": "image/jpeg",
  ".jpeg": "image/jpeg",
  ".gif": "image/gif",
};

function serveOut(): Plugin {
  return {
    name: "encarta-serve-out",
    configureServer(devServer) {
      if (!serveOutLocally) return;
      devServer.middlewares.use("/out", (req, res, next) => {
        const url = decodeURIComponent((req.url ?? "/").split("?")[0] ?? "/");
        const file = resolve(outDir, "." + url);
        if (file !== outDir && !file.startsWith(outDir + sep)) {
          res.statusCode = 403;
          res.end();
          return;
        }
        let size: number;
        try {
          const st = statSync(file);
          if (!st.isFile()) return next();
          size = st.size;
        } catch {
          res.statusCode = 404;
          res.end("not found");
          return;
        }
        res.setHeader("Content-Type", mimeTypes[extname(file).toLowerCase()] ?? "application/octet-stream");
        res.setHeader("Content-Length", String(size));
        res.setHeader("Cache-Control", "no-cache");
        createReadStream(file).pipe(res);
      });
    },
  };
}

/**
 * Vite's dev server would answer `/tours` and `/atlas` with the `tours.html` / `atlas.html` dev
 * pages; the SPA owns those routes (the Rust server's index.html fallback does the same in production).
 */
function spaRoutes(): Plugin {
  return {
    name: "encarta-spa-routes",
    configureServer(devServer) {
      devServer.middlewares.use((req, _res, next) => {
        const path = (req.url ?? "").split("?")[0] ?? "";
        if (/^\/(tours|atlas)\/?$/.test(path)) req.url = "/index.html";
        next();
      });
    },
  };
}

const proxied: Record<string, ProxyOptions> = {};
for (const path of ["/api", "/media", "/healthz", ...(serveOutLocally ? [] : ["/out"])]) {
  proxied[path] = { target: server, changeOrigin: false };
}

export default defineConfig({
  plugins: [react(), spaRoutes(), serveOut()],
  server: { port: 5173, strictPort: false, proxy: proxied },
  preview: { proxy: proxied },
  build: {
    target: "es2022",
    // The lazily loaded atlas chunk (MapLibre) is about 1.1 MB.
    chunkSizeWarningLimit: 1400,
    rollupOptions: {
      input: {
        index: resolve(__dirname, "index.html"),
        tours: resolve(__dirname, "tours.html"),
      },
    },
  },
  test: {
    include: ["src/**/*.test.ts", "src/**/*.test.tsx"],
    environment: "node",
  },
});
