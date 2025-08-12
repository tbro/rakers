# rakers

A CLI that renders JavaScript into HTML. Give it an HTML file, a URL, or a bare JS script and it returns the post-execution HTML.

Built on [html5ever](https://github.com/servo/html5ever) (Servo's HTML5 parser) and [boa_engine](https://github.com/boa-dev/boa) (pure-Rust JS engine).

## Install

```sh
cargo install --path .
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

1. HTML is parsed with **html5ever** into a DOM tree
2. `<script>` tags are collected and executed in order by **boa_engine**
3. Anything written via `document.write()` / `document.writeln()` is injected before `</body>`
4. The DOM is serialized back to HTML

`.js` files are automatically wrapped in a minimal HTML document before processing.

`console.log`, `console.warn`, and `console.error` print to stderr with a `[console]` prefix.

## Limitations

- No network requests from JS (`fetch`, `XMLHttpRequest`)
- No DOM query API (`getElementById`, `querySelector`, etc.)
- No timers (`setTimeout`, `setInterval`)

These are the natural next steps if you want to extend the tool.
