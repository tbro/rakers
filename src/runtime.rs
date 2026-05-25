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

// Flush one batch of _r_timers; returns the number of timers remaining after the flush.
// Called in a Rust loop so execute_pending_job() can drain Promise microtasks between passes.
const TIMER_FLUSH_JS: &str = r"
(function() {
    if (_r_timers.length === 0) return 0;
    var batch = _r_timers.splice(0, _r_timers.length);
    for (var i = 0; i < batch.length; i++) {
        try { batch[i](); } catch(e) {
            if (typeof console !== 'undefined') console.error('[rakers timer error]', e && (e.message || String(e)));
        }
    }
    return _r_timers.length;
})()
";

// Read the rendered DOM state after all timers and microtasks have been flushed.
const READBACK_JS: &str = r"
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
";

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
        // HttpConfig fields for XHR/_r_fetch_sync
        static XHR_UA:          RefCell<Option<String>> = const { RefCell::new(None) };
        static XHR_HEADERS:     RefCell<Vec<(String, String)>> = const { RefCell::new(Vec::new()) };
        static XHR_PROXY:       RefCell<Option<String>> = const { RefCell::new(None) };
        static XHR_TIMEOUT:     RefCell<Option<std::time::Duration>> = const { RefCell::new(None) };
        static XHR_FORWARD_HEADERS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }

    /// A sandboxed JavaScript execution context.
    pub struct JsRuntime;

    impl JsRuntime {
        /// Create a new runtime with a custom per-script timeout.
        ///
        /// Boa has no interrupt-handler API, so the timeout is accepted but not enforced.
        pub fn with_timeout(_timeout: std::time::Duration) -> Self {
            Self::new()
        }

        /// Create a new runtime with no per-script timeout.
        pub fn without_timeout() -> Self {
            Self::new()
        }

        fn new() -> Self {
            WRITTEN.with(|w| w.borrow_mut().clear());
            LOGGED.with(|l| l.borrow_mut().clear());
            BODY_INNER_HTML.with(|b| b.borrow_mut().clear());
            JsRuntime
        }

        /// Evaluate the browser bootstrap and then each script in `scripts` in order.
        ///
        /// Errors from individual scripts are printed to stderr and skipped; the method
        /// only returns `Err` if the bootstrap itself fails to evaluate.
        pub fn execute(
            &self,
            scripts: &[String],
            page_url: Option<&str>,
            _cfg: &crate::HttpConfig,
        ) -> anyhow::Result<()> {
            let mut ctx = Context::default();
            ctx.runtime_limits_mut().set_stack_size_limit(65536);
            ctx.runtime_limits_mut().set_recursion_limit(65536);
            setup_document(&mut ctx)?;
            setup_console(&mut ctx)?;
            // Register sync XHR fetch helper so bootstrap's XHR/send and appendChild can synchronously fetch resources.
            setup_xhr_fetch(&mut ctx)?;

            let bootstrap = super::make_bootstrap(page_url);
            ctx.eval(Source::from_bytes(bootstrap.as_bytes()))
                .map_err(|e| anyhow!("bootstrap error: {:?}", e))?;

            // Set XHR config from provided HttpConfig so _r_fetch_sync can use it.
            XHR_UA.with(|u| u.borrow_mut().clone_from(&_cfg.user_agent));
            XHR_HEADERS.with(|h| h.borrow_mut().clone_from(&_cfg.headers));
            XHR_PROXY.with(|p| p.borrow_mut().clone_from(&_cfg.proxy));
            XHR_TIMEOUT.with(|t| *t.borrow_mut() = None);
            XHR_FORWARD_HEADERS.with(|f| f.set(false));

            for script in scripts {
                if let Err(e) = ctx.eval(Source::from_bytes(script.as_bytes())) {
                    eprintln!("[js error] {:?}", e);
                }
            }

            // Flush timers in passes (boa has no separate microtask drain API).
            for _ in 0..64 {
                let remaining: i32 = ctx
                    .eval(Source::from_bytes(super::TIMER_FLUSH_JS.as_bytes()))
                    .ok()
                    .and_then(|v| v.to_number(&mut ctx).ok())
                    .map(|n| n as i32)
                    .unwrap_or(0);
                if remaining == 0 {
                    break;
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
        pub fn written_html() -> String {
            WRITTEN.with(|w| w.borrow().clone())
        }

        /// Return the final value of `document.body.innerHTML` (or registry element content).
        pub fn body_inner_html() -> String {
            BODY_INNER_HTML.with(|b| b.borrow().clone())
        }

        /// Return all messages logged via `console.log`, `console.warn`, or `console.error`.
        pub fn logged_messages() -> Vec<String> {
            LOGGED.with(|l| l.borrow().clone())
        }
    }

    /// Register `document.write` and `document.writeln`.
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

    /// Register `console.log`, `console.warn`, and `console.error`.
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

    // Synchronous fetch exposed to JS as `_r_fetch_sync(url)` so the bootstrap can
    // perform blocking template fetches for frameworks that rely on sync XHR.
    fn boa_fetch_sync(_this: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
        let url = if let Some(first) = args.first() {
            match first.to_string(ctx) {
                Ok(sv) => sv.to_std_string_escaped(),
                Err(_) => String::new(),
            }
        } else {
            String::new()
        };
        if url.is_empty() || !(url.starts_with("http://") || url.starts_with("https://")) {
            return Ok(JsValue::undefined());
        }
        let ua = XHR_UA.with(|u| u.borrow().clone());
        let headers = XHR_HEADERS.with(|h| h.borrow().clone());
        let proxy = XHR_PROXY.with(|p| p.borrow().clone());
        let timeout = XHR_TIMEOUT.with(|t| *t.borrow());
        let forward = XHR_FORWARD_HEADERS.with(std::cell::Cell::get);

        let mut builder = ureq::AgentBuilder::new();
        if let Some(ref proxy_url) = proxy {
            if let Ok(p) = ureq::Proxy::new(proxy_url) {
                builder = builder.proxy(p);
            }
        }
        if let Some(dur) = timeout {
            builder = builder.timeout(dur);
        }
        let agent = builder.build();
        let mut req = agent.get(&url);
        if let Some(ref ua_str) = ua {
            req = req.set("User-Agent", ua_str);
        }
        if forward {
            for (name, value) in &headers {
                req = req.set(name, value);
            }
        }
        let body = match req.call() {
            Ok(resp) => resp.into_string().unwrap_or_default(),
            Err(_) => String::new(),
        };
        if body.is_empty() {
            Ok(JsValue::undefined())
        } else {
            Ok(js_string!(body).into())
        }
    }

    fn setup_xhr_fetch(ctx: &mut Context) -> anyhow::Result<()> {
        // Build a temporary object with the native function and assign its field to the global name.
        let mut init = ObjectInitializer::new(ctx);
        init.function(NativeFunction::from_fn_ptr(boa_fetch_sync), js_string!("f"), 1);
        let obj = init.build();
        ctx.register_global_property(js_string!("__r_fetch_tmp"), obj, Attribute::all())
            .map_err(|e| anyhow!("{e:?}"))?;
        // Execute JS to move the function to a true global function and delete the temp.
        ctx.eval(Source::from_bytes(b"(function(){this._r_fetch_sync = __r_fetch_tmp.f; try{ delete __r_fetch_tmp; }catch(e){} })();")).map_err(|e| anyhow!("register fetch fn failed: {e:?}"))?;
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
        Context, Ctx, Function, Module, Object, Runtime, Value,
        context::EvalOptions,
        loader::{Loader, Resolver},
    };

    struct StubModuleSystem;

    impl Resolver for StubModuleSystem {
        fn resolve(&mut self, _ctx: &Ctx<'_>, _base: &str, name: &str) -> rquickjs::Result<String> {
            Ok(name.to_string())
        }
    }

    impl Loader for StubModuleSystem {
        fn load<'js>(&mut self, ctx: &Ctx<'js>, name: &str) -> rquickjs::Result<Module<'js>> {
            Module::declare(ctx.clone(), name, "export default {};")
        }
    }

    thread_local! {
        static WRITTEN:         RefCell<String>                        = const { RefCell::new(String::new()) };
        static LOGGED:          RefCell<Vec<String>>                   = const { RefCell::new(Vec::new()) };
        static BODY_INNER_HTML: RefCell<String>                        = const { RefCell::new(String::new()) };
        // Deadline for the currently-executing script; None means no limit active.
        static SCRIPT_DEADLINE: RefCell<Option<Instant>>               = const { RefCell::new(None) };
        // HttpConfig fields stored so the _r_fetch_sync native function can use them.
        static XHR_UA:          RefCell<Option<String>>                = const { RefCell::new(None) };
        static XHR_HEADERS:     RefCell<Vec<(String, String)>>         = const { RefCell::new(Vec::new()) };
        static XHR_PROXY:       RefCell<Option<String>>                = const { RefCell::new(None) };
        // Per-request timeout applied to XHR fetches; mirrors the script timeout.
        static XHR_TIMEOUT:          RefCell<Option<Duration>>         = const { RefCell::new(None) };
        // Whether to forward custom -H headers on XHR requests (off by default).
        static XHR_FORWARD_HEADERS:  std::cell::Cell<bool>             = const { std::cell::Cell::new(false) };
    }

    fn set_deadline(timeout: Duration) {
        SCRIPT_DEADLINE.with(|d| *d.borrow_mut() = Some(Instant::now() + timeout));
    }

    fn clear_deadline() {
        SCRIPT_DEADLINE.with(|d| *d.borrow_mut() = None);
    }

    /// A sandboxed JavaScript execution context.
    pub struct JsRuntime {
        timeout: Option<Duration>,
    }

    impl JsRuntime {
        /// Create a new runtime with a custom per-script timeout (useful in tests).
        pub fn with_timeout(timeout: Duration) -> Self {
            WRITTEN.with(|w| w.borrow_mut().clear());
            LOGGED.with(|l| l.borrow_mut().clear());
            BODY_INNER_HTML.with(|b| b.borrow_mut().clear());
            JsRuntime {
                timeout: Some(timeout),
            }
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
        /// assignments to undeclared globals are allowed, as used by `SvelteKit` and webpack.
        /// Errors from individual scripts are printed to stderr and skipped; the method
        /// only returns `Err` if the bootstrap itself fails to evaluate.
        pub fn execute(
            &self,
            scripts: &[String],
            page_url: Option<&str>,
            cfg: &crate::HttpConfig,
        ) -> anyhow::Result<()> {
            XHR_UA.with(|u| u.borrow_mut().clone_from(&cfg.user_agent));
            XHR_HEADERS.with(|h| h.borrow_mut().clone_from(&cfg.headers));
            XHR_PROXY.with(|p| p.borrow_mut().clone_from(&cfg.proxy));
            XHR_TIMEOUT.with(|t| *t.borrow_mut() = self.timeout);
            XHR_FORWARD_HEADERS.with(|f| f.set(cfg.forward_headers));

            let rt = Runtime::new().map_err(|e| anyhow!("quickjs runtime: {e:?}"))?;
            rt.set_loader(StubModuleSystem, StubModuleSystem);

            // Check the per-script deadline every 10 000 opcodes to keep overhead near zero.
            rt.set_interrupt_handler(Some(Box::new({
                let mut counter = 0u32;
                move || {
                    counter = counter.wrapping_add(1);
                    if !counter.is_multiple_of(10_000) {
                        return false;
                    }
                    SCRIPT_DEADLINE.with(|d| d.borrow().is_some_and(|dl| Instant::now() > dl))
                }
            })));

            let ctx = Context::full(&rt).map_err(|e| anyhow!("quickjs context: {e:?}"))?;

            ctx.with(|ctx| -> anyhow::Result<()> {
                setup_document(&ctx)?;
                setup_console(&ctx)?;
                setup_xhr_fetch(&ctx)?;

                let sloppy = || {
                    let mut o = EvalOptions::default();
                    o.strict = false;
                    o
                };

                let bootstrap = super::make_bootstrap(page_url);
                ctx.eval_with_options::<Value, _>(bootstrap, sloppy())
                    .map_err(|e| anyhow!("bootstrap error: {e:?}"))?;

                for script in scripts {
                    if let Some(t) = self.timeout {
                        set_deadline(t);
                    }
                    let result = ctx.eval_with_options::<Value, _>(script.as_str(), sloppy());
                    clear_deadline();
                    if result.is_err() {
                        let exc = ctx.catch();
                        if let Some(e) = exc.as_exception() {
                            let msg = e.message().unwrap_or_else(|| "unknown exception".into());
                            eprintln!("[js error] {msg}");
                            if crate::is_verbose()
                                && let Some(stack) = e.stack()
                            {
                                eprintln!("[js stack] {stack}");
                            }
                        }
                    }
                    // Drain Promise microtasks after each script so .then() chains fire
                    // before the next script runs.
                    while ctx.execute_pending_job() {}
                }

                // Flush _r_timers in a Rust loop, draining Promise microtasks between passes.
                // Ember/Glimmer's Backburner run loop schedules rendering via Promise chains,
                // so both queues must be drained together until both are empty.
                let mut consecutive_empty = 0u32;
                for _ in 0..128u32 {
                    if let Some(t) = self.timeout {
                        set_deadline(t);
                    }
                    let remaining: i32 = ctx
                        .eval_with_options::<Value, _>(super::TIMER_FLUSH_JS, sloppy())
                        .ok()
                        .and_then(|v| v.as_int())
                        .unwrap_or(0);
                    clear_deadline();
                    let mut had_jobs = false;
                    while ctx.execute_pending_job() {
                        had_jobs = true;
                    }
                    if remaining == 0 && !had_jobs {
                        consecutive_empty += 1;
                        if consecutive_empty >= 3 {
                            break;
                        }
                    } else {
                        consecutive_empty = 0;
                    }
                }

                if let Some(t) = self.timeout {
                    set_deadline(t);
                }
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
        pub fn written_html() -> String {
            WRITTEN.with(|w| w.borrow().clone())
        }

        /// Return the final value of `document.body.innerHTML` (or registry element content).
        pub fn body_inner_html() -> String {
            BODY_INNER_HTML.with(|b| b.borrow().clone())
        }

        /// Return all messages logged via `console.log`, `console.warn`, or `console.error`.
        pub fn logged_messages() -> Vec<String> {
            LOGGED.with(|l| l.borrow().clone())
        }
    }

    /// Register `document.write` and `document.writeln`.
    fn setup_document(ctx: &Ctx<'_>) -> anyhow::Result<()> {
        let doc = Object::new(ctx.clone()).map_err(|e| anyhow!("{e:?}"))?;

        doc.set(
            "write",
            Function::new(ctx.clone(), |s: String| {
                WRITTEN.with(|w| w.borrow_mut().push_str(&s));
                Ok::<(), rquickjs::Error>(())
            })
            .map_err(|e| anyhow!("{e:?}"))?,
        )
        .map_err(|e| anyhow!("{e:?}"))?;

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
            .map_err(|e| anyhow!("{e:?}"))?,
        )
        .map_err(|e| anyhow!("{e:?}"))?;

        ctx.globals()
            .set("document", doc)
            .map_err(|e| anyhow!("{e:?}"))?;
        Ok(())
    }

    /// Register `console.log`, `console.warn`, and `console.error`.
    fn setup_console(ctx: &Ctx<'_>) -> anyhow::Result<()> {
        use rquickjs::function::Rest;

        let console = Object::new(ctx.clone()).map_err(|e| anyhow!("{e:?}"))?;

        let log_fn = Function::new(ctx.clone(), |args: Rest<rquickjs::Coerced<String>>| {
            let parts: Vec<String> = args.0.into_iter().map(|s| s.0).collect();
            LOGGED.with(|l| l.borrow_mut().push(parts.join(" ")));
            Ok::<(), rquickjs::Error>(())
        })
        .map_err(|e| anyhow!("{e:?}"))?;

        let noop_fn = Function::new(ctx.clone(), || Ok::<(), rquickjs::Error>(()))
            .map_err(|e| anyhow!("{e:?}"))?;

        console
            .set("log", log_fn.clone())
            .map_err(|e| anyhow!("{e:?}"))?;
        console
            .set("warn", log_fn.clone())
            .map_err(|e| anyhow!("{e:?}"))?;
        console
            .set("error", log_fn.clone())
            .map_err(|e| anyhow!("{e:?}"))?;
        console
            .set("info", log_fn.clone())
            .map_err(|e| anyhow!("{e:?}"))?;
        console.set("debug", log_fn).map_err(|e| anyhow!("{e:?}"))?;
        console
            .set("table", noop_fn.clone())
            .map_err(|e| anyhow!("{e:?}"))?;
        console
            .set("group", noop_fn.clone())
            .map_err(|e| anyhow!("{e:?}"))?;
        console
            .set("groupEnd", noop_fn.clone())
            .map_err(|e| anyhow!("{e:?}"))?;
        console
            .set("groupCollapsed", noop_fn.clone())
            .map_err(|e| anyhow!("{e:?}"))?;
        console
            .set("time", noop_fn.clone())
            .map_err(|e| anyhow!("{e:?}"))?;
        console
            .set("timeEnd", noop_fn.clone())
            .map_err(|e| anyhow!("{e:?}"))?;
        console
            .set("assert", noop_fn)
            .map_err(|e| anyhow!("{e:?}"))?;

        ctx.globals()
            .set("console", console)
            .map_err(|e| anyhow!("{e:?}"))?;
        Ok(())
    }

    /// Register `_r_fetch_sync(url)` — a synchronous HTTP GET used by the XHR stub
    /// so frameworks that XHR-load templates (e.g. `RiotJS`) get real response bodies.
    fn setup_xhr_fetch(ctx: &Ctx<'_>) -> anyhow::Result<()> {
        let fetch_fn = Function::new(ctx.clone(), |url: String| -> String {
            let ua = XHR_UA.with(|u| u.borrow().clone());
            let headers = XHR_HEADERS.with(|h| h.borrow().clone());
            let proxy = XHR_PROXY.with(|p| p.borrow().clone());
            let timeout = XHR_TIMEOUT.with(|t| *t.borrow());
            let forward_hdrs = XHR_FORWARD_HEADERS.with(std::cell::Cell::get);
            if !url.starts_with("http://") && !url.starts_with("https://") {
                return String::new();
            }
            let mut builder = ureq::AgentBuilder::new();
            if let Some(ref proxy_url) = proxy
                && let Ok(p) = ureq::Proxy::new(proxy_url)
            {
                builder = builder.proxy(p);
            }
            if let Some(dur) = timeout {
                builder = builder.timeout(dur);
            }
            let agent = builder.build();
            let mut req = agent.get(&url);
            if let Some(ref ua_str) = ua {
                req = req.set("User-Agent", ua_str);
            }
            if forward_hdrs {
                for (name, value) in &headers {
                    req = req.set(name, value);
                }
            }
            match req.call() {
                Ok(resp) => resp.into_string().unwrap_or_default(),
                Err(_) => String::new(),
            }
        })
        .map_err(|e| anyhow!("{e:?}"))?;

        ctx.globals()
            .set("_r_fetch_sync", fetch_fn)
            .map_err(|e| anyhow!("{e:?}"))?;
        Ok(())
    }
}

#[cfg(feature = "rquickjs")]
pub use quickjs_rt::JsRuntime;
