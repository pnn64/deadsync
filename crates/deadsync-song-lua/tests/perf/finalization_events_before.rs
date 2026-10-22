// Frozen from 87e58ae89; only extracted into callable test helpers.
use super::*;

pub(super) fn finalize_sprite_textures<NoteskinSlot, ModelVertex>(
    overlays: &mut [crate::SongLuaOverlayCompileActor<
        SongLuaOverlayKind<NoteskinSlot, ModelVertex, TextAttribute>,
    >],
    sprite_textures: Vec<(usize, crate::SongLuaSpriteTexture)>,
) {
    for overlay in overlays {
        if let SongLuaOverlayKind::Sprite { textures, .. } = &mut overlay.actor.kind {
            let pointer = overlay.table.to_pointer() as usize;
            let mut bindings = sprite_textures
                .iter()
                .filter(|(actor, _)| *actor == pointer)
                .map(|(_, texture)| texture.clone())
                .collect::<Vec<_>>();
            bindings.sort_by(|a, b| a.second.total_cmp(&b.second));
            *textures = bindings.into();
        }
    }
}

pub(super) fn finalize_tracked_actors<NoteskinSlot, ModelVertex>(
    out: &mut DefaultCompiledSongLua<NoteskinSlot, ModelVertex>,
    tracked_actors: Vec<crate::SongLuaTrackedActor>,
    judgment_textures: Vec<(usize, crate::SongLuaJudgmentTexture)>,
) {
    for tracked in tracked_actors {
        match tracked.target {
            TrackedCompileActorTarget::Player(player) => out.player_actors[player] = tracked.actor,
            TrackedCompileActorTarget::PlayerJudgment(player) => {
                out.player_actors[player].judgment = SongLuaCapturedChildActor {
                    initial_state: tracked.actor.initial_state,
                    message_commands: tracked.actor.message_commands,
                    textures: {
                        let mut textures = judgment_textures
                            .iter()
                            .filter(|(index, _)| *index == player)
                            .map(|(_, texture)| texture.clone())
                            .collect::<Vec<_>>();
                        textures.sort_by(|a, b| a.second.total_cmp(&b.second));
                        textures.dedup_by(|a, b| a.path == b.path);
                        textures
                    },
                };
            }
            TrackedCompileActorTarget::PlayerCombo(player) => {
                out.player_actors[player].combo = SongLuaCapturedChildActor {
                    initial_state: tracked.actor.initial_state,
                    message_commands: tracked.actor.message_commands,
                    ..SongLuaCapturedChildActor::default()
                };
            }
            TrackedCompileActorTarget::SongForeground => out.song_foreground = tracked.actor,
            TrackedCompileActorTarget::ScreenLayer(index) => {
                out.screen_layers[index] = tracked.actor
            }
        }
    }
}

pub(super) fn merge_runtime_messages(
    messages: &mut Vec<SongLuaMessageEvent>,
    action_start: usize,
    runtime_broadcasts: &[(f32, String, bool)],
) {
    let action_end = messages.len();
    let mut matched = vec![false; action_end - action_start];
    for (runtime_beat, runtime_message, _) in runtime_broadcasts {
        let Some(index) = messages
            .iter()
            .enumerate()
            .take(action_end)
            .skip(action_start)
            .filter(|(index, event)| {
                !matched[*index - action_start]
                    && !event.message.starts_with("__songlua_overlay_fn_action_")
                    && event.message == *runtime_message
                    && event.beat <= *runtime_beat + f32::EPSILON
            })
            .max_by(|(_, left), (_, right)| left.beat.total_cmp(&right.beat))
            .map(|(index, _)| index)
        else {
            // MessageManager::Broadcast dispatches every runtime call, including
            // schedules stored under local or chart-specific table names. The
            // sampled tracks retain parameter-dependent listener writes.
            messages.push(SongLuaMessageEvent {
                beat: *runtime_beat,
                message: runtime_message.clone(),
                persists: false,
            });
            continue;
        };
        messages[index].beat = *runtime_beat;
        matched[index - action_start] = true;
    }
}
