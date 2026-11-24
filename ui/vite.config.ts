import { fileURLToPath } from 'node:url';
import { defineConfig, type Plugin } from 'vite';

// In a browser, `npm run dev` talks to `kerf-app --serve 127.0.0.1:8787`.
const api = process.env.KERF_API ?? 'http://127.0.0.1:8787';

// `VITE_TARGET=web` builds the static site: the engine runs in WebAssembly in the page
// (`scripts/build-web.sh` makes the module first). `BASE` is the URL path it is served under.
const web = process.env.VITE_TARGET === 'web';
const base = web ? (process.env.BASE ?? '/') : '/';

/** Other builds never load the wasm backend; its import resolves to a stub that refuses. */
const noWasm = (): Plugin => ({
  name: 'patchplay-no-wasm',
  enforce: 'pre',
  resolveId: (source, importer) =>
    source === './wasm-bridge' && importer?.endsWith('/web/index.ts') ? '\0no-wasm-bridge' : null,
  load: (id) =>
    id === '\0no-wasm-bridge'
      ? 'export const createWasmBridge = () => Promise.reject(new Error("This is not the web build."));'
      : null,
});

/** The static site's page: no mention of the CLI or agents, a manifest and a description. */
const webPage = (): Plugin => ({
  name: 'patchplay-web-page',
  apply: 'build',
  transformIndexHtml: (html) =>
    html
      .replace(/<span class="hint"[^>]*>GUI, CLI and agents send the same commands<\/span>/, '')
      .replace(
        '</head>',
        `  <meta name="description" content="patchplay: a modular synth you can read, in your browser. Free, nothing to install.">
  <meta name="theme-color" content="#1b1a17">
  <link rel="manifest" href="manifest.webmanifest">
  <link rel="apple-touch-icon" href="icon-512.png">
</head>`,
      ),
});

export default defineConfig({
  base,
  clearScreen: false,
  define: { __WEB__: JSON.stringify(web) },
  resolve: { alias: { '@kerf-web': fileURLToPath(new URL('./src/web/pkg', import.meta.url)) } },
  plugins: web ? [webPage()] : [noWasm()],
  server: {
    host: '127.0.0.1',
    port: 5173,
    strictPort: true,
    proxy: { '/api': api },
  },
  build: { target: 'es2022', outDir: web ? 'dist-web' : 'dist' },
  // The worklet is an ES module (`audioWorklet.addModule` loads it as one).
  worker: { format: 'es' },
  test: { include: ['src/**/*.test.ts'] },
});
