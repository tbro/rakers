# rakers

A CLI that renders JavaScript into HTML. Give it an HTML file, a URL, or a bare JS script and it returns the post-execution HTML — including content rendered by React, Vue, and other JS frameworks.

Built on [html5ever](https://github.com/servo/html5ever) (Servo's HTML5 parser) with a choice of JS engine: [QuickJS](https://bellard.org/quickjs/) via [rquickjs](https://github.com/DelSkayn/rquickjs) (default) or [boa_engine](https://github.com/boa-dev/boa) (pure-Rust, no C compiler required).

## Install

```sh
# Default build (QuickJS engine — requires a C compiler)
cargo install --path .

# boa engine (pure Rust, no C compiler required)
cargo install --path . --no-default-features --features boa
```

## Usage

```sh
rakers [OPTIONS] [INPUT]
```

`INPUT` is a file path, an `http`/`https` URL, or omit to read from stdin.

| Input type | Example |
|------------|---------|
| URL | `rakers https://example.com` |
| HTML file | `rakers page.html` |
| JS file | `rakers script.js` |
| stdin | `echo '<script>document.write("hi")</script>' \| rakers` |

By default output goes to stdout. Use `-o` to write to a file:

```sh
rakers https://example.com -o rendered.html
```

### Options

| Flag | Description |
|------|-------------|
| `-o FILE` | Write output to FILE instead of stdout |
| `-A UA` | Set the `User-Agent` header for all HTTP requests |
| `-H "Name: Value"` | Add a custom request header (repeatable) |
| `--clean` | Strip `<script>` elements and unwrap `<noscript>` — see [Clean mode](#clean-mode) |

## How it works

1. Fetches the page (or reads from file/stdin)
2. Parses HTML with **html5ever** into a DOM tree
3. Collects `<script>` tags — inline and external (`src="..."`) — and fetches any external scripts
   - External scripts that open with `import`/`export` (ES module files requiring a module loader) are automatically skipped; self-contained bundles tagged `type="module"` still execute
   - Cloudflare Rocket Loader (`type="<hash>-text/javascript"`) is recognized and executed
4. Executes all scripts in order in a sandboxed JS context with browser globals stubbed out
5. Flushes any deferred callbacks (`setTimeout`, `requestAnimationFrame`, `MessageChannel`, `queueMicrotask`) so async-rendered frameworks have a chance to run
6. Reads back `document.body.innerHTML` and serializes the final HTML
   - Large server-rendered bodies (SSR sites) are preserved when the JS-rendered body is substantially smaller, avoiding measurement/analytics divs from clobbering real content

`.js` files are automatically wrapped in a minimal HTML document before processing.

`console.log`, `console.warn`, and `console.error` print to stderr with a `[console]` prefix.
Script errors are non-fatal — execution continues with the next script.

## Clean mode

`--clean` applies a post-processing pass that produces a static, crawlable snapshot — similar to what prerendering services (Prerender.io, rendertron) deliver to search-engine bots:

- Removes all `<script>` elements (inline and external)
- Removes `<link rel="modulepreload">` and `<link rel="preload" as="script">`
- Unwraps `<noscript>` — strips the tags but keeps the inner content, so crawlers see any fallback markup (meta redirects, image links, etc.)

```sh
rakers --clean https://example.com -o static.html
```

The output is self-contained HTML with no executable code — safe to serve directly to crawlers or store as a static snapshot.

## JS engine choice

rakers supports two JS engines selectable at compile time.

| | rquickjs (default) | boa |
|--|-------------------|-----|
| **Build deps** | Requires a C compiler | Pure Rust, no C compiler |
| **ES standard** | ES2023 | ES2021 (partial) |
| **Real-world bundles** | Good | Limited — may stack-overflow on large bundles |
| **React / Vue SPAs** | Works | Often hits stack limits |
| **When to use** | Real-world sites (default) | CI without C toolchain |

### Building

```sh
# rquickjs (default — recommended)
cargo build
cargo install --path .

# boa (pure Rust, no C compiler needed)
cargo build --no-default-features --features boa
cargo install --path . --no-default-features --features boa
```

Only one engine can be enabled at a time; the build will fail with a clear
error if both or neither are selected.

### Running tests

Unit tests run with either engine:

```sh
cargo test                                       # rquickjs (default)
cargo test --no-default-features --features boa  # boa
```

Integration tests that fetch real SPAs require rquickjs (boa overflows the
native stack on large React/Rocket Loader bundles):

```sh
cargo test --test integration
```

## Browser environment

The following globals are stubbed so typical JS bundles run without errors:

- **`document`** — `createElement`, `getElementById`, `querySelector`, `body`, `head`, `currentScript`, and the full DOM manipulation API (`appendChild`, `insertBefore`, `setAttribute`, `innerHTML`, etc.)
- **`window`** — `location` (with `toString()`), `navigator`, `history`, `screen`, `performance`, `localStorage`, `sessionStorage`, `matchMedia`, `getComputedStyle`, and all standard event/observer constructors
- **`URL` / `URLSearchParams`** — relative URL resolution against the page URL; `searchParams` with full `get`/`set`/`has`
- **`fetch` / `XMLHttpRequest`** — stubbed as no-ops (network requests from JS are not made)
- **`DOMException` / `customElements`** — Web Components registry and DOM exception constructor
- **`process`** — Node.js-style globals for webpack/Vite bundler compatibility
- **Timers** — `setTimeout`, `setInterval`, `requestAnimationFrame`, `queueMicrotask`, and `MessageChannel` callbacks are collected and flushed after scripts finish

## Comparison

| | rakers | headless Chrome | Playwright / Puppeteer | Splash |
|--|--------|-----------------|------------------------|--------|
| **JS compatibility** | Good (QuickJS / ES2023) | Full | Full | Full (WebKit) |
| **Requires browser** | No | Yes | Yes | Yes (via Docker) |
| **Startup time** | ~10 ms | ~1–2 s | ~1–2 s | ~500 ms |
| **Memory** | ~10 MB | ~150–300 MB | ~150–300 MB | ~200 MB |
| **Network calls from JS** | No (stubbed) | Yes | Yes | Yes |
| **CSS / layout** | No | Yes | Yes | Yes |
| **Embeddable as library** | Yes (Rust crate) | No | No | No |
| **Installation** | Single binary | Chrome + chromedriver | Browser + Node | Docker image |
| **Language** | Rust | Any | JS / many bindings | Python / Lua |

**When to use rakers** — fast HTML extraction in a scraping pipeline, CI environments without a browser, embedding in a Rust service, or anywhere startup latency and memory footprint matter more than pixel-perfect rendering.

**When to use a headless browser** — pages that rely on CSS-driven layout, canvas, WebGL, WebSockets, or JavaScript that makes authenticated network requests during render.

## Demo

[TodoMVC React](https://todomvc.com/examples/react/dist/) is the canonical demo. The server returns a 645-byte skeleton:

```html
<section class="todoapp" id="root"></section>
```

rakers executes the React bundle and returns the fully rendered app:

```html
<div id="root">
  <header class="header">
    <h1>todos</h1>
    <div class="input-container">
      <input class="new-todo" type="text">
      ...
    </div>
  </header>
  ...
</div>
```

## Compatibility

Tested against real-world sites with rquickjs:

| Site | Framework | Result |
|------|-----------|--------|
| react.dev | Next.js (SSR) | ✓ no errors |
| svelte.dev | SvelteKit (SSR) | ✓ no errors |
| vuejs.org | Vite (SSR) | ✓ no errors |
| tailwindcss.com | Next.js (SSR) | ✓ no errors |
| remix.run | Remix (SSR) | ✓ no errors |
| jsbench.me | React SPA | ✓ full render |
| todomvc.com/examples/react | React SPA | ✓ full render |
| todomvc.com/examples/react-redux | React+Redux SPA | ✓ full render |
| babylonbee.com | Cloudflare Rocket Loader | ✓ articles intact |
| linear.app | Next.js | ✓ renders (1 minor error) |
| github.com | Custom SSR | ✓ renders (4 minor errors) |

A full sweep of all 20 TodoMVC examples runs automatically on every push via the `todomvc-compat` CI job.
