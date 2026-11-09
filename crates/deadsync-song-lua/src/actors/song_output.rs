//! Production tracked actor extraction; Lua access stays inside one host call.
use super::*;
use crate::{SongLuaCapturedActor, SongLuaTrackedActorTarget as Target};
use sprites::{field, set};

pub(crate) struct Tracked {
    pub actor: SongLuaCapturedActor,
    pub target: Target,
}
#[derive(Clone)]
struct Read {
    store: Shared,
    scene: u64,
    output: Arc<Mutex<Vec<Tracked>>>,
}
impl Actors {
    pub(crate) fn read_song_tracked_actors(
        &self,
        vm: &mut Vm,
        screen: ActorScene,
    ) -> Result<(Value, Vec<Tracked>), Error> {
        let output = Arc::new(Mutex::new(Vec::new()));
        let function = vm.new_host_function_with_data(
            read,
            &[Value::Table(self.registry), Value::Table(self.helpers)],
            Read {
                store: self.store.clone(),
                scene: screen.id,
                output: output.clone(),
            },
        )?;
        let values = vm.call(function, &[])?[0];
        let rows = std::mem::take(&mut *output.lock().unwrap());
        Ok((values, rows))
    }
}
fn optional_table(call: &HostCall<'_>, value: Value) -> Result<Option<TableId>, Error> {
    if value == Value::Nil {
        Ok(None)
    } else {
        table(call, value).map(Some)
    }
}
fn registry(call: &mut HostCall<'_>, actor: TableId) -> Result<TableId, Error> {
    let function = native_children::helper_function(call, b"native_registry")?;
    let value = call.call(function, &[Value::Table(actor)])?[0];
    table(call, value)
}
fn manual_hud(call: &mut HostCall<'_>, player: TableId) -> Result<bool, Error> {
    let value = field(call, player, "__songlua_children")?;
    let Some(children) = optional_table(call, value)? else {
        return Ok(false);
    };
    call.with_roots(&[value], |call| {
        for name in ["Judgment", "Combo"] {
            let value = field(call, children, name)?;
            let Some(child) = optional_table(call, value)? else {
                continue;
            };
            let present = call.with_roots(&[value], |call| {
                let value = field(call, child, "__songlua_manual_draw_state")?;
                Ok(optional_table(call, value)?.is_some())
            })?;
            if present {
                return Ok(true);
            }
        }
        Ok(false)
    })
}
fn push(
    call: &mut HostCall<'_>,
    data: &Read,
    batch: TableId,
    actor: TableId,
    target: Target,
) -> Result<(), Error> {
    // Pin the identity before any authored getter can detach it and collect.
    let index = call.raw_table_len(batch)? + 1;
    call.raw_table_set(batch, Value::Number(index as f64), Value::Table(actor))?;
    let state = if matches!(target, Target::Player(_)) {
        initial_state::read(call, &data.store, actor)?
    } else {
        let value = field(call, actor, "__songlua_manual_draw_state")?;
        if let Some(snapshot) = optional_table(call, value)? {
            call.with_roots(&[value], |call| {
                initial_state::read(call, &data.store, snapshot)
            })?
        } else {
            initial_state::read(call, &data.store, actor)?
        }
    };
    let manual_hud_draw = matches!(target, Target::Player(_)) && manual_hud(call, actor)?;
    data.output.lock().unwrap().push(Tracked {
        target,
        actor: SongLuaCapturedActor {
            initial_state: state,
            manual_hud_draw,
            ..Default::default()
        },
    });
    Ok(())
}
fn child(
    call: &mut HostCall<'_>,
    data: &Read,
    parent: TableId,
    name: &str,
) -> Result<TableId, Error> {
    let kind = assets::field(call, parent, b"__songlua_actor_type")?;
    let index = field(call, parent, "__songlua_player_index")?;
    if kind
        .as_deref()
        .is_some_and(|s| s.eq_ignore_ascii_case("PlayerActor"))
        && index != Value::Nil
    {
        let index = sprites::number(call, index)?;
        if index.fract() != 0. || !(0. ..crate::LUA_PLAYERS as f64).contains(&index) {
            return Err(call.error(b"tracked player index out of range"));
        }
        return player_parts::tracked_child(
            call,
            &data.store,
            data.scene,
            parent,
            index as usize,
            name,
        );
    }
    let top = kind
        .as_deref()
        .is_some_and(|s| s.eq_ignore_ascii_case("TopScreen"));
    let actor = if top {
        screen_parts::create(call, &data.store, data.scene, name)?
    } else {
        generated_players::dummy(call, &data.store, data.scene, "ChildActor", name, None)?
    };
    underlay::copy_tags(call, parent, actor)?;
    native_children::adopt(call, &data.store, actor, parent)?;
    if top {
        let value = call.try_intern(name.as_bytes())?;
        set(call, actor, "__songlua_top_screen_child_name", value)?;
    }
    Ok(actor)
}
fn read(call: &mut HostCall<'_>) -> Result<(), Error> {
    let data = call.function_data::<Read>().unwrap().clone();
    let batch = call.try_new_table()?;
    call.with_roots(&[Value::Table(batch)], |call| {
        let globals = table(call, helper(call, 5)?)?;
        let top = field(call, globals, "__songlua_top_screen")?;
        let top = table(call, top)?;
        call.with_roots(&[Value::Table(top)], |call| {
            let children = registry(call, top)?;
            call.with_roots(&[Value::Table(children)], |call| {
                let value = field(call, children, "SongForeground")?;
                let foreground = if let Some(actor) = optional_table(call, value)? {
                    actor
                } else {
                    let actor = child(call, &data, top, "SongForeground")?;
                    set(call, children, "SongForeground", Value::Table(actor))?;
                    actor
                };
                call.with_roots(&[Value::Table(foreground)], |call| {
                    // Native resolves both players before reading any of their state.
                    let p1 = field(call, globals, "__songlua_top_screen_player_1")?;
                    let p1 = table(call, p1)?;
                    call.with_roots(&[Value::Table(foreground), Value::Table(p1)], |call| {
                        let p2 = field(call, globals, "__songlua_top_screen_player_2")?;
                        let p2 = table(call, p2)?;
                        call.with_roots(&[Value::Table(p2)], |call| {
                            for (index, player) in [p1, p2].into_iter().enumerate() {
                                push(call, &data, batch, player, Target::Player(index))?;
                                let children = registry(call, player)?;
                                call.with_roots(&[Value::Table(children)], |call| {
                                    for (name, target) in [
                                        ("Judgment", Target::PlayerJudgment(index)),
                                        ("Combo", Target::PlayerCombo(index)),
                                    ] {
                                        let value = field(call, children, name)?;
                                        let actor =
                                            if let Some(actor) = optional_table(call, value)? {
                                                actor
                                            } else {
                                                child(call, &data, player, name)?
                                            };
                                        push(call, &data, batch, actor, target)?;
                                    }
                                    Ok(())
                                })?;
                            }
                            push(call, &data, batch, foreground, Target::SongForeground)?;
                            for (index, name) in ["Underlay", "Overlay", "SongBackground", "In"]
                                .into_iter()
                                .enumerate()
                            {
                                let value = field(call, children, name)?;
                                if let Some(actor) = optional_table(call, value)? {
                                    push(call, &data, batch, actor, Target::ScreenLayer(index))?;
                                }
                            }
                            call.push(Value::Table(batch))
                        })
                    })
                })
            })
        })
    })
}
