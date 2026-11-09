//! Typed observations for the production column spline and zoom-hide outputs.
use super::super::song_column_output::{Scratch, State};
use super::song_columns::{
    Numeric, note_field, numeric, optional_integer, point, text, with_table,
};
use super::*;
use crate::SongLuaNoteHideWindow;
use deadsync_gameplay::{SongLuaColumnSplineFrame as Frame, SongLuaSplineData as Spline};

impl Actors {
    pub(crate) fn capture_column_splines(
        &self,
        vm: &mut Vm,
        state: Arc<Mutex<Option<State>>>,
        second: f32,
    ) -> Result<(), Error> {
        let callback = vm.new_host_function_with_data(
            |call| {
                let (state, second) = call
                    .function_data::<(Arc<Mutex<Option<State>>>, f32)>()
                    .unwrap()
                    .clone();
                // No metadata or capture mutex is held across authored getters.
                let owned = state.lock().unwrap().take();
                let mut owned =
                    owned.ok_or_else(|| call.error(b"column spline capture is already active"))?;
                let result = read_splines(call, &mut owned)
                    .and_then(|()| owned.ingest(second).map_err(|e| call.error(e.as_bytes())));
                *state.lock().unwrap() = Some(owned);
                result
            },
            &[Value::Table(self.registry), Value::Table(self.helpers)],
            (state, second),
        )?;
        vm.call(callback, &[])?;
        Ok(())
    }
    pub(crate) fn read_column_zoom_hides(
        &self,
        vm: &mut Vm,
    ) -> Result<Vec<SongLuaNoteHideWindow>, Error> {
        let output = Arc::new(Mutex::new(Vec::new()));
        let callback = vm.new_host_function_with_data(
            |call| {
                let output = call
                    .function_data::<Arc<Mutex<Vec<SongLuaNoteHideWindow>>>>()
                    .unwrap()
                    .clone();
                let mut rows = Vec::new();
                for player in 0..crate::LUA_PLAYERS {
                    let globals = helper(call, 5)?;
                    let actor = super::super::field(
                        call,
                        globals,
                        if player == 0 {
                            "__songlua_top_screen_player_1"
                        } else {
                            "__songlua_top_screen_player_2"
                        },
                    )?;
                    // This reader retains each player while processing its
                    // columns. Spline-track capture instead gathers fields first.
                    with_table(call, actor, |call, actor| {
                        let Some(actor) = actor else {
                            return Ok(());
                        };
                        let field = player_field(call, actor)?;
                        with_table(call, field, |call, field| {
                            if let Some(field) = field {
                                read_hides_field(call, field, player, &mut rows)?;
                            }
                            Ok(())
                        })
                    })?;
                }
                crate::sort_note_hide_windows(&mut rows);
                *output.lock().unwrap() = rows;
                Ok(())
            },
            &[Value::Table(self.registry), Value::Table(self.helpers)],
            output.clone(),
        )?;
        vm.call(callback, &[])?;
        Ok(std::mem::take(&mut *output.lock().unwrap()))
    }
}
fn player_field(call: &mut HostCall<'_>, actor: TableId) -> Result<Value, Error> {
    if call.raw_table_len(actor)? == 0 {
        let key = call.try_intern(b"__songlua_children")?;
        let children = call.raw_table_get(actor, key)?;
        if children != Value::Nil {
            return with_table(call, children, |call, children| {
                let key = call.try_intern(b"NoteField")?;
                call.raw_table_get(children.unwrap(), key)
            });
        }
    }
    let children = native_children::named_children(call, actor)?;
    call.with_roots(&[children], |call| {
        super::super::field(call, children, "NoteField")
    })
}
fn field(call: &mut HostCall<'_>, owner: TableId, key: &str) -> Result<Value, Error> {
    sprites::field(call, owner, key)
}
fn float_field(
    call: &mut HostCall<'_>,
    owner: TableId,
    key: &str,
    default: Option<f32>,
) -> Result<f32, Error> {
    let value = field(call, owner, key)?;
    if value == Value::Nil
        && let Some(default) = default
    {
        return Ok(default);
    }
    numeric(call, value, Numeric::Float).map(|n| n as f32)
}
fn unsigned_field(call: &mut HostCall<'_>, owner: TableId, key: &str) -> Result<usize, Error> {
    let value = field(call, owner, key)?;
    numeric(call, value, Numeric::Unsigned).map(|n| n as usize)
}
fn read_splines(call: &mut HostCall<'_>, state: &mut State) -> Result<(), Error> {
    state.sampled.clear();
    let pending = call.try_new_table()?;
    call.with_roots(&[Value::Table(pending)], |call| {
        let mut slot = 1;
        for player in 0..crate::LUA_PLAYERS {
            if let Some(field) = note_field(call, player)? {
                call.raw_table_set(pending, Value::Number(slot as f64), Value::Table(field))?;
                slot += 1;
            }
        }
        for index in 1..slot {
            let key = Value::Number(index as f64);
            let value = call.raw_table_get(pending, key)?;
            let id = table(call, value)?;
            call.with_roots(&[value], |call| {
                call.raw_table_set(pending, key, Value::Nil)?;
                let columns = field(call, id, "__songlua_note_columns")?;
                with_table(call, columns, |call, columns| {
                    let Some(columns) = columns else {
                        return Ok(());
                    };
                    for i in 1usize.. {
                        let value = call.raw_table_get(columns, Value::Number(i as f64))?;
                        if value == Value::Nil {
                            break;
                        }
                        let column = table(call, value)?;
                        call.with_roots(&[value], |call| {
                            let player = unsigned_field(call, column, "__songlua_player_index")?;
                            let local = unsigned_field(call, column, "__songlua_column_index")?;
                            let last = state
                                .lanes
                                .get(&(player, local))
                                .and_then(|l| l.frames.last());
                            let position = read_position(
                                call,
                                column,
                                "__songlua_pos_handler",
                                last.and_then(|f| f.position.as_ref()),
                                &mut state.scratch,
                            )?;
                            let zoom = read_position(
                                call,
                                column,
                                "__songlua_zoom_handler",
                                last.and_then(|f| f.zoom.as_ref()),
                                &mut state.scratch,
                            )?;
                            state.sampled.push((
                                player,
                                local,
                                Frame {
                                    second: 0.,
                                    position,
                                    zoom,
                                },
                            ));
                            Ok(())
                        })?;
                    }
                    Ok(())
                })
            })?;
        }
        Ok(())
    })
}
fn points_match(points: &[[f32; 3]], coefficients: &[[[f32; 4]; 3]]) -> bool {
    points.len() == coefficients.len()
        && points
            .iter()
            .zip(coefficients)
            .all(|(point, coefficients)| {
                point
                    .iter()
                    .zip(coefficients)
                    .all(|(value, axis)| value.to_bits() == axis[0].to_bits())
            })
}
fn read_position(
    call: &mut HostCall<'_>,
    column: TableId,
    key: &str,
    previous: Option<&Spline>,
    scratch: &mut Scratch,
) -> Result<Option<Spline>, Error> {
    let handler = field(call, column, key)?;
    with_table(call, handler, |call, handler| {
        let Some(handler) = handler else {
            return Ok(None);
        };
        let value = field(call, handler, "__songlua_spline_mode")?;
        let mode = text(call, value, false)?;
        if !mode.eq_ignore_ascii_case("NoteColumnSplineMode_Position") {
            return Ok(None);
        }
        let spline = field(call, handler, "__songlua_spline")?;
        let spline = table(call, spline)?;
        call.with_roots(&[Value::Table(spline)], |call| {
            let size = unsigned_field(call, spline, "__songlua_spline_size")?;
            if size == 0 {
                return Ok(None);
            }
            if size > 65536 {
                return Err(call.error(b"Position spline exceeds 65536 points"));
            }
            let beats_per_t = float_field(call, handler, "__songlua_beats_per_t", None)?;
            let receptor_t = float_field(call, handler, "__songlua_receptor_t", None)?;
            if !beats_per_t.is_finite() || beats_per_t == 0. || !receptor_t.is_finite() {
                return Err(call.error(b"Position spline has an invalid beat conversion"));
            }
            let points = field(call, spline, "__songlua_spline_points")?;
            let points = table(call, points)?;
            call.with_roots(&[Value::Table(points)], |call| {
                scratch.points.clear();
                scratch.points.reserve_exact(size);
                let looping = field(call, spline, "__songlua_spline_loop")?.is_truthy();
                let mut unchanged =
                    previous.is_some_and(|p| p.coefficients.len() == size && p.looping == looping);
                for index in 1..=size {
                    let values = point(call, points, index)?;
                    if values.iter().any(|v| !v.is_finite()) {
                        return Err(call.error(b"Position spline contains a nonfinite point"));
                    }
                    unchanged = unchanged
                        && previous
                            .and_then(|p| p.coefficients.get(index - 1))
                            .is_some_and(|c| {
                                values
                                    .iter()
                                    .zip(c)
                                    .all(|(v, axis)| v.to_bits() == axis[0].to_bits())
                            });
                    scratch.points.push(values);
                }
                let constant = scratch.points.iter().all(|p| *p == scratch.points[0]);
                let coefficients = if unchanged {
                    Arc::clone(&previous.unwrap().coefficients)
                } else if let Some(recent) = &scratch.recent
                    && scratch.recent_looping == looping
                    && points_match(&scratch.points, recent)
                {
                    Arc::clone(recent)
                } else {
                    Arc::from(scratch.solver.solve_with_loop(&scratch.points, looping))
                };
                let result = Spline {
                    coefficients,
                    constant,
                    looping,
                    beats_per_t,
                    receptor_t,
                    subtract_song_beat: field(call, handler, "__songlua_subtract_song_beat")?
                        .is_truthy(),
                };
                if !unchanged
                    && !scratch
                        .recent
                        .as_ref()
                        .is_some_and(|r| Arc::ptr_eq(r, &result.coefficients))
                {
                    scratch.recent = Some(Arc::clone(&result.coefficients));
                    scratch.recent_looping = looping;
                }
                Ok(Some(result))
            })
        })
    })
}
fn read_hides_field(
    call: &mut HostCall<'_>,
    field_id: TableId,
    player: usize,
    out: &mut Vec<SongLuaNoteHideWindow>,
) -> Result<(), Error> {
    let columns = field(call, field_id, "__songlua_note_columns")?;
    with_table(call, columns, |call, columns| {
        let Some(columns) = columns else {
            return Ok(());
        };
        for i in 1usize.. {
            let value = call.raw_table_get(columns, Value::Number(i as f64))?;
            if value == Value::Nil {
                break;
            }
            let column = table(call, value)?;
            call.with_roots(&[value], |call| {
                let index = field(call, column, "__songlua_column_index")?;
                let Some(index) =
                    optional_integer(call, index)?.and_then(|n| usize::try_from(n).ok())
                else {
                    return Ok(());
                };
                read_hides(call, column, player, index, out)
            })?;
        }
        Ok(())
    })
}
fn read_hides(
    call: &mut HostCall<'_>,
    column: TableId,
    player: usize,
    index: usize,
    out: &mut Vec<SongLuaNoteHideWindow>,
) -> Result<(), Error> {
    let handler = field(call, column, "__songlua_zoom_handler")?;
    with_table(call, handler, |call, handler| {
        let Some(handler) = handler else {
            return Ok(());
        };
        let value = field(call, handler, "__songlua_spline_mode")?;
        let mode = text(call, value, true)?;
        let subtract = field(call, handler, "__songlua_subtract_song_beat")?.is_truthy();
        let beats = float_field(call, handler, "__songlua_beats_per_t", Some(1.))?;
        let Some(beats) = crate::note_column_zoom_hide_beats_per_t(&mode, subtract, beats) else {
            return Ok(());
        };
        let spline = field(call, handler, "__songlua_spline")?;
        with_table(call, spline, |call, spline| {
            let Some(spline) = spline else {
                return Ok(());
            };
            let size = field(call, spline, "__songlua_spline_size")?;
            let size = optional_integer(call, size)?
                .and_then(|n| usize::try_from(n).ok())
                .unwrap_or(0);
            let points = field(call, spline, "__songlua_spline_points")?;
            let points = table(call, points)?;
            call.with_roots(&[Value::Table(points)],|call| {
                let mut hidden=Vec::with_capacity(size);
                for i in 1..=size {
                    let value=call.raw_table_get(points,Value::Number(i as f64))?;
                    let hide=with_table(call,value,|call,point| {
                        let Some(point)=point else{return Ok(false);};
                        let mut values=[None;3];
                        for(axis,value)in values.iter_mut().enumerate(){*value=splines::optional_float(call,call.raw_table_get(point,Value::Number((axis+1)as f64))?);}
                        Ok(matches!(values,[Some(x),Some(y),Some(z)] if x<=-0.99&&y<=-0.99&&z<=-0.99))
                    })?;
                    hidden.push(hide);
                }
                out.extend(crate::note_hide_windows_from_flags(player,index,beats,&hidden));
                Ok(())
            })
        })
    })
}
