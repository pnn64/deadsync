//! Existing compiler message defaults and a retained, guarded command runner.
use super::*;

#[derive(Clone)]
struct Run {
    name: Vec<u8>,
}
impl Actors {
    pub(crate) fn default_message_command_params(
        &self,
        vm: &mut Vm,
        name: &str,
    ) -> Result<Option<Root>, Error> {
        let produce = vm.new_host_function_with_data(
            |call| {
                let name = call.function_data::<String>().unwrap().clone();
                let params = defaults(call, &name)?;
                call.push(params.unwrap_or(Value::Nil))
            },
            &[Value::Table(self.registry), Value::Table(self.helpers)],
            name.to_owned(),
        )?;
        let value = vm.call(produce, &[])?[0];
        if value == Value::Nil {
            Ok(None)
        } else {
            vm.pin(value).map(Some)
        }
    }

    pub(crate) fn message_capture_runner(
        &self,
        vm: &mut Vm,
        actor: Value,
        name: &str,
        command: Value,
    ) -> Result<Root, Error> {
        let mut inputs = Vec::new();
        let result = (|| {
            inputs.push(vm.pin(actor)?);
            inputs.push(vm.pin(command)?);
            if !matches!(actor, Value::Table(id) if vm.store().actors.contains_key(&id)) {
                return vm.with_native_stack(|s| Err(s.error(b"owned actor table expected")));
            }
            if !matches!(command, Value::Function(_) | Value::Native(_)) {
                return vm
                    .with_native_stack(|s| Err(s.error(b"message command function expected")));
            }
            let produce = vm.new_host_function_with_data(
                |call| {
                    let data = call.function_data::<Run>().unwrap().clone();
                    let name = std::str::from_utf8(&data.name).unwrap();
                    let params = defaults(call, name)?.unwrap_or(Value::Nil);
                    call.with_roots(&[params], |call| {
                        let mut captures = call.captures().to_vec();
                        captures.push(params);
                        let runner = call.new_host_function_with_data(run, &captures, data)?;
                        call.push(runner)
                    })
                },
                &[
                    Value::Table(self.registry),
                    Value::Table(self.helpers),
                    actor,
                    command,
                ],
                Run {
                    name: name.as_bytes().to_vec(),
                },
            )?;
            let runner = vm.call(produce, &[])?[0];
            vm.pin(runner)
        })();
        for input in inputs {
            vm.unpin(input)?;
        }
        result
    }
}

fn run(call: &mut HostCall<'_>) -> Result<(), Error> {
    let data = call.function_data::<Run>().unwrap().clone();
    let actor = table(call, call.captures()[2])?;
    let command = call.captures()[3];
    let params = call.captures()[4];
    // Use the selected function even when its source field was replaced. The
    // command path drains its receiver and work queued by it, not other actors.
    run_guarded_command(
        call,
        actor,
        data.name,
        command,
        (params != Value::Nil).then_some(params),
        true,
    )
}

fn constant(call: &mut HostCall<'_>) -> Result<(), Error> {
    call.push(call.captures()[0])
}
fn set_text(
    call: &mut HostCall<'_>,
    params: TableId,
    key: &[u8],
    text: &[u8],
) -> Result<(), Error> {
    let value = call.try_intern(text)?;
    judgment::set(call, params, key, value)
}
pub(super) fn defaults(call: &mut HostCall<'_>, name: &str) -> Result<Option<Value>, Error> {
    let Some(message) = name.strip_suffix("MessageCommand") else {
        return Ok(None);
    };
    let params = call.try_new_table()?;
    call.with_roots(&[Value::Table(params)], |call| {
        match message {
            "Judgment" => {
                set_text(call, params, b"Player", b"PlayerNumber_P1")?;
                set_text(call, params, b"TapNoteScore", b"TapNoteScore_W3")?;
                judgment::set(call, params, b"TapNoteOffset", Value::Number(0.))?;
                judgment::set(call, params, b"FirstTrack", Value::Number(0.))?;
                judgment::normalize(call, params)?;
            }
            "EarlyHit" => {
                set_text(call, params, b"Player", b"PlayerNumber_P1")?;
                set_text(call, params, b"TapNoteScore", b"TapNoteScore_W3")?;
                judgment::set(
                    call,
                    params,
                    b"TapNoteOffset",
                    Value::Number(f64::from(-0.02_f32)),
                )?;
                judgment::set(call, params, b"Early", Value::Boolean(true))?;
            }
            "LifeChanged" => {
                set_text(call, params, b"Player", b"PlayerNumber_P1")?;
                let meter = call.try_new_table()?;
                call.with_roots(&[Value::Table(meter)], |call| {
                    for (name, value) in [
                        (
                            b"GetLife".as_slice(),
                            Value::Number(f64::from(crate::SONG_LUA_INITIAL_LIFE)),
                        ),
                        (b"IsFailing", Value::Boolean(false)),
                        (b"IsHot", Value::Boolean(false)),
                        (b"IsInDanger", Value::Boolean(false)),
                    ] {
                        let method = call.new_host_function(constant, &[value])?;
                        judgment::set(call, meter, name, method)?;
                    }
                    judgment::set(call, params, b"LifeMeter", Value::Table(meter))
                })?;
            }
            "HealthStateChanged" => {
                set_text(call, params, b"Player", b"PlayerNumber_P1")?;
                set_text(call, params, b"PlayerNumber", b"PlayerNumber_P1")?;
                set_text(call, params, b"HealthState", b"HealthState_Alive")?;
            }
            "ExCountsChanged" => {
                set_text(call, params, b"Player", b"PlayerNumber_P1")?;
                let counts = call.try_new_table()?;
                call.with_roots(&[Value::Table(counts)], |call| {
                    for key in [
                        "W0", "W1", "W2", "W3", "W4", "W5", "Miss", "Held", "LetGo", "HitMine",
                    ] {
                        judgment::set(call, counts, key.as_bytes(), Value::Number(0.))?;
                    }
                    judgment::set(call, params, b"ExCounts", Value::Table(counts))
                })?;
                for key in ["ExScore", "ActualPoints"] {
                    judgment::set(call, params, key.as_bytes(), Value::Number(0.))?;
                }
                for key in ["ActualPossible", "CurrentPossible"] {
                    judgment::set(call, params, key.as_bytes(), Value::Number(1.))?;
                }
            }
            "PlayerOptionsChanged" => {
                set_text(call, params, b"Player", b"PlayerNumber_P1")?;
                set_text(call, params, b"PlayerNumber", b"PlayerNumber_P1")?;
            }
            _ => return Ok(None),
        }
        Ok(Some(Value::Table(params)))
    })
}
