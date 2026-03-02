//! JS engine abstraction used by the rendering pipeline.
//!
//! Exposes a single type [`JsRuntime`] backed by whichever engine feature is
//! enabled at compile time (`rquickjs` or `boa`).  Both backends share the same
//! public interface: create a runtime, execute a list of scripts, then read back
//! the accumulated `document.write` output, `document.body.innerHTML`, and
//! `console` messages.

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

/// Produce the browser-globals bootstrap by substituting the page URL into the template.
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

    /// A sandboxed JavaScript execution context backed by boa_engine.
    pub struct JsRuntime;

    impl JsRuntime {
        /// Create a new runtime, clearing any leftover thread-local state from a previous run.
        pub fn new() -> Self {
            WRITTEN.with(|w| w.borrow_mut().clear());
            LOGGED.with(|l| l.borrow_mut().clear());
            BODY_INNER_HTML.with(|b| b.borrow_mut().clear());
            JsRuntime
        }

        /// Evaluate the browser bootstrap and then each script in `scripts` in order.
        ///
        /// Errors from individual scripts are printed to stderr and skipped; the method
        /// only returns `Err` if the bootstrap itself fails to evaluate.
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

        /// Return the accumulated output of all `document.write` / `document.writeln` calls.
        pub fn written_html(&self) -> String {
            WRITTEN.with(|w| w.borrow().clone())
        }

        /// Return the final value of `document.body.innerHTML` (or registry element content).
        pub fn body_inner_html(&self) -> String {
            BODY_INNER_HTML.with(|b| b.borrow().clone())
        }

        /// Return all messages logged via `console.log`, `console.warn`, or `console.error`.
        pub fn logged_messages(&self) -> Vec<String> {
            LOGGED.with(|l| l.borrow().clone())
        }
    }

    /// Register `document.write` and `document.writeln` on the boa context.
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

    /// Register `console.log`, `console.warn`, and `console.error` on the boa context.
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
    use std::time::{Duration, Instant};

    use anyhow::anyhow;
    use rquickjs::{
        Context, Ctx, Function, Module, Object, Runtime, Value, context::EvalOptions,
        loader::{Loader, Resolver},
    };

    // Per-script wall-clock timeout.  Scripts that run longer are interrupted and
    // reported as [js error] so the render pipeline can continue with the next script.
    const SCRIPT_TIMEOUT: Duration = Duration::from_secs(30);

    struct StubModuleSystem;

    impl Resolver for StubModuleSystem {
        fn resolve<'js>(&mut self, _ctx: &Ctx<'js>, _base: &str, name: &str) -> rquickjs::Result<String> {
            Ok(name.to_string())
        }
    }

    impl Loader for StubModuleSystem {
        fn load<'js>(&mut self, ctx: &Ctx<'js>, name: &str) -> rquickjs::Result<Module<'js>> {
            Module::declare(ctx.clone(), name, "export default {};")
        }
    }

    thread_local! {
        static WRITTEN:         RefCell<String>           = const { RefCell::new(String::new()) };
        static LOGGED:          RefCell<Vec<String>>      = const { RefCell::new(Vec::new()) };
        static BODY_INNER_HTML: RefCell<String>           = const { RefCell::new(String::new()) };
        // Deadline for the currently-executing script; None means no limit active.
        static SCRIPT_DEADLINE: RefCell<Option<Instant>>  = RefCell::new(None);
    }

    fn set_deadline(timeout: Duration) {
        SCRIPT_DEADLINE.with(|d| *d.borrow_mut() = Some(Instant::now() + timeout));
    }

    fn clear_deadline() {
        SCRIPT_DEADLINE.with(|d| *d.borrow_mut() = None);
    }

    /// A sandboxed JavaScript execution context backed by QuickJS (via rquickjs).
    pub struct JsRuntime {
        timeout: Option<Duration>,
    }

    impl JsRuntime {
        /// Create a new runtime with the default 30-second per-script timeout.
        pub fn new() -> Self {
            Self::with_timeout(SCRIPT_TIMEOUT)
        }

        /// Create a new runtime with a custom per-script timeout (useful in tests).
        pub fn with_timeout(timeout: Duration) -> Self {
            WRITTEN.with(|w| w.borrow_mut().clear());
            LOGGED.with(|l| l.borrow_mut().clear());
            BODY_INNER_HTML.with(|b| b.borrow_mut().clear());
            JsRuntime { timeout: Some(timeout) }
        }

        /// Create a new runtime with no per-script timeout.
        pub fn without_timeout() -> Self {
            WRITTEN.with(|w| w.borrow_mut().clear());
            LOGGED.with(|l| l.borrow_mut().clear());
            BODY_INNER_HTML.with(|b| b.borrow_mut().clear());
            JsRuntime { timeout: None }
        }

        /// Evaluate the browser bootstrap and then each script in `scripts` in order.
        ///
        /// Scripts are evaluated in sloppy (non-strict) mode to match browser behaviour —
        /// assignments to undeclared globals are allowed, as used by SvelteKit and webpack.
        /// Errors from individual scripts are printed to stderr and skipped; the method
        /// only returns `Err` if the bootstrap itself fails to evaluate.
        pub fn execute(&self, scripts: &[String], page_url: Option<&str>) -> anyhow::Result<()> {
            let rt = Runtime::new().map_err(|e| anyhow!("quickjs runtime: {:?}", e))?;
            rt.set_loader(StubModuleSystem, StubModuleSystem);

            // Check the per-script deadline every 10 000 opcodes to keep overhead near zero.
            rt.set_interrupt_handler(Some(Box::new({
                let mut counter = 0u32;
                move || {
                    counter = counter.wrapping_add(1);
                    if counter % 10_000 != 0 {
                        return false;
                    }
                    SCRIPT_DEADLINE.with(|d| {
                        d.borrow().map_or(false, |dl| Instant::now() > dl)
                    })
                }
            })));

            let ctx = Context::full(&rt).map_err(|e| anyhow!("quickjs context: {:?}", e))?;

            ctx.with(|ctx| -> anyhow::Result<()> {
                setup_document(ctx.clone())?;
                setup_console(ctx.clone())?;

                let sloppy = || {
                    let mut o = EvalOptions::default();
                    o.strict = false;
                    o
                };

                let bootstrap = super::make_bootstrap(page_url);
                ctx.eval_with_options::<Value, _>(bootstrap, sloppy())
                    .map_err(|e| anyhow!("bootstrap error: {:?}", e))?;

                for script in scripts {
                    if let Some(t) = self.timeout { set_deadline(t); }
                    let result = ctx.eval_with_options::<Value, _>(script.as_str(), sloppy());
                    clear_deadline();
                    if result.is_err() {
                        let exc = ctx.catch();
                        let msg = exc
                            .as_exception()
                            .and_then(|e| e.message())
                            .unwrap_or_else(|| "unknown exception".into());
                        eprintln!("[js error] {}", msg);
                    }
                    // Drain the QuickJS pending-job queue (Promise microtasks) after each
                    // script so that .then() chains fire before the next script runs.
                    while ctx.execute_pending_job() {}
                }

                if let Some(t) = self.timeout { set_deadline(t); }
                let body_html: String = ctx
                    .eval_with_options::<Value, _>(super::READBACK_JS, sloppy())
                    .ok()
                    .and_then(|v| v.as_string().and_then(|s| s.to_string().ok()))
                    .unwrap_or_default();
                clear_deadline();

                let body_html = match body_html.as_str() {
                    "undefined" | "null" | "" => String::new(),
                    s => s.to_owned(),
                };
                BODY_INNER_HTML.with(|b| *b.borrow_mut() = body_html);

                Ok(())
            })?;

            Ok(())
        }

        /// Return the accumulated output of all `document.write` / `document.writeln` calls.
        pub fn written_html(&self) -> String {
            WRITTEN.with(|w| w.borrow().clone())
        }

        /// Return the final value of `document.body.innerHTML` (or registry element content).
        pub fn body_inner_html(&self) -> String {
            BODY_INNER_HTML.with(|b| b.borrow().clone())
        }

        /// Return all messages logged via `console.log`, `console.warn`, or `console.error`.
        pub fn logged_messages(&self) -> Vec<String> {
            LOGGED.with(|l| l.borrow().clone())
        }
    }

    /// Register `document.write` and `document.writeln` on the QuickJS context.
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

    /// Register `console.log`, `console.warn`, and `console.error` on the QuickJS context.
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
