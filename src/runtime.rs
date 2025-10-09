#[cfg(all(feature = "boa", feature = "rquickjs"))]
compile_error!("Enable only one JS engine at a time: 'boa' or 'rquickjs'");

#[cfg(not(any(feature = "boa", feature = "rquickjs")))]
compile_error!("Enable exactly one JS engine feature: 'boa' or 'rquickjs'");

// The JS bootstrap is embedded at compile time; `__HREF__` is substituted at runtime.
const BOOTSTRAP_TEMPLATE: &str = include_str!("bootstrap.js");

// Script run after user scripts to flush deferred callbacks and read the JS DOM state back.
const READBACK_JS: &str = r#"
(function() {
    // Flush deferred callbacks (setTimeout / requestAnimationFrame / MessageChannel / queueMicrotask).
    // Each flush pass can enqueue more callbacks; cap total iterations to avoid infinite loops.
    var maxPasses = 64;
    for (var pass = 0; pass < maxPasses && _r_timers.length > 0; pass++) {
        var batch = _r_timers.splice(0, _r_timers.length);
        for (var i = 0; i < batch.length; i++) {
            try { batch[i](); } catch(e) {
                if (typeof console !== 'undefined') console.error('[rakers timer error]', e && (e.message || String(e)));
            }
        }
    }

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

// ── boa engine ────────────────────────────────────────────────────────────────

#[cfg(feature = "boa")]
mod boa_rt {
    use std::cell::RefCell;

    use anyhow::anyhow;
    use boa_engine::{
        Context, JsResult, JsValue, NativeFunction, Source, js_string, object::ObjectInitializer,
        property::Attribute,
    };

    thread_local! {
        static WRITTEN:         RefCell<String>      = const { RefCell::new(String::new()) };
        static LOGGED:          RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
        static BODY_INNER_HTML: RefCell<String>      = const { RefCell::new(String::new()) };
    }

    pub struct JsRuntime;

    impl JsRuntime {
        pub fn new() -> Self {
            WRITTEN.with(|w| w.borrow_mut().clear());
            LOGGED.with(|l| l.borrow_mut().clear());
            BODY_INNER_HTML.with(|b| b.borrow_mut().clear());
            JsRuntime
        }

        pub fn execute(&self, scripts: &[String], page_url: Option<&str>) -> anyhow::Result<()> {
            let mut ctx = Context::default();
            ctx.runtime_limits_mut().set_stack_size_limit(65536);
            ctx.runtime_limits_mut().set_recursion_limit(65536);
            setup_document(&mut ctx)?;
            setup_console(&mut ctx)?;

            let bootstrap = super::make_bootstrap(page_url);
            ctx.eval(Source::from_bytes(bootstrap.as_bytes()))
                .map_err(|e| anyhow!("bootstrap error: {:?}", e))?;

            for script in scripts {
                if let Err(e) = ctx.eval(Source::from_bytes(script.as_bytes())) {
                    eprintln!("[js error] {:?}", e);
                }
            }

            let body_result = ctx.eval(Source::from_bytes(super::READBACK_JS.as_bytes()));
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

        pub fn written_html(&self) -> String {
            WRITTEN.with(|w| w.borrow().clone())
        }

        pub fn body_inner_html(&self) -> String {
            BODY_INNER_HTML.with(|b| b.borrow().clone())
        }

        pub fn logged_messages(&self) -> Vec<String> {
            LOGGED.with(|l| l.borrow().clone())
        }
    }

    fn setup_document(ctx: &mut Context) -> anyhow::Result<()> {
        let mut init = ObjectInitializer::new(ctx);
        init.function(
            NativeFunction::from_fn_ptr(doc_write),
            js_string!("write"),
            1,
        );
        init.function(
            NativeFunction::from_fn_ptr(doc_writeln),
            js_string!("writeln"),
            1,
        );
        let obj = init.build();
        ctx.register_global_property(js_string!("document"), obj, Attribute::all())
            .map_err(|e| anyhow!("{:?}", e))?;
        Ok(())
    }

    fn setup_console(ctx: &mut Context) -> anyhow::Result<()> {
        let mut init = ObjectInitializer::new(ctx);
        init.function(
            NativeFunction::from_fn_ptr(console_log),
            js_string!("log"),
            0,
        );
        init.function(
            NativeFunction::from_fn_ptr(console_log),
            js_string!("warn"),
            0,
        );
        init.function(
            NativeFunction::from_fn_ptr(console_log),
            js_string!("error"),
            0,
        );
        let obj = init.build();
        ctx.register_global_property(js_string!("console"), obj, Attribute::all())
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

    fn console_log(_this: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
        let parts: Vec<String> = args
            .iter()
            .map(|a| a.to_string(ctx).map(|s| s.to_std_string_escaped()))
            .collect::<Result<_, _>>()?;
        LOGGED.with(|l| l.borrow_mut().push(parts.join(" ")));
        Ok(JsValue::undefined())
    }

    fn js_first_arg_to_string(args: &[JsValue], ctx: &mut Context) -> JsResult<String> {
        args.first()
            .map(|v| v.to_string(ctx).map(|s| s.to_std_string_escaped()))
            .transpose()
            .map(|o| o.unwrap_or_default())
    }
}

#[cfg(feature = "boa")]
pub use boa_rt::JsRuntime;

// ── QuickJS engine ────────────────────────────────────────────────────────────

#[cfg(feature = "rquickjs")]
mod quickjs_rt {
    use std::cell::RefCell;

    use anyhow::anyhow;
    use rquickjs::{Context, Ctx, Function, Object, Runtime, Value};

    thread_local! {
        static WRITTEN:         RefCell<String>      = const { RefCell::new(String::new()) };
        static LOGGED:          RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
        static BODY_INNER_HTML: RefCell<String>      = const { RefCell::new(String::new()) };
    }

    pub struct JsRuntime;

    impl JsRuntime {
        pub fn new() -> Self {
            WRITTEN.with(|w| w.borrow_mut().clear());
            LOGGED.with(|l| l.borrow_mut().clear());
            BODY_INNER_HTML.with(|b| b.borrow_mut().clear());
            JsRuntime
        }

        pub fn execute(&self, scripts: &[String], page_url: Option<&str>) -> anyhow::Result<()> {
            let rt = Runtime::new().map_err(|e| anyhow!("quickjs runtime: {:?}", e))?;
            let ctx = Context::full(&rt).map_err(|e| anyhow!("quickjs context: {:?}", e))?;

            ctx.with(|ctx| -> anyhow::Result<()> {
                setup_document(ctx.clone())?;
                setup_console(ctx.clone())?;

                let bootstrap = super::make_bootstrap(page_url);
                ctx.eval::<Value, _>(bootstrap)
                    .map_err(|e| anyhow!("bootstrap error: {:?}", e))?;

                for script in scripts {
                    if ctx.eval::<Value, _>(script.as_str()).is_err() {
                        let exc = ctx.catch();
                        let msg = exc
                            .as_exception()
                            .and_then(|e| e.message())
                            .unwrap_or_else(|| "unknown exception".into());
                        eprintln!("[js error] {}", msg);
                    }
                }

                let body_html: String = ctx
                    .eval::<Value, _>(super::READBACK_JS)
                    .ok()
                    .and_then(|v| v.as_string().and_then(|s| s.to_string().ok()))
                    .unwrap_or_default();

                let body_html = match body_html.as_str() {
                    "undefined" | "null" | "" => String::new(),
                    s => s.to_owned(),
                };
                BODY_INNER_HTML.with(|b| *b.borrow_mut() = body_html);

                Ok(())
            })?;

            Ok(())
        }

        pub fn written_html(&self) -> String {
            WRITTEN.with(|w| w.borrow().clone())
        }

        pub fn body_inner_html(&self) -> String {
            BODY_INNER_HTML.with(|b| b.borrow().clone())
        }

        pub fn logged_messages(&self) -> Vec<String> {
            LOGGED.with(|l| l.borrow().clone())
        }
    }

    fn setup_document(ctx: Ctx<'_>) -> anyhow::Result<()> {
        let doc = Object::new(ctx.clone()).map_err(|e| anyhow!("{:?}", e))?;

        doc.set(
            "write",
            Function::new(ctx.clone(), |s: String| {
                WRITTEN.with(|w| w.borrow_mut().push_str(&s));
                Ok::<(), rquickjs::Error>(())
            })
            .map_err(|e| anyhow!("{:?}", e))?,
        )
        .map_err(|e| anyhow!("{:?}", e))?;

        doc.set(
            "writeln",
            Function::new(ctx.clone(), |s: String| {
                WRITTEN.with(|w| {
                    let mut w = w.borrow_mut();
                    w.push_str(&s);
                    w.push('\n');
                });
                Ok::<(), rquickjs::Error>(())
            })
            .map_err(|e| anyhow!("{:?}", e))?,
        )
        .map_err(|e| anyhow!("{:?}", e))?;

        ctx.globals()
            .set("document", doc)
            .map_err(|e| anyhow!("{:?}", e))?;
        Ok(())
    }

    fn setup_console(ctx: Ctx<'_>) -> anyhow::Result<()> {
        use rquickjs::function::Rest;

        let console = Object::new(ctx.clone()).map_err(|e| anyhow!("{:?}", e))?;

        let log_fn = Function::new(ctx.clone(), |args: Rest<rquickjs::Coerced<String>>| {
            let parts: Vec<String> = args.0.into_iter().map(|s| s.0).collect();
            LOGGED.with(|l| l.borrow_mut().push(parts.join(" ")));
            Ok::<(), rquickjs::Error>(())
        })
        .map_err(|e| anyhow!("{:?}", e))?;

        console
            .set("log", log_fn.clone())
            .map_err(|e| anyhow!("{:?}", e))?;
        console
            .set("warn", log_fn.clone())
            .map_err(|e| anyhow!("{:?}", e))?;
        console
            .set("error", log_fn)
            .map_err(|e| anyhow!("{:?}", e))?;

        ctx.globals()
            .set("console", console)
            .map_err(|e| anyhow!("{:?}", e))?;
        Ok(())
    }
}

#[cfg(feature = "rquickjs")]
pub use quickjs_rt::JsRuntime;
