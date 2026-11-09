//! Physical display metadata and logical screen dimensions stay independent.
use super::*;
use deadlib_lua::Root;

fn new_table(vm: &mut Vm, roots: &mut Vec<Root>) -> Result<TableId, Error> {
    let table = vm.try_new_table()?;
    roots.push(vm.pin(Value::Table(table))?);
    Ok(table)
}

pub(super) fn install(vm: &mut Vm, context: &crate::SongLuaCompileContext) -> Result<(), Error> {
    let mut roots = Vec::new();
    let result = install_inner(vm, context, &mut roots);
    for root in roots {
        vm.unpin(root)?;
    }
    result
}

fn install_inner(
    vm: &mut Vm,
    context: &crate::SongLuaCompileContext,
    roots: &mut Vec<Root>,
) -> Result<(), Error> {
    let width = context.display_width.max(0.).round() as i32;
    let height = context.display_height.max(0.).round() as i32;
    let display = new_table(vm, roots)?;
    let specs = new_table(vm, roots)?;
    let spec = new_table(vm, roots)?;
    let modes = new_table(vm, roots)?;
    let mode = new_table(vm, roots)?;
    for (name, value) in [
        ("GetWidth", width),
        ("GetHeight", height),
        ("GetRefreshRate", 60),
    ] {
        constant_method(vm, mode, name, Value::Number(value as f64))?;
    }
    vm.raw_table_set(modes, Value::Number(1.), Value::Table(mode))?;
    for (name, text) in [("GetId", "Default"), ("GetName", "Default Display")] {
        let value = vm.try_intern(text.as_bytes())?;
        constant_method(vm, spec, name, value)?;
    }
    for (name, value) in [
        ("GetSupportedModes", Value::Table(modes)),
        ("GetCurrentMode", Value::Table(mode)),
        ("IsVirtual", Value::Boolean(false)),
    ] {
        constant_method(vm, spec, name, value)?;
    }
    vm.raw_table_set(specs, Value::Number(1.), Value::Table(spec))?;
    let mt = new_table(vm, roots)?;
    let text = vm.try_intern(b"DisplaySpecs: songlua")?;
    constant_method(vm, mt, "__tostring", text)?;
    let setmetatable = vm.get_global(b"setmetatable")?;
    vm.call(setmetatable, &[Value::Table(specs), Value::Table(mt)])?;
    for (name, value) in [
        ("GetDisplayWidth", Value::Number(width as f64)),
        ("GetDisplayHeight", Value::Number(height as f64)),
        ("GetFPS", Value::Number(60.)),
        ("GetVPF", Value::Number(1.)),
        ("GetCumFPS", Value::Number(60.)),
        ("GetDisplaySpecs", Value::Table(specs)),
        (
            "SupportsRenderToTexture",
            Value::Boolean(context.render_to_texture),
        ),
        ("SupportsFullscreenBorderlessWindow", Value::Boolean(false)),
    ] {
        constant_method(vm, display, name, value)?;
    }
    vm.set_global(b"DISPLAY", Value::Table(display))?;
    let width = context.screen_width.max(1.);
    let height = context.screen_height.max(1.);
    for (name, value) in [
        ("SCREEN_WIDTH", (width.round() as i32) as f64),
        ("SCREEN_HEIGHT", (height.round() as i32) as f64),
        ("SCREEN_CENTER_X", (0.5 * width) as f64),
        ("SCREEN_CENTER_Y", (0.5 * height) as f64),
        ("scx", (0.5 * width) as f64),
        ("scy", (0.5 * height) as f64),
        ("SCREEN_LEFT", 0.),
        ("SCREEN_TOP", 0.),
        ("SCREEN_RIGHT", (width.round() as i32) as f64),
        ("SCREEN_BOTTOM", (height.round() as i32) as f64),
        (
            "ASPECT_SCALE_FACTOR",
            (width / (640. * (height / 480.))) as f64,
        ),
    ] {
        vm.set_global(name.as_bytes(), Value::Number(value))?;
    }
    let screen = new_table(vm, roots)?;
    for (name, value) in [
        ("w", width),
        ("h", height),
        ("cx", 0.5 * width),
        ("cy", 0.5 * height),
        ("l", 0.),
        ("t", 0.),
        ("r", width),
        ("b", height),
    ] {
        super::super::put(vm, screen, name, Value::Number(value as f64))?;
    }
    vm.set_global(b"_screen", Value::Table(screen))?;
    let globals = vm.get_global(b"_G")?;
    let tonumber = vm.get_global(b"tonumber")?;
    for (name, operation) in [
        (
            "GetScreenAspectRatio",
            Operation::Aspect(context.display_aspect_ratio),
        ),
        ("WideScale", Operation::Wide(width)),
        ("SL_WideScale", Operation::Wide(width)),
        (
            "IsUsingWideScreen",
            Operation::IsWide(width / height.max(1.) > 1.5),
        ),
    ] {
        let method = vm.new_host_function_with_data(callback, &[globals, tonumber], operation)?;
        let root = vm.pin(method)?;
        let result = vm.set_global(name.as_bytes(), method);
        vm.unpin(root)?;
        result?;
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum Operation {
    Aspect(f32),
    Wide(f32),
    IsWide(bool),
}

fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
    let operation = *call.function_data::<Operation>().expect("screen binding");
    let value = match operation {
        Operation::Aspect(fallback) => {
            let key = call.try_intern(b"PREFSMAN")?;
            let prefs = call.table_get(call.captures()[0], key)?;
            call.with_roots(&[prefs], |call| {
                let prefs = table(call, prefs)?;
                let key = call.try_intern(b"__songlua_preference_values")?;
                let values = call.raw_table_get(prefs, key)?;
                let values = table(call, values)?;
                let key = call.try_intern(b"DisplayAspectRatio")?;
                let value = call.raw_table_get(values, key)?;
                let n = if value == Value::Nil {
                    fallback
                } else {
                    let converted = call.call(call.captures()[1], &[value])?[0];
                    let Value::Number(value) = converted else {
                        return Err(call.error(b"display aspect ratio must be a number"));
                    };
                    value as f32
                };
                Ok(Value::Number(n as f64))
            })?
        }
        Operation::Wide(width) => Value::Number(crate::scale_value(
            width,
            640.,
            854.,
            call.number(0)? as f32,
            call.number(1)? as f32,
        ) as f64),
        Operation::IsWide(wide) => Value::Boolean(wide),
    };
    call.push(value)
}
