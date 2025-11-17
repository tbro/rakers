//! HTML parsing, script extraction, and serialization.
//!
//! Wraps html5ever and markup5ever_rcdom to provide the three operations
//! the rendering pipeline needs: parse an HTML string into a DOM, walk the
//! DOM to collect `<script>` sources in document order, and serialize the
//! (optionally mutated) DOM back to an HTML string.

use html5ever::{
    ParseOpts, parse_document,
    serialize::{SerializeOpts, TraversalScope, serialize},
    tendril::TendrilSink,
};
use markup5ever_rcdom::{Handle, NodeData, RcDom, SerializableHandle};

/// The source of a `<script>` element's JavaScript.
pub enum ScriptSource {
    /// Script whose code is inlined in the HTML.
    Inline(String),
    /// Script loaded via a `src` attribute; the value may be relative.
    External(String),
}

/// A parsed HTML document, ready for script extraction and serialization.
pub struct Document {
    dom: RcDom,
}

/// Parse `html` into a [`Document`] using the html5ever HTML5 parser.
pub fn parse(html: &str) -> Document {
    let dom = parse_document(RcDom::default(), ParseOpts::default())
        .from_utf8()
        .read_from(&mut html.as_bytes())
        .unwrap();
    Document { dom }
}

impl Document {
    /// Walk the DOM and return every executable `<script>` in document order.
    ///
    /// Inline scripts carry their text content; external scripts carry the raw
    /// `src` attribute value (which may be relative).  Non-JS types (JSON,
    /// templates, etc.) are skipped.
    pub fn extract_scripts(&self) -> Vec<ScriptSource> {
        let mut out = Vec::new();
        collect_scripts(&self.dom.document, &mut out);
        out
    }

    /// Serialize the DOM to an HTML string, applying post-execution mutations.
    ///
    /// `body_html` — if non-empty, replaces the content between `<body>` and `</body>`
    /// with the JS-rendered DOM (from `document.body.innerHTML`).
    ///
    /// `extra` — appended just before `</body>`; carries output from `document.write`.
    pub fn serialize_with_body_and_injection(&self, body_html: &str, extra: &str) -> String {
        let mut bytes = Vec::new();
        serialize(
            &mut bytes,
            &SerializableHandle::from(self.dom.document.clone()),
            SerializeOpts {
                traversal_scope: TraversalScope::ChildrenOnly(None),
                ..Default::default()
            },
        )
        .expect("serialization failed");

        let mut html = String::from_utf8(bytes).expect("html5ever always outputs utf-8");

        // Replace body content when JS rendered into the DOM.
        if !body_html.is_empty()
            && let Some((start, end)) = body_content_range(&html)
        {
            html.replace_range(start..end, body_html);
        }

        // Inject document.write() output just before </body>.
        if !extra.is_empty() {
            if let Some(pos) = html.rfind("</body>") {
                html.insert_str(pos, extra);
            } else {
                html.push_str(extra);
            }
        }

        html
    }
}

/// Return the byte range of the content between the opening `<body>` tag and `</body>`.
fn body_content_range(html: &str) -> Option<(usize, usize)> {
    let body_pos = html.find("<body")?;
    let tag_close = html[body_pos..].find('>')? + body_pos + 1;
    let body_end = html.rfind("</body>")?;
    if body_end >= tag_close {
        Some((tag_close, body_end))
    } else {
        None
    }
}

/// Recursively walk the subtree rooted at `handle`, appending any executable
/// scripts to `out` in document order.
fn collect_scripts(handle: &Handle, out: &mut Vec<ScriptSource>) {
    if let NodeData::Element {
        ref name,
        ref attrs,
        ..
    } = handle.data
        && &name.local == "script"
    {
        let attrs = attrs.borrow();

        // Skip non-JS types (JSON, templates, etc.).
        let type_val = attrs
            .iter()
            .find(|a| &a.name.local == "type")
            .map(|a| a.value.to_string());
        if let Some(t) = type_val {
            let t = t.trim().to_ascii_lowercase();
            let executable = match t.as_str() {
                ""
                | "text/javascript"
                | "application/javascript"
                | "module"
                | "text/rocketscript" => true,
                // Modern Cloudflare Rocket Loader rewrites type to "<hex-hash>-text/javascript"
                t => t.ends_with("-text/javascript") || t.ends_with("-application/javascript"),
            };
            if !executable {
                return;
            }
        }

        let src = attrs
            .iter()
            .find(|a| &a.name.local == "src")
            .map(|a| a.value.trim().to_string());

        if let Some(src) = src {
            if !src.is_empty() {
                out.push(ScriptSource::External(src));
            }
        } else {
            let mut content = String::new();
            for child in handle.children.borrow().iter() {
                if let NodeData::Text { ref contents } = child.data {
                    content.push_str(&contents.borrow());
                }
            }
            if !content.trim().is_empty() {
                out.push(ScriptSource::Inline(content));
            }
        }
        // Don't recurse into script children.
        return;
    }
    for child in handle.children.borrow().iter() {
        collect_scripts(child, out);
    }
}
