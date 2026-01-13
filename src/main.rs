use rakers::{HttpConfig, diff_html, pretty_print, render, set_verbose, to_json};

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

    /// Per-script wall-clock timeout in seconds.
    /// Scripts that exceed this limit are interrupted (non-fatal).  Default: 30.
    #[arg(long, value_name = "SECS")]
    timeout: Option<u64>,
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
    })
}

/// Read the input source and return `(content, is_js)`.
///
/// For URLs the content is fetched over HTTP. For file paths the content is read
/// from disk; `is_js` is `true` when the file extension is `.js`.
fn fetch(input: &str, cfg: &HttpConfig) -> anyhow::Result<(String, bool)> {
    if is_url(input) {
        let body = cfg.apply(ureq::get(input)).call()?.into_string()?;
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

    let script_timeout = cli.timeout.map(Duration::from_secs);
    let rendered = render(&input, is_js, page_url, &cfg, cli.clean, cli.max_scripts, script_timeout)?;
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
