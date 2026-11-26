//! The existing three-axis NoteColumn spline API and shared gameplay solver.
use super::*;
use deadlib_lua::NativeStack;
use sprites::set;

#[derive(Default)]
pub(super) struct Workspace {
    points: Vec<[f32; 3]>,
    solver: deadsync_gameplay::SongLuaSplineSolver,
}

pub(super) fn handler(call: &mut HostCall<'_>) -> Result<TableId, Error> {
    let owner = call.try_new_table()?;
    call.with_roots(&[Value::Table(owner)], |call| {
        let spline = spline(call)?;
        set(call, owner, "__songlua_spline", Value::Table(spline))?;
        let mode = call.intern_static(b"NoteColumnSplineMode_Disabled")?;
        set(call, owner, "__songlua_spline_mode", mode)?;
        set(
            call,
            owner,
            "__songlua_subtract_song_beat",
            Value::Boolean(true),
        )?;
        set(call, owner, "__songlua_receptor_t", Value::Number(0.))?;
        set(call, owner, "__songlua_beats_per_t", Value::Number(1.))?;
        let getter = call.new_host_function(
            |call| call.push(call.captures()[0]),
            &[Value::Table(spline)],
        )?;
        set(call, owner, "GetSpline", getter)?;
        set(call, owner, "get_spline", getter)?;
        for (name, alias) in [
            ("SetSplineMode", "set_spline_mode"),
            ("SetSubtractSongBeat", "set_subtract_song_beat"),
            ("SetReceptorT", "set_receptor_t"),
            ("SetBeatsPerT", "set_beats_per_t"),
        ] {
            let function =
                call.new_host_function_with_data(handler_method, &[Value::Table(owner)], name)?;
            set(call, owner, name, function)?;
            set(call, owner, alias, function)?;
        }
        Ok(owner)
    })
}

// read_f32 differs from numeric coercion: finite numeric values, but ordinary
// Rust float strings (including inf/nan), and no Lua hexadecimal string syntax.
pub(super) fn optional_float(call: &HostCall<'_>, value: Value) -> Option<f32> {
    match value {
        Value::Number(value) => (value as f32).is_finite().then_some(value as f32),
        Value::String(id) => std::str::from_utf8(call.string_bytes(id)?)
            .ok()?
            .trim()
            .parse()
            .ok(),
        _ => None,
    }
}
fn handler_method(call: &mut HostCall<'_>) -> Result<(), Error> {
    let owner = table(call, call.captures()[0])?;
    let name = *call.function_data::<&'static str>().unwrap();
    let value = arg(call, 1);
    match name {
        "SetSplineMode" => {
            if let Value::String(id) = value
                && call
                    .string_bytes(id)
                    .is_some_and(|s| std::str::from_utf8(s).is_ok())
            {
                set(call, owner, "__songlua_spline_mode", value)?;
            }
        }
        "SetSubtractSongBeat" => set(
            call,
            owner,
            "__songlua_subtract_song_beat",
            Value::Boolean(!matches!(value, Value::Nil | Value::Boolean(false))),
        )?,
        "SetReceptorT" | "SetBeatsPerT" => {
            if let Some(value) = optional_float(call, value) {
                set(
                    call,
                    owner,
                    if name == "SetReceptorT" {
                        "__songlua_receptor_t"
                    } else {
                        "__songlua_beats_per_t"
                    },
                    Value::Number(value as f64),
                )?;
            }
        }
        _ => unreachable!(),
    }
    call.push(Value::Table(owner))
}

fn spline(call: &mut HostCall<'_>) -> Result<TableId, Error> {
    let owner = call.try_new_table()?;
    call.with_roots(&[Value::Table(owner)], |call| {
        set(call, owner, "__songlua_spline_size", Value::Number(0.))?;
        set(call, owner, "__songlua_spline_loop", Value::Boolean(false))?;
        for key in ["__songlua_spline_points", "__songlua_spline_coefficients"] {
            let values = call.try_new_table()?;
            set(call, owner, key, Value::Table(values))?;
        }
        for (name, alias) in [
            ("evaluate", "Evaluate"),
            ("SetSize", "set_size"),
            ("GetSize", "get_size"),
            ("SetPoint", "set_point"),
            ("Solve", "solve"),
            ("SetPolygonal", "set_polygonal"),
            ("SetLoop", "set_loop"),
            ("get_loop", "get_loop"),
        ] {
            let function =
                call.new_host_function_with_data(spline_method, &[Value::Table(owner)], name)?;
            set(call, owner, name, function)?;
            if alias != name {
                set(call, owner, alias, function)?;
            }
        }
        Ok(owner)
    })
}

fn table_at(stack: &NativeStack<'_, '_>, index: isize) -> Result<(), Error> {
    if matches!(stack.value(index), Some(Value::Table(_))) {
        Ok(())
    } else {
        Err(stack.error(b"spline table expected"))
    }
}
fn integer(stack: &NativeStack<'_, '_>, index: isize, unsigned: bool) -> Result<f64, Error> {
    let n = stack.number(index)?.trunc();
    let valid = if unsigned {
        (0.0..(usize::MAX as f64)).contains(&n)
    } else {
        (i64::MIN as f64..-(i64::MIN as f64)).contains(&n)
    };
    if valid {
        Ok(n)
    } else {
        Err(stack.error(b"spline integer is out of range"))
    }
}
fn assign(
    stack: &mut NativeStack<'_, '_>,
    owner: usize,
    key: &[u8],
    value: Value,
) -> Result<(), Error> {
    stack.push_bytes(key)?;
    stack.push(value)?;
    stack.set_table(owner as isize, false)
}
fn raw_index(stack: &mut NativeStack<'_, '_>, owner: usize, index: usize) -> Result<(), Error> {
    if let Ok(index) = i32::try_from(index) {
        return stack.raw_get_integer(owner as isize, index);
    }
    stack.push(Value::Number(index as f64))?;
    stack.get_table(owner as isize, true)
}
fn raw_assign(
    stack: &mut NativeStack<'_, '_>,
    owner: usize,
    index: usize,
    value: Value,
) -> Result<(), Error> {
    if let Ok(index) = i32::try_from(index) {
        stack.push(value)?;
        return stack.raw_set_integer(owner as isize, index);
    }
    stack.push(Value::Number(index as f64))?;
    stack.push(value)?;
    stack.set_table(owner as isize, true)
}
fn point(stack: &mut NativeStack<'_, '_>, owner: usize, index: usize) -> Result<[f32; 3], Error> {
    let base = stack.top();
    raw_index(stack, owner, index)?;
    let mut out = [0.; 3];
    if stack.value(-1) != Some(Value::Nil) {
        table_at(stack, -1)?;
        let point = stack.top();
        for (axis, value) in out.iter_mut().enumerate() {
            raw_index(stack, point, axis + 1)?;
            if stack.value(-1) != Some(Value::Nil) {
                *value = stack.number(-1)? as f32;
            }
            stack.set_top(point)?;
        }
    }
    stack.set_top(base)?;
    Ok(out)
}
fn push_vector(stack: &mut NativeStack<'_, '_>, values: [f32; 3]) -> Result<(), Error> {
    stack.create_number_array(&values.map(f64::from))
}

fn spline_method(call: &mut HostCall<'_>) -> Result<(), Error> {
    let name = *call.function_data::<&'static str>().unwrap();
    let owner = call.captures()[0];
    if name == "Solve" && let Value::Table(table) = owner {
        let mut workspace = std::mem::take(&mut call.store().spline_workspace);
        let result = solve_numeric(call, table, &mut workspace);
        call.store().spline_workspace = workspace;
        if result? {
            return Ok(());
        }
    }
    if name == "SetPoint"
        && let Value::Table(table) = owner
        && set_numeric_point(call, table)?
    {
        return Ok(());
    }
    // Reserve argument slots before adding temporaries. Native spline methods
    // always use argument two, even for detached or non-table receiver calls.
    let mut stack = call.native_stack()?;
    stack.set_top(stack.top().max(3))?;
    stack.push(owner)?;
    let target = stack.top();
    match name {
        "GetSize" => {
            stack.get_field(target as isize, b"__songlua_spline_size")?;
            let value = integer(&stack, -1, false)?;
            stack.push(Value::Number(value))?;
            return stack.finish(1);
        }
        "get_loop" => {
            stack.get_field(target as isize, b"__songlua_spline_loop")?;
            let value = stack.to_boolean(-1);
            stack.push(Value::Boolean(value))?;
            return stack.finish(1);
        }
        "SetLoop" => {
            let value = stack.to_boolean(2);
            assign(
                &mut stack,
                target,
                b"__songlua_spline_loop",
                Value::Boolean(value),
            )?;
        }
        "SetPolygonal" => (),
        "SetSize" => {
            let size = stack
                .to_number(2)
                .filter(|n| n.is_finite())
                .ok_or_else(|| stack.error(b"Spline size must be a number"))?
                .trunc();
            if size < 0. {
                return Err(stack.error(b"A spline cannot have less than 0 points."));
            }
            if size > 65536. {
                return Err(stack.error(b"Spline exceeds 65536 points"));
            }
            stack.get_field(target as isize, b"__songlua_spline_size")?;
            let prior = integer(&stack, -1, false)? as i64;
            stack.get_field(target as isize, b"__songlua_spline_points")?;
            table_at(&stack, -1)?;
            let points = stack.top();
            stack.get_field(target as isize, b"__songlua_spline_coefficients")?;
            table_at(&stack, -1)?;
            let coefficients = stack.top();
            for index in size as i64 + 1..=prior {
                raw_assign(&mut stack, points, index as usize, Value::Nil)?;
                raw_assign(&mut stack, coefficients, index as usize, Value::Nil)?;
            }
            assign(
                &mut stack,
                target,
                b"__songlua_spline_size",
                Value::Number(size),
            )?;
        }
        "SetPoint" => {
            let index = stack
                .to_number(2)
                .filter(|n| n.is_finite())
                .ok_or_else(|| stack.error(b"Spline point index must be a number"))?
                .trunc();
            stack.get_field(target as isize, b"__songlua_spline_size")?;
            let size = integer(&stack, -1, false)?;
            if index < 1. || index > size {
                return Err(stack.error(b"Spline point index out of range."));
            }
            if !matches!(stack.value(3), Some(Value::Table(_))) {
                return Err(stack.error(b"Spline point must be a table."));
            }
            stack.create_table(3, 0)?;
            let point = stack.top();
            let dimensions = stack.object_len(3)?;
            for axis in 1..=dimensions.max(3) {
                let value = if axis <= dimensions {
                    raw_index(&mut stack, 3, axis)?;
                    let n = stack
                        .to_number(-1)
                        .ok_or_else(|| stack.error(b"Spline coordinate must be a number"))?
                        as f32;
                    stack.set_top(point)?;
                    n
                } else {
                    0.
                };
                if axis <= 3 {
                    raw_assign(&mut stack, point, axis, Value::Number(value as f64))?;
                }
            }
            stack.get_field(target as isize, b"__songlua_spline_points")?;
            table_at(&stack, -1)?;
            let points = stack.top();
            let value = stack.value(point as isize).unwrap();
            raw_assign(&mut stack, points, index as usize, value)?;
        }
        "Solve" => solve(&mut stack, target)?,
        "evaluate" => {
            let t = stack
                .to_number(2)
                .ok_or_else(|| stack.error(b"Spline evaluation requires a number"))?
                as f32;
            let value = evaluate(&mut stack, target, t)?;
            push_vector(&mut stack, value)?;
            return stack.finish(1);
        }
        _ => unreachable!(),
    }
    stack.push(owner)?;
    stack.finish(1)
}

fn set_numeric_point(call: &mut HostCall<'_>, owner: TableId) -> Result<bool, Error> {
    let [_, Value::Number(index), Value::Table(input)] = call.arguments() else {
        return Ok(false);
    };
    let index = index.trunc();
    let input = *input;
    if call.raw_metatable(owner)?.is_some() {
        return Ok(false);
    }
    let Value::Number(size) = get_static(call, owner, b"__songlua_spline_size")? else {
        return Ok(false);
    };
    let size = size.trunc();
    if !(0.0..=65536.0).contains(&size) || !(1.0..=size).contains(&index) {
        return Ok(false);
    }
    let Value::Table(points) = get_static(call, owner, b"__songlua_spline_points")? else {
        return Ok(false);
    };
    // Lua can supply sparse, wider, or coercible rows. Let the stack route
    // validate those with its original length and conversion rules.
    if call.raw_table_len(input)? != 3 {
        return Ok(false);
    }
    let [Value::Number(x), Value::Number(y), Value::Number(z)] =
        call.raw_table_read_array(input)?
    else {
        return Ok(false);
    };
    let values = [x as f32, y as f32, z as f32].map(f64::from);
    call.raw_table_set_number_array(points, index as u32, &values)?;
    call.push(Value::Table(owner))?;
    Ok(true)
}

fn solve(stack: &mut NativeStack<'_, '_>, owner: usize) -> Result<(), Error> {
    stack.get_field(owner as isize, b"__songlua_spline_size")?;
    let size = integer(stack, -1, true)? as usize;
    stack.get_field(owner as isize, b"__songlua_spline_points")?;
    table_at(stack, -1)?;
    let table = stack.top();
    let mut points = Vec::new();
    points
        .try_reserve_exact(size)
        .map_err(|_| stack.error(b"spline point allocation failed"))?;
    for index in 1..=size {
        points.push(point(stack, table, index)?);
    }
    stack.get_field(owner as isize, b"__songlua_spline_loop")?;
    let looping = stack.to_boolean(-1);
    let mut solver = deadsync_gameplay::SongLuaSplineSolver::default();
    write_coefficients(stack, owner, solver.solve_with_loop(&points, looping))
}

fn solve_numeric(
    call: &mut HostCall<'_>,
    owner: TableId,
    workspace: &mut Workspace,
) -> Result<bool, Error> {
    // Keep getter and setter dispatch on the stack path when Lua can observe it.
    if call.raw_metatable(owner)?.is_some() {
        return Ok(false);
    }
    let Value::Number(size) = get_static(call, owner, b"__songlua_spline_size")? else {
        return Ok(false);
    };
    let size = size.trunc();
    if !(0.0..=65536.0).contains(&size) {
        return Ok(false);
    }
    let Value::Table(points) = get_static(call, owner, b"__songlua_spline_points")? else {
        return Ok(false);
    };
    workspace.points.clear();
    workspace
        .points
        .try_reserve_exact(size as usize)
        .map_err(|_| call.error(b"spline point allocation failed"))?;
    let array = call.raw_table_array_values(points)?;
    for index in 0..size as usize {
        let row = match array.get(index) {
            Some(&value) => value,
            None => call.raw_table_get(points, Value::Number((index + 1) as f64))?,
        };
        let Some(point) = super::song_columns::raw_numeric_point(call, row)? else {
            return Ok(false);
        };
        workspace.points.push(point);
    }
    let looping = get_static(call, owner, b"__songlua_spline_loop")?;
    if looping == Value::Nil {
        return Ok(false);
    }
    let coefficients = workspace.solver.solve_with_loop(&workspace.points, looping.is_truthy());
    let mut stack = call.native_stack()?;
    stack.set_top(stack.top().max(3))?;
    let owner = Value::Table(owner);
    stack.push(owner)?;
    let target = stack.top();
    write_coefficients(&mut stack, target, coefficients)?;
    stack.push(owner)?;
    stack.finish(1)?;
    Ok(true)
}

fn write_coefficients(
    stack: &mut NativeStack<'_, '_>,
    owner: usize,
    coefficients: &[deadsync_gameplay::SongLuaSplinePointCoefficients],
) -> Result<(), Error> {
    stack.create_nested_number_arrays(
        coefficients
            .iter()
            .map(|point| point.map(|[_, b, c, d]| [b, c, d].map(f64::from))),
    )?;
    let value = stack.value(-1).unwrap();
    assign(stack, owner, b"__songlua_spline_coefficients", value)
}
fn evaluate(stack: &mut NativeStack<'_, '_>, owner: usize, t: f32) -> Result<[f32; 3], Error> {
    stack.get_field(owner as isize, b"__songlua_spline_size")?;
    let size = integer(stack, -1, true)? as usize;
    if size == 0 {
        return Ok([0.; 3]);
    }
    let index = t as i32;
    stack.get_field(owner as isize, b"__songlua_spline_loop")?;
    let (index, fraction) = if stack.to_boolean(-1) {
        let mut wrapped = t % size as f32;
        if wrapped < 0. {
            wrapped += size as f32;
        }
        (wrapped as usize, wrapped - wrapped.trunc())
    } else if index < 0 {
        (0, 0.)
    } else if index as usize >= size - 1 {
        (size - 1, 0.)
    } else {
        (index as usize, t - index as f32)
    };
    let square = fraction * fraction;
    let cube = square * fraction;
    stack.get_field(owner as isize, b"__songlua_spline_points")?;
    table_at(stack, -1)?;
    let table = stack.top();
    let mut values = point(stack, table, index + 1)?;
    stack.get_field(owner as isize, b"__songlua_spline_coefficients")?;
    table_at(stack, -1)?;
    let table = stack.top();
    raw_index(stack, table, index + 1)?;
    if stack.value(-1) != Some(Value::Nil) {
        table_at(stack, -1)?;
        let coefficients = stack.top();
        for (axis, value) in values.iter_mut().enumerate() {
            let [b, c, d] = point(stack, coefficients, axis + 1)?;
            *value = *value + b * fraction + c * square + d * cube;
        }
    }
    Ok(values)
}
