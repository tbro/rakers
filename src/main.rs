use rakers::{HttpConfig, diff_html, pretty_print, render, select_html, set_verbose, to_json};

use std::time::Duration;

use clap::Parser;
use std::{
    fs,
    io::{self, Read, Write},
    path::Path,
};

#[derive(Parser)]
#[command(
    name = "rakers",
    about = "Render JavaScript into HTML using Servo's HTML parser (html5ever)"
)]
struct Cli {
    /// File path, URL (http/https), or omit to read stdin as HTML
    input: Option<String>,

    /// Write output to FILE instead of stdout
    #[arg(short, long, value_name = "FILE")]
    output: Option<String>,

    /// Set the User-Agent header for all HTTP requests.
    /// Use a non-browser UA (e.g. "curl/8.0") to bypass Cloudflare Rocket Loader
    /// and other UA-gated server-side transformations.
    #[arg(short = 'A', long, value_name = "UA")]
    user_agent: Option<String>,

    /// Add a custom HTTP request header (repeatable). Format: "Name: Value".
    /// Example: -H "CF-No-Mirage: 1"
    #[arg(short = 'H', long = "header", value_name = "HEADER")]
    headers: Vec<String>,

    /// Strip <script> elements, <link rel="modulepreload">, and unwrap <noscript>
    /// from the output — produces a static, crawlable snapshot similar to what
    /// prerendering services deliver to search-engine bots.
    #[arg(long)]
    clean: bool,

    /// Format the output HTML with two-space indentation for human readability.
    #[arg(long)]
    pretty: bool,

    /// Emit a JSON object with raw_bytes, rendered_bytes, and html fields
    /// instead of bare HTML — useful for scripting and size comparisons.
    #[arg(long)]
    json: bool,

    /// Limit the number of remote <script src> fetches.
    /// Inline scripts are not counted.  Default: unlimited.
    #[arg(long, value_name = "N")]
    max_scripts: Option<usize>,

    /// Show a unified diff of raw vs rendered HTML instead of the full output.
    /// Both sides are pretty-printed before diffing for a readable result.
    #[arg(long)]
    diff: bool,

    /// Print informational messages to stderr: script fetches, skips, console output,
    /// and module-shim activations.  By default these are suppressed.
    #[arg(long)]
    verbose: bool,

    /// Per-script wall-clock timeout in seconds (fractions allowed, e.g. 0.5).
    /// Scripts that exceed this limit are interrupted (non-fatal).  Default: 30.
    /// Must be greater than zero; use --no-timeout to remove the cap entirely.
    #[arg(long, value_name = "SECS", conflicts_with = "no_timeout")]
    timeout: Option<f64>,

    /// Remove the per-script timeout entirely.  Use with care — a hung script
    /// will block the process indefinitely.
    #[arg(long)]
    no_timeout: bool,

    /// Filter rendered output to elements matching SELECTOR (CSS selector syntax).
    /// All matching elements are printed, each separated by a newline.
    #[arg(long, value_name = "SELECTOR")]
    selector: Option<String>,

    /// Proxy URL for all outbound HTTP requests (page fetch, script fetches, XHR).
    /// Supports SOCKS5 (socks5://), SOCKS4 (socks4://), and HTTP (http://) proxies.
    /// Example: --proxy socks5://127.0.0.1:9050 (routes traffic through Tor).
    #[arg(long, value_name = "URL")]
    proxy: Option<String>,

    /// Forward custom -H headers on XHR requests made by page scripts.
    /// By default headers are withheld from XHR to avoid leaking credentials
    /// to cross-origin destinations the page JavaScript controls.
    #[arg(long)]
    forward_headers: bool,
}

/// Return `true` if `s` is an `http://` or `https://` URL.
fn is_url(s: &str) -> bool {
    s.starts_with("http://") || s.starts_with("https://")
}

/// Build an [`HttpConfig`] from the parsed CLI arguments.
///
/// Returns an error if any `-H` header value is not in `"Name: Value"` format.
fn http_config_from_cli(cli: &Cli) -> anyhow::Result<HttpConfig> {
    let mut headers = Vec::new();
    for raw in &cli.headers {
        let (name, value) = raw
            .split_once(':')
            .ok_or_else(|| anyhow::anyhow!("invalid header {:?}: expected \"Name: Value\"", raw))?;
        headers.push((name.trim().to_owned(), value.trim().to_owned()));
    }
    Ok(HttpConfig {
        user_agent: cli.user_agent.clone(),
        headers,
        proxy: cli.proxy.clone(),
        forward_headers: cli.forward_headers,
    })
}

/// Read the input source and return `(content, is_js)`.
///
/// For URLs the content is fetched over HTTP. For file paths the content is read
/// from disk; `is_js` is `true` when the file extension is `.js`.
fn fetch(input: &str, cfg: &HttpConfig) -> anyhow::Result<(String, bool)> {
    if is_url(input) {
        let body = cfg.apply(cfg.agent().get(input)).call()?.into_string()?;
        Ok((body, false))
    } else {
        let content = fs::read_to_string(input)?;
        let is_js = Path::new(input)
            .extension()
            .map(|e| e == "js")
            .unwrap_or(false);
        Ok((content, is_js))
    }
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    set_verbose(cli.verbose);
    let cfg = http_config_from_cli(&cli)?;

    let page_url = cli.input.as_deref().filter(|s| is_url(s));

    let (input, is_js) = match &cli.input {
        Some(src) => fetch(src, &cfg)?,
        None => {
            let mut s = String::new();
            io::stdin().read_to_string(&mut s)?;
            (s, false)
        }
    };

    let script_timeout = if cli.no_timeout {
        None
    } else if let Some(secs) = cli.timeout {
        if secs <= 0.0 {
            anyhow::bail!("--timeout must be greater than zero (use --no-timeout to remove the cap)");
        }
        Some(Duration::from_secs_f64(secs))
    } else {
        Some(Duration::from_secs(30))
    };
    let rendered = render(&input, is_js, page_url, &cfg, cli.clean, cli.max_scripts, script_timeout)?;
    let rendered = match &cli.selector {
        Some(sel) => select_html(&rendered, sel)?,
        None => rendered,
    };
    let result = if cli.diff {
        diff_html(&input, &rendered)
    } else {
        let html = if cli.pretty { pretty_print(&rendered) } else { rendered };
        if cli.json { to_json(input.len(), &html) } else { html }
    };

    match &cli.output {
        Some(path) => fs::write(path, &result)?,
        None => io::stdout().write_all(result.as_bytes())?,
    }

    Ok(())
}
