//! Synchronous Player messages and NoteField callback returns.
use super::*;
use crate::live::{TAP_SCORES, judgment_score};
use crate::rust_engine::number;
use deadsync_gameplay::GameplayNoteFieldTap;
use deadsync_rules::{
    judgment::Judgment,
    note::{HoldResult, Note},
};

#[derive(Clone)]
pub(in crate::rust_engine) enum Event {
    Step {
        player: usize,
        column: usize,
    },
    Row {
        player: usize,
        first_track: usize,
        judgment: Judgment,
        early: Option<Judgment>,
        notes: Vec<(usize, Note)>,
        holds: Vec<(usize, Note)>,
    },
    Hold {
        player: usize,
        columns: usize,
        note: Note,
        result: HoldResult,
    },
    Mine {
        player: usize,
        column: usize,
    },
}
impl Actors {
    pub(in crate::rust_engine) fn gameplay_fields(&self, vm: &mut Vm) -> Result<Value, Error> {
        let f = vm.new_host_function_with_data(
            fields,
            &[Value::Table(self.registry), Value::Table(self.helpers)],
            self.store.clone(),
        )?;
        Ok(vm.call(f, &[])?.first().copied().unwrap_or(Value::Nil))
    }
    pub(in crate::rust_engine) fn gameplay_event(
        &self,
        vm: &mut Vm,
        event: Event,
    ) -> Result<(), Error> {
        let f = vm.new_host_function_with_data(
            deliver,
            &[Value::Table(self.registry), Value::Table(self.helpers)],
            (self.store.clone(), event),
        )?;
        let (scene, first) = self.gameplay_scope(vm)?;
        let result = vm.call(f, &[]).map(|_| ());
        if let Some(scene) = scene {
            song_activation::adopt_created(&self.store, scene, first);
        }
        result
    }
    pub(in crate::rust_engine) fn gameplay_tap(
        &self,
        vm: &mut Vm,
        field: Value,
        callback: Value,
        tap: GameplayNoteFieldTap,
        columns: usize,
    ) -> Result<GameplayNoteFieldTap, Error> {
        let output = Arc::new(Mutex::new(tap));
        let f = vm.new_host_function_with_data(
            tap_note,
            &[
                Value::Table(self.registry),
                Value::Table(self.helpers),
                field,
                callback,
            ],
            (self.store.clone(), tap, columns, output.clone()),
        )?;
        let (scene, first) = self.gameplay_scope(vm)?;
        let result = vm.call(f, &[]).map(|_| ());
        if let Some(scene) = scene {
            song_activation::adopt_created(&self.store, scene, first);
        }
        result?;
        let result = *output.lock().unwrap();
        Ok(result)
    }
    fn gameplay_scope(&self, vm: &mut Vm) -> Result<(Option<u64>, u64), Error> {
        let screen = vm.with_native_stack(|s| {
            let base = s.top();
            s.push(s.value(-10002).unwrap())?;
            let g = s.top_offset();
            s.push_bytes(b"__songlua_top_screen")?;
            s.get_table(g, true)?;
            let screen = s.value(-1).unwrap();
            s.set_top(base)?;
            Ok(screen)
        })?;
        let store = self.store.lock().unwrap();
        let scene = if let Value::Table(t) = screen {
            store.actors.get(&t).map(|a| a.scene)
        } else {
            None
        };
        Ok((scene, store.next))
    }
    pub(in crate::rust_engine) fn gameplay_error(
        &self,
        vm: &mut Vm,
        owner: Option<Value>,
        flag: &'static str,
        label: &'static str,
        error: Error,
    ) {
        let e = error.value().unwrap_or(Value::Nil);
        let f = vm.new_host_function_with_data(
            |call| {
                let (store, owner, flag, label, error) = call
                    .function_data::<(Shared, Option<Value>, &'static str, &'static str, Error)>()
                    .unwrap()
                    .clone();
                let owner = match owner {
                    Some(v) => v,
                    None => {
                        let g = table(call, helper(call, 5)?)?;
                        let key = call.try_intern(b"__songlua_top_screen")?;
                        call.raw_table_get(g, key)?
                    }
                };
                let Value::Table(owner) = owner else {
                    return Ok(());
                };
                diagnostics::report(
                    call,
                    &store,
                    owner,
                    diagnostics::Callback::Gameplay(flag, label),
                    error,
                )
            },
            &[
                Value::Table(self.registry),
                Value::Table(self.helpers),
                owner.unwrap_or(Value::Nil),
                e,
            ],
            (self.store.clone(), owner, flag, label, error),
        );
        if let Ok(f) = f {
            let _ = vm.call(f, &[]);
        }
    }
}
fn fields(call: &mut HostCall<'_>) -> Result<(), Error> {
    let globals = helper(call, 5)?;
    let key = call.try_intern(b"__songlua_top_screen")?;
    let screen = call.table_get(globals, key)?;
    let children = native_children::named_children(call, table(call, screen)?)?;
    call.with_roots(&[children], |call| {
        let batch = call.try_new_table()?;
        call.with_roots(&[Value::Table(batch)], |call| {
            for player in 0..crate::LUA_PLAYERS {
                let key = call.try_intern(format!("PlayerP{}", player + 1).as_bytes())?;
                let child = call.table_get(children, key)?;
                if child == Value::Nil {
                    continue;
                }
                let nested = native_children::named_children(call, table(call, child)?)?;
                call.with_roots(&[nested], |call| {
                    let key = call.try_intern(b"NoteField")?;
                    let field = call.table_get(nested, key)?;
                    if field != Value::Nil {
                        table(call, field)?;
                    }
                    call.raw_table_set(batch, Value::Number((player + 1) as f64), field)
                })?;
            }
            call.push(Value::Table(batch))
        })
    })
}
fn set(call: &mut HostCall<'_>, t: TableId, name: &str, value: Value) -> Result<(), Error> {
    let key = call.try_intern(name.as_bytes())?;
    call.raw_table_set(t, key, value)
}
fn text(call: &mut HostCall<'_>, t: TableId, name: &str, value: &str) -> Result<(), Error> {
    let value = call.try_intern(value.as_bytes())?;
    set(call, t, name, value)
}
fn score(
    call: &mut HostCall<'_>,
    t: TableId,
    key: &str,
    value: Option<Judgment>,
) -> Result<(), Error> {
    text(
        call,
        t,
        key,
        value.map_or("TapNoteScore_None", judgment_score),
    )
}
fn note(call: &mut HostCall<'_>, params: TableId, n: &Note) -> Result<Value, Error> {
    use deadsync_core::note::NoteType;
    let t = call.try_new_table()?;
    call.with_roots(&[Value::Table(t)], |call| {
        text(
            call,
            t,
            "TapNoteType",
            match n.note_type {
                NoteType::Tap => "TapNoteType_Tap",
                NoteType::Hold | NoteType::Roll => "TapNoteType_HoldHead",
                NoteType::Mine => "TapNoteType_Mine",
                NoteType::Lift => "TapNoteType_Lift",
                NoteType::Fake => "TapNoteType_Fake",
            },
        )?;
        text(
            call,
            t,
            "TapNoteSubType",
            if n.note_type == NoteType::Roll {
                "TapNoteSubType_Roll"
            } else {
                "TapNoteSubType_Hold"
            },
        )?;
        set(
            call,
            t,
            "HoldDuration",
            number(n.hold.as_ref().map_or(0., |h| h.end_beat - n.beat)),
        )?;
        set(call, t, "KeysoundIndex", Value::Number(-1.))?;
        score(call, t, "TapNoteScore", n.result)?;
        set(
            call,
            t,
            "TapNoteOffset",
            number(n.result.map_or(0., |r| r.time_error_ms / 1000.)),
        )?;
        set(
            call,
            t,
            "Held",
            Value::Boolean(n.result.is_some_and(|r| r.miss_because_held)),
        )?;
        messages::judgment::normalize_note(call, params, t)?;
        let value = match n.participant {
            Some(p) => call.try_intern(if p == 0 {
                b"PlayerNumber_P1"
            } else {
                b"PlayerNumber_P2"
            })?,
            None => Value::Nil,
        };
        let f = call.new_host_function(|call| call.push(call.captures()[0]), &[value])?;
        set(call, t, "GetPlayerNumber", f)?;
        Ok(Value::Table(t))
    })
}
fn deliver(call: &mut HostCall<'_>) -> Result<(), Error> {
    let (store, event) = call.function_data::<(Shared, Event)>().unwrap().clone();
    let params = call.try_new_table()?;
    call.with_roots(&[Value::Table(params)], |call| {
        let (player, message) = match &event {
            Event::Step { player, column } => {
                text(
                    call,
                    params,
                    "PlayerNumber",
                    crate::player_number_name(*player),
                )?;
                set(call, params, "Column", Value::Number(*column as f64))?;
                (*player, "Step")
            }
            Event::Row {
                player,
                first_track,
                judgment,
                early,
                notes,
                holds,
            } => {
                set(
                    call,
                    params,
                    "FirstTrack",
                    Value::Number(*first_track as f64),
                )?;
                score(call, params, "TapNoteScore", Some(*judgment))?;
                set(
                    call,
                    params,
                    "TapNoteOffset",
                    number(judgment.time_error_ms / 1000.),
                )?;
                set(
                    call,
                    params,
                    "Early",
                    Value::Boolean(judgment.time_error_ms < 0.),
                )?;
                score(call, params, "EarlyTapNoteScore", *early)?;
                set(
                    call,
                    params,
                    "EarlyTapNoteOffset",
                    number(early.map_or(0., |j| j.time_error_ms / 1000.)),
                )?;
                if judgment.grade == deadsync_rules::judgment::JudgeGrade::Miss {
                    set(
                        call,
                        params,
                        "HeldMiss",
                        Value::Boolean(judgment.miss_because_held),
                    )?;
                }
                text(call, params, "Player", crate::player_number_name(*player))?;
                for (key, notes) in [("Notes", notes), ("Holds", holds)] {
                    let list = call.try_new_table()?;
                    call.with_roots(&[Value::Table(list)], |call| {
                        for (column, n) in notes {
                            let n = note(call, params, n)?;
                            call.raw_table_set(list, Value::Number((column + 1) as f64), n)?;
                        }
                        set(call, params, key, Value::Table(list))
                    })?;
                }
                (*player, "Judgment")
            }
            Event::Hold {
                player,
                columns,
                note: n,
                result,
            } => {
                text(call, params, "Player", crate::player_number_name(*player))?;
                set(
                    call,
                    params,
                    "FirstTrack",
                    Value::Number((n.column - player * columns) as f64),
                )?;
                set(call, params, "NumTracks", Value::Number(*columns as f64))?;
                score(call, params, "TapNoteScore", n.result)?;
                text(
                    call,
                    params,
                    "HoldNoteScore",
                    match result {
                        HoldResult::Held => "HoldNoteScore_Held",
                        HoldResult::LetGo => "HoldNoteScore_LetGo",
                        HoldResult::Missed => "HoldNoteScore_MissedHold",
                    },
                )?;
                score(call, params, "EarlyTapNoteScore", n.early_result)?;
                let n = note(call, params, n)?;
                set(call, params, "TapNote", n)?;
                (*player, "Judgment")
            }
            Event::Mine { player, column } => {
                text(call, params, "Player", crate::player_number_name(*player))?;
                text(call, params, "TapNoteScore", "TapNoteScore_AvoidMine")?;
                set(call, params, "FirstTrack", Value::Number(*column as f64))?;
                (*player, "Judgment")
            }
        };
        messages::dispatch(
            call,
            &store,
            message.as_bytes(),
            Some(Value::Table(params)),
            false,
        )?;
        if matches!(event, Event::Step { .. }) {
            let empty = Value::Table(call.try_new_table()?);
            call.with_roots(&[empty], |call| {
                messages::dispatch(
                    call,
                    &store,
                    format!("StepP{}", player + 1).as_bytes(),
                    Some(empty),
                    false,
                )
            })?;
        }
        Ok(())
    })
}
fn tap_note(call: &mut HostCall<'_>) -> Result<(), Error> {
    let (store, mut tap, columns, output) = call
        .function_data::<(
            Shared,
            GameplayNoteFieldTap,
            usize,
            Arc<Mutex<GameplayNoteFieldTap>>,
        )>()
        .unwrap()
        .clone();
    let field = table(call, call.captures()[2])?;
    let score = call.try_intern(format!("TapNoteScore_{}", tap.score).as_bytes())?;
    let values = call
        .call(
            call.captures()[3],
            &[
                Value::Number((tap.column + 1) as f64),
                score,
                Value::Boolean(tap.bright),
            ],
        )?
        .to_vec();
    call.with_roots(&values, |call| {
        if let Some(&value) = values.first() {
            let mut s = call.native_stack()?;
            s.push(value)?;
            let column = s.to_number(-1);
            drop(s);
            if let Some(column) = column {
                let column = (column - 1.).trunc();
                if column.is_finite() && column >= 0. && column < columns as f64 {
                    tap.column = column as usize;
                } else {
                    report_return(
                        call,
                        &store,
                        field,
                        "__songlua_did_tap_note_column_error_reported",
                        &format!("Column returned by callback must be between 1 and {columns}"),
                    )?;
                }
            }
        }
        if let Some(&value) = values.get(1).filter(|&&v| v != Value::Nil) {
            let score = match value {
                Value::Number(n) if n.is_finite() && n >= 0. && n.fract() == 0. => {
                    TAP_SCORES.get(n as usize).copied()
                }
                Value::String(id) => {
                    let s = std::str::from_utf8(call.string_bytes(id).unwrap())
                        .map_err(|_| call.error(b"invalid utf-8 sequence"))?;
                    TAP_SCORES
                        .iter()
                        .copied()
                        .find(|n| s == format!("TapNoteScore_{n}") || s.eq_ignore_ascii_case(n))
                }
                _ => None,
            };
            if let Some(score) = score {
                tap.score = score;
            } else {
                report_return(
                    call,
                    &store,
                    field,
                    "__songlua_did_tap_note_score_error_reported",
                    "Expected TapNoteScore",
                )?;
            }
        }
        if let Some(Value::Boolean(v)) = values.get(2) {
            tap.bright = *v;
        }
        *output.lock().unwrap() = tap;
        Ok(())
    })
}
fn report_return(
    call: &mut HostCall<'_>,
    store: &Shared,
    field: TableId,
    flag: &'static str,
    message: &str,
) -> Result<(), Error> {
    let error = call.error(message.as_bytes());
    diagnostics::report(
        call,
        store,
        field,
        diagnostics::Callback::Gameplay(flag, "DidTapNote callback return"),
        error,
    )
}
