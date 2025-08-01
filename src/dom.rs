use html5ever::{
    parse_document,
    serialize::{serialize, SerializeOpts, TraversalScope},
    tendril::TendrilSink,
    ParseOpts,
};
use markup5ever_rcdom::{Handle, NodeData, RcDom, SerializableHandle};

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
    /// Walk the DOM tree and collect the text content of every <script> element.
    pub fn extract_scripts(&self) -> Vec<String> {
        let mut out = Vec::new();
        collect_scripts(&self.dom.document, &mut out);
        out
    }

    /// Serialize the DOM to an HTML string, injecting `extra` just before </body>.
    pub fn serialize_with_injection(&self, extra: &str) -> String {
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

fn collect_scripts(handle: &Handle, out: &mut Vec<String>) {
    if let NodeData::Element { ref name, .. } = handle.data {
        if &name.local == "script" {
            let mut content = String::new();
            for child in handle.children.borrow().iter() {
                if let NodeData::Text { ref contents } = child.data {
                    content.push_str(&contents.borrow());
                }
            }
            if !content.trim().is_empty() {
                out.push(content);
            }
            // Don't recurse into script children.
            return;
        }
    }
    for child in handle.children.borrow().iter() {
        collect_scripts(child, out);
    }
}
