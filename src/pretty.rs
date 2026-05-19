/// Indent `html` with two-space indentation for human readability.
///
/// Block-level elements each start on their own line. `<script>`, `<style>`,
/// `<pre>`, and `<textarea>` content is passed through verbatim.  Inline
/// elements and text stay on the same line as their containing block.
#[must_use]
pub fn pretty_print(html: &str) -> String {
    let bytes = html.as_bytes();
    let mut out = String::with_capacity(html.len() * 5 / 4);
    let mut pos = 0;
    let mut indent: usize = 0;
    let mut in_raw: Option<String> = None;

    while pos < bytes.len() {
        // ── Raw content zone (script / style / pre / textarea) ──────────────
        if let Some(ref raw_tag) = in_raw.clone() {
            let close = format!("</{raw_tag}");
            let found = (pos..bytes.len()).find(|&i| {
                i + close.len() <= bytes.len()
                    && html[i..i + close.len()].eq_ignore_ascii_case(&close)
            });
            if let Some(i) = found {
                out.push_str(&html[pos..i]);
                pos = i;
                in_raw = None;
                // fall through to parse the closing tag normally
            } else {
                out.push_str(&html[pos..]);
                break;
            }
        }
        if pos >= bytes.len() {
            break;
        }

        // ── Text content ────────────────────────────────────────────────────
        if bytes[pos] != b'<' {
            let start = pos;
            while pos < bytes.len() && bytes[pos] != b'<' {
                pos += 1;
            }
            emit_text(&mut out, &html[start..pos], indent);
            continue;
        }

        // ── Tag ─────────────────────────────────────────────────────────────
        let tag_start = pos;
        pos += 1;
        if pos >= bytes.len() {
            out.push('<');
            break;
        }

        // DOCTYPE and comment
        if let Some(new_pos) = try_special_markup(html, bytes, pos, tag_start, &mut out, indent) {
            pos = new_pos;
            continue;
        }

        // Closing tag?
        let is_closing = bytes[pos] == b'/';
        if is_closing {
            pos += 1;
        }

        // Tag name
        let name_start = pos;
        while pos < bytes.len() && (bytes[pos].is_ascii_alphanumeric() || bytes[pos] == b'-') {
            pos += 1;
        }
        let tag_name = html[name_start..pos].to_ascii_lowercase();

        // Scan to closing '>' (respecting quoted attributes)
        pos = scan_tag_end(bytes, pos);
        let tag_str = &html[tag_start..pos];

        let is_block = is_block_tag(&tag_name);
        let is_void = is_void_tag(&tag_name);
        emit_tag(
            &mut out,
            tag_str,
            &tag_name,
            is_closing,
            is_block,
            is_void,
            &mut indent,
            &mut in_raw,
        );
    }

    let trimmed = out.trim_end();
    format!("{trimmed}\n")
}

fn emit_text(out: &mut String, text: &str, indent: usize) {
    let inner = text.trim();
    if inner.is_empty() {
        return;
    }
    if out.ends_with('\n') {
        push_indent(out, indent);
        out.push_str(inner);
    } else {
        if text.starts_with(|c: char| c.is_ascii_whitespace()) && !out.ends_with(' ') {
            out.push(' ');
        }
        out.push_str(inner);
    }
    if text.ends_with(|c: char| c.is_ascii_whitespace()) {
        out.push(' ');
    }
}

/// Return the position after the special markup (DOCTYPE or comment), or `None` if neither.
fn try_special_markup(
    html: &str,
    bytes: &[u8],
    pos: usize,
    tag_start: usize,
    out: &mut String,
    indent: usize,
) -> Option<usize> {
    if bytes[pos..].starts_with(b"!DOCTYPE") || bytes[pos..].starts_with(b"!doctype") {
        let end = bytes[pos..]
            .iter()
            .position(|&b| b == b'>')
            .map_or(bytes.len(), |i| pos + i + 1);
        out.push_str(&html[tag_start..end]);
        out.push('\n');
        return Some(end);
    }
    if bytes[pos..].starts_with(b"!--") {
        let end = html[pos + 3..]
            .find("-->")
            .map_or(bytes.len(), |i| pos + 3 + i + 3);
        ensure_newline_indent(out, indent);
        out.push_str(&html[tag_start..end]);
        return Some(end);
    }
    None
}

fn scan_tag_end(bytes: &[u8], mut pos: usize) -> usize {
    let mut in_quote: Option<u8> = None;
    while pos < bytes.len() {
        match in_quote {
            Some(q) if bytes[pos] == q => {
                in_quote = None;
                pos += 1;
            }
            Some(_) => {
                pos += 1;
            }
            None => match bytes[pos] {
                b'"' | b'\'' => {
                    in_quote = Some(bytes[pos]);
                    pos += 1;
                }
                b'>' => {
                    pos += 1;
                    break;
                }
                _ => {
                    pos += 1;
                }
            },
        }
    }
    pos
}

#[allow(clippy::too_many_arguments)]
fn emit_tag(
    out: &mut String,
    tag_str: &str,
    tag_name: &str,
    is_closing: bool,
    is_block: bool,
    is_void: bool,
    indent: &mut usize,
    in_raw: &mut Option<String>,
) {
    if is_closing {
        if is_block {
            *indent = indent.saturating_sub(1);
            ensure_newline_indent(out, *indent);
            out.push_str(tag_str);
            out.push('\n');
        } else {
            out.push_str(tag_str);
        }
    } else if is_block {
        ensure_newline_indent(out, *indent);
        out.push_str(tag_str);
        out.push('\n');
        if !is_void {
            *indent += 1;
            if is_raw_content_tag(tag_name) {
                *in_raw = Some(tag_name.to_owned());
            }
        }
    } else {
        out.push_str(tag_str);
    }
}

fn ensure_newline_indent(out: &mut String, indent: usize) {
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    push_indent(out, indent);
}

fn push_indent(out: &mut String, indent: usize) {
    for _ in 0..indent {
        out.push_str("  ");
    }
}

fn is_block_tag(name: &str) -> bool {
    matches!(
        name,
        "html"
            | "head"
            | "body"
            | "div"
            | "section"
            | "article"
            | "main"
            | "nav"
            | "header"
            | "footer"
            | "aside"
            | "p"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "ul"
            | "ol"
            | "li"
            | "dl"
            | "dt"
            | "dd"
            | "table"
            | "thead"
            | "tbody"
            | "tfoot"
            | "tr"
            | "td"
            | "th"
            | "caption"
            | "colgroup"
            | "col"
            | "form"
            | "fieldset"
            | "legend"
            | "details"
            | "summary"
            | "figure"
            | "figcaption"
            | "blockquote"
            | "address"
            | "script"
            | "style"
            | "pre"
            | "textarea"
            | "noscript"
            | "template"
            | "title"
            | "meta"
            | "link"
            | "base"
            | "hr"
            | "br"
            | "canvas"
            | "video"
            | "audio"
            | "iframe"
            | "object"
            | "picture"
    )
}

fn is_void_tag(name: &str) -> bool {
    matches!(
        name,
        "area"
            | "base"
            | "br"
            | "col"
            | "embed"
            | "hr"
            | "img"
            | "input"
            | "link"
            | "meta"
            | "param"
            | "source"
            | "track"
            | "wbr"
    )
}

fn is_raw_content_tag(name: &str) -> bool {
    matches!(name, "script" | "style" | "pre" | "textarea")
}

#[cfg(test)]
mod tests {
    use super::pretty_print;

    #[test]
    fn block_elements_are_indented() {
        let input = "<html><body><div><p>hello</p></div></body></html>";
        let out = pretty_print(input);
        assert!(out.contains("\n  <body>"), "body not indented");
        assert!(out.contains("\n    <div>"), "div not indented");
        assert!(out.contains("\n      <p>"), "p not indented");
    }

    #[test]
    fn inline_whitespace_preserved() {
        let input = "<html><body><p>Some <strong>bold</strong> text.</p></body></html>";
        let out = pretty_print(input);
        assert!(
            out.contains("Some <strong>bold</strong> text."),
            "spaces around inline element were lost: {out}"
        );
    }

    #[test]
    fn script_content_verbatim() {
        // '<' inside a script body must not be parsed as a tag.
        let input = "<html><body><script>var x = 1 < 2;</script></body></html>";
        let out = pretty_print(input);
        assert!(
            out.contains("var x = 1 < 2;"),
            "script content was corrupted: {out}"
        );
    }

    #[test]
    fn doctype_preserved() {
        let input = "<!DOCTYPE html><html><body></body></html>";
        let out = pretty_print(input);
        assert!(
            out.starts_with("<!DOCTYPE html>\n"),
            "doctype not at top: {out}"
        );
    }

    #[test]
    fn void_elements_on_own_line() {
        let input = "<html><head><meta charset=\"utf-8\"><link rel=\"stylesheet\" href=\"a.css\"></head><body></body></html>";
        let out = pretty_print(input);
        assert!(out.contains("\n    <meta"), "meta not indented");
        assert!(out.contains("\n    <link"), "link not indented");
    }
}
