//! Existing multitap factory preparation, using owned curves and rooted actors.
use super::*;
use crate::{
    MultitapDesc, SongLuaNoteskinResolver, SongLuaOverlayEase, SongLuaOverlayKind,
    SongLuaOverlayMessageCommand, SongLuaOverlayState, named_overlay_indices_by_name,
    overlay_descendants_by_parent, push_multitap_actor_eases, push_multitap_explosion_eases,
    song_lua_style_info,
};
use std::sync::Arc;

impl SongRuntime {
    pub fn song_multitap_update_only(&mut self, root: Value) -> Result<bool, Error> {
        self.actors.multitap_update_only(&mut self.vm, root)
    }

    pub fn read_song_multitap_descs(
        &mut self,
        context: &SongLuaCompileContext,
    ) -> Result<Option<Vec<MultitapDesc>>, String> {
        let difficulty = context.players[0]
            .difficulty
            .sm_name()
            .trim_start_matches("Difficulty_");
        let double = song_lua_style_info(&context.style_name).name == "double";
        let key = if double {
            format!("Double_{difficulty}")
        } else {
            difficulty.to_owned()
        };
        let output = Arc::new(Mutex::new(None));
        let globals = self.compiler_globals()?;
        let function = self
            .vm
            .new_host_function_with_data(read, &[globals], (key, double, output.clone()))
            .map_err(|e| e.to_string())?;
        self.vm.call(function, &[]).map_err(|e| e.to_string())?;
        Ok(output.lock().unwrap().take().unwrap())
    }

    pub fn install_song_multitap_hits<K>(
        &mut self,
        context: &SongLuaCompileContext,
        overlays: &mut SongOverlayActors<K>,
    ) -> Result<(), String> {
        let indices = named_overlay_indices_by_name(overlays.actors.len(), |i| {
            overlays.actors[i].actor.name.as_deref()
        });
        let globals = self.compiler_globals()?;
        for player in 0..LUA_PLAYERS {
            let pn = player + 1;
            if !context.players[player].enabled
                || !indices.contains_key(format!("MultitapFrameP{pn}").as_str())
            {
                continue;
            }
            let callback =
                self.compiler_value_field(globals, &format!("multitap_note_callback_P{pn}"))?;
            if !matches!(callback, Value::Function(_) | Value::Native(_)) {
                continue;
            }
            for lane in 1..=song_lua_style_info(&context.style_name).columns {
                if let Some(&index) = indices.get(format!("MultitapExplosionP{pn}_{lane}").as_str())
                {
                    let children =
                        overlay_descendants_by_parent(overlays.actors.len(), index, |i| {
                            overlays.actors[i].actor.parent_index
                        });
                    for index in std::iter::once(index).chain(children) {
                        self.capture_multitap_hits(overlays, index, lane, pn)?;
                    }
                }
            }
        }
        Ok(())
    }

    fn capture_multitap_hits<K>(
        &mut self,
        overlays: &mut SongOverlayActors<K>,
        index: usize,
        lane: usize,
        pn: usize,
    ) -> Result<(), String> {
        let actor = self
            .song_overlay_actor_value(&overlays.actors[index])
            .map_err(|e| e.to_string())?;
        let Value::Table(table) = actor else {
            unreachable!()
        };
        for grade in ["W1", "W2", "W3", "W4", "W5"] {
            let mut roots = super::song_compiler_roots::Roots::default();
            let result: Result<(), String> = (|| {
                let mut commands = Vec::new();
                for name in ["Judgment", "Dim", grade] {
                    let key = self
                        .vm
                        .try_intern(format!("{name}Command").as_bytes())
                        .map_err(|e| e.to_string())?;
                    let command = self
                        .vm
                        .raw_table_get(table, key)
                        .map_err(|e| e.to_string())?;
                    if command != Value::Nil {
                        if !matches!(command, Value::Function(_) | Value::Native(_)) {
                            return Err("function expected".into());
                        }
                        roots.keep(&mut self.vm, command)?;
                        commands.push(command);
                    }
                }
                if commands.is_empty() {
                    return Ok(());
                }
                let message = format!("__songlua_tap_{pn}_{lane}_{grade}");
                let key = format!("{message}Command");
                let invoke = self
                    .vm
                    .new_host_function(
                        |call| {
                            let actor = call.arguments().first().copied().unwrap_or(Value::Nil);
                            for i in 0..call.captures().len() {
                                let command = call.captures()[i];
                                call.call(command, &[actor])?;
                            }
                            call.push(actor)
                        },
                        &commands,
                    )
                    .map_err(|e| e.to_string())?;
                let install = self
                    .vm
                    .new_host_function_with_data(
                        |call| {
                            let key = call.function_data::<String>().unwrap().clone();
                            super::set(call, call.captures()[0], &key, call.captures()[1])
                        },
                        &[actor, invoke],
                        key.clone(),
                    )
                    .map_err(|e| e.to_string())?;
                self.vm.call(install, &[]).map_err(|e| e.to_string())?;
                let blocks = self.capture_song_actor_command(actor, &key);
                let key = self
                    .vm
                    .try_intern(key.as_bytes())
                    .map_err(|e| e.to_string())?;
                self.vm
                    .raw_table_set(table, key, Value::Nil)
                    .map_err(|e| e.to_string())?;
                let blocks = blocks?;
                if !blocks.is_empty() {
                    overlays.actors[index].actor.message_commands.push(
                        SongLuaOverlayMessageCommand {
                            frame_advance: 0.,
                            message,
                            blocks,
                        },
                    );
                }
                Ok(())
            })();
            roots.clear(&mut self.vm)?;
            result?;
        }
        Ok(())
    }

    fn multitap_noteskin<K>(
        &mut self,
        row: &SongOverlayActor<K>,
        context: &SongLuaCompileContext,
        player: usize,
    ) -> Result<String, String> {
        let actor = self
            .song_overlay_actor_value(row)
            .map_err(|e| e.to_string())?;
        let read = self
            .vm
            .new_host_function(
                |call| {
                    let original =
                        super::field(call, call.captures()[0], "__songlua_noteskin_name")?;
                    let value = if original == Value::Nil {
                        Value::Nil
                    } else {
                        Value::String(
                            call.coerce_string(original)?
                                .ok_or_else(|| call.error(b"string expected"))?,
                        )
                    };
                    call.push(value)
                },
                &[actor],
            )
            .map_err(|e| e.to_string())?;
        let value = self.vm.call(read, &[]).map_err(|e| e.to_string())?[0];
        let text = if let Value::String(id) = value {
            Some(
                std::str::from_utf8(self.vm.string_bytes(id).unwrap())
                    .map_err(|e| e.to_string())?
                    .to_owned(),
            )
        } else {
            None
        };
        Ok(text
            .filter(|v| !v.trim().is_empty())
            .unwrap_or_else(|| context.players[player].noteskin_name.clone()))
    }

    pub(super) fn ensure_multitap_visual<S, V, A>(
        &mut self,
        overlays: &mut SongOverlayActors<SongLuaOverlayKind<S, V, A>>,
        arrow: usize,
        noteskin: &str,
        visual: &mut impl FnMut(&str) -> Option<(SongLuaOverlayKind<S, V, A>, SongLuaOverlayState)>,
    ) -> Result<(), String> {
        let has_visual = overlays.actors.iter().enumerate().any(|(index, row)| {
            if !matches!(
                row.actor.kind,
                SongLuaOverlayKind::Sprite { .. }
                    | SongLuaOverlayKind::Model { .. }
                    | SongLuaOverlayKind::NoteskinActor { .. }
            ) {
                return false;
            }
            if index == arrow {
                return true;
            }
            let mut parent = row.actor.parent_index;
            while let Some(index) = parent {
                if index == arrow {
                    return true;
                }
                parent = overlays
                    .actors
                    .get(index)
                    .and_then(|r| r.actor.parent_index);
            }
            false
        });
        if has_visual {
            return Ok(());
        }
        let Some((kind, initial_state)) = visual(noteskin) else {
            return Ok(());
        };
        let owner = overlays.owner.ok_or("released song overlays")?;
        let Value::Table(array) = self.vm.rooted(owner).map_err(|e| e.to_string())? else {
            unreachable!()
        };
        let dummy = self
            .actors
            .multitap_dummy(&mut self.vm)
            .map_err(|e| e.to_string())?;
        let result = (|| {
            let value = self.vm.rooted(dummy).map_err(|e| e.to_string())?;
            let slot = self.vm.raw_table_len(array).map_err(|e| e.to_string())? + 1;
            self.vm
                .raw_table_set(array, Value::Number(slot as f64), value)
                .map_err(|e| e.to_string())?;
            overlays.actors.push(SongOverlayActor {
                owner,
                slot,
                message_sounds: Vec::new(),
                actor: crate::SongLuaOverlayActor {
                    kind,
                    name: None,
                    parent_index: Some(arrow),
                    initial_state,
                    message_commands: Vec::new(),
                },
            });
            Ok(())
        })();
        self.vm.unpin(dummy).map_err(|e| e.to_string())?;
        result
    }
}

type Descs = Arc<Mutex<Option<Option<Vec<MultitapDesc>>>>>;
fn read(call: &mut HostCall<'_>) -> Result<(), Error> {
    let (key, double, output) = call
        .function_data::<(String, bool, Descs)>()
        .unwrap()
        .clone();
    let value = super::field(call, call.captures()[0], "multitaps")?;
    let result = if value == Value::Nil {
        None
    } else {
        call.with_roots(&[value], |call| {
            if !matches!(value, Value::Table(_)) {
                return Err(call.error(b"table expected"));
            }
            let mut selected =
                super::field(call, value, &key).and_then(|v| optional_table(call, v))?;
            if selected == Value::Nil && !double {
                selected = super::field(call, value, "Challenge")
                    .and_then(|v| optional_table(call, v))
                    .unwrap_or(Value::Nil);
            }
            if selected == Value::Nil {
                return Ok(None);
            }
            call.with_roots(&[selected], |call| {
                let Value::Table(array) = selected else {
                    unreachable!()
                };
                let mut rows = Vec::new();
                for i in 1usize.. {
                    let row = call.raw_table_get(array, Value::Number(i as f64))?;
                    if row == Value::Nil {
                        break;
                    }
                    if !matches!(row, Value::Table(_)) {
                        continue;
                    }
                    let desc = call.with_roots(&[row], |call| {
                        let lane = super::field(call, row, "lane")?;
                        if lane == Value::Nil {
                            return Ok(None);
                        }
                        let lane = integer(call, lane)?;
                        let Some(lane) = usize::try_from(lane)
                            .ok()
                            .filter(|v| (1..=crate::SONG_LUA_DOUBLE_NOTE_COLUMNS).contains(v))
                        else {
                            return Ok(None);
                        };
                        let taps = super::field(call, row, "taps")?;
                        let taps = optional_table(call, taps)?;
                        if taps == Value::Nil {
                            return Ok(None);
                        }
                        let mut taps = call.with_roots(&[taps], |call| {
                            let Value::Table(array) = taps else {
                                unreachable!()
                            };
                            let mut taps = Vec::new();
                            for i in 1usize.. {
                                let tap = call.raw_table_get(array, Value::Number(i as f64))?;
                                if tap == Value::Nil {
                                    break;
                                }
                                if let Some(tap) = number(call, tap)
                                    && tap.is_finite()
                                {
                                    taps.push(tap);
                                }
                            }
                            Ok(taps)
                        })?;
                        if taps.is_empty() {
                            return Ok(None);
                        }
                        taps.sort_by(f32::total_cmp);
                        let peak = super::field(call, row, "peak")
                            .ok()
                            .and_then(|v| number(call, v))
                            .filter(|v| v.is_finite());
                        Ok(Some(MultitapDesc { lane, taps, peak }))
                    })?;
                    if let Some(desc) = desc {
                        rows.push(desc);
                    }
                }
                Ok(Some(rows))
            })
        })?
    };
    *output.lock().unwrap() = Some(result);
    Ok(())
}
fn optional_table(call: &HostCall<'_>, value: Value) -> Result<Value, Error> {
    if matches!(value, Value::Nil | Value::Table(_)) {
        Ok(value)
    } else {
        Err(call.error(b"table expected"))
    }
}
fn integer(call: &mut HostCall<'_>, value: Value) -> Result<isize, Error> {
    let read = call.new_host_function(
        |call| {
            let value = call.captures()[0];
            let mut stack = call.native_stack()?;
            stack.push(value)?;
            let integer = stack
                .to_integer(-1)
                .ok_or_else(|| stack.error(b"integer expected"))?;
            stack.push(Value::Number(integer as f64))?;
            stack.finish(1)
        },
        &[value],
    )?;
    let Value::Number(integer) = call.call(read, &[])?[0] else {
        unreachable!()
    };
    Ok(integer as isize)
}
fn number(call: &HostCall<'_>, value: Value) -> Option<f32> {
    // The legacy descriptor reader accepts string parsing by Rust, rather than
    // Lua numeric coercion (including its whitespace/hexadecimal differences).
    match value {
        Value::Number(n) => Some(n as f32),
        Value::String(id) => std::str::from_utf8(call.string_bytes(id).unwrap())
            .ok()
            .and_then(|v| v.trim().parse().ok()),
        _ => None,
    }
}

impl SongRuntime {
    /// Preserve the existing factory optimization. None selects chronological
    /// Rust replay; callers must also retain replay for active model clocks.
    pub fn compile_song_multitap_update_overlays<S, V, A>(
        &mut self,
        root: Value,
        context: &SongLuaCompileContext,
        overlays: &mut SongOverlayActors<SongLuaOverlayKind<S, V, A>>,
        noteskin_resolver: SongLuaNoteskinResolver,
        mut visual: impl FnMut(&str) -> Option<(SongLuaOverlayKind<S, V, A>, SongLuaOverlayState)>,
    ) -> Result<Option<Vec<SongLuaOverlayEase>>, String> {
        // An omitted root frame can own additional callbacks. Check the complete
        // update plan before replacing the factory's sampled replay.
        if !self
            .song_multitap_update_only(root)
            .map_err(|err| err.to_string())?
        {
            return Ok(None);
        }
        let Some(multitaps) = self.read_song_multitap_descs(context)? else {
            return Ok(None);
        };
        if multitaps.is_empty() {
            return Ok(None);
        }
        // The analytic curves encode `beat > tap` with tap.next_up(). A float
        // seconds conversion can collapse that edge onto the tap itself. Replay
        // the authored Lua in that case, preserving its native frame predicates.
        if context.song_timing.as_ref().is_some_and(|timing| {
            multitaps.iter().flat_map(|desc| &desc.taps).any(|&tap| {
                timing.get_time_for_beat_exact(tap.next_up()) <= timing.get_time_for_beat_exact(tap)
            })
        }) {
            return Ok(None);
        }
        let overlay_indices = named_overlay_indices_by_name(overlays.actors.len(), |index| {
            overlays.actors[index].actor.name.as_deref()
        });
        let mut out = Vec::new();
        for player in 0..LUA_PLAYERS {
            if !context.players[player].enabled {
                continue;
            }
            let pn = player + 1;
            if !overlay_indices.contains_key(format!("MultitapFrameP{pn}").as_str()) {
                return Ok(None);
            }
            // The startup update already copied the Player and NoteField transforms
            // into their distinct wrappers. Preserve both, including NoteField Y.
            let globals = self.compiler_globals()?;
            let rotation_x = self
                .compiler_value_field(globals, "MYSTERIOUS_VERSION_DEPENDENT_RADIAN_POLTERGEIST")?;
            let rotation_x = if rotation_x == Value::Nil {
                0.
            } else {
                let read = self
                    .vm
                    .new_host_function(
                        |call| {
                            let value = call.captures()[0];
                            let mut stack = call.native_stack()?;
                            stack.push(value)?;
                            let n = stack
                                .to_number(-1)
                                .ok_or_else(|| stack.error(b"number expected"))?;
                            stack.push(Value::Number(n))?;
                            stack.finish(1)
                        },
                        &[rotation_x],
                    )
                    .map_err(|e| e.to_string())?;
                let Value::Number(n) = self.vm.call(read, &[]).map_err(|e| e.to_string())?[0]
                else {
                    unreachable!()
                };
                n as f32 + crate::host::arrow_effects_rotation_x(0)
            };
            for (mti, desc) in multitaps.iter().enumerate() {
                let index = mti + 1;
                let Some(&frame_index) =
                    overlay_indices.get(format!("MultitapP{pn}_{index}").as_str())
                else {
                    return Ok(None);
                };
                let Some(&arrow_index) =
                    overlay_indices.get(format!("MultitapArrowP{pn}_{index}").as_str())
                else {
                    return Ok(None);
                };
                let numbered =
                    overlay_indices.contains_key(format!("MultitapTextP{pn}_{index}").as_str());
                let Some(&deco_index) = overlay_indices.get(
                    if numbered {
                        format!("MultitapTextP{pn}_{index}")
                    } else {
                        format!("MultitapDeco{pn}_{index}")
                    }
                    .as_str(),
                ) else {
                    return Ok(None);
                };
                if numbered {
                    let SongLuaOverlayKind::BitmapText { text_changes, .. } =
                        &mut overlays.actors[deco_index].actor.kind
                    else {
                        return Ok(None);
                    };
                    *text_changes = std::iter::once((
                        crate::multitap::multitap_visible_start(desc.taps[0]),
                        Arc::from(desc.taps.len().to_string()),
                    ))
                    .chain(
                        desc.taps
                            .iter()
                            .take(desc.taps.len().saturating_sub(2))
                            .enumerate()
                            .map(|(tap, beat)| {
                                (
                                    beat.next_up(),
                                    Arc::from((desc.taps.len() - tap - 1).to_string()),
                                )
                            }),
                    )
                    .collect();
                }
                overlays.actors[arrow_index]
                    .actor
                    .initial_state
                    .texcoord_offset
                    .get_or_insert([0.0, 0.0]);
                let noteskin =
                    self.multitap_noteskin(&overlays.actors[arrow_index], context, player)?;
                self.ensure_multitap_visual(overlays, arrow_index, &noteskin, &mut visual)?;
                let deco_children =
                    overlay_descendants_by_parent(overlays.actors.len(), deco_index, |index| {
                        overlays
                            .actors
                            .get(index)
                            .and_then(|overlay| overlay.actor.parent_index)
                    })
                    .into_iter()
                    .map(|index| (index, overlays.actors[index].actor.initial_state))
                    .collect::<Vec<_>>();
                overlays.actors[arrow_index].actor.initial_state.rot_x_deg += rotation_x;
                push_multitap_actor_eases(
                    &mut out,
                    frame_index,
                    overlays.actors[frame_index].actor.initial_state,
                    arrow_index,
                    overlays.actors[arrow_index].actor.initial_state,
                    deco_index,
                    overlays.actors[deco_index].actor.initial_state,
                    &deco_children,
                    context,
                    player,
                    noteskin_resolver,
                    &noteskin,
                    desc,
                    numbered,
                );
            }
            for lane in 1..=song_lua_style_info(&context.style_name).columns {
                let Some(&explosion_index) =
                    overlay_indices.get(format!("MultitapExplosionP{pn}_{lane}").as_str())
                else {
                    continue;
                };
                push_multitap_explosion_eases(
                    &mut out,
                    explosion_index,
                    overlays.actors[explosion_index].actor.initial_state,
                    context,
                    &multitaps,
                    lane,
                );
            }
        }
        Ok(Some(out))
    }
}

impl SongRuntime {
    /// The existing compiler's update stage: capture factory hits, install
    /// material clocks, then choose analytic curves or chronological Rust replay.
    #[allow(clippy::too_many_arguments)]
    pub fn compile_song_update_stage<S, V, A>(
        &mut self,
        screen: ActorScene,
        root: Value,
        context: &SongLuaCompileContext,
        overlays: &mut SongOverlayActors<SongLuaOverlayKind<S, V, A>>,
        tracked: &mut SongTrackedActors,
        messages: &[crate::SongLuaMessageEvent],
        activation: &SongActivation,
        sounds: &mut Vec<crate::SongLuaSoundEvent>,
        judgments: &mut Vec<(usize, crate::SongLuaJudgmentTexture)>,
        sprites: &mut Vec<(usize, crate::SongLuaSpriteTexture)>,
        noteskins: SongLuaNoteskinResolver,
        mut read_layer: impl FnMut(&S) -> Option<crate::SongLuaOverlayModelLayer<V>>,
        visual: impl FnMut(&str) -> Option<(SongLuaOverlayKind<S, V, A>, SongLuaOverlayState)>,
    ) -> Result<SongUpdateOutput, String> {
        self.actor_scene_root(screen)
            .ok_or("released or foreign song screen")?;
        let mut roots = super::song_compiler_roots::Roots::default();
        let result = (|| {
            roots.keep(&mut self.vm, root)?;
            self.install_song_multitap_hits(context, overlays)?;
            let mut materials = Vec::new();
            for row in &overlays.actors {
                let initial = row.actor.initial_state.sprite_state_index;
                let clocks = match &row.actor.kind {
                    SongLuaOverlayKind::Model { layers } => layers
                        .iter()
                        .map(|layer| {
                            crate::model_texture::Clock::with_initial_state(layer, initial)
                        })
                        .collect::<Vec<_>>(),
                    SongLuaOverlayKind::NoteskinActor { slots, .. } => slots
                        .iter()
                        .map(|slot| {
                            read_layer(slot).as_ref().and_then(|layer| {
                                crate::model_texture::Clock::with_initial_state(layer, initial)
                            })
                        })
                        .collect(),
                    _ => continue,
                };
                if clocks.iter().any(Option::is_some) {
                    let Value::Table(actor) = self
                        .song_overlay_actor_value(row)
                        .map_err(|e| e.to_string())?
                    else {
                        unreachable!()
                    };
                    materials.push((actor, clocks));
                }
            }
            let active = self
                .actors
                .install_compiler_materials(&mut self.vm, materials)?;
            if !active
                && let Some(overlay_eases) = self.compile_song_multitap_update_overlays(
                    root, context, overlays, noteskins, visual,
                )?
            {
                return Ok(SongUpdateOutput {
                    overlay_eases,
                    ..Default::default()
                });
            }
            self.compile_song_update_functions(
                screen, root, context, overlays, tracked, messages, activation, sounds, judgments,
                sprites,
            )
        })();
        roots.clear(&mut self.vm)?;
        result
    }
}
