# rakers

A CLI that renders JavaScript into HTML. Give it an HTML file, a URL, or a bare JS script and it returns the post-execution HTML — including content rendered by React, Vue, and other JS frameworks.

Built on [html5ever](https://github.com/servo/html5ever) (Servo's HTML5 parser) with a choice of JS engine: [boa_engine](https://github.com/boa-dev/boa) (pure-Rust, default) or [QuickJS](https://bellard.org/quickjs/) via [rquickjs](https://github.com/DelSkayn/rquickjs) (recommended for real-world sites).

## Install

```sh
# Default build (boa engine — pure Rust, no C compiler required)
cargo install --path .

# QuickJS engine (better compatibility with real-world JS bundles)
cargo install --path . --no-default-features --features rquickjs
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

## How it works

1. Fetches the page (or reads from file/stdin)
2. Parses HTML with **html5ever** into a DOM tree
3. Collects `<script>` tags — inline and external (`src="..."`) — and fetches any external scripts
4. Executes all scripts in order in a sandboxed JS context with browser globals stubbed out
5. Flushes any deferred callbacks (`setTimeout`, `requestAnimationFrame`, `MessageChannel`, `queueMicrotask`) so async-rendered frameworks have a chance to run
6. Reads back `document.body.innerHTML` and serializes the final HTML

`.js` files are automatically wrapped in a minimal HTML document before processing.

`console.log`, `console.warn`, and `console.error` print to stderr with a `[console]` prefix.
Script errors are non-fatal — execution continues with the next script.

## JS engine choice

rakers supports two JS engines selectable at compile time.

| | boa (default) | rquickjs |
|--|---------------|----------|
| **Build deps** | Pure Rust, no C compiler | Requires a C compiler |
| **ES standard** | ES2021 (partial) | ES2023 |
| **Real-world bundles** | Limited — may stack-overflow on large bundles | Good |
| **React / Vue SPAs** | Often hits stack limits | Works |
| **When to use** | Simple pages, CI without C toolchain | Real-world sites |

### Building

```sh
# boa (default — pure Rust, no extra toolchain needed)
cargo build
cargo install --path .

# QuickJS via rquickjs (recommended for real-world sites)
cargo build --no-default-features --features rquickjs
cargo install --path . --no-default-features --features rquickjs
```

Only one engine can be enabled at a time; the build will fail with a clear
error if both or neither are selected.

### Running tests

Unit tests run with either engine:

```sh
cargo test                                            # boa
cargo test --no-default-features --features rquickjs  # rquickjs
```

Integration tests that fetch real SPAs require rquickjs (boa overflows the
native stack on large React/Rocket Loader bundles):

```sh
cargo test --test integration --no-default-features --features rquickjs
```

## Browser environment

The following globals are stubbed so typical JS bundles run without errors:

- **`document`** — `createElement`, `getElementById`, `querySelector`, `body`, `head`, and the full DOM manipulation API (`appendChild`, `insertBefore`, `setAttribute`, etc.)
- **`window`** — `location`, `navigator`, `history`, `screen`, `performance`, `localStorage`, `sessionStorage`, `matchMedia`, `getComputedStyle`, and all standard event/observer constructors
- **`fetch` / `XMLHttpRequest`** — stubbed as no-ops (network requests from JS are not made)
- **`process`** — Node.js-style globals for webpack/Vite bundler compatibility
- **Timers** — `setTimeout`, `setInterval`, `requestAnimationFrame`, `queueMicrotask`, and `MessageChannel` callbacks are collected and flushed after scripts finish
