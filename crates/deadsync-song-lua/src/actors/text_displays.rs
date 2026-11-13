//! Existing text-display adapters whose methods are private instance bindings.
//! These classes inherit the public Actor table, not BitmapText's theme helpers.
use super::{arg, assets, float_scalar, get, helper, render, sprites, table, text};
use deadlib_lua::{Error, HostCall, TableId, Value};
use sprites::{field, set};

pub(super) const METHODS: &[&str] = &[
    "Load",
    "targetnumber",
    "SetTargetNumber",
    "GetTargetNumber",
    "settextf",
    "mult_attrs_with_diffuse",
];

pub(super) fn is_display(kind: &str) -> bool {
    matches!(
        kind,
        "RollingNumbers" | "BPMDisplay" | "DeviceList" | "InputList"
    )
}
pub(super) fn is_text(kind: &str) -> bool {
    kind == "BitmapText" || is_display(kind)
}

pub(super) fn install(call: &mut HostCall<'_>, actor: TableId) -> Result<(), Error> {
    let bound = get(call, actor, b"__songlua_bound_methods")?;
    let bound = table(call, bound)?;
    let helpers = table(call, call.captures()[1])?;
    let methods = get(call, helpers, b"text_display_methods")?;
    let methods = table(call, methods)?;
    for &name in text::METHODS.iter().chain(METHODS) {
        let method = get(call, methods, name.as_bytes())?;
        let captures = [
            call.captures()[0],
            call.captures()[1],
            Value::Table(actor),
            method,
        ];
        let function = call.new_host_function_with_data(super::invoke_bound, &captures, name)?;
        let key = call.try_intern(name.as_bytes())?;
        call.raw_table_set(bound, key, function)?;
    }
    Ok(())
}

fn formatted(call: &mut HostCall<'_>) -> Result<String, Error> {
    if call.arguments().len() <= 1 {
        return Ok(String::new());
    }
    let globals = table(call, helper(call, 5)?)?;
    let string = field(call, globals, "string")?;
    call.with_roots(&[string], |call| {
        let string = table(call, string)?;
        let function = field(call, string, "format")?;
        if !matches!(function, Value::Function(_) | Value::Native(_)) {
            return Err(call.error(b"string.format must be a function"));
        }
        let arguments = call.arguments()[1..].to_vec();
        let result = call
            .call(function, &arguments)?
            .first()
            .copied()
            .unwrap_or(Value::Nil);
        text::text_value(call, result)
    })
}

pub(super) fn method(call: &mut HostCall<'_>, actor: TableId, name: &str) -> Result<(), Error> {
    match name {
        "Load" => {
            let kind = assets::field(call, actor, b"__songlua_actor_type")?;
            if kind
                .as_deref()
                .is_some_and(|s| s.eq_ignore_ascii_case("RollingNumbers"))
            {
                if let Some(metric) = render::text(call, arg(call, 1)).map(str::to_owned) {
                    let value = call.try_intern(metric.as_bytes())?;
                    set(call, actor, "__songlua_rolling_numbers_metric", value)?;
                    let format =
                        call.try_intern(crate::rolling_numbers_format(&metric).as_bytes())?;
                    set(call, actor, "__songlua_rolling_numbers_format", format)?;
                }
            } else {
                return sprites::method(call, actor, "Load");
            }
        }
        "targetnumber" | "SetTargetNumber" => {
            if let Some(number) = float_scalar(call, arg(call, 1), false) {
                // Publish the target first. A failing format getter leaves that
                // new target in place while the previous Text is preserved.
                set(
                    call,
                    actor,
                    "__songlua_target_number",
                    Value::Number(number as f64),
                )?;
                let format = assets::field(call, actor, b"__songlua_rolling_numbers_format")?
                    .unwrap_or_else(|| "%.0f".into());
                let text = crate::format_rolling_number(&format, number);
                let value = call.try_intern(text.as_bytes())?;
                set(call, actor, "Text", value)?;
            }
        }
        "GetTargetNumber" => {
            let value = field(call, actor, "__songlua_target_number")?;
            let n = if value == Value::Nil {
                0.
            } else {
                sprites::number(call, value)? as f32
            };
            return call.push(Value::Number(n as f64));
        }
        "settextf" => {
            let text = formatted(call)?;
            let value = call.try_intern(text.as_bytes())?;
            set(call, actor, "Text", value)?;
        }
        "mult_attrs_with_diffuse" => {
            return text::method(call, actor, "set_mult_attrs_with_diffuse");
        }
        _ => unreachable!(),
    }
    call.push(Value::Table(actor))
}
