//! HTML parsing, script extraction, and serialization.
//!
//! Wraps html5ever and `markup5ever_rcdom` to provide the three operations
//! the rendering pipeline needs: parse an HTML string into a DOM, walk the
//! DOM to collect `<script>` sources in document order, and serialize the
//! (optionally mutated) DOM back to an HTML string.

use anyhow::anyhow;
use html5ever::{
    ParseOpts, parse_document,
    serialize::{SerializeOpts, TraversalScope, serialize},
    tendril::TendrilSink,
};
use markup5ever_rcdom::{Handle, NodeData, RcDom, SerializableHandle};

const VOID_ELEMENTS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr",
];

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
pub fn parse(html: &str) -> anyhow::Result<Document> {
    let dom = parse_document(RcDom::default(), ParseOpts::default())
        .from_utf8()
        .read_from(&mut html.as_bytes())
        .map_err(|e| anyhow!("html parse failed: {e:?}"))?;
    Ok(Document { dom })
}

impl Document {
    /// Collect all `<meta name="…" content="…">` elements, returning a map of
    /// `name → content`.  Used to expose Ember's config meta tag to the JS runtime.
    pub fn collect_meta(&self) -> std::collections::HashMap<String, String> {
        let mut map = std::collections::HashMap::new();
        collect_meta_tags(&self.dom.document, &mut map);
        map
    }

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
    pub fn serialize_with_body_and_injection(
        &self,
        body_html: &str,
        extra: &str,
    ) -> anyhow::Result<String> {
        let mut bytes = Vec::new();
        serialize(
            &mut bytes,
            &SerializableHandle::from(self.dom.document.clone()),
            SerializeOpts {
                traversal_scope: TraversalScope::ChildrenOnly(None),
                ..Default::default()
            },
        )
        .map_err(|e| anyhow!("serialization failed: {e:?}"))?;

        let mut html = String::from_utf8(bytes)
            .map_err(|e| anyhow!("html5ever produced invalid UTF-8: {e}"))?;

        // Replace body content when JS rendered into the DOM.
        // Prefer targeted replacement (swap just the root element by id) so that
        // static siblings like <footer class="info"> are preserved.  Fall back to
        // a full body-content replacement when no root id can be identified.
        if !body_html.is_empty() {
            let replaced = first_element_id(body_html)
                .and_then(|id| find_element_range_by_id(&html, &id))
                .map(|range| html.replace_range(range, body_html))
                .is_some();

            if !replaced && let Some((start, end)) = body_content_range(&html) {
                html.replace_range(start..end, body_html);
            }
        }

        // Inject document.write() output just before </body>.
        if !extra.is_empty() {
            if let Some(pos) = html.rfind("</body>") {
                html.insert_str(pos, extra);
            } else {
                html.push_str(extra);
            }
        }

        Ok(html)
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

/// Extract the `id` attribute value from the first element in `html`.
///
/// html5ever always serializes attribute values with double quotes, so we only
/// need to look for `id="…"`.
fn first_element_id(html: &str) -> Option<String> {
    let s = html.trim_start();
    let tag_end = s.find('>')?;
    let tag = &s[1..tag_end];
    let marker = "id=\"";
    let pos = tag.find(marker)? + marker.len();
    let end = tag[pos..].find('"')? + pos;
    let id = &tag[pos..end];
    if id.is_empty() {
        None
    } else {
        Some(id.to_owned())
    }
}

/// Find the byte range of the element with `id="<id>"` in `html`.
///
/// Returns a range spanning the full element — opening tag through closing tag.
/// Handles nested same-name elements via a depth counter.
fn find_element_range_by_id(html: &str, id: &str) -> Option<std::ops::Range<usize>> {
    let needle = format!("id=\"{id}\"");
    let attr_pos = html.find(&needle)?;
    let tag_start = html[..attr_pos].rfind('<')?;

    let after_lt = &html[tag_start + 1..];
    let name_len = after_lt.find(|c: char| c.is_ascii_whitespace() || c == '>' || c == '/')?;
    let tag_name = after_lt[..name_len].to_ascii_lowercase();

    let open_end = html[tag_start..].find('>')? + tag_start + 1;

    if VOID_ELEMENTS.contains(&tag_name.as_str()) || html[tag_start..open_end].ends_with("/>") {
        return Some(tag_start..open_end);
    }

    // Walk forward, counting open/close tags of the same name to find the match.
    let open_pat = format!("<{tag_name}"); // e.g. "<div"
    let close_pat = format!("</{tag_name}>"); // e.g. "</div>"
    let mut depth: usize = 1;
    let mut pos = open_end;

    while depth > 0 {
        let rest = &html[pos..];
        let next_open = rest.find(&open_pat).map(|p| p + pos);
        let next_close = rest.find(&close_pat).map(|p| p + pos);

        match (next_open, next_close) {
            (Some(o), Some(c)) if o < c => {
                // Verify this is a real tag boundary (next char is whitespace, '>', or '/').
                let after = html.as_bytes().get(o + open_pat.len()).copied();
                if matches!(after, Some(b' ' | b'\t' | b'\n' | b'>' | b'/')) {
                    depth += 1;
                }
                pos = o + open_pat.len();
            }
            (_, Some(c)) => {
                depth -= 1;
                let close_end = c + close_pat.len();
                if depth == 0 {
                    return Some(tag_start..close_end);
                }
                pos = close_end;
            }
            _ => return None,
        }
    }

    None
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
        if let Some(ref t) = type_val {
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
                // Non-JS typed script with a src (e.g. riot/tag): register it in
                // _r_nonstandard_scripts so querySelectorAll('script[type="X"]') can find it.
                let src = attrs
                    .iter()
                    .find(|a| &a.name.local == "src")
                    .map(|a| a.value.trim().to_string());
                if let Some(src) = src.filter(|s| !s.is_empty()) {
                    let t_escaped = t.replace('\'', "\\'");
                    let s_escaped = src.replace('\\', "\\\\").replace('\'', "\\'");
                    out.push(ScriptSource::Inline(format!(
                        "_r_nonstandard_scripts.push({{type:'{t_escaped}',src:'{s_escaped}',\
                        getAttribute:function(n){{return n==='src'?this.src:n==='type'?this.type:null;}},\
                        innerHTML:''}});"
                    )));
                }
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

/// Recursively collect `<meta name="…" content="…">` elements into `map`.
fn collect_meta_tags(handle: &Handle, map: &mut std::collections::HashMap<String, String>) {
    if let NodeData::Element {
        ref name,
        ref attrs,
        ..
    } = handle.data
        && &name.local == "meta"
    {
        let attrs = attrs.borrow();
        let name_val = attrs
            .iter()
            .find(|a| &a.name.local == "name")
            .map(|a| a.value.to_string());
        let content_val = attrs
            .iter()
            .find(|a| &a.name.local == "content")
            .map(|a| a.value.to_string());
        if let (Some(n), Some(c)) = (name_val, content_val)
            && !n.is_empty()
        {
            map.insert(n, c);
        }
    }
    for child in handle.children.borrow().iter() {
        collect_meta_tags(child, map);
    }
}
