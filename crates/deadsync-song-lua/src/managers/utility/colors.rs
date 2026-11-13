//! Existing Color constants, lookup and alpha helper used by theme commands.
use super::super::fields::{array, object, set};
use super::super::*;

pub(super) fn install(vm: &mut Vm) -> Result<(), Error> {
    let captures = [vm.get_global(b"tonumber")?, vm.get_global(b"setmetatable")?];
    let function = vm.new_host_function(initialize, &captures)?;
    let value = vm.call(function, &[])?[0];
    vm.set_global(b"Color", value)?;
    let alpha = vm.new_host_function(has_alpha, &captures[..1])?;
    vm.set_global(b"HasAlpha", alpha)?;
    for (source, name) in [
        (
            include_bytes!("native_hsv.lua").as_slice(),
            "native HSV color helpers",
        ),
        (
            include_bytes!("native_brightness.lua").as_slice(),
            "@theme:/Scripts/02 Colors.lua",
        ),
    ] {
        vm.execute(&deadlib_lua::compile_named(source, name.as_bytes())?)?;
    }
    Ok(())
}

fn initialize(call: &mut HostCall<'_>) -> Result<(), Error> {
    let color = object(call, |call, color| {
        for (name, text) in [
            ("Black", "0,0,0,1"),
            ("Blue", "#00aeef"),
            ("Green", "#39b54a"),
            ("HoloBlue", "#33B5E5"),
            ("HoloDarkBlue", "#0099CC"),
            ("HoloDarkGreen", "#669900"),
            ("HoloDarkOrange", "#FF8800"),
            ("HoloDarkPurple", "#9933CC"),
            ("HoloDarkRed", "#CC0000"),
            ("HoloGreen", "#99CC00"),
            ("HoloOrange", "#FFBB33"),
            ("HoloPurple", "#AA66CC"),
            ("HoloRed", "#FF4444"),
            ("Invisible", "1,1,1,0"),
            ("Orange", "#f7941d"),
            ("Outline", "0,0,0,0.5"),
            ("Purple", "#92278f"),
            ("Red", "#ed1c24"),
            ("Stealth", "0,0,0,0"),
            ("White", "1,1,1,1"),
            ("Yellow", "#fff200"),
        ] {
            let values = crate::parse_color_text(text).unwrap_or([1.; 4]);
            let values = array(call, values)?;
            call.with_roots(&[Value::Table(values)], |call| {
                set(color, name, values, call)
            })?;
        }
        let alpha = call.new_host_function(alpha, &[call.captures()[0]])?;
        call.with_roots(&[alpha], |call| set(color, "Alpha", alpha, call))?;
        let mt = object(call, |call, mt| {
            let lookup = call.new_host_function(lookup, &[Value::Table(color)])?;
            call.with_roots(&[lookup], |call| set(mt, "__call", lookup, call))
        })?;
        call.call(call.captures()[1], &[Value::Table(color), Value::Table(mt)])?;
        Ok(())
    })?;
    call.push(Value::Table(color))
}

fn alpha(call: &mut HostCall<'_>) -> Result<(), Error> {
    let value = call.arguments().first().copied().unwrap_or(Value::Nil);
    let alpha = call.number(1)? as f32;
    let mut components = components(call, value)?;
    components[3] = alpha;
    let result = array(call, components)?;
    call.push(Value::Table(result))
}

fn has_alpha(call: &mut HostCall<'_>) -> Result<(), Error> {
    let value = call.arguments().first().copied().unwrap_or(Value::Nil);
    let alpha = components(call, value)?[3];
    call.push(Value::Number(alpha as f64))
}

fn components(call: &mut HostCall<'_>, value: Value) -> Result<[f32; 4], Error> {
    let mut components = [1.; 4];
    if let Value::Table(table) = value {
        let mut parsed = [1.; 4];
        let mut valid = true;
        for (index, component) in parsed.iter_mut().enumerate() {
            let value = call.raw_table_get(table, Value::Number((index + 1) as f64))?;
            if index == 3 && value == Value::Nil {
                continue;
            }
            match call.call(call.captures()[0], &[value])?.first() {
                Some(Value::Number(n)) => *component = *n as f32,
                _ => {
                    valid = false;
                    break;
                }
            }
        }
        if valid {
            components = parsed;
        }
    } else if let Value::String(id) = value
        && let Some(text) = call
            .string_bytes(id)
            .and_then(|v| std::str::from_utf8(v).ok())
    {
        components = crate::parse_color_text(text).unwrap_or([1.; 4]);
    }
    Ok(components)
}

fn lookup(call: &mut HostCall<'_>) -> Result<(), Error> {
    table(
        call,
        call.arguments().first().copied().unwrap_or(Value::Nil),
    )?;
    let value = call.arguments().get(1).copied().unwrap_or(Value::Nil);
    let Some(text) = super::super::catalog::text(call, value) else {
        return call.push(Value::Nil);
    };
    let existing = call.table_get(call.captures()[0], value)?;
    if existing != Value::Nil {
        return call.push(existing);
    }
    let result = if let Some(color) = crate::parse_color_text(&text) {
        Value::Table(array(call, color)?)
    } else {
        Value::Nil
    };
    call.push(result)
}
