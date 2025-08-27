use std::cell::RefCell;

use anyhow::anyhow;
use boa_engine::{
    js_string, object::ObjectInitializer, property::Attribute, Context, JsResult, JsValue,
    NativeFunction, Source,
};

thread_local! {
    static WRITTEN:        RefCell<String>      = RefCell::new(String::new());
    static LOGGED:         RefCell<Vec<String>> = RefCell::new(Vec::new());
    static BODY_INNER_HTML:RefCell<String>      = RefCell::new(String::new());
}

// The JS bootstrap is embedded at compile time; `__HREF__` is substituted at runtime.
const BOOTSTRAP_TEMPLATE: &str = include_str!("bootstrap.js");

// Script run after user scripts to read the JS DOM state back into Rust.
const READBACK_JS: &str = r#"
(function() {
    var body = document.body && document.body.innerHTML;
    if (body) return body;
    // If scripts wrote into registry elements but never appended them to body,
    // collect any that have content.
    var parts = [];
    var keys  = Object.keys(_r_reg);
    for (var i = 0; i < keys.length; i++) {
        var el = _r_reg[keys[i]];
        if (el && el.innerHTML) parts.push(_r_serialize(el));
    }
    return parts.join('');
})()
"#;

fn make_bootstrap(page_url: Option<&str>) -> String {
    let href = page_url.unwrap_or("about:blank");
    let escaped = href.replace('\\', "\\\\").replace('"', "\\\"");
    BOOTSTRAP_TEMPLATE.replace("__HREF__", &escaped)
}

// ── Public runtime ────────────────────────────────────────────────────────────

pub struct JsRuntime;

impl JsRuntime {
    pub fn new() -> Self {
        WRITTEN.with(|w|        w.borrow_mut().clear());
        LOGGED.with(|l|         l.borrow_mut().clear());
        BODY_INNER_HTML.with(|b| b.borrow_mut().clear());
        JsRuntime
    }

    /// Execute `scripts` in a fresh JS context.
    /// `page_url` populates `window.location.href`.
    /// Script errors are non-fatal: logged to stderr, execution continues.
    pub fn execute(&self, scripts: &[String], page_url: Option<&str>) -> anyhow::Result<()> {
        let mut ctx = Context::default();
        setup_document(&mut ctx)?;
        setup_console(&mut ctx)?;

        let bootstrap = make_bootstrap(page_url);
        ctx.eval(Source::from_bytes(bootstrap.as_bytes()))
            .map_err(|e| anyhow!("bootstrap error: {:?}", e))?;

        for script in scripts {
            if let Err(e) = ctx.eval(Source::from_bytes(script.as_bytes())) {
                eprintln!("[js error] {:?}", e);
            }
        }

        // Read the JS-rendered body back into Rust.
        let body_result = ctx.eval(Source::from_bytes(READBACK_JS.as_bytes()));
        let body_html = body_result
            .ok()
            .and_then(|v| v.to_string(&mut ctx).ok())
            .map(|s| s.to_std_string_escaped())
            .unwrap_or_default();

        let body_html = match body_html.as_str() {
            "undefined" | "null" | "" => String::new(),
            s => s.to_owned(),
        };
        BODY_INNER_HTML.with(|b| *b.borrow_mut() = body_html);

        Ok(())
    }

    /// HTML accumulated by `document.write()` / `document.writeln()`.
    pub fn written_html(&self) -> String {
        WRITTEN.with(|w| w.borrow().clone())
    }

    /// The value of `document.body.innerHTML` after all scripts ran.
    pub fn body_inner_html(&self) -> String {
        BODY_INNER_HTML.with(|b| b.borrow().clone())
    }

    pub fn logged_messages(&self) -> Vec<String> {
        LOGGED.with(|l| l.borrow().clone())
    }
}

// ── document global (write/writeln wired to Rust; rest added by bootstrap JS) ─

fn setup_document(ctx: &mut Context) -> anyhow::Result<()> {
    let mut init = ObjectInitializer::new(ctx);
    init.function(NativeFunction::from_fn_ptr(doc_write),   js_string!("write"),   1);
    init.function(NativeFunction::from_fn_ptr(doc_writeln), js_string!("writeln"), 1);
    let obj = init.build();

    ctx.register_global_property(js_string!("document"), obj, Attribute::all())
        .map_err(|e| anyhow!("{:?}", e))?;
    Ok(())
}

fn doc_write(_this: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    let s = js_first_arg_to_string(args, ctx)?;
    WRITTEN.with(|w| w.borrow_mut().push_str(&s));
    Ok(JsValue::undefined())
}

fn doc_writeln(_this: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    let s = js_first_arg_to_string(args, ctx)?;
    WRITTEN.with(|w| { let mut w = w.borrow_mut(); w.push_str(&s); w.push('\n'); });
    Ok(JsValue::undefined())
}

// ── console global ────────────────────────────────────────────────────────────

fn setup_console(ctx: &mut Context) -> anyhow::Result<()> {
    let mut init = ObjectInitializer::new(ctx);
    init.function(NativeFunction::from_fn_ptr(console_log), js_string!("log"),   0);
    init.function(NativeFunction::from_fn_ptr(console_log), js_string!("warn"),  0);
    init.function(NativeFunction::from_fn_ptr(console_log), js_string!("error"), 0);
    let obj = init.build();

    ctx.register_global_property(js_string!("console"), obj, Attribute::all())
        .map_err(|e| anyhow!("{:?}", e))?;
    Ok(())
}

fn console_log(_this: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    let parts: Vec<String> = args
        .iter()
        .map(|a| a.to_string(ctx).map(|s| s.to_std_string_escaped()))
        .collect::<Result<_, _>>()?;
    LOGGED.with(|l| l.borrow_mut().push(parts.join(" ")));
    Ok(JsValue::undefined())
}

// ── helpers ───────────────────────────────────────────────────────────────────

fn js_first_arg_to_string(args: &[JsValue], ctx: &mut Context) -> JsResult<String> {
    args.first()
        .map(|v| v.to_string(ctx).map(|s| s.to_std_string_escaped()))
        .transpose()
        .map(|o| o.unwrap_or_default())
}
