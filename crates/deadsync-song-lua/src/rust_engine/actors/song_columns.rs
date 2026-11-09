//! Typed spline observations for the existing compiler's column transforms.
use super::*;
use crate::{SongLuaColumnTransformSample as Sample, SongLuaColumnTransformTarget as Target};

const POS: &str = "__songlua_pos_handler";
const ZOOM: &str = "__songlua_zoom_handler";
const ROT: &str = "__songlua_rot_handler";
const MODE: &str = "__songlua_spline_mode";
const SPLINE: &str = "__songlua_spline";
const SIZE: &str = "__songlua_spline_size";
const POINTS: &str = "__songlua_spline_points";
const DISABLED: &str = "NoteColumnSplineMode_Disabled";
const OFFSET: &str = "NoteColumnSplineMode_Offset";
const POSITION: &str = "NoteColumnSplineMode_Position";

#[derive(Clone, Copy)]
pub(super) enum Numeric {
    Float,
    Integer,
    Unsigned,
}
pub(super) fn install(vm: &mut Vm, helpers: TableId) -> Result<(), Error> {
    for (name, mode) in [
        ("column_float", Numeric::Float),
        ("column_integer", Numeric::Integer),
        ("column_unsigned", Numeric::Unsigned),
    ] {
        let function = vm.new_host_function_with_data(convert, &[], mode)?;
        super::super::put(vm, helpers, name, function)?;
    }
    Ok(())
}
fn convert(call: &mut HostCall<'_>) -> Result<(), Error> {
    let mode = *call.function_data::<Numeric>().unwrap();
    let mut stack = call.native_stack()?;
    let number = match mode {
        Numeric::Float => stack
            .to_number(1)
            .ok_or_else(|| stack.error(b"column number expected"))?,
        Numeric::Integer => stack
            .to_integer(1)
            .ok_or_else(|| stack.error(b"column integer expected"))?
            as f64,
        Numeric::Unsigned => {
            let number = stack
                .to_number(1)
                .ok_or_else(|| stack.error(b"column index expected"))?;
            if matches!(stack.value(1), Some(Value::Number(_)))
                && number >= 0.
                && number < (usize::MAX as f64)
            {
                (number as usize) as f64
            } else {
                usize::try_from(
                    stack
                        .to_integer(1)
                        .ok_or_else(|| stack.error(b"column index expected"))?,
                )
                .map_err(|_| stack.error(b"column index out of range"))? as f64
            }
        }
    };
    stack.push(Value::Number(number))?;
    stack.finish(1)
}
pub(super) fn numeric(call: &mut HostCall<'_>, value: Value, mode: Numeric) -> Result<f64, Error> {
    let name: &[u8] = match mode {
        Numeric::Float => b"column_float",
        Numeric::Integer => b"column_integer",
        Numeric::Unsigned => b"column_unsigned",
    };
    let function = native_children::helper_function(call, name)?;
    let Value::Number(value) = call.call(function, &[value])?[0] else {
        unreachable!()
    };
    Ok(value)
}
fn field(call: &mut HostCall<'_>, actor: TableId, key: &str) -> Result<Value, Error> {
    sprites::field(call, actor, key)
}
pub(super) fn text(call: &mut HostCall<'_>, value: Value, optional: bool) -> Result<String, Error> {
    match initial_state::value_text(call, value)? {
        Some(text) => Ok(text),
        None if optional => Ok(String::new()),
        None => Err(call.error(b"column string expected")),
    }
}
fn mode(call: &mut HostCall<'_>, handler: TableId, optional: bool) -> Result<String, Error> {
    let value = field(call, handler, MODE)?;
    text(call, value, optional)
}
pub(super) fn with_table<T>(
    call: &mut HostCall<'_>,
    value: Value,
    read: impl FnOnce(&mut HostCall<'_>, Option<TableId>) -> Result<T, Error>,
) -> Result<T, Error> {
    if value == Value::Nil {
        return read(call, None);
    }
    let table = table(call, value)?;
    call.with_roots(&[value], |call| read(call, Some(table)))
}
fn required_table(call: &HostCall<'_>, value: Value) -> Result<TableId, Error> {
    table(call, value)
}
pub(super) fn optional_integer(
    call: &mut HostCall<'_>,
    value: Value,
) -> Result<Option<i64>, Error> {
    if value == Value::Nil {
        Ok(None)
    } else {
        Ok(Some(numeric(call, value, Numeric::Integer)? as i64))
    }
}
pub(super) fn point(
    call: &mut HostCall<'_>,
    points: TableId,
    index: usize,
) -> Result<[f32; 3], Error> {
    let value = call.raw_table_get(points, Value::Number(index as f64))?;
    with_table(call, value, |call, point| {
        let Some(point) = point else {
            return Ok([0.; 3]);
        };
        let mut result = [0.; 3];
        for (axis, result) in result.iter_mut().enumerate() {
            let value = call.raw_table_get(point, Value::Number((axis + 1) as f64))?;
            if value != Value::Nil {
                *result = numeric(call, value, Numeric::Float)? as f32;
            }
        }
        Ok(result)
    })
}
fn spline_size(call: &mut HostCall<'_>, spline: TableId) -> Result<usize, Error> {
    let value = field(call, spline, SIZE)?;
    Ok(optional_integer(call, value)?
        .and_then(|n| usize::try_from(n).ok())
        .unwrap_or(0))
}
fn uniform(
    call: &mut HostCall<'_>,
    column: TableId,
    key: &str,
    component: usize,
    baseline: f32,
) -> Result<Option<f32>, Error> {
    let handler = field(call, column, key)?;
    with_table(call, handler, |call, handler| {
        let Some(handler) = handler else {
            return Ok(None);
        };
        let mode = mode(call, handler, true)?;
        if mode.eq_ignore_ascii_case(DISABLED) {
            return Ok(Some(baseline));
        }
        let offset = mode.eq_ignore_ascii_case(OFFSET);
        if !offset && !mode.eq_ignore_ascii_case(POSITION) {
            return Ok(None);
        }
        let spline = field(call, handler, SPLINE)?;
        with_table(call, spline, |call, spline| {
            let Some(spline) = spline else {
                return Ok(None);
            };
            let size = spline_size(call, spline)?;
            if size == 0 {
                return Ok(Some(baseline));
            }
            let points = field(call, spline, POINTS)?;
            let points = required_table(call, points)?;
            call.with_roots(&[Value::Table(points)], |call| {
                if !offset && key == ZOOM {
                    return Ok(Some(baseline));
                }
                if !offset {
                    return Ok(Some(point(call, points, 1)?[component]));
                }
                let witness = match component {
                    0 => "__songlua_nonuniform_x",
                    1 => "__songlua_nonuniform_y",
                    _ => "__songlua_nonuniform_z",
                };
                let index = field(call, spline, witness)?;
                if index != Value::Nil {
                    let index = numeric(call, index, Numeric::Unsigned)? as usize;
                    if index <= size {
                        let first = point(call, points, 1).ok().map(|p| p[component]);
                        let other = point(call, points, index).ok().map(|p| p[component]);
                        if first.zip(other).is_some_and(|(a, b)| (a - b).abs() > 0.001) {
                            return Ok(None);
                        }
                    }
                }
                let mut first = None::<f32>;
                for index in 1..=size {
                    let value = point(call, points, index)?[component];
                    if let Some(prior) = first {
                        if (value - prior).abs() > 0.001 {
                            sprites::set(call, spline, witness, Value::Number(index as f64))?;
                            return Ok(None);
                        }
                    } else {
                        first = Some(value);
                    }
                }
                Ok(Some(baseline + first.unwrap_or(0.)))
            })
        })
    })
}
fn x_offset(call: &mut HostCall<'_>, column: TableId) -> Result<Option<f32>, Error> {
    let handler = field(call, column, POS)?;
    with_table(call, handler, |call, handler| {
        let Some(handler) = handler else {
            return Ok(None);
        };
        let mode = mode(call, handler, false)?;
        if mode.eq_ignore_ascii_case(OFFSET) {
            uniform(call, column, POS, 0, 0.)
        } else {
            Ok(
                (mode.eq_ignore_ascii_case(DISABLED) || mode.eq_ignore_ascii_case(POSITION))
                    .then_some(0.),
            )
        }
    })
}
fn y_offset(call: &mut HostCall<'_>, column: TableId) -> Result<Option<f32>, Error> {
    let handler = field(call, column, POS)?;
    with_table(call, handler, |call, handler| {
        let Some(handler) = handler else {
            return Ok(None);
        };
        let mode = mode(call, handler, true)?;
        if mode.eq_ignore_ascii_case(DISABLED) {
            return Ok(Some(0.));
        }
        if !mode.eq_ignore_ascii_case(OFFSET) {
            return Ok(None);
        }
        let spline = field(call, handler, SPLINE)?;
        with_table(call, spline, |call, spline| {
            let Some(spline) = spline else {
                return Ok(None);
            };
            let size = spline_size(call, spline)?;
            if size == 0 {
                return Ok(Some(0.));
            }
            let points = field(call, spline, POINTS)?;
            let points = required_table(call, points)?;
            call.with_roots(&[Value::Table(points)], |call| {
                let mut first = None::<f32>;
                let mut valid = true;
                for index in 1..=size {
                    let [x, y, _] = point(call, points, index)?;
                    valid &= x.is_finite() && y.is_finite();
                    if let Some(first) = first {
                        valid &= (y - first).abs() <= 0.001;
                    } else {
                        first = Some(y)
                    }
                }
                Ok(if valid { first } else { None })
            })
        })
    })
}
fn position(
    call: &mut HostCall<'_>,
    column: TableId,
) -> Result<Option<[(Target, f32); 10]>, Error> {
    let handler = field(call, column, POS)?;
    with_table(call, handler, |call, handler| {
        let Some(handler) = handler else {
            return Ok(None);
        };
        if !mode(call, handler, false)?.eq_ignore_ascii_case(OFFSET) {
            return Ok(None);
        }
        let spline = field(call, handler, SPLINE)?;
        let spline = required_table(call, spline)?;
        call.with_roots(&[Value::Table(spline)], |call| {
            let size = field(call, spline, SIZE)?;
            if numeric(call, size, Numeric::Integer)? != 2. {
                return Ok(None);
            }
            let points = field(call, spline, POINTS)?;
            let points = required_table(call, points)?;
            call.with_roots(&[Value::Table(points)], |call| {
                let mut out = [(Target::PositionEnabled, 0.); 10];
                for p in 0..2u8 {
                    let values = point(call, points, usize::from(p) + 1)?;
                    for axis in 0..3u8 {
                        let value = values[usize::from(axis)];
                        if !value.is_finite() {
                            return Ok(None);
                        }
                        out[usize::from(p) * 3 + usize::from(axis)] =
                            (Target::PositionPoint { point: p, axis }, value);
                    }
                }
                let beats = field(call, handler, "__songlua_beats_per_t")?;
                let beats = numeric(call, beats, Numeric::Float)? as f32;
                let receptor = field(call, handler, "__songlua_receptor_t")?;
                let receptor = numeric(call, receptor, Numeric::Float)? as f32;
                if !beats.is_finite() || beats == 0. || !receptor.is_finite() {
                    return Ok(None);
                }
                let subtract = field(call, handler, "__songlua_subtract_song_beat")?.is_truthy();
                out[6..].copy_from_slice(&[
                    (Target::PositionEnabled, 1.),
                    (Target::PositionBeatsPerT, beats),
                    (Target::PositionReceptorT, receptor),
                    (Target::PositionSubtractBeat, if subtract { 1. } else { 0. }),
                ]);
                Ok(Some(out))
            })
        })
    })
}
fn read_field(
    call: &mut HostCall<'_>,
    field_id: TableId,
    out: &mut Vec<Sample>,
) -> Result<(), Error> {
    let columns = field(call, field_id, "__songlua_note_columns")?;
    with_table(call, columns, |call, columns| {
        let Some(columns) = columns else {
            return Ok(());
        };
        for index in 1usize.. {
            let column = call.raw_table_get(columns, Value::Number(index as f64))?;
            if column == Value::Nil {
                break;
            }
            let id = table(call, column)?;
            call.with_roots(&[column], |call| {
                let player = field(call, id, "__songlua_player_index")?;
                let Some(player) =
                    optional_integer(call, player)?.and_then(|n| usize::try_from(n).ok())
                else {
                    return Ok(());
                };
                let column = field(call, id, "__songlua_column_index")?;
                let Some(column) =
                    optional_integer(call, column)?.and_then(|n| usize::try_from(n).ok())
                else {
                    return Ok(());
                };
                let mut push = |target, value: Option<f32>| {
                    if let Some(value) = value.filter(|v| v.is_finite()) {
                        out.push(Sample {
                            player,
                            column,
                            target,
                            value,
                        })
                    }
                };
                let position = position(call, id)?;
                let handler = field(call, id, POS)?;
                let absolute = with_table(call, handler, |call, handler| {
                    Ok(handler.is_some_and(|h| {
                        mode(call, h, false).is_ok_and(|m| m.eq_ignore_ascii_case(POSITION))
                    }))
                })?;
                if position.is_some() || absolute {
                    push(Target::OffsetX, Some(0.));
                    push(Target::OffsetY, Some(0.));
                } else {
                    push(Target::OffsetX, x_offset(call, id)?);
                    push(Target::OffsetY, y_offset(call, id)?);
                }
                push(Target::Zoom, uniform(call, id, ZOOM, 0, 1.)?);
                push(Target::RotationZ, uniform(call, id, ROT, 2, 0.)?);
                if let Some(position) = position {
                    for (target, value) in position {
                        push(target, Some(value));
                    }
                } else {
                    push(Target::PositionEnabled, Some(0.));
                }
                Ok(())
            })?;
        }
        Ok(())
    })
}
pub(super) fn note_field(call: &mut HostCall<'_>, index: usize) -> Result<Option<TableId>, Error> {
    let globals = helper(call, 5)?;
    let player = super::super::field(
        call,
        globals,
        if index == 0 {
            "__songlua_top_screen_player_1"
        } else {
            "__songlua_top_screen_player_2"
        },
    )?;
    with_table(call, player, |call, player| {
        let Some(player) = player else {
            return Ok(None);
        };
        let key = call.try_intern(b"__songlua_children")?;
        let children = call.raw_table_get(player, key)?;
        if call.raw_table_len(player)? == 0 && children != Value::Nil {
            return with_table(call, children, |call, children| {
                let key = call.try_intern(b"NoteField")?;
                let value = call.raw_table_get(children.unwrap(), key)?;
                if value == Value::Nil {
                    Ok(None)
                } else {
                    table(call, value).map(Some)
                }
            });
        }
        let children = native_children::named_children(call, player)?;
        call.with_roots(&[children], |call| {
            let value = super::super::field(call, children, "NoteField")?;
            if value == Value::Nil {
                Ok(None)
            } else {
                table(call, value).map(Some)
            }
        })
    })
}
fn read_pending(
    call: &mut HostCall<'_>,
    pending: TableId,
    out: &mut Vec<Sample>,
) -> Result<(), Error> {
    for i in 1usize.. {
        let key = Value::Number(i as f64);
        let value = call.raw_table_get(pending, key)?;
        if value == Value::Nil {
            break;
        }
        let field = table(call, value)?;
        call.with_roots(&[value], |call| {
            // Native consumes its Vec<Table>: retain future fields, but drop
            // each processed field before getters on the next field can run.
            call.raw_table_set(pending, key, Value::Nil)?;
            read_field(call, field, out)
        })?;
    }
    Ok(())
}
fn global_fields(call: &mut HostCall<'_>, out: &mut Vec<Sample>) -> Result<(), Error> {
    let pending = call.try_new_table()?;
    call.with_roots(&[Value::Table(pending)], |call| {
        let mut slot = 1;
        for index in 0..crate::LUA_PLAYERS {
            if let Some(field) = note_field(call, index)? {
                call.raw_table_set(pending, Value::Number(slot as f64), Value::Table(field))?;
                slot += 1;
            }
        }
        read_pending(call, pending, out)
    })
}
impl Actors {
    pub(in crate::rust_engine) fn read_column_samples(
        &self,
        vm: &mut Vm,
        fields: Option<&[Value]>,
    ) -> Result<Vec<Sample>, Error> {
        let mut pins = Vec::new();
        let mut input_pin = None;
        let output = Arc::new(Mutex::new(Vec::new()));
        let result = (|| {
            let input = if let Some(fields) = fields {
                for &field in fields {
                    pins.push(vm.pin(field)?);
                    if !matches!(field, Value::Table(_)) {
                        return vm.with_native_stack(|stack| {
                            Err(stack.error(b"note field table expected"))
                        });
                    }
                }
                let table = vm.try_new_table()?;
                input_pin = Some(vm.pin(Value::Table(table))?);
                for (i, &field) in fields.iter().enumerate() {
                    vm.raw_table_set(table, Value::Number((i + 1) as f64), field)?;
                }
                for pin in pins.drain(..) {
                    vm.unpin(pin)?;
                }
                Value::Table(table)
            } else {
                Value::Nil
            };
            let callback = vm.new_host_function_with_data(
                |call| {
                    let output = call
                        .function_data::<Arc<Mutex<Vec<Sample>>>>()
                        .unwrap()
                        .clone();
                    let mut samples = Vec::new();
                    if let Value::Table(input) = call.captures()[2] {
                        read_pending(call, input, &mut samples)?;
                    } else {
                        global_fields(call, &mut samples)?;
                    }
                    *output.lock().unwrap() = samples;
                    Ok(())
                },
                &[
                    Value::Table(self.registry),
                    Value::Table(self.helpers),
                    input,
                ],
                output.clone(),
            )?;
            vm.call(callback, &[])?;
            Ok(std::mem::take(&mut *output.lock().unwrap()))
        })();
        for pin in pins {
            vm.unpin(pin)?;
        }
        if let Some(pin) = input_pin {
            vm.unpin(pin)?;
        }
        result
    }
}
