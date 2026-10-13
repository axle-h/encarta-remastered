# web

## Web client

React 18 + Vite + TypeScript single-page app for the converted encyclopedia. Design and decisions:
[docs/web-client.md](../docs/web-client.md); the API it calls: [docs/api.md](../docs/api.md).

```sh
npm install            # pinned versions
npm run dev            # http://localhost:5173/, proxies /api /media /out /healthz to ENCARTA_SERVER (default http://127.0.0.1:8080)
npm test               # vitest (synthetic data only)
npm run build          # tsc --noEmit + vite build → dist/ (serve with encarta-server --static web/dist)
```

Start the content server first (`encarta-server --out out`, see docs/api.md). Dev pages: `/tours.html`
(3D tour runtime, docs/tour-runtime.md; also built) and `/atlas.html` (atlas; dev server only). `ENCARTA_DEV_OUT=1 npm run dev` serves
`../out` from Vite for the tour dev page without the Rust server.
