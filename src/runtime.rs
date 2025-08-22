use std::cell::RefCell;

use anyhow::anyhow;
use boa_engine::{
    js_string, object::ObjectInitializer, property::Attribute, Context, JsResult, JsValue,
    NativeFunction, Source,
};

thread_local! {
    static WRITTEN: RefCell<String> = RefCell::new(String::new());
    static LOGGED: RefCell<Vec<String>> = RefCell::new(Vec::new());
}

// ---------------------------------------------------------------------------
// Browser globals bootstrap (injected before user scripts)
// ---------------------------------------------------------------------------

// __HREF__ is replaced at runtime with the page URL as a JS string literal.
const BOOTSTRAP_TEMPLATE: &str = r#"
var window = globalThis;
var self = window;

window.location = {
    href: "__HREF__",
    hostname: '', pathname: '/', search: '', hash: '', protocol: 'https:',
    assign: function(){}, replace: function(){}, reload: function(){}
};
window.navigator = {
    userAgent: 'rakers/0.1.0', language: 'en-US', languages: ['en-US'],
    onLine: false, cookieEnabled: false, platform: 'Linux'
};
window.screen = { width: 1920, height: 1080, availWidth: 1920, availHeight: 1080 };
window.history = {
    length: 1,
    pushState: function(){}, replaceState: function(){},
    back: function(){}, forward: function(){}, go: function(){}
};
window.performance = { now: function(){ return 0; }, timing: {} };
window.localStorage = {
    getItem: function(){ return null; }, setItem: function(){},
    removeItem: function(){}, clear: function(){}, key: function(){ return null; }, length: 0
};
window.sessionStorage = window.localStorage;
window.setTimeout  = function(fn, delay){ return 0; };
window.clearTimeout  = function(id){};
window.setInterval = function(fn, delay){ return 0; };
window.clearInterval = function(id){};
window.requestAnimationFrame = function(fn){ return 0; };
window.cancelAnimationFrame  = function(id){};
window.alert   = function(msg){};
window.confirm = function(msg){ return false; };
window.prompt  = function(msg, def){ return null; };
window.fetch   = function(){ return Promise.reject(new Error('fetch not available in rakers')); };
window.XMLHttpRequest = function(){
    this.open = function(){}; this.send = function(){};
    this.setRequestHeader = function(){}; this.status = 0; this.responseText = '';
};
window.matchMedia = function(q){
    return { matches: false, media: q, addEventListener: function(){}, removeEventListener: function(){} };
};
window.getComputedStyle = function(el){ return {}; };
window.MutationObserver    = function(cb){ this.observe = function(){}; this.disconnect = function(){}; };
window.ResizeObserver      = function(cb){ this.observe = function(){}; this.disconnect = function(){}; };
window.IntersectionObserver = function(cb){ this.observe = function(){}; this.disconnect = function(){}; };
window.CustomEvent = function(type, init){ this.type = type; this.detail = init && init.detail || null; };
window.Event       = function(type){ this.type = type; this.bubbles = false; this.cancelable = false; };

// Expand the document stub set up by Rust with the full DOM API surface.
document.getElementById          = function(id){ return null; };
document.getElementsByClassName  = function(cls){ return []; };
document.getElementsByTagName    = function(tag){ return []; };
document.querySelector           = function(sel){ return null; };
document.querySelectorAll        = function(sel){ return []; };
document.createElement = function(tag){
    return {
        tagName: tag.toUpperCase(), innerHTML: '', textContent: '',
        className: '', id: '', type: '', value: '', href: '', src: '',
        style: {}, dataset: {}, children: [], childNodes: [],
        parentNode: null, parentElement: null,
        classList: {
            add: function(){}, remove: function(){}, toggle: function(){},
            contains: function(){ return false; }, length: 0
        },
        addEventListener: function(){}, removeEventListener: function(){}, dispatchEvent: function(){},
        setAttribute: function(){}, getAttribute: function(){ return null; }, hasAttribute: function(){ return false; },
        appendChild: function(c){ return c; }, removeChild: function(){}, insertBefore: function(n){ return n; },
        cloneNode: function(){ return this; },
        getBoundingClientRect: function(){ return {top:0,left:0,bottom:0,right:0,width:0,height:0}; },
        focus: function(){}, blur: function(){}, click: function(){},
        contains: function(){ return false; }, closest: function(){ return null; }, matches: function(){ return false; },
        querySelector: function(){ return null; }, querySelectorAll: function(){ return []; },
        insertAdjacentHTML: function(){}, insertAdjacentElement: function(){}
    };
};
document.createTextNode      = function(text){ return {textContent: text, nodeValue: text, nodeType: 3}; };
document.createDocumentFragment = function(){ return {appendChild: function(){}, querySelector: function(){ return null; }, querySelectorAll: function(){ return []; }}; };
document.createEvent         = function(type){ return {initEvent: function(){}, type: ''}; };
document.addEventListener    = function(){};
document.removeEventListener = function(){};
document.dispatchEvent       = function(){};
document.readyState  = 'complete';
document.cookie      = '';
document.referrer    = '';
document.domain      = '';
document.title       = '';
document.body        = document.createElement('body');
document.head        = document.createElement('head');
document.documentElement = document.createElement('html');
"#;

fn make_bootstrap(page_url: Option<&str>) -> String {
    let href = page_url.unwrap_or("about:blank");
    // Escape backslashes and double-quotes so the URL is safe inside a JS string literal.
    let escaped = href.replace('\\', "\\\\").replace('"', "\\\"");
    BOOTSTRAP_TEMPLATE.replace("__HREF__", &escaped)
}

// ---------------------------------------------------------------------------
// Public runtime
// ---------------------------------------------------------------------------

pub struct JsRuntime;

impl JsRuntime {
    pub fn new() -> Self {
        WRITTEN.with(|w| w.borrow_mut().clear());
        LOGGED.with(|l| l.borrow_mut().clear());
        JsRuntime
    }

    /// Execute `scripts` in a fresh JS context.
    /// `page_url` is used to populate `window.location.href`.
    /// Script errors are non-fatal: they are logged to stderr and execution continues.
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

        Ok(())
    }

    pub fn written_html(&self) -> String {
        WRITTEN.with(|w| w.borrow().clone())
    }

    pub fn logged_messages(&self) -> Vec<String> {
        LOGGED.with(|l| l.borrow().clone())
    }
}

// ---------------------------------------------------------------------------
// document global (write/writeln wired to Rust; rest added by bootstrap JS)
// ---------------------------------------------------------------------------

fn setup_document(ctx: &mut Context) -> anyhow::Result<()> {
    let mut init = ObjectInitializer::new(ctx);
    init.function(NativeFunction::from_fn_ptr(doc_write), js_string!("write"), 1);
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
    WRITTEN.with(|w| {
        let mut w = w.borrow_mut();
        w.push_str(&s);
        w.push('\n');
    });
    Ok(JsValue::undefined())
}

// ---------------------------------------------------------------------------
// console global
// ---------------------------------------------------------------------------

fn setup_console(ctx: &mut Context) -> anyhow::Result<()> {
    let mut init = ObjectInitializer::new(ctx);
    init.function(NativeFunction::from_fn_ptr(console_log), js_string!("log"), 0);
    init.function(NativeFunction::from_fn_ptr(console_log), js_string!("warn"), 0);
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

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn js_first_arg_to_string(args: &[JsValue], ctx: &mut Context) -> JsResult<String> {
    args.first()
        .map(|v| v.to_string(ctx).map(|s| s.to_std_string_escaped()))
        .transpose()
        .map(|o| o.unwrap_or_default())
}
