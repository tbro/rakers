use html5ever::{
    ParseOpts, parse_document,
    serialize::{SerializeOpts, TraversalScope, serialize},
    tendril::TendrilSink,
};
use markup5ever_rcdom::{Handle, NodeData, RcDom, SerializableHandle};

pub enum ScriptSource {
    Inline(String),
    External(String), // value of src="..." (may be relative)
}

pub struct Document {
    dom: RcDom,
}

pub fn parse(html: &str) -> Document {
    let dom = parse_document(RcDom::default(), ParseOpts::default())
        .from_utf8()
        .read_from(&mut html.as_bytes())
        .unwrap();
    Document { dom }
}

impl Document {
    /// Walk the DOM tree and return every script in document order.
    /// Inline scripts carry their text; external scripts carry the src value.
    pub fn extract_scripts(&self) -> Vec<ScriptSource> {
        let mut out = Vec::new();
        collect_scripts(&self.dom.document, &mut out);
        out
    }

    /// Serialize the DOM, optionally replacing the body content with `body_html`
    /// (from JS DOM mutations) and appending `extra` (from document.write).
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

/// Returns the byte range of the content inside `<body>...</body>`.
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
