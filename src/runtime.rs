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

pub struct JsRuntime;

impl JsRuntime {
    pub fn new() -> Self {
        WRITTEN.with(|w| w.borrow_mut().clear());
        LOGGED.with(|l| l.borrow_mut().clear());
        JsRuntime
    }

    pub fn execute(&self, scripts: &[String]) -> anyhow::Result<()> {
        let mut ctx = Context::default();
        setup_document(&mut ctx)?;
        setup_console(&mut ctx)?;

        for script in scripts {
            ctx.eval(Source::from_bytes(script.as_bytes()))
                .map_err(|e| anyhow!("JS error: {:?}", e))?;
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
// document global
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
    let s = js_args_to_string(args, ctx)?;
    WRITTEN.with(|w| w.borrow_mut().push_str(&s));
    Ok(JsValue::undefined())
}

fn doc_writeln(_this: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    let s = js_args_to_string(args, ctx)?;
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

fn js_args_to_string(args: &[JsValue], ctx: &mut Context) -> JsResult<String> {
    args.first()
        .map(|v| v.to_string(ctx).map(|s| s.to_std_string_escaped()))
        .transpose()
        .map(|o| o.unwrap_or_default())
}
