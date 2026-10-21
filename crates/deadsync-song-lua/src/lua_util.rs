use crate::callback_args::{CheckedPrefix, IgnoredArgs, NumericArgs};
use crate::tween_pose::TweenPose;
use crate::native_vector::ActorVectorValue;
use crate::render_flags::{RenderFlag, RenderFlags};
use crate::base_transform::{BaseField, BaseTransform};
use deadlib_present::actors::TextAttribute;
use deadlib_present::anim::EffectClock;
use mlua::{FromLua, Function, Lua, MultiValue, Table, Value, ffi};
use rustc_hash::FxHashMap;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::ffi::c_int;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use crate::{
    GRAPH_DISPLAY_VALUE_RESOLUTION, LUA_PLAYERS, SONG_LUA_BROADCASTS_KEY,
    SONG_LUA_CAPTURE_ACTOR_SET_KEY, SONG_LUA_CAPTURE_ACTORS_KEY, SONG_LUA_CAPTURE_SNAPSHOTS_KEY,
    SONG_LUA_DANGER_LIFE, SONG_LUA_INITIAL_LIFE, SONG_LUA_PROBE_ACTOR_SET_KEY,
    SONG_LUA_PROBE_ACTORS_KEY, SONG_LUA_PROBE_METHODS_KEY, SONG_LUA_SOUND_CALLS_KEY,
    SONG_LUA_SOUND_PATHS_KEY, SONG_LUA_SPRITE_STATE_CLEAR, SONG_LUA_STARTUP_MESSAGE,
    SONG_LUA_SUPPRESS_BROADCAST_KEY, SONG_LUA_THEME_PATH_PREFIX, SONG_LUA_TOP_SCREEN_OPTION_ROWS,
    SongLuaActorMultiVertexPoint, SongLuaCapturedActor, SongLuaColumnOffsetBuildParams,
    SongLuaColumnOffsetSample, SongLuaColumnOffsetWindow, SongLuaColumnTransformSample,
    SongLuaColumnTransformTarget, SongLuaCompileContext, SongLuaCompileInfo, SongLuaDifficulty,
    SongLuaEaseTarget, SongLuaMessageEvent, SongLuaNoteHideWindow, SongLuaOverlayBlendMode,
    SongLuaOverlayCommandBlock, SongLuaOverlayEase, SongLuaOverlayEaseBuildParams,
    SongLuaOverlayKind, SongLuaOverlayMeshVertex, SongLuaOverlayMessageCommand,
    SongLuaOverlayModelLayer, SongLuaOverlayState, SongLuaOverlayStateDelta,
    SongLuaOverlayUpdateTarget, SongLuaOverlayUpdateValue, SongLuaPlayerContext,
    SongLuaProxyTarget, SongLuaSoundEvent, SongLuaSpanMode, SongLuaStatefulMessageCapture,
    SongLuaStatefulMessageWrite, SongLuaTimeUnit, SongLuaTrackedActor, SongLuaTrackedActorTarget,
    THEME_RECEPTOR_Y_REV, THEME_RECEPTOR_Y_STD, TOP_SCREEN_THEME_CHILD_NAMES,
    UNDERLAY_THEME_CHILD_NAMES, call_with_chunk_env, call_with_script_dir, call_with_script_path,
    clone_lua_value, column_transform_windows_from_samples, compile_song_runtime_delta_values,
    compile_song_runtime_values, create_chunk_env_proxy, create_life_record_table,
    crop_texture_rect, display_bpms_text, effect_clock_label, file_path_string,
    format_rolling_number, graph_display_body_size, initial_chunk_environment,
    input_status_actor_text, is_song_lua_audio_path, is_song_lua_image_path,
    is_song_lua_media_path, is_song_lua_video_path, message_command_lists_have_listener,
    note_song_lua_side_effect, offset_texture_rect as song_lua_offset_texture_rect,
    option_row_default_text, overlay_eases_from_captures, overlay_state_after_blocks,
    overlay_text_align_label, parse_color_text, parse_overlay_blend_mode,
    parse_overlay_effect_clock, parse_overlay_effect_mode, parse_overlay_text_align,
    parse_overlay_text_glow_mode, parse_sprite_sheet_dims, player_child_proxy_name,
    player_index_from_value, player_number_name, push_unique_compile_detail,
    read_actions_with_function_capture, read_boolish, read_f32, read_f64, read_i32_value,
    read_song_lua_broadcasts, read_string, read_u32_value,
    record_unsupported_function_action_capture, register_loader_env, resolve_load_actor_path,
    resolve_script_path, rolling_numbers_format, scale_to_rect_plan, set_compile_song_runtime_beat,
    set_compile_song_runtime_values, set_string_method, song_lua_halign_value,
    song_lua_human_player_count, song_lua_side_effect_count, song_lua_span_end,
    song_lua_style_column_x, song_lua_style_info, song_lua_text_align_value,
    song_lua_valid_sprite_state_index, song_lua_valign_value, song_music_rate,
    sprite_animation_state_at, sprite_frame_count, sprite_image_frame_size, sprite_texture_rect,
    text_glow_mode_label, texture_pixel_offset_rect, theme_metric_number, theme_path,
    top_screen_danger_index, top_screen_life_meter_index,
    top_screen_life_meter_name, top_screen_option_row_name, top_screen_player_index,
    top_screen_player_name, top_screen_score_index, top_screen_score_name,
    top_screen_score_percent_name,
    top_screen_step_stats_pane_index, top_screen_steps_display_index, truthy, underlay_score_index,
    underlay_score_name,
};

type ActorAssetPrefixKey = (String, String);
type ActorAssetPrefixCache = Mutex<HashMap<ActorAssetPrefixKey, Option<PathBuf>>>;
static ACTOR_ASSET_PREFIX_CACHE: OnceLock<ActorAssetPrefixCache> = OnceLock::new();
const SONG_LUA_CHILD_GROUP_KEY: &str = "__songlua_child_group";
const SONG_LUA_AFT_COUNTER_KEY: &str = "__songlua_aft_counter";
const DRAW_CALLBACK_ACTIVE_KEY: &str = "__songlua_draw_callback_active";
const MANUAL_DRAW_STATE_KEY: &str = "__songlua_manual_draw_state";
const ACTIVE_BROADCAST_KEY: &str = "__songlua_active_broadcast";
const UPDATE_FN_ERROR_KEY: &str = "__songlua_update_function_error_reported";
const UPDATE_CMD_ERROR_KEY: &str = "__songlua_update_command_error_reported";
const UPDATE_QUEUE_ERROR_KEY: &str = "__songlua_update_queue_error_reported";

enum ActorScriptDir {
    Blank,
    Text(mlua::LuaString),
}

impl mlua::FromLua for ActorScriptDir {
    fn from_lua(value: Value, lua: &Lua) -> mlua::Result<Self> {
        // Keep String's coercions and errors for non-stack conversions.
        let text = String::from_lua(value.clone(), lua)?;
        if text.trim().is_empty() {
            Ok(Self::Blank)
        } else {
            Ok(Self::Text(mlua::LuaString::from_lua(value, lua)?))
        }
    }

    unsafe fn from_stack(index: c_int, lua: &mlua::state::RawLua) -> mlua::Result<Self> {
        let state = lua.state();
        // SAFETY: mlua owns this live stack slot. Borrow only a type-checked
        // string, validate it while it stays on the stack, then retain it using
        // mlua's owning handle. No raw pointer or borrowed bytes escape.
        unsafe {
            let blank = if ffi::lua_type(state, index) == ffi::LUA_TSTRING {
                let mut len = 0;
                let data = ffi::lua_tolstring(state, index, &mut len);
                let bytes = std::slice::from_raw_parts(data.cast::<u8>(), len);
                std::str::from_utf8(bytes)
                    .map_err(|err| mlua::Error::FromLuaConversionError {
                        from: "string",
                        to: "String".into(),
                        message: Some(err.to_string()),
                    })?
                    .trim()
                    .is_empty()
            } else {
                // Numeric coercion and non-string errors stay identical.
                String::from_stack(index, lua)?.trim().is_empty()
            };
            if blank {
                Ok(Self::Blank)
            } else {
                Ok(Self::Text(mlua::LuaString::from_stack(index, lua)?))
            }
        }
    }
}

// Short, transient Lua fields stay on the stack. Keep String's coercions and
// diagnostics, including the direct-stack UTF-8 error, for existing scripts.
pub(crate) struct LuaFieldText<const N: usize>(smallvec::SmallVec<[u8; N]>)
where
    [u8; N]: smallvec::Array<Item = u8>;

impl<const N: usize> LuaFieldText<N>
where
    [u8; N]: smallvec::Array<Item = u8>,
{
    pub(crate) fn as_str(&self) -> &str {
        std::str::from_utf8(&self.0).expect("LuaFieldText contains validated UTF-8")
    }
}

impl<const N: usize> mlua::FromLua for LuaFieldText<N>
where
    [u8; N]: smallvec::Array<Item = u8>,
{
    fn from_lua(value: Value, lua: &Lua) -> mlua::Result<Self> {
        let text = String::from_lua(value, lua)?;
        Ok(Self(smallvec::SmallVec::from_vec(text.into_bytes())))
    }

    unsafe fn from_stack(index: c_int, lua: &mlua::state::RawLua) -> mlua::Result<Self> {
        let state = lua.state();
        // SAFETY: mlua supplies a live stack index for the duration of this
        // conversion. Check its type before borrowing Lua's string storage,
        // and copy all bytes into owned storage before returning.
        unsafe {
            if ffi::lua_type(state, index) == ffi::LUA_TSTRING {
                let mut len = 0;
                let data = ffi::lua_tolstring(state, index, &mut len);
                let bytes = std::slice::from_raw_parts(data.cast::<u8>(), len);
                let text = std::str::from_utf8(bytes).map_err(|err| {
                    mlua::Error::FromLuaConversionError {
                        from: "string",
                        to: "String".into(),
                        message: Some(err.to_string()),
                    }
                })?;
                return Ok(Self(smallvec::SmallVec::from_slice(text.as_bytes())));
            }
            // Preserve mlua's number coercion and non-string errors.
            let text = String::from_stack(index, lua)?;
            Ok(Self(smallvec::SmallVec::from_vec(text.into_bytes())))
        }
    }
}

pub struct TopScreenLuaTables {
    pub top_screen: Table,
    pub players: [Table; LUA_PLAYERS],
}

pub struct SongLuaOverlayCompileActor<Kind> {
    pub table: Table,
    pub actor: crate::SongLuaOverlayActor<Kind>,
    pub message_sounds: Vec<(String, PathBuf)>,
}

#[cfg(test)]
impl<Kind> std::borrow::Borrow<Table> for SongLuaOverlayCompileActor<Kind> {
    fn borrow(&self) -> &Table {
        &self.table
    }
}

struct SongLuaOverlayUpdateCapture {
    actor_indices: FxHashMap<usize, usize>,
    // Load-time replay owns new wrappers and their initial poses. Their capture
    // indices follow temporary player/layer slots until final remapping.
    wrapper_start: Option<usize>,
    wrappers: Vec<(usize, Table, SongLuaOverlayState)>,
    wrapper_seen: usize,
    active_broadcast: Option<String>,
    // Scoped with active_broadcast so nested dispatch restores both together.
    active_broadcast_command: Option<mlua::LuaString>,
    active_broadcast_params: bool,
    queued_depth: usize,
    #[cfg(feature = "test-support")]
    update_command: bool,
    runtime_broadcasts: Vec<(f32, String, bool)>,
    runtime_sounds: Vec<SongLuaSoundEvent>,
    judgment_textures: Vec<(usize, crate::SongLuaJudgmentTexture)>,
    sprite_textures: Vec<(usize, crate::SongLuaSpriteTexture)>,
    message_advances: BTreeMap<(usize, String), f32>,
    #[cfg(feature = "test-support")]
    writes: BTreeMap<(usize, SongLuaOverlayUpdateTarget), Vec<crate::SongLuaOverlayUpdateSample>>,
    touched: Vec<usize>,
    touched_flags: Vec<bool>,
    values: Vec<Vec<(SongLuaOverlayUpdateTarget, SongLuaOverlayUpdateValue)>>,
    final_values: Vec<Vec<(SongLuaOverlayUpdateTarget, SongLuaOverlayUpdateValue)>>,
    scheduled: Vec<Vec<SongLuaScheduledOverlayUpdate>>,
    pending_tweens: Vec<Vec<(SongLuaOverlayUpdateTarget, SongLuaOverlayUpdateValue)>>,
    tween_resets: Vec<bool>,
    // One load-time replay owns this fixed actor-indexed buffer. Reuse it each
    // frame so callbacks can read actors whose update has not happened yet.
    prior_positions: Vec<Option<[f32; 3]>>,
    stateful_messages: BTreeMap<String, BTreeMap<usize, BTreeSet<SongLuaOverlayUpdateTarget>>>,
    stateful_writes: BTreeMap<String, Vec<SongLuaStatefulMessageWrite>>,
}

struct SongLuaProbeCaptureActive;
struct SongLuaActionCaptureActive {
    functions: HashSet<usize>,
    locals: Vec<FunctionActionSnapshot>,
}
pub(crate) struct SongLuaUpdateErrors(pub Vec<String>);

struct SongLuaDeferredMessage {
    actor: Table,
    command: String,
    detail: String,
    observed: bool,
}

// Owned by one load-time Lua session. Retry once after chronological replay;
// actual broadcasts capture their writes while no static block is available.
struct SongLuaDeferredMessages(Vec<SongLuaDeferredMessage>);

fn has_deferred_message(lua: &Lua, actor: &Table, command: Option<&str>) -> bool {
    lua.app_data_ref::<SongLuaDeferredMessages>()
        .is_some_and(|deferred| {
            deferred.0.iter().any(|message| {
                message.actor.to_pointer() == actor.to_pointer()
                    && command.is_none_or(|command| message.command == command)
            })
        })
}

fn defer_actor_message(lua: &Lua, actor: &Table, command: &str, detail: String) -> String {
    if lua.app_data_ref::<SongLuaDeferredMessages>().is_none() {
        lua.set_app_data(SongLuaDeferredMessages(Vec::new()));
    }
    let mut deferred = lua
        .app_data_mut::<SongLuaDeferredMessages>()
        .expect("deferred message list was initialized");
    if let Some(existing) = deferred
        .0
        .iter()
        .find(|entry| entry.actor.to_pointer() == actor.to_pointer() && entry.command == command)
    {
        return existing.detail.clone();
    }
    deferred.0.push(SongLuaDeferredMessage {
        actor: actor.clone(),
        command: command.to_owned(),
        detail: detail.clone(),
        observed: false,
    });
    detail
}

#[derive(Clone)]
pub struct SongLuaScheduledOverlayUpdate {
    /// Immutable native time-left samples baked at load, shared by all writes
    /// to the queue's back tween. No sampling work or allocation during play.
    pub progress: Option<std::sync::Arc<[[f32; 2]]>>,
    /// Queued commands cannot render before their actual replay frame.
    pub dispatch_seconds: Option<f64>,
    pub frame_advance: f32,
    pub initial_value: Option<SongLuaOverlayUpdateValue>,
    pub delay_seconds: f32,
    pub duration_seconds: f32,
    pub easing: Option<String>,
    pub opt1: Option<f32>,
    pub target: SongLuaOverlayUpdateTarget,
    pub value: SongLuaOverlayUpdateValue,
}

impl SongLuaOverlayUpdateCapture {
    fn new(actor_indices: FxHashMap<usize, usize>) -> Self {
        // Shared actor tables can replace earlier map entries without changing
        // their overlay indices. Size storage by the index domain, not key count.
        let actor_count = actor_indices.values().max().map_or(0, |index| index + 1);
        Self {
            actor_indices,
            wrapper_start: None,
            wrappers: Vec::new(),
            wrapper_seen: 0,
            active_broadcast: None,
            active_broadcast_command: None,
            active_broadcast_params: false,
            queued_depth: 0,
            #[cfg(feature = "test-support")]
            update_command: false,
            runtime_broadcasts: Vec::new(),
            runtime_sounds: Vec::new(),
            judgment_textures: Vec::new(),
            sprite_textures: Vec::new(),
            message_advances: BTreeMap::new(),
            #[cfg(feature = "test-support")]
            writes: BTreeMap::new(),
            touched: Vec::with_capacity(actor_count),
            touched_flags: vec![false; actor_count],
            values: (0..actor_count).map(|_| Vec::new()).collect(),
            final_values: (0..actor_count).map(|_| Vec::new()).collect(),
            scheduled: (0..actor_count).map(|_| Vec::new()).collect(),
            pending_tweens: (0..actor_count).map(|_| Vec::new()).collect(),
            tween_resets: vec![false; actor_count],
            prior_positions: vec![None; actor_count],
            stateful_messages: BTreeMap::new(),
            stateful_writes: BTreeMap::new(),
        }
    }

    fn record_stateful_message(
        &mut self,
        actor: &Table,
        beat: f32,
        target: SongLuaOverlayUpdateTarget,
        value: SongLuaOverlayUpdateValue,
        delay_seconds: f32,
        duration_seconds: f32,
        easing: Option<String>,
        opt1: Option<f32>,
    ) -> Option<usize> {
        if self.active_broadcast.is_none() {
            return None;
        }
        let index = self.touch(actor)?;
        let message = self
            .active_broadcast
            .as_deref()
            .expect("broadcast is active");
        if let Some(targets) = self.stateful_messages.get_mut(message) {
            targets.entry(index).or_default().insert(target);
        } else {
            self.stateful_messages.insert(
                message.to_owned(),
                BTreeMap::from([(index, BTreeSet::from([target]))]),
            );
        }
        let write = SongLuaStatefulMessageWrite {
            overlay_index: index,
            beat,
            target,
            value,
            delay_seconds,
            duration_seconds,
            easing,
            opt1,
        };
        if let Some(writes) = self.stateful_writes.get_mut(message) {
            writes.push(write);
        } else {
            self.stateful_writes.insert(message.to_owned(), vec![write]);
        }
        Some(index)
    }

    fn touch(&mut self, actor: &Table) -> Option<usize> {
        let &index = self.actor_indices.get(&(actor.to_pointer() as usize))?;
        if !self.touched_flags[index] {
            self.touched_flags[index] = true;
            self.touched.push(index);
        }
        Some(index)
    }

    fn record(
        &mut self,
        actor: &Table,
        beat: f32,
        target: SongLuaOverlayUpdateTarget,
        value: SongLuaOverlayUpdateValue,
    ) -> bool {
        let index = if self.active_broadcast.is_some() {
            self.record_stateful_message(actor, beat, target, value.clone(), 0.0, 0.0, None, None)
        } else {
            self.touch(actor)
        };
        let Some(index) = index else {
            return false;
        };
        #[cfg(feature = "test-support")]
        self.record_write(index, beat, target, &value, false);
        Self::replace_value(&mut self.final_values[index], target, value.clone());
        Self::replace_value(&mut self.values[index], target, value);
        true
    }

    #[cfg(feature = "test-support")]
    fn record_write(
        &mut self,
        index: usize,
        beat: f32,
        target: SongLuaOverlayUpdateTarget,
        value: &SongLuaOverlayUpdateValue,
        scheduled: bool,
    ) {
        if self.active_broadcast.is_none() && (self.queued_depth == 0 || self.update_command) {
            let samples = self.writes.entry((index, target)).or_default();
            let sample = crate::SongLuaOverlayUpdateSample {
                time: beat,
                value: value.clone(),
            };
            if samples.last().is_some_and(|last| last.time == beat) {
                if !scheduled {
                    *samples.last_mut().expect("sample exists") = sample;
                }
            } else {
                samples.push(sample);
            }
        }
    }

    fn replace_value(
        values: &mut Vec<(SongLuaOverlayUpdateTarget, SongLuaOverlayUpdateValue)>,
        target: SongLuaOverlayUpdateTarget,
        value: SongLuaOverlayUpdateValue,
    ) {
        if let Some((_, current)) = values.iter_mut().find(|(current, _)| *current == target) {
            *current = value;
        } else {
            values.push((target, value));
        }
    }

    fn record_scheduled(
        &mut self,
        actor: &Table,
        beat: f32,
        delay_seconds: f32,
        duration_seconds: f32,
        easing: Option<String>,
        opt1: Option<f32>,
        target: SongLuaOverlayUpdateTarget,
        value: SongLuaOverlayUpdateValue,
    ) -> bool {
        let index = if self.active_broadcast.is_some() {
            self.record_stateful_message(
                actor,
                beat,
                target,
                value.clone(),
                delay_seconds,
                duration_seconds,
                easing.clone(),
                opt1,
            )
        } else {
            self.touch(actor)
        };
        let Some(index) = index else {
            return false;
        };
        #[cfg(feature = "test-support")]
        self.record_write(index, beat, target, &value, true);
        Self::replace_value(&mut self.final_values[index], target, value.clone());
        let initial_value = self.values[index]
            .iter()
            .find(|(property, _)| *property == target)
            // BeginTweening copies the back tween's destination. Starting
            // from its current interpolated pose adds lag to repeated tweens.
            .or_else(|| {
                self.pending_tweens[index]
                    .iter()
                    .find(|(property, _)| *property == target)
            })
            .map(|(_, value)| value.clone());
        self.scheduled[index].push(SongLuaScheduledOverlayUpdate {
            progress: None,
            dispatch_seconds: None,
            frame_advance: 0.0,
            initial_value,
            delay_seconds,
            duration_seconds,
            easing,
            opt1,
            target,
            value,
        });
        true
    }

    fn set_scheduled_time(
        &mut self,
        index: usize,
        dispatch_seconds: Option<f64>,
        frame_advance: f32,
        progress: Option<std::sync::Arc<[[f32; 2]]>>,
    ) {
        let updates = &mut self.scheduled[index];
        let Some((last, prior)) = updates.split_last_mut() else {
            return;
        };
        last.dispatch_seconds = dispatch_seconds;
        last.frame_advance = frame_advance;
        last.progress = progress;
        // Setters mutate one back tween state. Repeated property writes in
        // that state replace its destination while retaining its starting pose.
        let duplicate = prior.iter().rposition(|update| {
            update.target == last.target
                && update.dispatch_seconds == last.dispatch_seconds
                && update.frame_advance == last.frame_advance
                && update.delay_seconds == last.delay_seconds
                && update.duration_seconds == last.duration_seconds
                && update.easing == last.easing
                && update.opt1 == last.opt1
        });
        if let Some(previous) = duplicate
            && let Some(last) = updates.pop()
        {
            updates[previous].value = last.value;
        }
    }
}

pub(crate) fn begin_overlay_update_capture_from_indices(
    lua: &Lua,
    actor_indices: impl IntoIterator<Item = (usize, usize)>,
) {
    // Actor pointers are trusted internal keys. Build the fast map directly
    // from compiler actors or an existing map without an intermediate map.
    lua.set_app_data(SongLuaOverlayUpdateCapture::new(
        actor_indices.into_iter().collect(),
    ));
}

pub(crate) fn begin_wrapper_capture(lua: &Lua) {
    if let Some(mut capture) = lua.app_data_mut::<SongLuaOverlayUpdateCapture>() {
        capture.wrapper_start = Some(capture.values.len());
    }
}

fn capture_new_wrapper(lua: &Lua, owner: &Table, wrapper: &Table) -> mlua::Result<()> {
    ensure_tween_replay(lua, wrapper)?;
    let owner = owner.to_pointer() as usize;
    if lua
        .app_data_ref::<SongLuaOverlayUpdateCapture>()
        .is_none_or(|capture| {
            capture.wrapper_start.is_none() || !capture.actor_indices.contains_key(&owner)
        })
    {
        return Ok(());
    }
    let initial = actor_overlay_initial_state(wrapper).map_err(mlua::Error::external)?;
    if let Some(mut capture) = lua.app_data_mut::<SongLuaOverlayUpdateCapture>() {
        let index = capture.values.len();
        capture
            .actor_indices
            .insert(wrapper.to_pointer() as usize, index);
        capture.touched_flags.push(false);
        capture.values.push(Vec::new());
        capture.final_values.push(Vec::new());
        capture.scheduled.push(Vec::new());
        capture.pending_tweens.push(Vec::new());
        capture.tween_resets.push(false);
        capture.prior_positions.push(None);
        capture.wrappers.push((owner, wrapper.clone(), initial));
    }
    Ok(())
}

pub(crate) fn append_wrapper_actors(
    lua: &Lua,
    actors: &mut Vec<Table>,
    states: &mut [&mut Vec<SongLuaOverlayState>],
) {
    let Some(mut capture) = lua.app_data_mut::<SongLuaOverlayUpdateCapture>() else {
        return;
    };
    let Some(start) = capture.wrapper_start else {
        return;
    };
    for (offset, (_, actor, initial)) in
        capture.wrappers[capture.wrapper_seen..].iter().enumerate()
    {
        debug_assert_eq!(actors.len(), start + capture.wrapper_seen + offset);
        actors.push(actor.clone());
        for buffer in states.iter_mut() {
            buffer.push(*initial);
        }
    }
    capture.wrapper_seen = capture.wrappers.len();
}

pub(crate) fn finish_wrapper_capture(
    lua: &Lua,
    overlay_count: usize,
) -> (usize, Vec<(usize, Table, SongLuaOverlayState)>) {
    let Some(mut capture) = lua.app_data_mut::<SongLuaOverlayUpdateCapture>() else {
        return (usize::MAX, Vec::new());
    };
    let start = capture.wrapper_start.take().unwrap_or(usize::MAX);
    let end = start.saturating_add(capture.wrappers.len());
    let remap = |index: usize| {
        if index < overlay_count {
            Some(index)
        } else if (start..end).contains(&index) {
            Some(overlay_count + index - start)
        } else {
            None
        }
    };
    capture.message_advances = std::mem::take(&mut capture.message_advances)
        .into_iter()
        .filter_map(|((index, message), advance)| Some(((remap(index)?, message), advance)))
        .collect();
    for targets in capture.stateful_messages.values_mut() {
        *targets = std::mem::take(targets)
            .into_iter()
            .filter_map(|(index, fields)| Some((remap(index)?, fields)))
            .collect();
    }
    for writes in capture.stateful_writes.values_mut() {
        writes.retain_mut(|write| {
            remap(write.overlay_index).is_some_and(|index| {
                write.overlay_index = index;
                true
            })
        });
    }
    #[cfg(feature = "test-support")]
    {
        capture.writes = std::mem::take(&mut capture.writes)
            .into_iter()
            .filter_map(|((index, target), samples)| Some(((remap(index)?, target), samples)))
            .collect();
    }
    (start, std::mem::take(&mut capture.wrappers))
}

pub fn drain_overlay_update_capture(
    lua: &Lua,
    mut visit: impl FnMut(
        usize,
        &[(SongLuaOverlayUpdateTarget, SongLuaOverlayUpdateValue)],
        &[SongLuaScheduledOverlayUpdate],
        &[(SongLuaOverlayUpdateTarget, SongLuaOverlayUpdateValue)],
        bool,
    ) -> Result<(), String>,
) -> Result<(), String> {
    let Some(mut capture) = lua.app_data_mut::<SongLuaOverlayUpdateCapture>() else {
        return Ok(());
    };
    for position in 0..capture.touched.len() {
        let index = capture.touched[position];
        visit(
            index,
            &capture.values[index],
            &capture.scheduled[index],
            &capture.final_values[index],
            capture.tween_resets[index],
        )?;
    }
    for position in 0..capture.touched.len() {
        let index = capture.touched[position];
        capture.values[index].clear();
        capture.final_values[index].clear();
        capture.scheduled[index].clear();
        capture.tween_resets[index] = false;
        capture.touched_flags[index] = false;
    }
    capture.touched.clear();
    Ok(())
}

pub(crate) fn apply_message_advances<Kind>(
    lua: &Lua,
    overlays: &mut [SongLuaOverlayCompileActor<Kind>],
) {
    let Some(capture) = lua.app_data_ref::<SongLuaOverlayUpdateCapture>() else {
        return;
    };
    for ((index, name), advance) in &capture.message_advances {
        if let Some(overlay) = overlays.get_mut(*index) {
            if let Some(command) = overlay
                .actor
                .message_commands
                .iter_mut()
                .find(|command| command.message == *name)
            {
                command.frame_advance = *advance;
            }
        }
    }
}

#[cfg(feature = "test-support")]
pub(crate) struct SongLuaOverlayWrites(pub Vec<crate::SongLuaOverlayUpdateTrack>);

pub fn end_overlay_update_capture(lua: &Lua) {
    let capture = lua.remove_app_data::<SongLuaOverlayUpdateCapture>();
    #[cfg(feature = "test-support")]
    if let Some(capture) = capture {
        lua.set_app_data(SongLuaOverlayWrites(
            capture
                .writes
                .into_iter()
                .map(
                    |((overlay_index, target), samples)| crate::SongLuaOverlayUpdateTrack {
                        overlay_index,
                        target,
                        samples,
                    },
                )
                .collect(),
        ));
    }
    #[cfg(not(feature = "test-support"))]
    drop(capture);
}

pub(crate) fn set_pending_tweens(
    lua: &Lua,
    values: impl IntoIterator<Item = (usize, SongLuaOverlayUpdateTarget, SongLuaOverlayUpdateValue)>,
) {
    let Some(mut capture) = lua.app_data_mut::<SongLuaOverlayUpdateCapture>() else {
        return;
    };
    for pending in &mut capture.pending_tweens {
        pending.clear();
    }
    for (index, target, value) in values {
        let pending = &mut capture.pending_tweens[index];
        if let Some((_, dest)) = pending.iter_mut().find(|(key, _)| *key == target) {
            *dest = value;
        } else {
            pending.push((target, value));
        }
    }
}

const POSITION_CURRENT_KEYS: [&str; 3] = [
    "__songlua_current_x",
    "__songlua_current_y",
    "__songlua_current_z",
];
const POSITION_DEST_KEYS: [&str; 3] = [
    "__songlua_state_x",
    "__songlua_state_y",
    "__songlua_state_z",
];

pub(crate) fn set_prior_positions(lua: &Lua, states: &[SongLuaOverlayState]) {
    if let Some(mut capture) = lua.app_data_mut::<SongLuaOverlayUpdateCapture>() {
        for (position, state) in capture.prior_positions.iter_mut().zip(states) {
            *position = Some([state.x, state.y, state.z]);
        }
    }
}

fn actor_current_position(lua: &Lua, actor: &Table, axis: usize) -> mlua::Result<f32> {
    if lua
        .app_data_ref::<SongLuaCompileUpdatePhase>()
        .is_some_and(|phase| {
            phase.active && queue_actor_order(lua, actor.to_pointer() as usize) > phase.order
        })
    {
        if let Some(capture) = lua.app_data_ref::<SongLuaOverlayUpdateCapture>() {
            if let Some(position) = capture
                .actor_indices
                .get(&(actor.to_pointer() as usize))
                .and_then(|index| capture.prior_positions[*index])
            {
                return Ok(position[axis]);
            }
        }
    }
    if let Some(value) = live_current_value(lua, actor, |current| [current.x, current.y, current.z][axis]) {
        return Ok(value);
    }
    Ok(actor
        .get::<Option<f32>>(POSITION_CURRENT_KEYS[axis])?
        .or(actor.get::<Option<f32>>(POSITION_DEST_KEYS[axis])?)
        .unwrap_or(0.0))
}

fn advance_capture_position(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    let Some(capture) = lua.app_data_ref::<SongLuaOverlayUpdateCapture>() else {
        return Ok(());
    };
    let Some(&index) = capture.actor_indices.get(&(actor.to_pointer() as usize)) else {
        return Ok(());
    };
    let mut destinations = [None; 3];
    for update in &capture.scheduled[index] {
        let axis = match update.target {
            SongLuaOverlayUpdateTarget::X => 0,
            SongLuaOverlayUpdateTarget::Y => 1,
            SongLuaOverlayUpdateTarget::Z => 2,
            _ => continue,
        };
        let SongLuaOverlayUpdateValue::F32(to) = update.value else {
            continue;
        };
        let from = destinations[axis]
            .or_else(|| match update.initial_value {
                Some(SongLuaOverlayUpdateValue::F32(value)) => Some(value),
                _ => None,
            })
            .unwrap_or(
                actor
                    .get::<Option<f32>>(POSITION_CURRENT_KEYS[axis])?
                    .unwrap_or(0.0),
            );
        destinations[axis] = Some(to);
        if update.frame_advance + 1.0e-7 < update.delay_seconds {
            continue;
        }
        let factor = if update.duration_seconds <= f32::EPSILON {
            1.0
        } else {
            ((update.frame_advance - update.delay_seconds) / update.duration_seconds)
                .clamp(0.0, 1.0)
        };
        let factor =
            crate::overlay_command_ease_factor(update.easing.as_deref(), factor, update.opt1);
        actor.set(
            POSITION_CURRENT_KEYS[axis],
            (to - from).mul_add(factor, from),
        )?;
    }
    Ok(())
}

fn finish_pending_tweens(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    let values = {
        let Some(mut capture) = lua.app_data_mut::<SongLuaOverlayUpdateCapture>() else {
            return Ok(());
        };
        let Some(index) = capture.touch(actor) else {
            return Ok(());
        };
        capture.tween_resets[index] = true;
        let mut values = capture.pending_tweens[index].clone();
        for update in &capture.scheduled[index] {
            if let Some((_, dest)) = values.iter_mut().find(|(key, _)| *key == update.target) {
                *dest = update.value.clone();
            } else {
                values.push((update.target, update.value.clone()));
            }
        }
        capture.pending_tweens[index].clear();
        capture.scheduled[index].clear();
        values
    };
    let beat = compile_song_runtime_values(lua)?.0;
    for (target, value) in values {
        set_actor_overlay_update_getter_value(lua, actor, target, &value)
            .map_err(mlua::Error::runtime)?;
        if let Some(mut capture) = lua.app_data_mut::<SongLuaOverlayUpdateCapture>() {
            capture.record(actor, beat, target, value);
        }
    }
    Ok(())
}

fn stop_pending_tweens(lua: &Lua, actor: &Table, position: [f32; 3]) {
    let Some(mut capture) = lua.app_data_mut::<SongLuaOverlayUpdateCapture>() else {
        return;
    };
    let Some(index) = capture.touch(actor) else {
        return;
    };
    capture.tween_resets[index] = true;
    let targets = capture.scheduled[index]
        .iter()
        .fold(0_u128, |mask, update| {
            mask | (1_u128 << update.target as usize)
        });
    capture.final_values[index].retain(|(target, _)| targets & (1_u128 << *target as usize) == 0);
    capture.scheduled[index].clear();
    capture.pending_tweens[index].clear();
    for (target, value) in [
        SongLuaOverlayUpdateTarget::X,
        SongLuaOverlayUpdateTarget::Y,
        SongLuaOverlayUpdateTarget::Z,
    ]
    .into_iter()
    .zip(position)
    {
        let value = SongLuaOverlayUpdateValue::F32(value);
        SongLuaOverlayUpdateCapture::replace_value(
            &mut capture.values[index],
            target,
            value.clone(),
        );
        SongLuaOverlayUpdateCapture::replace_value(&mut capture.final_values[index], target, value);
    }
}

pub(crate) fn captured_update_target_mask(lua: &Lua, actor: &Table) -> u128 {
    let Some(capture) = lua.app_data_ref::<SongLuaOverlayUpdateCapture>() else {
        return 0;
    };
    let Some(&index) = capture.actor_indices.get(&(actor.to_pointer() as usize)) else {
        return 0;
    };
    capture.values[index]
        .iter()
        .map(|(target, _)| *target)
        .chain(capture.scheduled[index].iter().map(|update| update.target))
        .fold(0, |mask, target| mask | (1_u128 << target as usize))
}

pub fn stateful_message_captures(lua: &Lua) -> Vec<SongLuaStatefulMessageCapture> {
    let Some(capture) = lua.app_data_ref::<SongLuaOverlayUpdateCapture>() else {
        return Vec::new();
    };
    capture
        .stateful_messages
        .iter()
        .map(|(message, targets)| SongLuaStatefulMessageCapture {
            message: message.clone(),
            overlay_targets: targets
                .iter()
                .map(|(&index, properties)| (index, properties.iter().copied().collect()))
                .collect(),
            writes: capture
                .stateful_writes
                .get(message)
                .cloned()
                .unwrap_or_default(),
        })
        .collect()
}

pub fn runtime_broadcast_captures(lua: &Lua) -> Vec<(f32, String, bool)> {
    lua.app_data_ref::<SongLuaOverlayUpdateCapture>()
        .map(|capture| capture.runtime_broadcasts.clone())
        .unwrap_or_default()
}

pub(crate) fn take_runtime_broadcasts(lua: &Lua) -> Vec<(f32, String, bool)> {
    if let Some(mut events) = lua.app_data_mut::<LiveBroadcasts>() {
        return std::mem::take(&mut events.0);
    }
    lua.app_data_mut::<SongLuaOverlayUpdateCapture>()
        .map(|mut capture| std::mem::take(&mut capture.runtime_broadcasts))
        .unwrap_or_default()
}

pub(crate) fn capture_runtime_sound(lua: &Lua, path: PathBuf) {
    let beat = compile_song_runtime_values(lua).map_or(0.0, |(beat, _)| beat);
    let queued_startup = lua.app_data_ref::<SongLuaQueuedStartup>().is_some();
    if let Some(mut capture) = lua.app_data_mut::<SongLuaOverlayUpdateCapture>() {
        let source_message = capture.active_broadcast.clone();
        capture.runtime_sounds.push(SongLuaSoundEvent {
            beat,
            path,
            source_message,
            queued_startup,
        });
    }
}

pub(crate) fn take_runtime_sounds(lua: &Lua) -> Vec<SongLuaSoundEvent> {
    lua.app_data_mut::<SongLuaOverlayUpdateCapture>()
        .map(|mut capture| std::mem::take(&mut capture.runtime_sounds))
        .unwrap_or_default()
}

pub(crate) fn take_judgment_textures(lua: &Lua) -> Vec<(usize, crate::SongLuaJudgmentTexture)> {
    lua.app_data_mut::<SongLuaOverlayUpdateCapture>()
        .map(|mut capture| std::mem::take(&mut capture.judgment_textures))
        .unwrap_or_default()
}

pub(crate) fn take_sprite_textures(lua: &Lua) -> Vec<(usize, crate::SongLuaSpriteTexture)> {
    lua.app_data_mut::<SongLuaOverlayUpdateCapture>()
        .map(|mut capture| std::mem::take(&mut capture.sprite_textures))
        .unwrap_or_default()
}

fn capture_sprite_texture(lua: &Lua, actor: &Table, previous: Option<PathBuf>) -> mlua::Result<()> {
    if !actor_type_is(actor, "Sprite")? {
        return Ok(());
    }
    note_song_lua_side_effect(lua)?;
    let Some(path) = actor_texture_path(actor)? else {
        return Ok(());
    };
    if previous.as_ref() != Some(&path) {
        // Sprite::SetTexture unloads a different texture, which resets its
        // current state and seconds into that state before loading new frames.
        capture_immediate_u32(lua, actor, "sprite_state_index", 0)?;
        let seconds = lua
            .globals()
            .raw_get::<Option<Table>>(crate::SONG_LUA_RUNTIME_KEY)?
            .map(|runtime| runtime.get::<f32>(crate::SONG_LUA_RUNTIME_SECONDS_KEY))
            .transpose()?;
        actor.set("__songlua_state_sprite_texture_epoch", seconds)?;
    }
    if lua.app_data_ref::<SongLuaOverlayUpdateCapture>().is_none() {
        return Ok(());
    }
    if lua.app_data_ref::<SongLuaQueuedStartup>().is_some() {
        // Startup discovery visits future queued bodies. Their actual loads
        // are recorded by chronological replay, on the dispatching frame.
        return capture_judgment_texture(lua, actor);
    }
    let texture = crate::SongLuaSpriteTexture {
        second: compile_song_runtime_values(lua)?.1,
        key: Arc::from(path.to_string_lossy().as_ref()),
        path,
        states: read_sprite_states(actor)
            .map_err(mlua::Error::runtime)?
            .into(),
        animation_epoch: actor.get::<Option<f32>>("__songlua_state_sprite_texture_epoch")?,
        frame_sheet: actor
            .raw_get::<Option<Table>>("__songlua_state_sprite_frame_sheet")?
            .map(|sheet| Ok::<_, mlua::Error>((sheet.raw_get(1)?, sheet.raw_get(2)?)))
            .transpose()?,
    };
    if let Some(mut capture) = lua.app_data_mut::<SongLuaOverlayUpdateCapture>() {
        capture
            .sprite_textures
            .push((actor.to_pointer() as usize, texture));
    }
    capture_judgment_texture(lua, actor)
}

fn capture_judgment_texture(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    let Some(player) = actor.get::<Option<usize>>("__songlua_judgment_sprite_player")? else {
        return Ok(());
    };
    if lua.app_data_ref::<SongLuaOverlayUpdateCapture>().is_none() {
        return Ok(());
    }
    let Some(path) = actor_texture_path(actor)? else {
        return Ok(());
    };
    let Some((width, height)) = actor_image_frame_size(actor)? else {
        return Ok(());
    };
    let (cols, rows) = actor_sprite_sheet_dims(actor)?.unwrap_or((1, 1));
    let second = compile_song_runtime_values(lua)?.1;
    let texture = crate::SongLuaJudgmentTexture {
        second,
        key: Arc::from(path.to_string_lossy().as_ref()),
        path,
        frame_size: [width, height],
        frame_cols: cols as usize,
        frame_rows: rows as usize,
    };
    if let Some(mut capture) = lua.app_data_mut::<SongLuaOverlayUpdateCapture>() {
        capture.judgment_textures.push((player, texture));
    }
    Ok(())
}

fn capture_target_for_key(key: &str) -> Option<SongLuaOverlayUpdateTarget> {
    use SongLuaOverlayUpdateTarget as Target;
    Some(match key {
        "aux" => Target::Aux,
        "x" => Target::X,
        "y" => Target::Y,
        "z" => Target::Z,
        "z_bias" => Target::ZBias,
        "draw_order" => Target::DrawOrder,
        "draw_by_z_position" => Target::DrawByZPosition,
        "aft_preserve" => Target::AftPreserve,
        "aft_created" => Target::AftCreated,
        "halign" => Target::HAlign,
        "valign" => Target::VAlign,
        "text_align" => Target::TextAlign,
        "uppercase" => Target::Uppercase,
        "shadow_len" => Target::ShadowLen,
        "shadow_color" => Target::ShadowColor,
        "glow" => Target::Glow,
        "fov" => Target::Fov,
        "vanishpoint" => Target::Vanishpoint,
        "diffuse" => Target::Diffuse,
        "vertex_colors" => Target::VertexColors,
        "visible" => Target::Visible,
        "hibernating" => Target::Hibernating,
        "cropleft" => Target::CropLeft,
        "cropright" => Target::CropRight,
        "croptop" => Target::CropTop,
        "cropbottom" => Target::CropBottom,
        "fadeleft" => Target::FadeLeft,
        "faderight" => Target::FadeRight,
        "fadetop" => Target::FadeTop,
        "fadebottom" => Target::FadeBottom,
        "mask_source" => Target::MaskSource,
        "mask_dest" => Target::MaskDest,
        "depth_test" => Target::DepthTest,
        "depth_write" => Target::DepthWrite,
        "depth_test_on_fail" => Target::DepthTestOnFail,
        "cull_mode" => Target::CullMode,
        "zoom" => Target::Zoom,
        "zoom_x" => Target::ZoomX,
        "zoom_y" => Target::ZoomY,
        "zoom_z" => Target::ZoomZ,
        "basezoom" => Target::BaseZoom,
        "basezoom_x" => Target::BaseZoomX,
        "basezoom_y" => Target::BaseZoomY,
        "basezoom_z" => Target::BaseZoomZ,
        "rot_x_deg" => Target::RotationX,
        "rot_y_deg" => Target::RotationY,
        "rot_z_deg" => Target::RotationZ,
        "base_rotation" => Target::BaseRotation,
        "skew_x" => Target::SkewX,
        "skew_y" => Target::SkewY,
        "blend" => Target::Blend,
        "vibrate" => Target::Vibrate,
        "effect_magnitude" => Target::EffectMagnitude,
        "effect_clock" => Target::EffectClock,
        "effect_timer" => Target::EffectTimer,
        "effect_mode" => Target::EffectMode,
        "effect_color1" => Target::EffectColor1,
        "effect_color2" => Target::EffectColor2,
        "effect_period" => Target::EffectPeriod,
        "effect_offset" => Target::EffectOffset,
        "effect_timing" => Target::EffectTiming,
        "rainbow" => Target::Rainbow,
        "rainbow_scroll" => Target::RainbowScroll,
        "text_jitter" => Target::TextJitter,
        "text_distortion" => Target::TextDistortion,
        "text_glow_mode" => Target::TextGlowMode,
        "mult_attrs_with_diffuse" => Target::MultAttrsWithDiffuse,
        "sprite_texture" => Target::SpriteTexture,
        "sprite_animate" => Target::SpriteAnimate,
        "sprite_loop" => Target::SpriteLoop,
        "sprite_playback_rate" => Target::SpritePlaybackRate,
        "sprite_state_delay" => Target::SpriteStateDelay,
        "sprite_state_index" => Target::SpriteStateIndex,
        "vert_spacing" => Target::VertSpacing,
        "wrap_width_pixels" => Target::WrapWidthPixels,
        "max_width" => Target::MaxWidth,
        "max_height" => Target::MaxHeight,
        "max_w_pre_zoom" => Target::MaxWPreZoom,
        "max_h_pre_zoom" => Target::MaxHPreZoom,
        "max_dimension_uses_zoom" => Target::MaxDimensionUsesZoom,
        "texture_filtering" => Target::TextureFiltering,
        "texture_wrapping" => Target::TextureWrapping,
        "texcoord_offset" => Target::TexcoordOffset,
        "custom_texture_rect" => Target::CustomTextureRect,
        "texcoord_velocity" => Target::TexcoordVelocity,
        "size" => Target::Size,
        "stretch_rect" => Target::StretchRect,
        _ => return None,
    })
}

fn record_overlay_update_capture(
    lua: &Lua,
    actor: &Table,
    key: &str,
    value: SongLuaOverlayUpdateValue,
) -> bool {
    let Some(target) = capture_target_for_key(key) else {
        return live_actor_playback(lua);
    };
    if let Some(recorded) = record_tween_write(lua, actor, target, &value) {
        return recorded;
    }
    // Live setters have already updated native pose state, or will write an
    // immediate property below. Neither needs an offline command-history block.
    if live_actor_playback(lua) { return true; }
    let direct_message_actor = {
        let Some(capture) = lua.app_data_ref::<SongLuaOverlayUpdateCapture>() else {
            return false;
        };
        if !capture
            .actor_indices
            .contains_key(&(actor.to_pointer() as usize))
        {
            return false;
        }
        !capture.active_broadcast_params
            && capture
                .active_broadcast_command
                .as_ref()
                .is_some_and(|command| {
                    if command
                        .to_str()
                        .is_ok_and(|name| has_deferred_message(lua, actor, Some(&name)))
                    {
                        return false;
                    }
                    actor
                        .get::<Option<Function>>(command)
                        .ok()
                        .flatten()
                        .is_some()
                })
    };
    // The receiving actor's own command is compiled as a timed message block.
    // Parameter-dependent commands need their actual writes: those parameters
    // were unavailable when the static blocks were captured.
    // Mutations to other actors must remain in the sequential capture because
    // stateful commands can select a different target on every broadcast.
    // X/Y/Z getters expose the current tween state. Even a receiver's own
    // runtime command must feed their chronological replay; destination
    // getters and timed effect blocks retain the existing message path.
    if direct_message_actor {
        record_message_advance(lua, actor);
    }
    if direct_message_actor
        && lua.app_data_ref::<SongLuaCompileFrames>().is_none()
        && !matches!(
            target,
            SongLuaOverlayUpdateTarget::X
                | SongLuaOverlayUpdateTarget::Y
                | SongLuaOverlayUpdateTarget::Z
        )
    {
        return lua
            .app_data_mut::<SongLuaOverlayUpdateCapture>()
            .is_some_and(|mut capture| capture.touch(actor).is_some());
    }
    let cursor = actor
        .get::<Option<f32>>("__songlua_capture_cursor")
        .ok()
        .flatten()
        .unwrap_or(0.0)
        .max(0.0);
    let duration = actor
        .get::<Option<f32>>("__songlua_capture_duration")
        .ok()
        .flatten()
        .unwrap_or(0.0)
        .max(0.0);
    let easing = actor
        .get::<Option<String>>("__songlua_capture_easing")
        .ok()
        .flatten();
    let opt1 = actor
        .get::<Option<f32>>("__songlua_capture_opt1")
        .ok()
        .flatten();
    let dispatch_seconds = queued_dispatch_seconds(lua);
    let frame_advance = lua
        .app_data_ref::<SongLuaQueuedCommand>()
        .map(|scope| queued_render_advance(lua, *scope, actor, false))
        .unwrap_or_else(|| actor_frame_advance(lua, actor));
    // Sleep appends an empty zero-time tail. A setter reached through a
    // parent's command changes that tail, not the last earlier tween that
    // happened to write this property.
    let timed = cursor > f32::EPSILON || duration > f32::EPSILON;
    let progress = if timed {
        actor_tween_progress(lua, actor)
    } else {
        queued_zero_progress(lua, actor)
    };
    let beat = compile_song_runtime_values(lua).map_or(0.0, |(beat, _)| beat);
    lua.app_data_mut::<SongLuaOverlayUpdateCapture>()
        .is_some_and(|mut capture| {
            if timed || progress.is_some() {
                let recorded = capture
                    .record_scheduled(actor, beat, cursor, duration, easing, opt1, target, value);
                if recorded {
                    let index = capture.actor_indices[&(actor.to_pointer() as usize)];
                    capture.set_scheduled_time(index, dispatch_seconds, frame_advance, progress);
                }
                recorded
            } else {
                capture.record(actor, beat, target, value)
            }
        })
}

fn record_overlay_update_capture_immediate(
    lua: &Lua,
    actor: &Table,
    key: &str,
    value: SongLuaOverlayUpdateValue,
) -> bool {
    if live_actor_playback(lua) { return true; }
    let Some(target) = capture_target_for_key(key) else {
        return false;
    };
    let direct_message_actor = {
        let Some(capture) = lua.app_data_ref::<SongLuaOverlayUpdateCapture>() else {
            return false;
        };
        if !capture
            .actor_indices
            .contains_key(&(actor.to_pointer() as usize))
        {
            return false;
        }
        !capture.active_broadcast_params
            && capture
                .active_broadcast_command
                .as_ref()
                .is_some_and(|command| {
                    if command
                        .to_str()
                        .is_ok_and(|name| has_deferred_message(lua, actor, Some(&name)))
                    {
                        return false;
                    }
                    actor
                        .get::<Option<Function>>(command)
                        .ok()
                        .flatten()
                        .is_some()
                })
    };
    // X/Y/Z getters expose the current tween state. Even a receiver's own
    // runtime command must feed their chronological replay; destination
    // getters and timed effect blocks retain the existing message path.
    if direct_message_actor {
        record_message_advance(lua, actor);
    }
    if direct_message_actor
        && lua.app_data_ref::<SongLuaCompileFrames>().is_none()
        && !matches!(
            target,
            SongLuaOverlayUpdateTarget::X
                | SongLuaOverlayUpdateTarget::Y
                | SongLuaOverlayUpdateTarget::Z
        )
    {
        return true;
    }
    let beat = compile_song_runtime_values(lua).map_or(0.0, |(beat, _)| beat);
    let queued = lua
        .app_data_ref::<SongLuaQueuedCommand>()
        .map(|scope| *scope);
    let advance = queued
        .map(|scope| queued_render_advance(lua, scope, actor, true))
        .unwrap_or(0.0);
    let dispatch_seconds = queued_dispatch_seconds(lua);
    lua.app_data_mut::<SongLuaOverlayUpdateCapture>()
        .is_some_and(|mut capture| {
            if let Some(scope) = queued {
                // These setters bypass the tween queue. Preserve their final
                // same-frame write even though dispatch uses a scheduled sample.
                #[cfg(feature = "test-support")]
                {
                    let index = capture.actor_indices[&(actor.to_pointer() as usize)];
                    capture.record_write(index, beat, target, &value, false);
                }
                let recorded = capture.record_scheduled(
                    actor,
                    beat,
                    scope.start,
                    0.0,
                    None,
                    None,
                    target,
                    value,
                );
                if recorded {
                    let index = capture.actor_indices[&(actor.to_pointer() as usize)];
                    capture.set_scheduled_time(index, dispatch_seconds, advance, None);
                }
                recorded
            } else {
                capture.record(actor, beat, target, value)
            }
        })
}

pub fn overlay_compile_actor_tables_for_indices<Kind>(
    overlays: &[SongLuaOverlayCompileActor<Kind>],
    indices: &[usize],
) -> Vec<(usize, Table)> {
    indices
        .iter()
        .filter_map(|&index| {
            overlays
                .get(index)
                .map(|overlay| (index, overlay.table.clone()))
        })
        .collect()
}

pub fn reset_overlay_compile_actor_capture_tables<Kind>(
    lua: &Lua,
    overlays: &[SongLuaOverlayCompileActor<Kind>],
) -> Result<(), String> {
    // This runs for every per-frame sample; borrow actors directly.
    for overlay in overlays {
        reset_actor_capture(lua, &overlay.table).map_err(|err| err.to_string())?;
    }
    Ok(())
}

pub fn read_song_lua_sound_paths(lua: &Lua) -> Result<Vec<PathBuf>, String> {
    let globals = lua.globals();
    let Some(paths) = globals
        .get::<Option<Table>>(SONG_LUA_SOUND_PATHS_KEY)
        .map_err(|err| err.to_string())?
    else {
        return Ok(Vec::new());
    };
    let mut out = Vec::with_capacity(paths.raw_len());
    for path in paths.sequence_values::<String>() {
        out.push(PathBuf::from(path.map_err(|err| err.to_string())?));
    }
    Ok(out)
}

pub fn load_script_file(lua: &Lua, path: &Path, song_dir: &Path) -> mlua::Result<Function> {
    load_script_file_with_env(lua, path, song_dir, None)
}

pub(crate) fn load_script_file_with_env(
    lua: &Lua,
    path: &Path,
    song_dir: &Path,
    environment: Option<Table>,
) -> mlua::Result<Function> {
    load_script_file_with_source(lua, path, path, song_dir, environment)
}

fn load_script_file_with_source(
    lua: &Lua,
    path: &Path,
    source_path: &Path,
    song_dir: &Path,
    environment: Option<Table>,
) -> mlua::Result<Function> {
    let source = fs::read_to_string(path).map_err(mlua::Error::external)?;
    let source = crate::cmd::preprocess_lua_cmd_syntax_cow(&source).map_err(mlua::Error::external)?;
    let environment = match environment {
        Some(environment) => environment,
        None => initial_chunk_environment(lua, source_path)?,
    };
    let chunk_env = create_chunk_env_proxy(lua, environment)?;
    let chunk = lua
        .load(source.as_ref())
        .set_name(source_path.to_string_lossy().as_ref())
        .set_environment(chunk_env.clone());
    let inner = chunk.into_function()?;
    let script_dir = source_path.parent().unwrap_or(song_dir).to_path_buf();
    let script_path = file_path_string(source_path);
    let chunk_env_for_call = chunk_env.clone();
    let wrapper = lua.create_function(move |lua, args: MultiValue| {
        call_with_script_dir(lua, &script_dir, || {
            call_with_script_path(lua, &script_path, || {
                call_with_chunk_env(lua, &chunk_env_for_call, || {
                    // Lua loadfile chunks can return several values, including
                    // trailing nils and a secondary initialization function.
                    inner.call::<MultiValue>(args.clone())
                })
            })
        })
    })?;
    register_loader_env(lua, &wrapper, &chunk_env)?;
    Ok(wrapper)
}

pub fn execute_script_file(lua: &Lua, path: &Path, song_dir: &Path) -> mlua::Result<Value> {
    let loader = load_script_file(lua, path, song_dir)?;
    loader.call::<Value>(())
}

pub(crate) fn execute_script_file_with_source(
    lua: &Lua,
    path: &Path,
    source_path: &Path,
    song_dir: &Path,
) -> mlua::Result<Value> {
    load_script_file_with_source(lua, path, source_path, song_dir, None)?.call::<Value>(())
}

pub fn create_loader_function(lua: &Lua, song_dir: &Path, path: &str) -> mlua::Result<Function> {
    create_loader_function_with_env(lua, song_dir, path, None)
}

fn create_loader_function_with_env(
    lua: &Lua,
    song_dir: &Path,
    path: &str,
    environment: Option<Table>,
) -> mlua::Result<Function> {
    let path = path.trim();
    let basename = Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(path);
    let resolved = resolve_script_path(lua, song_dir, path)?;
    if basename.eq_ignore_ascii_case("easing.lua") && !resolved.is_file() {
        let ease: Table = lua.globals().get("ease")?;
        return lua.create_function(move |_, _args: IgnoredArgs| Ok(Value::Table(ease.clone())));
    }
    load_script_file_with_env(lua, &resolved, song_dir, environment)
}

fn caller_chunk_environment(lua: &Lua) -> Option<Table> {
    lua.inspect_stack(1, |debug| debug.function().environment())
        .flatten()
}

pub fn install_file_loader_globals(
    lua: &Lua,
    song_dir: PathBuf,
    create_dummy_actor: fn(&Lua, &'static str) -> mlua::Result<Table>,
) -> mlua::Result<()> {
    let globals = lua.globals();
    let song_dir_for_loadfile = song_dir.clone();
    globals.set(
        "loadfile",
        lua.create_function(move |lua, path: String| {
            let mut out = MultiValue::new();
            match create_loader_function_with_env(
                lua,
                &song_dir_for_loadfile,
                &path,
                caller_chunk_environment(lua),
            ) {
                Ok(loader) => {
                    out.push_back(Value::Function(loader));
                    out.push_back(Value::Nil);
                }
                Err(err) => {
                    out.push_back(Value::Nil);
                    out.push_back(Value::String(lua.create_string(err.to_string())?));
                }
            }
            Ok(out)
        })?,
    )?;
    let song_dir_for_dofile = song_dir.clone();
    globals.set(
        "dofile",
        lua.create_function(move |lua, path: String| {
            let loader = create_loader_function(lua, &song_dir_for_dofile, &path)?;
            loader.call::<Value>(())
        })?,
    )?;
    globals.set(
        "LoadActor",
        lua.create_function(move |lua, mut args: MultiValue| {
            let Some(Value::String(path)) = args.pop_front() else {
                return create_dummy_actor(lua, "LoadActor").map(Value::Table);
            };
            let path = path.to_str()?;
            // ActorDef.lua::ResolveRelativePath uses the calling function's
            // source, even when a command invokes a helper from another file.
            // The actor's directory is only the fallback for a C caller.
            let caller_dir = lua.inspect_stack(1, |frame| {
                let source = frame.source();
                let source = source.source.as_deref()?.trim_start_matches('@');
                if source.starts_with('=') || source == "[C]" { return None; }
                Path::new(source).parent().filter(|dir| !dir.as_os_str().is_empty())
                    .map(Path::to_path_buf)
            }).flatten();
            match caller_dir {
                Some(dir) => call_with_script_dir(lua, &dir, || {
                    load_actor_path(lua, &song_dir, &path, args, create_dummy_actor)
                }),
                None => load_actor_path(lua, &song_dir, &path, args, create_dummy_actor),
            }
        })?,
    )?;
    Ok(())
}

pub fn load_actor_path(
    lua: &Lua,
    song_dir: &Path,
    path: &str,
    args: MultiValue,
    create_dummy_actor: fn(&Lua, &'static str) -> mlua::Result<Table>,
) -> mlua::Result<Value> {
    if deadlib_assets::generated_texture_shared_key(path).is_some() {
        return create_media_actor(lua, "Sprite", Path::new(path), create_dummy_actor).map(Value::Table);
    }
    if let Some(actor) = create_theme_path_actor(lua, path, create_dummy_actor)? {
        return Ok(Value::Table(actor));
    }
    let resolved = resolve_load_actor_path(lua, song_dir, path)?;
    if is_song_lua_media_path(&resolved) {
        let actor_type = if is_song_lua_audio_path(&resolved) {
            "Sound"
        } else {
            "Sprite"
        };
        return Ok(Value::Table(create_media_actor(
            lua,
            actor_type,
            resolved.as_path(),
            create_dummy_actor,
        )?));
    }
    load_script_file(lua, &resolved, song_dir)?.call::<Value>(args)
}

pub fn create_theme_path_actor(
    lua: &Lua,
    path: &str,
    create_dummy_actor: fn(&Lua, &'static str) -> mlua::Result<Table>,
) -> mlua::Result<Option<Table>> {
    let theme_path = path.trim_start_matches('/');
    if !theme_path.starts_with(SONG_LUA_THEME_PATH_PREFIX) {
        return Ok(None);
    }
    let path_ref = Path::new(theme_path);
    let actor_type = if is_song_lua_audio_path(path_ref) {
        "Sound"
    } else if is_song_lua_image_path(path_ref) || is_song_lua_video_path(path_ref) {
        "Sprite"
    } else {
        "Actor"
    };
    let actor = create_dummy_actor(lua, actor_type)?;
    if actor_type.eq_ignore_ascii_case("Sound") {
        actor.set("File", path)?;
    } else if actor_type.eq_ignore_ascii_case("Sprite") {
        actor.set("Texture", path)?;
    }
    Ok(Some(actor))
}

pub fn create_media_actor(
    lua: &Lua,
    actor_type: &'static str,
    resolved_path: &Path,
    create_dummy_actor: fn(&Lua, &'static str) -> mlua::Result<Table>,
) -> mlua::Result<Table> {
    let actor = create_dummy_actor(lua, actor_type)?;
    let absolute = std::path::absolute(resolved_path).map_err(mlua::Error::external)?;
    let resolved_path = absolute.as_path();
    if actor_type.eq_ignore_ascii_case("Sound") {
        actor.set("File", file_path_string(resolved_path))?;
    } else {
        actor.set("Texture", file_path_string(resolved_path))?;
        set_actor_decode_movie_for_texture(&actor)?;
        movie_source_size(lua, &actor)?;
    }
    Ok(actor)
}

#[derive(Clone, Copy)]
struct ActorDefinitionHost {
    human_player_count: usize,
    create_dummy_actor: fn(&Lua, &'static str) -> mlua::Result<Table>,
    install_methods: fn(&Lua, &Table) -> mlua::Result<()>,
}

pub fn install_def_globals(
    lua: &Lua,
    human_player_count: usize,
    create_dummy_actor: fn(&Lua, &'static str) -> mlua::Result<Table>,
    install_actor_methods: fn(&Lua, &Table) -> mlua::Result<()>,
) -> mlua::Result<()> {
    let globals = lua.globals();
    if lua.app_data_ref::<crate::live::LiveMode>().is_some() {
        lua.set_app_data(ActorDefinitionHost { human_player_count, create_dummy_actor, install_methods: install_actor_methods });
    }
    let def = lua.create_table()?;
    for &(name, actor_type) in &[
        ("Actor", "Actor"),
        ("ActorFrame", "ActorFrame"),
        ("Sprite", "Sprite"),
        ("Banner", "Banner"),
        ("ActorMultiVertex", "ActorMultiVertex"),
        ("Sound", "Sound"),
        ("BitmapText", "BitmapText"),
        ("BPMDisplay", "BPMDisplay"),
        ("RollingNumbers", "RollingNumbers"),
        ("GraphDisplay", "GraphDisplay"),
        ("SongMeterDisplay", "SongMeterDisplay"),
        ("CourseContentsList", "CourseContentsList"),
        ("DeviceList", "DeviceList"),
        ("InputList", "InputList"),
        ("Model", "Model"),
        ("Quad", "Quad"),
        ("ActorProxy", "ActorProxy"),
        ("ActorFrameTexture", "ActorFrameTexture"),
    ] {
        def.set(
            name,
            make_actor_ctor(
                lua,
                actor_type,
                human_player_count,
                create_dummy_actor,
                install_actor_methods,
            )?,
        )?;
    }
    // _fallback/02 ActorDef.lua supplies this reusable empty declaration.
    // MakeActor constructs an independent instance for every occurrence.
    let null_actor = create_dummy_actor(lua, "Actor")?;
    null_actor.raw_set("Class", "Actor")?;
    null_actor.raw_set("_Source", "(null actor)")?;
    null_actor.raw_set("__songlua_def_owned", true)?;
    globals.set("NullActor", null_actor)?;
    globals.set("Def", def)?;
    crate::actor_classes::install(lua)?;
    // _fallback/Scripts/02 Sprite.lua builds authored animation states.
    lua.load(r#"
function Sprite.LinearFrames(NumFrames, Seconds)
    local Frames = {}
    for i = 0,NumFrames-1 do
        Frames[#Frames+1] = {Frame = i, Delay = (1/NumFrames)*Seconds}
    end
    return Frames
end
"#).set_name("_fallback/Scripts/02 Sprite.lua").exec()?;
    // Native Player/NoteField classes inherit ActorFrame. Their own method
    // inventories also keep SM5.2-only feature probes absent on ITGmania.
    for (name, methods) in [
        (
            "Player",
            &[
                "SetLife",
                "ChangeLife",
                "SetActorWithJudgmentPosition",
                "SetActorWithComboPosition",
                "GetPlayerTimingData",
                "get_oitg_zoom_mode",
                "set_oitg_zoom_mode",
            ][..],
        ),
        (
            "NoteField",
            &[
                "set_step_callback",
                "set_set_pressed_callback",
                "set_did_tap_note_callback",
                "set_did_hold_note_callback",
                "step",
                "set_pressed",
                "did_tap_note",
                "did_hold_note",
                "get_column_actors",
                "GetBeatBars",
                "SetBeatBars",
                "SetBeatBarsAlpha",
            ][..],
        ),
    ] {
        let class = lua.create_table()?;
        for method in methods {
            crate::actor_classes::forward(lua, &class, method)?;
        }
        let parent = lua.create_table()?;
        parent.set("__index", globals.get::<Table>("ActorFrame")?)?;
        class.set_metatable(Some(parent))?;
        globals.set(name, class)?;
    }
    globals.set(
        "LoadFont",
        lua.create_function(move |lua, args: MultiValue| {
            let font = args
                .front()
                .cloned()
                .and_then(read_string)
                .unwrap_or_default();
            let actor = create_dummy_actor(lua, "BitmapText")?;
            actor.set("Font", font)?;
            Ok(actor)
        })?,
    )?;
    Ok(())
}

pub fn read_tracked_compile_actors(
    lua: &Lua,
    create_named_child_actor: fn(&Lua, &Table, &str) -> mlua::Result<Table>,
) -> Result<Vec<SongLuaTrackedActor>, String> {
    let globals = lua.globals();
    let top_screen = globals
        .get::<Table>("__songlua_top_screen")
        .map_err(|err| err.to_string())?;
    let children = actor_children(lua, &top_screen).map_err(|err| err.to_string())?;
    let song_foreground = if let Some(actor) = children
        .get::<Option<Table>>("SongForeground")
        .map_err(|err| err.to_string())?
    {
        actor
    } else {
        let actor = create_named_child_actor(lua, &top_screen, "SongForeground")
            .map_err(|err| err.to_string())?;
        children
            .set("SongForeground", actor.clone())
            .map_err(|err| err.to_string())?;
        actor
    };
    let players = [
        globals
            .get::<Table>("__songlua_top_screen_player_1")
            .map_err(|err| err.to_string())?,
        globals
            .get::<Table>("__songlua_top_screen_player_2")
            .map_err(|err| err.to_string())?,
    ];
    let mut out = Vec::with_capacity(7);
    for (player_index, player) in players.into_iter().enumerate() {
        out.push(tracked_song_lua_actor(
            player.clone(),
            SongLuaTrackedActorTarget::Player(player_index),
        )?);
        let children = actor_children(lua, &player).map_err(|err| err.to_string())?;
        for (name, target) in [
            (
                "Judgment",
                SongLuaTrackedActorTarget::PlayerJudgment(player_index),
            ),
            (
                "Combo",
                SongLuaTrackedActorTarget::PlayerCombo(player_index),
            ),
        ] {
            let child = if let Some(child) = children
                .get::<Option<Table>>(name)
                .map_err(|err| err.to_string())?
            {
                child
            } else {
                create_named_child_actor(lua, &player, name).map_err(|err| err.to_string())?
            };
            out.push(tracked_song_lua_actor(child, target)?);
        }
    }
    out.push(tracked_song_lua_actor(
        song_foreground,
        SongLuaTrackedActorTarget::SongForeground,
    )?);
    for (index, name) in ["Underlay", "Overlay", "SongBackground", "In"]
        .iter()
        .enumerate()
    {
        if let Some(actor) = children
            .get::<Option<Table>>(*name)
            .map_err(|err| err.to_string())?
        {
            out.push(tracked_song_lua_actor(
                actor,
                SongLuaTrackedActorTarget::ScreenLayer(index),
            )?);
        }
    }
    Ok(out)
}

fn make_actor_ctor(
    lua: &Lua,
    actor_type: &'static str,
    human_player_count: usize,
    create_dummy_actor: fn(&Lua, &'static str) -> mlua::Result<Table>,
    install_actor_methods: fn(&Lua, &Table) -> mlua::Result<()>,
) -> mlua::Result<Function> {
    if lua.app_data_ref::<ActorDefinitionHost>().is_some() {
        // Native Def.* decorates the declaration; it does not construct an
        // Actor. Runtime bookkeeping here changes Lua 5.1 sparse-table layout.
        let mt = lua.create_table()?;
        mt.set("__concat", lua.create_function(|lua, (left, right): (Table, Table)| {
            let merged = lua.create_table()?;
            left.for_each::<Value, Value>(|key, value| merged.raw_set(key, value))?;
            merge_actor_concat(lua, &merged, &right)?;
            merged.set_metatable(left.metatable())?;
            Ok(merged)
        })?)?;
        let factory: Function = lua.load(r#"
return function(class, mt, getinfo)
    return function(t)
        t.Class = class
        local level = t._Level and t._Level + 1 or 2
        repeat
            local info = getinfo(level, 'Sl')
            if not info then break end
            t._Source = info.source
            t._Line = info.currentline
            local source = info.source:gsub('^@', '')
            if info.source:sub(1,1) == '@' or source:match('^/') or source:match('^%a:[/\\]') then
                t._Dir = source:match('^(.*[/\\])')
            end
            level = level + 1
        until t._Dir
        return setmetatable(t, mt)
    end
end
"#).eval()?;
        // Native ActorDef walks real frames, including theme loading before
        // any authored song path exists. Never use the compatibility debug table
        // for this constructor: it cannot signal the end of the Lua stack.
        let getinfo = lua.create_function(|lua, (level, _what): (usize, String)| {
            lua.inspect_stack(level, |frame| {
                let source = frame.source();
                let info = lua.create_table()?;
                info.raw_set("source", source.source.as_deref().unwrap_or("=[C]"))?;
                info.raw_set("currentline", frame.current_line().map_or(-1_i64, |line| line as i64))?;
                Ok(Value::Table(info))
            }).unwrap_or(Ok(Value::Nil))
        })?;
        return factory.call((actor_type, mt, getinfo));
    }
    lua.create_function(move |lua, value: Value| {
        let table = match value {
            Value::Table(table) => table,
            _ => lua.create_table()?,
        };
        initialize_actor_instance(lua, &table, actor_type, human_player_count, create_dummy_actor, install_actor_methods)?;
        own_actor_children(lua, &table, install_actor_methods)?;
        Ok(table)
    })
}

fn initialize_actor_instance(
    lua: &Lua,
    table: &Table,
    actor_type: &'static str,
    human_player_count: usize,
    create_dummy_actor: fn(&Lua, &'static str) -> mlua::Result<Table>,
    install_actor_methods: fn(&Lua, &Table) -> mlua::Result<()>,
) -> mlua::Result<()> {
    init_actor_type(lua, table, actor_type)?;
    if let Some(script_dir) = table.raw_get::<Option<String>>("__songlua_script_dir")?.or(lua
        .globals()
        .get::<Option<String>>("__songlua_script_dir")?)
    {
        table.set("__songlua_script_dir", script_dir)?;
    }
    if let Some(song_dir) = lua.globals().get::<Option<String>>("__songlua_song_dir")? {
        table.set("__songlua_song_dir", song_dir)?;
    }
    set_actor_decode_movie_for_texture(table)?;
    movie_source_size(lua, table)?;
    if actor_type.eq_ignore_ascii_case("Sprite") || actor_type.eq_ignore_ascii_case("Banner") {
        // Actor::Actor starts with animation enabled in ITGmania.  Sprite
        // sheets rely on that default unless a command explicitly pauses
        // them.
        table.set("__songlua_state_sprite_animate", true)?;
    }
    install_actor_methods(lua, table)?;
    install_actor_metatable(lua, table, install_actor_methods)?;
    reset_actor_capture(lua, table)?;
    if actor_type.eq_ignore_ascii_case("GraphDisplay") {
        let [width, height] = graph_display_body_size(human_player_count);
        let size = lua.create_table()?;
        size.raw_set(1, width)?;
        size.raw_set(2, height)?;
        table.set("__songlua_state_size", size)?;
        install_graph_display_children(lua, table, create_dummy_actor)?;
    }
    if actor_type.eq_ignore_ascii_case("SongMeterDisplay") {
        install_song_meter_display_children(lua, table)?;
    }
    if actor_type.eq_ignore_ascii_case("CourseContentsList") {
        install_course_contents_list_children(table)?;
    }
    if let Some(text) = input_status_actor_text(actor_type) {
        table.set("Text", text)?;
    }
    Ok(())
}

fn materialize_actor_declaration(lua: &Lua, declaration: &Table) -> mlua::Result<Table> {
    let host = *lua.app_data_ref::<ActorDefinitionHost>()
        .ok_or_else(|| mlua::Error::runtime("actor declaration has no gameplay host"))?;
    let class = declaration.raw_get::<Option<String>>("Class")?
        .or(declaration.raw_get::<Option<String>>("Type")?)
        .unwrap_or_default();
    // ActorUtil::LoadFromNode returns a fresh Actor for a missing class.
    // It does not load that declaration's properties, commands or children.
    if class.is_empty() {
        return (host.create_dummy_actor)(lua, "Actor");
    }
    let kind = match class.as_str() {
        "Actor" => "Actor", "ActorFrame" => "ActorFrame", "Sprite" => "Sprite",
        "Banner" => "Banner", "ActorMultiVertex" => "ActorMultiVertex", "Sound" => "Sound",
        "BitmapText" => "BitmapText", "BPMDisplay" => "BPMDisplay", "RollingNumbers" => "RollingNumbers",
        "GraphDisplay" => "GraphDisplay", "SongMeterDisplay" => "SongMeterDisplay",
        "CourseContentsList" => "CourseContentsList", "DeviceList" => "DeviceList", "InputList" => "InputList",
        "Model" => "Model", "Quad" => "Quad", "ActorProxy" => "ActorProxy", "ActorFrameTexture" => "ActorFrameTexture",
        _ => return Err(mlua::Error::runtime(format!("unknown actor declaration class {class}"))),
    };
    // Resolve the native declaration order before adding any instance fields.
    let (_, children) = actor_def_children(lua, declaration)?;
    let actor = lua.create_table()?;
    declaration.for_each::<Value, Value>(|key, value| {
        if let Value::String(name) = &key {
            if name.to_str()? != "children" { actor.raw_set(key, value)?; }
        }
        Ok(())
    })?;
    if let Some(dir) = declaration.raw_get::<Option<String>>("_Dir")? {
        actor.raw_set("__songlua_script_dir", dir)?;
    }
    initialize_actor_instance(lua, &actor, kind, host.human_player_count,
        host.create_dummy_actor, host.install_methods)?;
    let mut index = actor.raw_len() + 1;
    for (_, child) in children {
        if matches!(child.raw_get::<Value>("Condition")?, Value::Boolean(false)) { continue; }
        let child = own_actor_root(lua, Value::Table(child), host.install_methods)?;
        if let Value::Table(child) = child {
            actor.raw_set(index, child)?;
            index += 1;
        }
    }
    actor.raw_set("__songlua_def_owned", true)?;
    Ok(actor)
}

fn actor_def_children(
    lua: &Lua,
    actor: &Table,
) -> mlua::Result<(Table, smallvec::SmallVec<[(Value, Table); 16]>)> {
    let explicit = match actor.raw_get::<Value>("children")? {
        Value::Table(children) => Some(children),
        _ => None,
    };
    let array_only = explicit.is_none();
    let source = explicit.unwrap_or_else(|| actor.clone());
    let mut children = smallvec::SmallVec::<[(Value, Table); 16]>::new();
    // XmlFileUtil::XNodeFromTableRecursive visits raw array entries until the
    // first nil, then lua_next. Its duplicate range counts tables, not slots.
    for index in 1_i64.. {
        match source.raw_get::<Value>(index)? {
            Value::Nil => break,
            Value::Table(child) => children.push((Value::Integer(index), child)),
            _ => {}
        }
    }
    let prefix = children.len() as f64;
    source.for_each::<Value, Value>(|key, value| {
        let Value::Table(child) = value else { return Ok(()) };
        let already_added = lua.coerce_number(key.clone())?.is_some_and(|index| {
            index >= 1.0 && index <= prefix && index.fract() == 0.0
        });
        if !already_added {
            children.push((key, child));
        }
        Ok(())
    })?;
    // ActorFrame::LoadChildrenFromNode accepts all names in an explicit
    // children table; direct children require IsAnInt (ASCII digits only).
    if array_only {
        let mut selected = smallvec::SmallVec::<[(Value, Table); 16]>::new();
        for (key, child) in children {
            if lua.coerce_string(key.clone())?.is_some_and(|name| {
                let bytes = name.as_bytes();
                !bytes.is_empty() && bytes.iter().all(u8::is_ascii_digit)
            }) {
                selected.push((key, child));
            }
        }
        children = selected;
    }
    Ok((source, children))
}

fn own_actor_children(
    lua: &Lua,
    actor: &Table,
    install_methods: fn(&Lua, &Table) -> mlua::Result<()>,
) -> mlua::Result<()> {
    let (source, children) = actor_def_children(lua, actor)?;
    for (key, child) in children {
        if child.raw_get::<Option<String>>("__songlua_actor_type")?.is_none() {
            if lua.app_data_ref::<ActorDefinitionHost>().is_some() {
                source.raw_set(key, materialize_actor_declaration(lua, &child)?)?;
            }
            continue;
        }
        // ActorFrame::LoadChildrenFromNode constructs an Actor per occurrence,
        // even when Lua reuses the same definition table in several slots.
        let child = if child
            .get::<Option<bool>>("__songlua_def_owned")?
            .unwrap_or(false)
        {
            clone_actor_def(lua, &child, install_methods, true)?
        } else {
            child
        };
        child.set("__songlua_def_owned", true)?;
        source.raw_set(key, child)?;
    }
    Ok(())
}

pub(crate) fn own_actor_root(
    lua: &Lua,
    root: Value,
    install_methods: fn(&Lua, &Table) -> mlua::Result<()>,
) -> mlua::Result<Value> {
    let Value::Table(actor) = root else { return Ok(root) };
    if actor.raw_get::<Option<String>>("__songlua_actor_type")?.is_none()
        && lua.app_data_ref::<ActorDefinitionHost>().is_some()
    {
        return materialize_actor_declaration(lua, &actor).map(Value::Table);
    }
    let actor = if actor.raw_get::<Option<bool>>("__songlua_def_owned")?.unwrap_or(false) {
        clone_actor_def(lua, &actor, install_methods, true)?
    } else {
        actor
    };
    actor.raw_set("__songlua_def_owned", true)?;
    Ok(Value::Table(actor))
}

fn clone_actor_def(
    lua: &Lua,
    source: &Table,
    install_methods: fn(&Lua, &Table) -> mlua::Result<()>,
    own_children: bool,
) -> mlua::Result<Table> {
    let actor = lua.create_table()?;
    source.for_each::<Value, Value>(|key, value| {
        let name = if let Value::String(ref key) = key {
            Some(key.to_str()?)
        } else {
            None
        };
        if name.as_deref().is_some_and(|name| {
            matches!(
                name,
                "__songlua_def_owned"
                    | "__songlua_parent"
                    | "__songlua_load_name"
                    | "__songlua_native_name"
                    | "__songlua_init_commands_ran"
                    | "__songlua_startup_commands_ran"
                    | "__songlua_startup_command_started"
                    | "__songlua_bound_methods"
            )
        }) {
            return Ok(());
        }
        let value = if name
            .as_deref()
            .is_some_and(|name| name == "Frames" || is_actor_mutable_state_key(name))
        {
            clone_lua_value(lua, value)?
        } else {
            value
        };
        actor.raw_set(key, value)
    })?;
    copy_live_actor_pose(lua, source, &actor)?;
    // Installed methods close over their owning table; rebind them to the new
    // instance while sharing user command functions and their Lua upvalues.
    install_methods(lua, &actor)?;
    install_actor_metatable(lua, &actor, install_methods)?;
    if own_children {
        own_actor_children(lua, &actor, install_methods)?;
    }
    Ok(actor)
}

fn install_graph_display_children(
    lua: &Lua,
    actor: &Table,
    create_dummy_actor: fn(&Lua, &'static str) -> mlua::Result<Table>,
) -> mlua::Result<()> {
    let body_group = create_actor_child_group(lua)?;
    for index in 1..=2 {
        let child = create_dummy_actor(lua, "GraphDisplayBody")?;
        child.set("__songlua_parent", actor.clone())?;
        body_group.raw_set(index, child)?;
    }
    let line = create_dummy_actor(lua, "GraphDisplayLine")?;
    line.set("Name", "Line")?;
    line.set("__songlua_parent", actor.clone())?;
    let children = actor_children(lua, actor)?;
    children.set("", body_group)?;
    children.set("Line", line)?;
    Ok(())
}

pub fn lua_text_value(value: Value) -> mlua::Result<String> {
    match value {
        Value::String(text) => Ok(text.to_str()?.to_string()),
        Value::Integer(value) => Ok(value.to_string()),
        Value::Number(value) => Ok(value.to_string()),
        Value::Boolean(value) => Ok(value.to_string()),
        _ => Ok(String::new()),
    }
}

pub fn table_string_field(table: &Table, names: &[&str]) -> mlua::Result<Option<String>> {
    for name in names {
        if let Some(value) = read_string(table.get::<Value>(*name)?) {
            return Ok(Some(value));
        }
    }
    Ok(None)
}

pub fn table_f32_field(table: &Table, names: &[&str]) -> mlua::Result<Option<f32>> {
    for name in names {
        if let Some(value) = read_f32(table.get::<Value>(*name)?) {
            return Ok(Some(value));
        }
    }
    Ok(None)
}

pub fn table_i32_field(table: &Table, names: &[&str]) -> mlua::Result<Option<i32>> {
    for name in names {
        if let Some(value) = read_i32_value(table.get::<Value>(*name)?) {
            return Ok(Some(value));
        }
    }
    Ok(None)
}

pub fn table_bool_field(table: &Table, names: &[&str]) -> mlua::Result<Option<bool>> {
    for name in names {
        if let Some(value) = read_boolish(table.get::<Value>(*name)?) {
            return Ok(Some(value));
        }
    }
    Ok(None)
}

pub fn current_song_lua_style_name(lua: &Lua) -> String {
    let owned_style = lua.globals().get::<Table>("GAMESTATE").and_then(|state| {
        state.raw_get::<Table>("__songlua_current_style")?.raw_get::<Value>(1)
    });
    let Ok(Value::Table(style)) = owned_style
        .or_else(|_| current_gamestate_value(lua, "GetCurrentStyle")) else {
        return "single".to_string();
    };
    if let Ok(Some(key)) = style.raw_get::<Option<String>>("__songlua_style_key") { return key; }
    let Ok(Some(method)) = style.get::<Option<Function>>("GetName") else {
        return "single".to_string();
    };
    method
        .call::<Value>(style)
        .ok()
        .and_then(read_string)
        .unwrap_or_else(|| "single".to_string())
}

// Inline storage intentionally avoids an allocation for small actor lists.
#[allow(clippy::large_enum_variant)]
enum ActorChildPointers {
    Small(smallvec::SmallVec<[usize; 32]>),
    Large(rustc_hash::FxHashSet<usize>),
}

impl ActorChildPointers {
    fn push(&mut self, out: &mut Vec<Table>, child: Table) {
        if self.insert(child.to_pointer() as usize) {
            out.push(child);
        }
    }

    fn new() -> Self {
        Self::Small(smallvec::SmallVec::new())
    }

    fn insert(&mut self, pointer: usize) -> bool {
        match self {
            Self::Small(pointers) => {
                if pointers.contains(&pointer) {
                    return false;
                }
                if pointers.len() < 32 {
                    pointers.push(pointer);
                } else {
                    // Small actor lists need no membership allocation. Large
                    // lists switch to hashing instead of quadratic scans.
                    let mut set =
                        rustc_hash::FxHashSet::with_capacity_and_hasher(64, Default::default());
                    set.extend(pointers.iter().copied());
                    set.insert(pointer);
                    *self = Self::Large(set);
                }
                true
            }
            Self::Large(pointers) => pointers.insert(pointer),
        }
    }
}

pub fn push_unique_actor_child(out: &mut Vec<Table>, seen: &mut Vec<usize>, child: Table) {
    let ptr = child.to_pointer() as usize;
    if seen.contains(&ptr) {
        return;
    }
    seen.push(ptr);
    out.push(child);
}

pub fn actor_children(lua: &Lua, actor: &Table) -> mlua::Result<Table> {
    if let Some(children) = actor.get::<Option<Table>>("__songlua_children")? {
        return Ok(children);
    }
    let children = lua.create_table()?;
    let mt = lua.create_table()?;
    let order = lua.create_table()?;
    mt.set("__songlua_child_names", order.clone())?;
    mt.set(
        "__newindex",
        lua.create_function(|_, (registry, key, value): (Table, Value, Value)| {
            if !value.is_nil() {
                // Keep the order table owned by Lua. Capturing it in Rust
                // retains an auxiliary stack slot for every child registry.
                let order = registry.metatable().expect("owned child registry metatable")
                    .raw_get::<Table>("__songlua_child_names")?;
                order.raw_set(order.raw_len() + 1, key.clone())?;
            }
            registry.raw_set(key, value)
        })?,
    )?;
    children.set_metatable(Some(mt))?;
    actor.set("__songlua_children", children.clone())?;
    Ok(children)
}

pub(crate) fn actor_name(actor: &Table) -> mlua::Result<String> {
    if let Some(name) = actor.raw_get::<Option<String>>("__songlua_load_name")? {
        return Ok(name);
    }
    if let Some(name) = actor.raw_get::<Option<String>>("__songlua_native_name")? {
        return Ok(name);
    }
    Ok(actor.raw_get::<Option<String>>("Name")?.unwrap_or_default())
}

pub(crate) fn set_actor_name(actor: &Table, name: mlua::LuaString) -> mlua::Result<()> {
    if !matches!(actor.raw_get::<Value>("__songlua_load_name")?, Value::Nil) {
        actor.raw_set("__songlua_load_name", name.clone())?;
    }
    actor.raw_set("__songlua_native_name", name)
}

pub fn actor_named_children(lua: &Lua, actor: &Table) -> mlua::Result<Table> {
    let children = lua.create_table()?;
    if actor.raw_get::<Option<bool>>("__songlua_owned_child_lookup")?.unwrap_or(false) {
        // Native PushChildrenTable traverses m_SubActors after BeginScreen's
        // stable draw-order sort, not the named lookup registry's insertion order.
        for child in actor.sequence_values::<Table>() {
            let child = child?;
            let name = actor_name(&child)?;
            match children.raw_get::<Value>(name.as_str())? {
                Value::Table(group) if actor_is_child_group(&group)? => {
                    push_sequence_child_once(&group, child)?;
                }
                Value::Table(previous) => {
                    let group = create_actor_child_group(lua)?;
                    group.raw_set(1, previous)?;
                    group.raw_set(2, child)?;
                    children.raw_set(name.as_str(), group)?;
                }
                _ => children.raw_set(name.as_str(), child)?,
            }
        }
        return Ok(children);
    }
    let source = actor_children(lua, actor)?;
    let mut copy = |key: Value, value: Value| -> mlua::Result<()> {
        if value.is_nil() || !children.raw_get::<Value>(key.clone())?.is_nil() {
            return Ok(());
        }
        let value = match value {
            Value::Table(group) if actor_is_child_group(&group)? => {
                let snapshot = create_actor_child_group(lua)?;
                for child in group.sequence_values::<Table>() {
                    push_sequence_child_once(&snapshot, child?)?;
                }
                if snapshot.raw_len() == 1 {
                    snapshot.raw_get::<Value>(1)?
                } else {
                    Value::Table(snapshot)
                }
            }
            value => value,
        };
        children.raw_set(key, value)
    };
    if let Some(order) = source
        .metatable()
        .map(|mt| mt.raw_get::<Option<Table>>("__songlua_child_names"))
        .transpose()?
        .flatten()
    {
        for key in order.sequence_values::<Value>() {
            let key = key?;
            copy(key.clone(), source.raw_get::<Value>(key)?)?;
        }
    }
    // Raw registries also occur in embedded actors and preserve their lookup
    // aliases. Registered runtime children use the creation order above.
    source.for_each::<Value, Value>(&mut copy)?;
    for child in actor.sequence_values::<Value>() {
        let Value::Table(child) = child? else {
            continue;
        };
        let name = actor_name(&child)?;
        match children.raw_get::<Value>(name.as_str())? {
            Value::Table(group) if actor_is_child_group(&group)? => {
                push_sequence_child_once(&group, child)?;
            }
            Value::Table(existing) => {
                if existing.to_pointer() != child.to_pointer() {
                    let group = create_actor_child_group(lua)?;
                    group.raw_set(1, existing)?;
                    group.raw_set(2, child)?;
                    children.raw_set(name.as_str(), group)?;
                }
            }
            _ => {
                children.raw_set(name.as_str(), child)?;
            }
        }
    }
    Ok(children)
}

pub fn actor_direct_children(lua: &Lua, actor: &Table) -> mlua::Result<Vec<Table>> {
    let mut out = Vec::new();
    let mut seen = ActorChildPointers::new();
    for value in actor.sequence_values::<Value>() {
        let Value::Table(child) = value? else {
            continue;
        };
        seen.push(&mut out, canonical_live_actor_handle(lua, child));
    }
    // Named keys do not participate in child identity or first-seen order.
    // Raw callback traversal avoids retaining a cloned iterator key.
    actor_children(lua, actor)?.for_each::<Value, Value>(|_, value| {
        let Value::Table(child) = value else {
            return Ok(());
        };
        if actor_is_child_group(&child)? {
            for group_value in child.sequence_values::<Value>() {
                if let Value::Table(group_child) = group_value? {
                    seen.push(&mut out, canonical_live_actor_handle(lua, group_child));
                }
            }
        } else {
            seen.push(&mut out, canonical_live_actor_handle(lua, child));
        }
        Ok(())
    })?;
    Ok(out)
}

pub fn can_create_named_child_actor(parent: &Table, name: &str) -> mlua::Result<bool> {
    let parent_type = parent.get::<Option<String>>("__songlua_actor_type")?;
    if parent_type
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("PlayerActor"))
        && parent
            .get::<Option<i64>>("__songlua_player_index")?
            .is_some()
    {
        return Ok(
            name.eq_ignore_ascii_case("NoteField") || player_child_proxy_name(name).is_some()
        );
    }
    if parent
        .get::<Option<String>>("__songlua_top_screen_child_name")?
        .as_deref()
        .is_some_and(|child_name| child_name.eq_ignore_ascii_case("Underlay"))
    {
        let player = underlay_score_index(name)
            .or_else(|| top_screen_step_stats_pane_index(name))
            .or_else(|| top_screen_danger_index(name));
        if let Some(player) = player {
            let key = ["__songlua_player1_enabled", "__songlua_player2_enabled"][player];
            if parent.raw_get::<Option<bool>>(key)? == Some(false) {
                return Ok(false);
            }
        }
        return Ok(underlay_score_index(name).is_some()
            || name.eq_ignore_ascii_case("SongMeter")
            || name.eq_ignore_ascii_case("Header"));
    }
    Ok(false)
}

pub fn remove_actor_child(lua: &Lua, actor: &Table, name: &str) -> mlua::Result<()> {
    let children = actor_children(lua, actor)?;
    children.set(name, Value::Nil)?;
    if let Some(order) = children
        .metatable()
        .map(|mt| mt.raw_get::<Option<Table>>("__songlua_child_names"))
        .transpose()?
        .flatten()
    {
        let mut write = 1;
        for read in 1..=order.raw_len() {
            let key = order.raw_get::<Value>(read)?;
            if matches!(&key, Value::String(key) if key.as_bytes().as_ref() == name.as_bytes()) {
                continue;
            }
            order.raw_set(write, key)?;
            write += 1;
        }
        for index in write..=order.raw_len() {
            order.raw_set(index, Value::Nil)?;
        }
    }
    let mut write = 1;
    for read in 1..=actor.raw_len() {
        let value = actor.raw_get::<Value>(read)?;
        let remove = match &value {
            Value::Table(child) => actor_name(child)? == name,
            _ => false,
        };
        if remove {
            continue;
        }
        if write != read {
            actor.raw_set(write, value)?;
        }
        write += 1;
    }
    for index in write..=actor.raw_len() {
        actor.raw_set(index, Value::Nil)?;
    }
    Ok(())
}

pub fn remove_all_actor_children(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    for index in 1..=actor.raw_len() {
        actor.raw_set(index, Value::Nil)?;
    }
    let children = actor_children(lua, actor)?;
    // Retain the registry's identity, metatable, and reusable capacity.
    children.clear()?;
    if let Some(order) = children
        .metatable()
        .map(|mt| mt.raw_get::<Option<Table>>("__songlua_child_names"))
        .transpose()?
        .flatten()
    {
        order.clear()?;
    }
    Ok(())
}

pub fn push_sequence_child_once(actor: &Table, child: Table) -> mlua::Result<bool> {
    let child_ptr = child.to_pointer();
    for index in 1..=actor.raw_len() {
        if let Some(Value::Table(existing)) = actor.raw_get::<Option<Value>>(index)?
            && existing.to_pointer() == child_ptr
        {
            return Ok(false);
        }
    }
    actor.raw_set(actor.raw_len() + 1, child)?;
    Ok(true)
}

pub fn install_course_contents_list_children(actor: &Table) -> mlua::Result<()> {
    actor.set("__songlua_scroller_current_item", 0.0_f32)?;
    actor.set("__songlua_scroller_destination_item", 0.0_f32)?;
    actor.set("__songlua_scroller_num_items", 1_i64)?;
    if let Some(display) = actor.get::<Option<Table>>("Display")? {
        if display.get::<Option<String>>("Name")?.is_none() {
            display.set("Name", "Display")?;
        }
        display.set("__songlua_parent", actor.clone())?;
        push_sequence_child_once(actor, display)?;
    }
    Ok(())
}

pub fn position_scroller_items(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    let Some(display) = actor_named_children(lua, actor)?.get::<Option<Table>>("Display")? else {
        return Ok(());
    };
    let num_items = actor
        .get::<Option<i64>>("__songlua_scroller_num_items")?
        .unwrap_or(1)
        .max(1);
    if let Some(transform) =
        actor.get::<Option<Function>>("__songlua_scroller_transform_function")?
    {
        transform.call::<()>((display, 0.0_f32, 0_i64, num_items))?;
    } else if let Some(height) = actor.get::<Option<f32>>("__songlua_scroller_item_height")? {
        display.set("Y", 0.0_f32 * height)?;
    } else if let Some(width) = actor.get::<Option<f32>>("__songlua_scroller_item_width")? {
        display.set("X", 0.0_f32 * width)?;
    }
    Ok(())
}

pub fn populate_course_contents_display(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    let Some(display) = actor_named_children(lua, actor)?.get::<Option<Table>>("Display")? else {
        return Ok(());
    };
    for player_index in 0..LUA_PLAYERS {
        let params = create_course_contents_params(lua, player_index)?;
        let params = Some(Value::Table(params));
        run_actor_named_command_with_drain_and_params(
            lua,
            &display,
            "SetSongCommand",
            true,
            params.clone(),
        )?;
        run_named_command_on_leaves(lua, &display, "SetSongCommand", params)?;
    }
    Ok(())
}

fn create_course_contents_params(lua: &Lua, player_index: usize) -> mlua::Result<Table> {
    let params = lua.create_table()?;
    params.set("Number", 0_i64)?;
    params.set("PlayerNumber", player_number_name(player_index))?;
    params.set("Song", current_song_value(lua)?)?;
    let steps = current_steps_value(lua, player_index)?;
    params.set("Steps", steps.clone())?;
    set_course_contents_steps_params(lua, &params, steps)?;
    Ok(params)
}

pub fn current_song_value(lua: &Lua) -> mlua::Result<Value> {
    current_gamestate_value(lua, "GetCurrentSong")
}

pub fn current_gamestate_value(lua: &Lua, method_name: &str) -> mlua::Result<Value> {
    let Some(gamestate) = lua.globals().get::<Option<Table>>("GAMESTATE")? else {
        return Ok(Value::Nil);
    };
    let Some(method) = gamestate.get::<Option<Function>>(method_name)? else {
        return Ok(Value::Nil);
    };
    method.call::<Value>(gamestate)
}

pub fn current_steps_value(lua: &Lua, player_index: usize) -> mlua::Result<Value> {
    current_gamestate_player_value(lua, "GetCurrentSteps", player_index)
}

pub fn current_gamestate_player_value(
    lua: &Lua,
    method_name: &str,
    player_index: usize,
) -> mlua::Result<Value> {
    let Some(gamestate) = lua.globals().get::<Option<Table>>("GAMESTATE")? else {
        return Ok(Value::Nil);
    };
    let Some(method) = gamestate.get::<Option<Function>>(method_name)? else {
        return Ok(Value::Nil);
    };
    method.call::<Value>((gamestate, player_number_name(player_index)))
}

fn set_course_contents_steps_params(lua: &Lua, params: &Table, steps: Value) -> mlua::Result<()> {
    let Value::Table(steps) = steps else {
        params.set("Meter", "?")?;
        params.set("Difficulty", SongLuaDifficulty::Medium.sm_name())?;
        return Ok(());
    };
    let meter = call_table_method(&steps, "GetMeter")?;
    params.set(
        "Meter",
        if matches!(meter, Value::Nil) {
            Value::String(lua.create_string("?")?)
        } else {
            meter
        },
    )?;
    let difficulty = call_table_method(&steps, "GetDifficulty")?;
    params.set(
        "Difficulty",
        if matches!(difficulty, Value::Nil) {
            Value::String(lua.create_string(SongLuaDifficulty::Medium.sm_name())?)
        } else {
            difficulty
        },
    )
}

pub fn call_table_method(table: &Table, method_name: &str) -> mlua::Result<Value> {
    let Some(method) = table.get::<Option<Function>>(method_name)? else {
        return Ok(Value::Nil);
    };
    method.call::<Value>(table.clone())
}

pub fn create_actor_child_group(lua: &Lua) -> mlua::Result<Table> {
    let group = lua.create_table()?;
    let mt = lua.create_table()?;
    mt.set(SONG_LUA_CHILD_GROUP_KEY, true)?;
    mt.set(
        "__index",
        lua.create_function(|lua, (group, key): (Table, Value)| {
            if lua.coerce_number(key.clone())?.is_some() {
                return group.raw_get::<Value>(key);
            }
            let Some(actor) = group.raw_get::<Option<Table>>(group.raw_len())? else {
                return Ok(Value::Nil);
            };
            let value = actor.get::<Value>(key.clone())?;
            let Value::Function(_) = value else {
                return Ok(value);
            };
            Ok(Value::Function(lua.create_function(
                move |_, args: MultiValue| {
                    let mut args = args.into_vec();
                    let Some(Value::Table(group)) = args.first() else {
                        return Err(mlua::Error::runtime("child group receiver must be a table"));
                    };
                    let actor = group.raw_get::<Table>(group.raw_len())?;
                    // Dummy actor methods bind their receiver. Resolve the
                    // current last child's method at call time as well.
                    let method = actor.get::<Function>(key.clone())?;
                    args[0] = Value::Table(actor);
                    method.call::<MultiValue>(MultiValue::from_vec(args))
                },
            )?))
        })?,
    )?;
    let _ = group.set_metatable(Some(mt));
    Ok(group)
}

pub fn actor_is_child_group(table: &Table) -> mlua::Result<bool> {
    table
        .metatable()
        .map(|mt| mt.get::<Option<bool>>(SONG_LUA_CHILD_GROUP_KEY))
        .transpose()
        .map(|value| value.flatten().unwrap_or(false))
}

pub fn actor_wrappers(lua: &Lua, actor: &Table) -> mlua::Result<Table> {
    if let Some(wrappers) = actor.get::<Option<Table>>("__songlua_wrappers")? {
        return Ok(wrappers);
    }
    let wrappers = lua.create_table()?;
    actor.set("__songlua_wrappers", wrappers.clone())?;
    Ok(wrappers)
}

pub fn copy_dummy_actor_tags(from: &Table, into: &Table) -> mlua::Result<()> {
    for key in ["__songlua_player1_enabled", "__songlua_player2_enabled"] {
        if let Some(enabled) = from.raw_get::<Option<bool>>(key)? {
            into.raw_set(key, enabled)?;
        }
    }
    for key in ["__songlua_player_index", "__songlua_compile_layer"] {
        if let Some(index) = from.get::<Option<i64>>(key)? {
            into.set(key, index)?;
        }
    }
    if let Some(child_name) = from.get::<Option<String>>("__songlua_player_child_name")? {
        into.set("__songlua_player_child_name", child_name)?;
    }
    if let Some(child_name) = from.get::<Option<String>>("__songlua_top_screen_child_name")? {
        into.set("__songlua_top_screen_child_name", child_name)?;
    }
    for key in [
        "__songlua_main_title",
        "__songlua_bpm_text",
        "__songlua_style_name",
    ] {
        if let Some(value) = from.get::<Option<String>>(key)? {
            into.set(key, value)?;
        }
    }
    Ok(())
}

fn is_actor_mutable_state_key(key: &str) -> bool {
    key.starts_with("__songlua_state_")
        || key.starts_with("__songlua_capture_")
        || key.starts_with("__songlua_graph_display_")
        || key.starts_with("__songlua_scroller_")
        || key.starts_with("__songlua_aft_")
        || key.starts_with("__songlua_command_queue")
        || key.starts_with("__songlua_recurring_update_")
        || matches!(
            key,
            "__songlua_visible"
                | "__songlua_native_name"
                | "__songlua_wrappers"
                | "__songlua_hibernate_seconds"
                | "__songlua_theme_hibernating"
                | "__songlua_update_rate"
                | "__songlua_current_x"
                | "__songlua_current_y"
                | "__songlua_current_z"
                | "__songlua_diffuse"
                | "__songlua_text_attributes"
                | "__songlua_stroke_color"
                | "__songlua_aux"
                | "__songlua_stream_width"
                | "__songlua_sprite_animation_length_seconds"
                | "__songlua_sprite_effect_mode"
                | "Text"
                | "Texture"
                | "File"
                | "StreamWidth"
        )
}

fn is_actor_semantic_state_key(key: &str) -> bool {
    is_actor_mutable_state_key(key) && !key.starts_with("__songlua_capture_")
}

pub fn snapshot_actor_mutable_state(
    lua: &Lua,
    actor: &Table,
) -> mlua::Result<Vec<(String, Value)>> {
    snapshot_actor_state(lua, actor, is_actor_mutable_state_key)
}

fn snapshot_actor_semantic_state(lua: &Lua, actor: &Table) -> mlua::Result<Vec<(String, Value)>> {
    snapshot_actor_state(lua, actor, is_actor_semantic_state_key)
}

fn clone_actor_state(lua: &Lua, key: &str, value: Value) -> mlua::Result<Value> {
    if key == "__songlua_wrappers" {
        if let Value::Table(wrappers) = value {
            // Copy membership, retaining the live Actor identities and method
            // closures. Deep copying a wrapper would also recurse into its owner.
            let copy = lua.create_table_with_capacity(wrappers.raw_len(), 0)?;
            for (index, wrapper) in wrappers.sequence_values::<Table>().enumerate() {
                copy.raw_set(index + 1, wrapper?)?;
            }
            return Ok(Value::Table(copy));
        }
    }
    clone_lua_value(lua, value)
}

fn retained_key_text(lua: &Lua, key: &mlua::LuaString, keep: fn(&str) -> bool) -> mlua::Result<Option<String>> {
    // SAFETY: one rooted, checked Lua string; UTF-8 validation and the fixed
    // Rust predicate do not call Lua. Only owned text leaves the read scope.
    unsafe {
        crate::raw_vm::with_read_stack(lua, 1, |raw, base| {
            raw.push(key)?;
            let mut len = 0;
            let data = ffi::lua_tolstring(raw.state(), base + 1, &mut len);
            let bytes = std::slice::from_raw_parts(data.cast::<u8>(), len);
            let text = std::str::from_utf8(bytes).map_err(|error| mlua::Error::FromLuaConversionError {
                from: "string", to: "&str".into(), message: Some(error.to_string()),
            })?;
            Ok(keep(text).then(|| text.to_owned()))
        })
    }
}

fn remove_scalar_global(lua: &Lua, key: &mlua::LuaString, scalar: bool, retained: &HashSet<&str>) -> mlua::Result<bool> {
    // SAFETY: same rooted immutable-string read; hashing is pure Rust.
    unsafe {
        crate::raw_vm::with_read_stack(lua, 1, |raw, base| {
            raw.push(key)?;
            let mut len = 0;
            let data = ffi::lua_tolstring(raw.state(), base + 1, &mut len);
            let bytes = std::slice::from_raw_parts(data.cast::<u8>(), len);
            let text = std::str::from_utf8(bytes).map_err(|error| mlua::Error::FromLuaConversionError {
                from: "string", to: "&str".into(), message: Some(error.to_string()),
            })?;
            Ok(scalar && !retained.contains(text))
        })
    }
}

fn snapshot_actor_state(
    lua: &Lua,
    actor: &Table,
    keep_key: fn(&str) -> bool,
) -> mlua::Result<Vec<(String, Value)>> {
    let mut out = Vec::new();
    let key_lua = actor.weak_lua().upgrade();
    actor.for_each::<Value, Value>(|key, value| {
        let Value::String(key) = key else {
            return Ok(());
        };
        if let Some(key) = retained_key_text(&key_lua, &key, keep_key)? {
            let value = clone_actor_state(lua, &key, value)?;
            out.push((key, value));
        }
        Ok(())
    })?;
    {
        let pose = live_pose_snapshot(lua, actor)?.into_iter().flatten();
        let flags = live_render_flags(lua, actor).into_iter().flat_map(RenderFlags::snapshot);
        let base = live_base_transform(lua, actor).into_iter().flat_map(BaseTransform::snapshot);
        for (key, value) in pose.chain(flags).chain(base) {
            if !keep_key(key) { continue; }
            if let Some(entry) = out.iter_mut().find(|(name, _)| name == key) {
                entry.1 = value;
            } else if !value.is_nil() {
                out.push((key.to_owned(), value));
            }
        }
    }
    Ok(out)
}

pub fn restore_actor_mutable_state(
    actor: &Table,
    snapshot: Vec<(String, Value)>,
) -> mlua::Result<()> {
    restore_actor_state(actor, snapshot, is_actor_mutable_state_key)
}

fn restore_actor_semantic_state(actor: &Table, snapshot: Vec<(String, Value)>) -> mlua::Result<()> {
    restore_actor_state(actor, snapshot, is_actor_semantic_state_key)
}

fn restore_actor_state(
    actor: &Table,
    snapshot: Vec<(String, Value)>,
    clear_key: fn(&str) -> bool,
) -> mlua::Result<()> {
    let mut keys = smallvec::SmallVec::<[mlua::LuaString; 16]>::new();
    actor.for_each::<Value, Value>(|key, _| {
        let Value::String(key) = key else {
            return Ok(());
        };
        // Keep Lua string handles alive until iteration finishes; only retained
        // snapshot entries need an owned Rust key. Validate all string keys.
        if clear_key(&key.to_str()?) {
            keys.push(key);
        }
        Ok(())
    })?;
    for key in keys {
        actor.set(key, Value::Nil)?;
    }
    for (key, value) in snapshot {
        actor.set(key, value)?;
    }
    Ok(())
}

fn is_scalar_lua_value(value: &Value) -> bool {
    matches!(
        value,
        Value::Boolean(_) | Value::Integer(_) | Value::Number(_) | Value::String(_)
    )
}

fn snapshot_scalar_globals(lua: &Lua) -> mlua::Result<Vec<(String, Value)>> {
    let mut out = Vec::new();
    // Non-scalar globals need no retained key. Raw callback traversal avoids
    // the iterator's shared-handle allocation for those ignored entries.
    lua.globals().for_each::<Value, Value>(|key, value| {
        let Value::String(key) = key else {
            return Ok(());
        };
        if is_scalar_lua_value(&value) {
            out.push((key.to_str()?.to_string(), value));
        }
        Ok(())
    })?;
    Ok(out)
}

fn restore_scalar_globals(lua: &Lua, snapshot: Vec<(String, Value)>) -> mlua::Result<()> {
    let globals = lua.globals();
    let keys = snapshot
        .iter()
        .map(|(key, _)| key.as_str())
        .collect::<HashSet<_>>();
    let mut remove = smallvec::SmallVec::<[mlua::LuaString; 16]>::new();
    globals.for_each::<Value, Value>(|key, value| {
        let Value::String(key) = key else {
            return Ok(());
        };
        // Validate every string key before making any changes, including keys
        // whose values are not scalar. Retain Lua handles instead of copying
        // temporary key bytes into Rust strings.
        let should_remove = remove_scalar_global(lua, &key, is_scalar_lua_value(&value), &keys)?;
        if should_remove {
            remove.push(key);
        }
        Ok(())
    })?;
    for key in remove {
        globals.set(key, Value::Nil)?;
    }
    for (key, value) in snapshot {
        globals.set(key, value)?;
    }
    Ok(())
}

pub fn snapshot_actors_semantic_state(
    lua: &Lua,
    actors: &[Table],
) -> mlua::Result<Vec<(Table, Vec<(String, Value)>)>> {
    actors
        .iter()
        .map(|actor| Ok((actor.clone(), snapshot_actor_semantic_state(lua, actor)?)))
        .collect()
}

pub fn restore_actors_semantic_state(
    snapshots: Vec<(Table, Vec<(String, Value)>)>,
) -> mlua::Result<()> {
    for (actor, snapshot) in snapshots {
        restore_actor_semantic_state(&actor, snapshot)?;
    }
    Ok(())
}

pub fn snapshot_actor_semantic_state_table(lua: &Lua, actor: &Table) -> mlua::Result<Table> {
    if live_destination_value(lua, actor, |_| ()).is_some() {
        let snapshot = lua.create_table()?;
        for (index, (key, value)) in snapshot_actor_semantic_state(lua, actor)?.into_iter().enumerate() {
            let entry = lua.create_table_with_capacity(2, 0)?;
            entry.raw_set(1, key)?;
            entry.raw_set(2, value)?;
            snapshot.raw_set(index + 1, entry)?;
        }
        return Ok(snapshot);
    }
    let snapshot = lua.create_table()?;
    let mut index = 1usize;
    let key_lua = actor.weak_lua().upgrade();
    actor.for_each::<Value, Value>(|key, value| {
        let Value::String(key) = key else {
            return Ok(());
        };
        // Validate every string key, including keys excluded from the snapshot.
        let Some(name) = retained_key_text(&key_lua, &key, is_actor_semantic_state_key)? else {
            return Ok(());
        };
        let value = clone_actor_state(lua, &name, value)?;
        let entry = lua.create_table_with_capacity(2, 0)?;
        entry.raw_set(1, key)?;
        entry.raw_set(2, value)?;
        snapshot.raw_set(index, entry)?;
        index += 1;
        Ok(())
    })?;
    Ok(snapshot)
}

pub fn read_actor_semantic_state_table(snapshot: &Table) -> mlua::Result<Vec<(String, Value)>> {
    let mut out = Vec::with_capacity(snapshot.raw_len());
    for entry in snapshot.sequence_values::<Table>() {
        let entry = entry?;
        out.push((entry.raw_get(1)?, entry.raw_get(2)?));
    }
    Ok(out)
}

pub fn song_lua_actor_registry(lua: &Lua) -> mlua::Result<Table> {
    let globals = lua.globals();
    if let Some(registry) = globals.get::<Option<Table>>("__songlua_actor_registry")? {
        return Ok(registry);
    }
    let registry = lua.create_table()?;
    globals.set("__songlua_actor_registry", registry.clone())?;
    Ok(registry)
}

#[cfg(feature = "test-support")]
struct SongLuaMessageOrder(FxHashMap<String, usize>);

#[cfg(feature = "test-support")]
#[derive(Default)]
struct SongLuaMessageCoverage {
    seen: rustc_hash::FxHashSet<String>,
    pending: Vec<String>,
}

pub(crate) fn take_message_coverage(lua: &Lua) -> Vec<String> {
    #[cfg(feature = "test-support")]
    if let Some(mut coverage) = lua.app_data_mut::<SongLuaMessageCoverage>() {
        return std::mem::take(&mut coverage.pending);
    }
    let _ = lua;
    Vec::new()
}

#[cfg(feature = "test-support")]
pub(crate) fn install_message_order(lua: &Lua, ranks: &[(String, usize)]) -> mlua::Result<()> {
    if ranks.is_empty() { return Ok(()); }
    let mut order = FxHashMap::default();
    let mut identities = rustc_hash::FxHashSet::default();
    for (path, rank) in ranks {
        if *rank == 0 || order.insert(path.clone(), *rank).is_some() || !identities.insert(*rank) {
            return Err(mlua::Error::external("invalid captured subscriber identities"));
        }
    }
    lua.set_app_data(SongLuaMessageOrder(order));
    Ok(())
}

#[cfg(feature = "test-support")]
pub(crate) fn message_actor_path(actor: &Table) -> mlua::Result<String> {
    let mut current = actor.clone();
    let mut parts = Vec::new();
    let mut seen = rustc_hash::FxHashSet::default();
    loop {
        if !seen.insert(current.to_pointer() as usize) {
            return Err(mlua::Error::external("cycle in subscriber ownership"));
        }
        if let Some(path) = current.raw_get::<Option<String>>("__songlua_message_path")? {
            parts.push(path);
            break;
        }
        if current.raw_get::<Option<String>>("__songlua_actor_type")?.as_deref() == Some("TopScreen") {
            parts.push("ScreenGameplay".into());
            break;
        }
        let Some(parent) = current.raw_get::<Option<Table>>("__songlua_parent")? else {
            return Err(mlua::Error::external("subscriber has no captured actor path"));
        };
        let mut component = None;
        if let Some(wrappers) = parent.raw_get::<Option<Table>>("__songlua_wrappers")? {
            for (index, wrapper) in wrappers.sequence_values::<Table>().enumerate() {
                if wrapper?.to_pointer() == current.to_pointer() {
                    component = Some(format!("WrapperState{}", index + 1));
                    break;
                }
            }
        }
        if component.is_none() {
            for (index, child) in parent.sequence_values::<Value>().enumerate() {
                if let Value::Table(child) = child? && child.to_pointer() == current.to_pointer() {
                    component = Some((index + 1).to_string());
                    break;
                }
            }
        }
        let name = component.or(current.raw_get::<Option<String>>("Name")?);
        let Some(name) = name.filter(|name| !name.is_empty()) else {
            return Err(mlua::Error::external("subscriber has no captured child identity"));
        };
        parts.push(name);
        current = parent;
    }
    parts.reverse();
    Ok(parts.join("/"))
}

#[cfg(feature = "test-support")]
fn captured_message_actor_identity(lua: &Lua, actor: &Table) -> mlua::Result<Option<usize>> {
    let path = message_actor_path(actor)?;
    let rank = lua.app_data_ref::<SongLuaMessageOrder>()
        .and_then(|order| order.0.get(&path).copied());
    if rank.is_none() {
        if lua.app_data_ref::<SongLuaMessageCoverage>().is_none() {
            lua.set_app_data(SongLuaMessageCoverage::default());
        }
        let mut coverage = lua.app_data_mut::<SongLuaMessageCoverage>().unwrap();
        let gap = format!("missing captured subscriber identity for {path}");
        if coverage.seen.insert(gap.clone()) {
            coverage.pending.push(gap);
        }
    }
    Ok(rank)
}

#[cfg(test)]
#[test]
fn broadcast_identity_order() {
    let lua = Lua::new();
    lua.load(r#"
        seen = {}
        local function receive(self) seen[#seen + 1] = self end
        a = { GoMessageCommand = receive }
        b = { GoMessageCommand = receive }
        __songlua_actor_registry = { a, b }
    "#).exec().expect("live subscribers");
    let a = lua.globals().get::<Table>("a").expect("subscriber a");
    let b = lua.globals().get::<Table>("b").expect("subscriber b");
    let expected = if a.to_pointer() < b.to_pointer() { [a.clone(), b.clone()] } else { [b.clone(), a.clone()] };
    for reversed in [false, true] {
        lua.globals().set("seen", lua.create_table().expect("observations")).expect("reset observations");
        let registry = lua.create_sequence_from(if reversed { [b.clone(), a.clone()] } else { [a.clone(), b.clone()] })
            .expect("registry permutation");
        lua.globals().set("__songlua_actor_registry", registry).expect("install registry");
        broadcast_song_lua_message(&lua, "Go", None).expect("deliver by identity");
        let seen = lua.globals().get::<Table>("seen").expect("observed recipients");
        assert_eq!(seen.raw_len(), 2);
        for (index, actor) in expected.iter().enumerate() {
            assert_eq!(seen.raw_get::<Table>(index + 1).expect("recipient").to_pointer(), actor.to_pointer());
        }
    }
}

pub fn register_song_lua_actor(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    if crate::live::active(lua) {
        ensure_tween_replay(lua, actor)?;
    }
    let registry = song_lua_actor_registry(lua)?;
    if !registry_contains_actor(lua, &registry, actor)? {
        registry.raw_set(registry.raw_len() + 1, actor)?;
    }
    Ok(())
}

fn registry_contains_actor(lua: &Lua, registry: &Table, actor: &Table) -> mlua::Result<bool> {
    // SAFETY: registry/actor are rooted in two reserved slots. A third holds
    // one raw sequence value. Reads and identity checks cannot invoke Lua.
    unsafe {
        crate::raw_vm::with_read_stack(lua, 3, |raw, base| {
            raw.push(registry)?;
            raw.push(actor)?;
            let state = raw.state();
            for index in 1.. {
                ffi::lua_rawgeti(state, base + 1, index);
                match ffi::lua_type(state, base + 3) {
                    ffi::LUA_TNIL => return Ok(false),
                    ffi::LUA_TTABLE => {
                        if ffi::lua_rawequal(state, base + 2, base + 3) != 0 { return Ok(true); }
                    }
                    // Preserve checked conversion of unusual registry values.
                    _ => { let _ = Value::from_stack(base + 3, raw)?; }
                }
                ffi::lua_pop(state, 1);
            }
            unreachable!()
        })
    }
}

pub fn inherit_actor_dirs(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    if let Some(script_dir) = lua
        .globals()
        .get::<Option<String>>("__songlua_script_dir")?
    {
        actor.set("__songlua_script_dir", script_dir)?;
    }
    if let Some(song_dir) = lua.globals().get::<Option<String>>("__songlua_song_dir")? {
        actor.set("__songlua_song_dir", song_dir)?;
    }
    Ok(())
}

pub fn create_dummy_actor(
    lua: &Lua,
    actor_type: &'static str,
    install_actor_methods: fn(&Lua, &Table) -> mlua::Result<()>,
) -> mlua::Result<Table> {
    let actor = lua.create_table()?;
    init_actor_type(lua, &actor, actor_type)?;
    inherit_actor_dirs(lua, &actor)?;
    install_actor_methods(lua, &actor)?;
    install_actor_metatable(lua, &actor, install_actor_methods)?;
    reset_actor_capture(lua, &actor)?;
    register_song_lua_actor(lua, &actor)?;
    Ok(actor)
}

fn init_actor_type(lua: &Lua, actor: &Table, actor_type: &'static str) -> mlua::Result<()> {
    actor.set("__songlua_actor_type", actor_type)?;
    // Quad and ActorMultiVertex constructors load RageTexture_Default. Its manager lookup key is
    // __blank__, but the resource itself has an empty filename and 1x1 size.
    if actor_type.eq_ignore_ascii_case("Quad") || actor_type.eq_ignore_ascii_case("ActorMultiVertex") {
        actor.raw_set("__songlua_default_texture", true)?;
    }
    if actor_type.eq_ignore_ascii_case("ActorFrameTexture")
        && actor_aft_capture_name(actor)?.is_none()
    {
        let globals = lua.globals();
        let next = globals
            .get::<Option<i64>>(SONG_LUA_AFT_COUNTER_KEY)?
            .unwrap_or(0)
            .saturating_add(1);
        globals.set(SONG_LUA_AFT_COUNTER_KEY, next)?;
        actor.set(
            "__songlua_aft_capture_name",
            format!("ActorFrameTexture {next}"),
        )?;
    }
    Ok(())
}

pub fn set_proxy_target_fields(actor: &Table, target: &Table) -> mlua::Result<()> {
    actor.set("__songlua_proxy_target_kind", Value::Nil)?;
    actor.set("__songlua_proxy_player_index", Value::Nil)?;
    actor.set("__songlua_proxy_target_actor", target.clone())?;
    if let Some(child_name) = target.get::<Option<String>>("__songlua_top_screen_child_name")? {
        if let Some(player_index) = underlay_score_index(&child_name) {
            actor.set("__songlua_proxy_target_kind", "score")?;
            actor.set("__songlua_proxy_player_index", player_index)?;
            return Ok(());
        }
        let kind = if child_name.eq_ignore_ascii_case("Underlay") {
            Some("underlay")
        } else if child_name.eq_ignore_ascii_case("Overlay") {
            Some("overlay")
        } else {
            None
        };
        if let Some(kind) = kind {
            actor.set("__songlua_proxy_target_kind", kind)?;
        }
        return Ok(());
    }
    let Some(player_index) = target.get::<Option<i64>>("__songlua_player_index")? else {
        if target
            .get::<Option<String>>("__songlua_actor_type")?
            .is_some()
        {
            actor.set("__songlua_proxy_target_kind", "actor")?;
        }
        return Ok(());
    };
    actor.set("__songlua_proxy_player_index", player_index)?;
    let kind = match target
        .get::<Option<String>>("__songlua_player_child_name")?
        .as_deref()
    {
        Some(name) if name.eq_ignore_ascii_case("NoteField") => "notefield",
        Some(name) if name.eq_ignore_ascii_case("Judgment") => "judgment",
        Some(name) if name.eq_ignore_ascii_case("Combo") => "combo",
        _ => "player",
    };
    actor.set("__songlua_proxy_target_kind", kind)?;
    Ok(())
}

pub fn normalize_broadcast_params(
    lua: &Lua,
    message: &str,
    params: Option<Value>,
) -> mlua::Result<Option<Value>> {
    match params {
        Some(Value::Table(table)) => {
            if message.eq_ignore_ascii_case("Judgment") {
                normalize_judgment_params(lua, &table)?;
            }
            Ok(Some(Value::Table(table)))
        }
        params => Ok(params),
    }
}

fn runtime_broadcast_beat(lua: &Lua) -> f32 {
    // Use the queued command's dispatch frame, including pauses and split
    // chart timing, rather than the callback that originally queued it.
    lua
        .app_data_ref::<SongLuaQueuedCommand>()
        .and_then(|scope| scope.time.map(|(frame, _)| frame))
        .and_then(|frame| {
            lua.app_data_ref::<SongLuaCompileFrames>()
                .and_then(|frames| frames.beats.get(frame).copied())
        })
        .unwrap_or_else(|| compile_song_runtime_values(lua).map_or(0.0, |(beat, _)| beat))
}

fn source_broadcast_value(value: Value, depth: usize, seen: &mut Vec<usize>) -> Result<serde_json::Value, String> {
    use serde_json::{Value as Json, json};
    Ok(match value {
        Value::Nil => Json::Null,
        Value::Boolean(value) => json!(value),
        Value::Integer(value) => source_broadcast_number(value as f64),
        Value::Number(value) if value.is_finite() => {
            source_broadcast_number(value)
        }
        Value::String(value) => Json::String(value.to_str().map_err(|error| error.to_string())?.replace('\\', "/")),
        Value::Function(value) => {
            // Like the native safe_value logger, inspect the original closure
            // without calling or wrapping it. Dispatch retains its identity.
            let info = value.info();
            json!({"type":"function", "source":info.source.unwrap_or_default().replace('\\', "/"),
                "line":info.line_defined.map_or(-1, |line| line as i64),
                "last_line":info.last_line_defined.map_or(-1, |line| line as i64)})
        }
        Value::Table(table) => {
            let identity = table.to_pointer() as usize;
            if depth >= 5 { return Err("broadcast table exceeds native trace depth".into()); }
            if seen.contains(&identity) { return Err("broadcast table contains a cycle".into()); }
            // Native actor identity requires the runtime definition map. Do
            // not serialize actors as ordinary tables and certify their data.
            for key in ["GetName", "GetParent"] {
                if matches!(table.raw_get::<Value>(key).map_err(|error| error.to_string())?, Value::Function(_)) {
                    return Err("broadcast actor identity remains pending".into());
                }
            }
            seen.push(identity);
            let result = source_broadcast_table(table, depth, seen);
            seen.pop();
            result?
        }
        value => return Err(format!("broadcast {} trace remains pending", value.type_name())),
    })
}

fn source_broadcast_number(value: f64) -> serde_json::Value {
    // RageUtil.cpp's LuaFunc_JsonEncode tries Json::UInt and Json::Int
    // (both 32-bit), then retains a double. Integral doubles outside those
    // ranges must remain JSON floats, including Lua power/hash results.
    if value == (value as u32) as f64 {
        serde_json::json!(value as u32)
    } else if value == (value as i32) as f64 {
        serde_json::json!(value as i32)
    } else {
        serde_json::json!(value)
    }
}

fn source_broadcast_table(table: Table, depth: usize, seen: &mut Vec<usize>) -> Result<serde_json::Value, String> {
    use serde_json::Value as Json;
    let mut entries = Vec::new();
    let mut max_index = 0;
    let mut array = true;
    for pair in table.pairs::<Value, Value>().take(257) {
        if entries.len() == 256 { return Err("broadcast table exceeds native trace entry limit".into()); }
        let (key, value) = pair.map_err(|error| error.to_string())?;
        let (name, index) = match key {
            Value::String(key) => (key.to_str().map_err(|error| error.to_string())?.to_string(), None),
            Value::Boolean(key) => (key.to_string(), None),
            Value::Integer(key) => (key.to_string(), usize::try_from(key).ok().filter(|&key| key > 0)),
            Value::Number(key) if key.is_finite() && key.fract() == 0.0 => (
                key.to_string(), (key > 0.0 && key <= 256.0).then_some(key as usize)),
            _ => return Err("broadcast table key representation remains pending".into()),
        };
        array &= index.is_some();
        max_index = max_index.max(index.unwrap_or(0));
        entries.push((name, index, source_broadcast_value(value, depth + 1, seen)?));
    }
    if array && max_index == entries.len() {
        let mut values = vec![Json::Null; entries.len()];
        for (_, index, value) in entries {
            values[index.expect("dense array keys were checked") - 1] = value;
        }
        Ok(Json::Array(values))
    } else {
        let mut values = serde_json::Map::new();
        for (name, _, value) in entries {
            if values.insert(name, value).is_some() {
                return Err("broadcast table has colliding stringified keys".into());
            }
        }
        Ok(Json::Object(values))
    }
}

pub(crate) fn capture_source_broadcast(lua: &Lua, message: &str, params: Option<&Value>) {
    if lua.app_data_ref::<LiveSourceBroadcasts>().is_none() { return; }
    if !crate::live::observes_source_call(lua) { return; }
    let beat = runtime_broadcast_beat(lua);
    let music_seconds = compile_song_runtime_values(lua).map_or(0.0, |(_, seconds)| f64::from(seconds));
    let params = match params {
        None => Ok(None),
        // The native probe records an explicit nil argument, separately from
        // an omitted argument. Delivery still receives the original Lua value.
        Some(Value::Nil) => Ok(Some(serde_json::json!({"type":"nil"}))),
        Some(value) => source_broadcast_value(value.clone(), 0, &mut Vec::new()).map(Some),
    };
    if let Some(mut events) = lua.app_data_mut::<LiveSourceBroadcasts>() {
        events.0.push(crate::live::SongLuaLiveBroadcast { beat, music_seconds, name: message.to_owned(), params });
    }
}

pub(crate) fn take_source_broadcasts(lua: &Lua) -> Vec<crate::live::SongLuaLiveBroadcast> {
    lua.app_data_mut::<LiveSourceBroadcasts>()
        .map_or_else(Vec::new, |mut events| std::mem::take(&mut events.0))
}

pub fn broadcast_song_lua_message(
    lua: &Lua,
    message: &str,
    params: Option<Value>,
) -> mlua::Result<()> {
    broadcast_song_lua_message_impl(lua, message, params, true)
}

/// Engine parameters already contain native note objects and optional fields.
/// Do not add the compiler's sampled Judgment defaults to a real engine event.
pub(crate) fn broadcast_native_song_lua_message(
    lua: &Lua, message: &str, params: Option<Value>,
) -> mlua::Result<()> {
    broadcast_song_lua_message_impl(lua, message, params, false)
}

fn broadcast_song_lua_message_impl(
    lua: &Lua, message: &str, params: Option<Value>, normalize: bool,
) -> mlua::Result<()> {
    if message.trim().is_empty() { return Ok(()); }
    let command = ActorCommandName::new(message, "MessageCommand");
    let globals = lua.globals();
    let beat = runtime_broadcast_beat(lua);
    // Synchronous On broadcasts are already part of the startup snapshot.
    // Queued startup commands run after this scope, on the first update.
    let on_startup = lua.app_data_ref::<SongLuaStartupQueues>().is_some()
        && lua.app_data_ref::<SongLuaZeroUpdate>().is_none();
    if let Some(mut events) = lua.app_data_mut::<LiveBroadcasts>() {
        // A live session has no baked startup-message snapshot. Retain every
        // actual Init/On/update event, even before overlay capture is installed.
        events.0.push((beat, message.to_string(), params.is_some()));
    } else if !on_startup && let Some(mut capture) = lua.app_data_mut::<SongLuaOverlayUpdateCapture>() {
        capture
            .runtime_broadcasts
            .push((beat, message.to_string(), params.is_some()));
    }
    // Keep the Lua key itself: converting a Rust string for each property
    // lookup also allocates for long names. Prepare it before changing scope.
    let command_key = if lua.app_data_ref::<SongLuaOverlayUpdateCapture>().is_some() {
        Some(lua.create_string(command.as_str())?)
    } else {
        None
    };
    let previous_broadcast = globals.raw_get::<Value>(ACTIVE_BROADCAST_KEY)?;
    globals.raw_set(ACTIVE_BROADCAST_KEY, message)?;
    let capture_broadcast = lua
        .app_data_mut::<SongLuaOverlayUpdateCapture>()
        .map(|mut capture| {
            let previous = capture.active_broadcast.replace(message.to_string());
            let previous_command =
                std::mem::replace(&mut capture.active_broadcast_command, command_key);
            let previous_params =
                std::mem::replace(&mut capture.active_broadcast_params, params.is_some());
            (previous, previous_command, previous_params)
        });
    let result = || {
        let registry = song_lua_actor_registry(lua)?;
        let mut actors = smallvec::SmallVec::<[(usize, Table); 32]>::with_capacity(registry.raw_len());
        for value in registry.sequence_values::<Value>() {
            let Value::Table(actor) = value? else {
                continue;
            };
            if !matches!(actor.get::<Value>(command.as_str())?, Value::Function(_)) {
                continue;
            }
            let actor = canonical_live_actor_handle(lua, actor);
            actors.push((actor.to_pointer() as usize, actor));
        }
        // MessageManager.cpp stores subscribers in an identity-ordered set.
        // Tree construction and definition-table order do not define delivery.
        actors.sort_unstable_by_key(|(identity, _)| *identity);
        // Imported identities are observational inputs. Preserve their relative
        // order in the known slots; unobserved listeners keep their VM identity
        // slots and report missing coverage without disabling message delivery.
        #[cfg(feature = "test-support")]
        if actors.len() > 1 && lua.app_data_ref::<SongLuaMessageOrder>().is_some() {
            let mut slots = smallvec::SmallVec::<[usize; 32]>::new();
            let mut known = smallvec::SmallVec::<[(usize, Table); 32]>::new();
            for (slot, (_, actor)) in actors.iter().enumerate() {
                if let Some(rank) = captured_message_actor_identity(lua, actor)? {
                    slots.push(slot);
                    known.push((rank, actor.clone()));
                }
            }
            known.sort_unstable_by_key(|(rank, _)| *rank);
            for (slot, entry) in slots.into_iter().zip(known) {
                actors[slot] = entry;
            }
        }
        let params = if normalize { normalize_broadcast_params(lua, message, params)? } else { params };
        // Drain in place so consuming the snapshot does not move its inline buffer.
        actors.drain(..).try_for_each(|(_, actor)| {
            // Native Actor::HandleMessage invokes the command immediately,
            // including a reentrant broadcast of the same message. Queue
            // discovery guards must not suppress that synchronous delivery.
            let active = actor_active_commands(lua, &actor)?;
            let previous_active = active.raw_get::<Value>(command.as_str())?;
            active.raw_set(command.as_str(), Value::Nil)?;
            let result = run_actor_named_command_with_drain_and_params(
                lua,
                &actor,
                &command,
                true,
                params.clone(),
            );
            active.raw_set(command.as_str(), previous_active)?;
            result?;
            if lua.app_data_ref::<SongLuaCompileFrames>().is_some()
                && let Some(mut deferred) = lua.app_data_mut::<SongLuaDeferredMessages>()
            {
                for entry in &mut deferred.0 {
                    if entry.actor.to_pointer() == actor.to_pointer()
                        && entry.command == command.as_str()
                    {
                        entry.observed = true;
                    }
                }
            }
            Ok(())
        })
    };
    let result = result();
    if let Some((previous, previous_command, previous_params)) = capture_broadcast
        && let Some(mut capture) = lua.app_data_mut::<SongLuaOverlayUpdateCapture>()
    {
        capture.active_broadcast = previous;
        capture.active_broadcast_command = previous_command;
        capture.active_broadcast_params = previous_params;
    }
    let restore = globals.raw_set(ACTIVE_BROADCAST_KEY, previous_broadcast);
    restore?;
    result
}

fn normalize_judgment_params(lua: &Lua, params: &Table) -> mlua::Result<()> {
    if matches!(params.get::<Value>("Player")?, Value::Nil) {
        params.set("Player", player_number_name(0))?;
    }
    if matches!(params.get::<Value>("TapNoteScore")?, Value::Nil) {
        params.set("TapNoteScore", "TapNoteScore_W3")?;
    }
    if matches!(params.get::<Value>("HoldNoteScore")?, Value::Nil) {
        params.set("HoldNoteScore", "HoldNoteScore_None")?;
    }
    if matches!(params.get::<Value>("TapNoteOffset")?, Value::Nil) {
        params.set("TapNoteOffset", 0.0_f32)?;
    }

    let notes = match params.get::<Value>("Notes")? {
        Value::Table(notes) => notes,
        _ => {
            let notes = lua.create_table()?;
            let first_track = table_i32_field(params, &["FirstTrack", "Column"])?.unwrap_or(0);
            notes.raw_set(
                i64::from(first_track.max(0)) + 1,
                create_tap_note_table(lua, params, None)?,
            )?;
            params.set("Notes", notes.clone())?;
            notes
        }
    };
    if table_has_entries(&notes)? {
        // Snapshot every pair before normalization can mutate the notes table.
        let mut entries =
            smallvec::SmallVec::<[(Value, Value); 16]>::with_capacity(notes.raw_len());
        for entry in notes.pairs::<Value, Value>() {
            entries.push(entry?);
        }
        for (key, value) in entries.drain(..) {
            let note = match value {
                Value::Table(note) => normalize_tap_note_table(lua, params, note)?,
                value => create_tap_note_table(lua, params, Some(value))?,
            };
            notes.set(key, note)?;
        }
    } else {
        notes.raw_set(1, create_tap_note_table(lua, params, None)?)?;
    }
    Ok(())
}

pub fn default_message_command_params(
    lua: &Lua,
    command_name: &str,
) -> mlua::Result<Option<Value>> {
    let Some(message) = command_name.strip_suffix("MessageCommand") else {
        return Ok(None);
    };
    let params = lua.create_table()?;
    match message {
        "Judgment" => {
            params.set("Player", player_number_name(0))?;
            params.set("TapNoteScore", "TapNoteScore_W3")?;
            params.set("TapNoteOffset", 0.0_f32)?;
            params.set("FirstTrack", 0_i64)?;
            normalize_judgment_params(lua, &params)?;
        }
        "EarlyHit" => {
            params.set("Player", player_number_name(0))?;
            params.set("TapNoteScore", "TapNoteScore_W3")?;
            params.set("TapNoteOffset", -0.02_f32)?;
            params.set("Early", true)?;
        }
        "LifeChanged" => {
            params.set("Player", player_number_name(0))?;
            params.set("LifeMeter", create_life_meter_param_table(lua)?)?;
        }
        "HealthStateChanged" => {
            params.set("Player", player_number_name(0))?;
            params.set("PlayerNumber", player_number_name(0))?;
            params.set("HealthState", "HealthState_Alive")?;
        }
        "ExCountsChanged" => {
            params.set("Player", player_number_name(0))?;
            params.set("ExCounts", create_ex_counts_param_table(lua)?)?;
            params.set("ExScore", 0.0_f32)?;
            params.set("ActualPoints", 0.0_f32)?;
            params.set("ActualPossible", 1.0_f32)?;
            params.set("CurrentPossible", 1.0_f32)?;
        }
        "PlayerOptionsChanged" => {
            params.set("Player", player_number_name(0))?;
            params.set("PlayerNumber", player_number_name(0))?;
        }
        _ => return Ok(None),
    }
    Ok(Some(Value::Table(params)))
}

fn create_life_meter_param_table(lua: &Lua) -> mlua::Result<Table> {
    let meter = lua.create_table()?;
    meter.set(
        "GetLife",
        lua.create_function(|_, _args: IgnoredArgs| Ok(SONG_LUA_INITIAL_LIFE))?,
    )?;
    meter.set(
        "IsFailing",
        lua.create_function(|_, _args: IgnoredArgs| Ok(false))?,
    )?;
    meter.set(
        "IsHot",
        lua.create_function(|_, _args: IgnoredArgs| Ok(false))?,
    )?;
    meter.set(
        "IsInDanger",
        lua.create_function(|_, _args: IgnoredArgs| Ok(false))?,
    )?;
    Ok(meter)
}

fn create_ex_counts_param_table(lua: &Lua) -> mlua::Result<Table> {
    let counts = lua.create_table()?;
    for key in [
        "W0", "W1", "W2", "W3", "W4", "W5", "Miss", "Held", "LetGo", "HitMine",
    ] {
        counts.set(key, 0_i64)?;
    }
    Ok(counts)
}

fn table_has_entries(table: &Table) -> mlua::Result<bool> {
    Ok(table.pairs::<Value, Value>().next().transpose()?.is_some())
}

pub(crate) fn normalize_tap_note_table(lua: &Lua, params: &Table, note: Table) -> mlua::Result<Table> {
    if !matches!(note.get::<Value>("GetTapNoteType")?, Value::Nil) {
        return Ok(note);
    }
    let result = match note.get::<Value>("TapNoteResult")? {
        Value::Table(result) => normalize_tap_note_result_table(lua, params, result, Some(&note))?,
        _ => create_tap_note_result_table(lua, params, Some(&note))?,
    };
    note.set("TapNoteResult", result.clone())?;
    note.set("HoldNoteResult", result.clone())?;
    install_tap_note_methods(lua, params, &note, result)?;
    Ok(note)
}

fn create_tap_note_table(
    lua: &Lua,
    params: &Table,
    note_value: Option<Value>,
) -> mlua::Result<Table> {
    let note = lua.create_table()?;
    if let Some(value) = note_value {
        note.set("TapNoteType", value)?;
    }
    let result = create_tap_note_result_table(lua, params, Some(&note))?;
    note.set("TapNoteResult", result.clone())?;
    note.set("HoldNoteResult", result.clone())?;
    install_tap_note_methods(lua, params, &note, result)?;
    Ok(note)
}

fn install_tap_note_methods(
    lua: &Lua,
    params: &Table,
    note: &Table,
    result: Table,
) -> mlua::Result<()> {
    let note_type = table_string_field(note, &["TapNoteType", "Type", "NoteType"])?
        .or(table_string_field(
            params,
            &["TapNoteType", "Type", "NoteType"],
        )?)
        .unwrap_or_else(|| "TapNoteType_Tap".to_string());
    let source = table_string_field(note, &["TapNoteSource", "Source"])?
        .unwrap_or_else(|| "TapNoteSource_Original".to_string());
    let subtype = table_string_field(note, &["TapNoteSubType", "SubType"])?
        .unwrap_or_else(|| "TapNoteSubType_Hold".to_string());
    let player = table_string_field(params, &["Player"])?
        .unwrap_or_else(|| player_number_name(0).to_string());
    let hold_duration = table_f32_field(note, &["HoldDuration"])?.unwrap_or(0.0);
    let attack_duration = table_f32_field(note, &["AttackDuration"])?.unwrap_or(0.0);
    let attack_mods = table_string_field(note, &["AttackModifiers"])?.unwrap_or_default();
    let keysound = table_i32_field(note, &["KeysoundIndex"])?.unwrap_or(0);

    set_string_method(lua, note, "GetTapNoteType", &note_type)?;
    set_string_method(lua, note, "GetTapNoteSource", &source)?;
    set_string_method(lua, note, "GetTapNoteSubType", &subtype)?;
    set_string_method(lua, note, "GetPlayerNumber", &player)?;
    set_string_method(lua, note, "GetAttackModifiers", &attack_mods)?;
    // Judgment objects can outlive a callback. Root their result as native
    // LuaReference does, without one auxiliary-stack slot per retained note.
    let result = std::sync::Arc::new(lua.create_registry_value(result)?);
    note.set(
        "GetTapNoteResult",
        lua.create_function({
            let result = result.clone();
            move |lua, _args: IgnoredArgs| lua.registry_value::<Table>(result.as_ref())
        })?,
    )?;
    note.set(
        "GetHoldNoteResult",
        lua.create_function({
            let result = result;
            move |lua, _args: IgnoredArgs| lua.registry_value::<Table>(result.as_ref())
        })?,
    )?;
    note.set(
        "GetHoldDuration",
        lua.create_function(move |_, _args: IgnoredArgs| Ok(hold_duration))?,
    )?;
    note.set(
        "GetAttackDuration",
        lua.create_function(move |_, _args: IgnoredArgs| Ok(attack_duration))?,
    )?;
    note.set(
        "GetKeysoundIndex",
        lua.create_function(move |_, _args: IgnoredArgs| Ok(keysound))?,
    )?;
    Ok(())
}

fn create_tap_note_result_table(
    lua: &Lua,
    params: &Table,
    note: Option<&Table>,
) -> mlua::Result<Table> {
    let result = lua.create_table()?;
    install_tap_note_result_methods(lua, params, note, &result)?;
    Ok(result)
}

fn normalize_tap_note_result_table(
    lua: &Lua,
    params: &Table,
    result: Table,
    note: Option<&Table>,
) -> mlua::Result<Table> {
    if matches!(result.get::<Value>("GetHeld")?, Value::Nil) {
        install_tap_note_result_methods(lua, params, note, &result)?;
    }
    Ok(result)
}

fn install_tap_note_result_methods(
    lua: &Lua,
    params: &Table,
    note: Option<&Table>,
    result: &Table,
) -> mlua::Result<()> {
    let held = match note {
        Some(note) => table_bool_field(note, &["Held", "held"])?,
        None => None,
    }
    .unwrap_or(false);
    let hidden = match note {
        Some(note) => table_bool_field(note, &["Hidden", "hidden"])?,
        None => None,
    }
    .unwrap_or(false);
    let offset = match note {
        Some(note) => table_f32_field(note, &["TapNoteOffset", "Offset"])?,
        None => None,
    }
    .or(table_f32_field(params, &["TapNoteOffset", "Offset"])?)
    .unwrap_or(0.0);
    let score = match note {
        Some(note) => table_string_field(note, &["TapNoteScore", "Score"])?,
        None => None,
    }
    .or(table_string_field(params, &["TapNoteScore", "Score"])?)
    .unwrap_or_else(|| "TapNoteScore_None".to_string());

    result.set(
        "GetHeld",
        lua.create_function(move |_, _args: IgnoredArgs| Ok(held))?,
    )?;
    result.set(
        "GetHidden",
        lua.create_function(move |_, _args: IgnoredArgs| Ok(hidden))?,
    )?;
    result.set(
        "GetTapNoteOffset",
        lua.create_function(move |_, _args: IgnoredArgs| Ok(offset))?,
    )?;
    set_string_method(lua, result, "GetTapNoteScore", &score)?;
    Ok(())
}

pub fn run_actor_named_command(lua: &Lua, actor: &Table, name: &str) -> mlua::Result<()> {
    run_actor_named_command_with_drain(lua, actor, name, true)
}

pub fn run_actor_named_command_with_drain(
    lua: &Lua,
    actor: &Table,
    name: &str,
    drain_queue: bool,
) -> mlua::Result<()> {
    run_actor_named_command_with_drain_and_params(lua, actor, name, drain_queue, None)
}

// Message handlers share the native direct command map. Retain their
// authored field name when a direct command executes them in the live trace.
fn lookup_actor_named_command<'a>(
    actor: &Table,
    name: &'a str,
) -> mlua::Result<Option<(Function, std::borrow::Cow<'a, str>)>> {
    if let Some(command) = actor.get::<Option<Function>>(name)? {
        return Ok(Some((command, std::borrow::Cow::Borrowed(name))));
    }
    let Some(base) = name.strip_suffix("Command").filter(|base| !base.ends_with("Message")) else {
        return Ok(None);
    };
    let field = format!("{base}MessageCommand");
    Ok(actor.get::<Option<Function>>(field.as_str())?
        .map(|command| (command, std::borrow::Cow::Owned(field))))
}

pub fn run_actor_named_command_with_drain_and_params(
    lua: &Lua,
    actor: &Table,
    name: &str,
    drain_queue: bool,
    params: Option<Value>,
) -> mlua::Result<()> {
    let Some((command, field)) = lookup_actor_named_command(actor, name)? else {
        return Ok(());
    };
    run_guarded_actor_command(lua, actor, field.as_ref(), &command, drain_queue, params)
}

fn run_actor_message_with_params(
    lua: &Lua,
    actor: &Table,
    name: &str,
    params: Option<Value>,
) -> mlua::Result<()> {
    if !live_actor_playback(lua)
        && let Some(scope) = lua.app_data_ref::<SongLuaQueuedCommand>() {
        let cursor = actor
            .get::<Option<f32>>("__songlua_capture_cursor")?
            .unwrap_or(0.0);
        if cursor < scope.start {
            flush_actor_capture(actor)?;
            actor.set("__songlua_capture_cursor", scope.start)?;
        }
    }
    run_actor_named_command_with_drain_and_params(lua, actor, name, true, params.clone())?;
    // Screen derives from ActorFrame and forwards direct commands to children.
    if actor_type_is(actor, "ActorFrame")? || actor_type_is(actor, "ActorFrameTexture")?
        || actor_type_is(actor, "TopScreen")?
    {
        for child in actor_direct_children(lua, actor)? {
            run_actor_message_with_params(lua, &child, name, params.clone())?;
        }
    }
    Ok(())
}

pub const fn actor_runs_startup_commands(actor: &Table) -> mlua::Result<bool> {
    let _ = actor;
    Ok(true)
}

pub fn call_actor_function(
    lua: &Lua,
    actor: &Table,
    command: &Function,
    params: Option<Value>,
) -> mlua::Result<()> {
    // A probe can reach a command through playcommand, a broadcast or a queue,
    // even when its closure is absent from the entry command's upvalues.
    let preserve = lua
        .app_data_mut::<SongLuaActionCaptureActive>()
        .is_some_and(|mut scope| scope.functions.insert(command.to_pointer() as usize));
    if preserve {
        let functions = lua.create_sequence_from([command.clone()])?;
        let locals = snapshot_function_locals(lua, &functions, lua.create_table()?)?;
        lua.app_data_mut::<SongLuaActionCaptureActive>()
            .expect("capture scope remains active while snapshotting")
            .locals
            .push(locals);
    }
    let call = || match params {
        Some(params) => command.call::<()>((actor, params)),
        None => command.call::<()>((actor,)),
    };
    if let Some(ActorScriptDir::Text(script_dir)) =
        actor.get::<Option<ActorScriptDir>>("__songlua_script_dir")?
    {
        let globals = lua.globals();
        let previous = globals.get::<Value>("__songlua_script_dir")?;
        // Reuse Lua's immutable string, including long paths, without creating
        // either a Rust copy or a second Lua string for the scoped directory.
        globals.set("__songlua_script_dir", script_dir)?;
        let result = call();
        globals.set("__songlua_script_dir", previous)?;
        return result;
    }
    call()
}

fn run_guarded_actor_command(
    lua: &Lua,
    actor: &Table,
    name: &str,
    command: &Function,
    drain_queue: bool,
    params: Option<Value>,
) -> mlua::Result<()> {
    let active = actor_active_commands(lua, actor)?;
    if active.get::<Option<bool>>(name)?.unwrap_or(false) {
        return Ok(());
    }
    active.set(name, true)?;
    let recurring_cursor_key = "__songlua_recurring_update_start_cursor";
    // This long key is not interned by Lua; reuse it across all scope accesses.
    let previous_recurring_state = if live_actor_playback(lua) { None } else {
        let recurring_exact_key = lua.create_string("__songlua_recurring_update_exact_interval")?;
        let previous_cursor = actor.get::<Value>(recurring_cursor_key)?;
        let previous_exact = actor.get::<Value>(&recurring_exact_key)?;
        actor.set(
            recurring_cursor_key,
            actor
                .get::<Option<f32>>("__songlua_capture_cursor")?
                .unwrap_or(0.0),
        )?;
        actor.set(&recurring_exact_key, 0.0_f64)?;
        Some((recurring_exact_key, previous_cursor, previous_exact))
    };
    #[cfg(feature = "test-support")]
    let prior_update = lua
        .app_data_mut::<SongLuaOverlayUpdateCapture>()
        .map(|mut capture| std::mem::replace(&mut capture.update_command, name == "UpdateCommand"));
    // Legacy capture tools wrap declaration functions only. The capture's
    // source-backed observer profile can also observe real dynamic dispatch.
    // Snapshot provenance before the callback can replace its command.
    let dynamic = actor.raw_get::<Option<Table>>("__songlua_dynamic_commands")?
        .map(|commands| commands.raw_get::<Option<Function>>(name)).transpose()?.flatten();
    let observed = crate::live::observes_dynamic_commands(lua)
        || dynamic.is_none_or(|function| function.to_pointer() != command.to_pointer());
    if observed { crate::live::trace_command(lua, command, name, "begin"); }
    let result = call_actor_function(lua, actor, command, params).map_err(|err| {
        mlua::Error::external(format!(
            "{} failed for {}: {err}",
            name,
            actor_debug_label(actor)
        ))
    });
    // Actor::RunCommands reports native theme script errors and returns to the
    // message loop. SongLuaTrace wraps authored commands only; keep theme
    // diagnostics separate while preserving authored callback failures.
    let result = match result {
        Err(error) if !crate::live::observes_song_callback(command) => {
            log::warn!("Native theme callback: {error}");
            #[cfg(feature = "test-support")]
            {
                if lua.app_data_ref::<SongLuaMessageCoverage>().is_none() {
                    lua.set_app_data(SongLuaMessageCoverage::default());
                }
                let gap = format!("native capture omits theme callback errors: {} {name}",
                    command.info().source.unwrap_or_default());
                if let Some(mut coverage) = lua.app_data_mut::<SongLuaMessageCoverage>()
                    && coverage.seen.insert(gap.clone()) { coverage.pending.push(gap); }
            }
            Ok(())
        },
        result => result,
    };
    if observed { crate::live::trace_command(lua, command, name, "end"); }
    #[cfg(feature = "test-support")]
    if let Some(prior) = prior_update {
        lua.app_data_mut::<SongLuaOverlayUpdateCapture>()
            .expect("command capture remains active during dispatch")
            .update_command = prior;
    }
    let result = result.and_then(|()| {
        if drain_queue {
            drain_actor_command_queue(lua, actor)?;
        }
        Ok(())
    });
    if let Some((key, cursor, exact)) = previous_recurring_state {
        actor.set(recurring_cursor_key, cursor)?;
        actor.set(key, exact)?;
    }
    active.set(name, Value::Nil)?;
    result
}

pub fn actor_active_commands(lua: &Lua, actor: &Table) -> mlua::Result<Table> {
    if let Some(active) = actor.get::<Option<Table>>("__songlua_active_commands")? {
        return Ok(active);
    }
    let active = lua.create_table()?;
    actor.set("__songlua_active_commands", active.clone())?;
    Ok(active)
}

fn actor_has_active_command(lua: &Lua, actor: &Table) -> mlua::Result<bool> {
    for pair in actor_active_commands(lua, actor)?.pairs::<Value, Value>() {
        let (_, value) = pair?;
        if truthy(&value) {
            return Ok(true);
        }
    }
    Ok(false)
}

pub fn actor_command_queue(lua: &Lua, actor: &Table) -> mlua::Result<Table> {
    if let Some(queue) = actor.get::<Option<Table>>("__songlua_command_queue")? {
        return Ok(queue);
    }
    let queue = lua.create_table()?;
    actor.set("__songlua_command_queue", queue.clone())?;
    Ok(queue)
}

fn enqueue_actor_command(lua: &Lua, actor: &Table, name: &str) -> mlua::Result<()> {
    prepare_capture_scope_actor(lua, actor)?;
    // Commands and messages append a zero-duration state after the active tween.
    finish_overflowing_tween_queue(lua, actor)?;
    flush_actor_capture(actor)?;
    if queue_replay_command(lua, actor, name) {
        return Ok(());
    }
    record_queue_step(lua, actor, None)?;
    let queue = actor_command_queue(lua, actor)?;
    let index = queue.raw_len() + 1;
    {
        let starts = match actor.get::<Option<Table>>("__songlua_command_queue_starts")? {
            Some(starts) => starts,
            None => {
                let starts = lua.create_table()?;
                actor.set("__songlua_command_queue_starts", starts.clone())?;
                starts
            }
        };
        starts.raw_set(
            index,
            actor
                .get::<Option<f32>>("__songlua_capture_cursor")?
                .unwrap_or(0.0),
        )?;
    }
    queue.raw_set(index, name)?;
    if lua.app_data_ref::<SongLuaStartupQueues>().is_some()
        || !actor_has_active_command(lua, actor)?
    {
        drain_actor_command_queue(lua, actor)?;
    }
    Ok(())
}

pub fn drain_actor_command_queue(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    if has_tween_replay(lua, actor) {
        return Ok(());
    }
    // Queue dispatch owns startup, frame timing and reentrant command scopes;
    // keep these transitions together so callbacks cannot observe half a pop.
    let queue = actor_command_queue(lua, actor)?;
    if let Some(mut startup) = lua.app_data_mut::<SongLuaStartupQueues>() {
        if queue.raw_len() > 0
            && !startup
                .0
                .iter()
                .any(|queued| queued.to_pointer() == actor.to_pointer())
        {
            startup.0.push(actor.clone());
        }
        return Ok(());
    }
    while queue.raw_len() > 0 {
        let Some(name) = queue.raw_get::<Option<String>>(1)? else {
            break;
        };
        if lua.app_data_ref::<SongLuaCompileFrames>().is_none()
            && (lua.app_data_ref::<SongLuaQueuedStartup>().is_some()
                || actor
                    .raw_get::<Option<bool>>("__songlua_command_queue_deferred")?
                    .unwrap_or(false))
            && (name.starts_with('!')
                || actor
                    .get::<Option<Table>>("__songlua_command_queue_starts")?
                    .map(|starts| starts.raw_get::<Option<f32>>(1))
                    .transpose()?
                    .flatten()
                    .is_some_and(|start| start > 0.0))
        {
            // Even zero-duration startup messages need a positive Actor update.
            // Keep Lua globals and broadcasts unchanged until replay reaches it.
            actor.set("__songlua_command_queue_deferred", true)?;
            invalidate_compile_update_plan(lua);
            return Ok(());
        }
        let time = drain_queue_clock(lua, actor);
        if time.is_some_and(|(frame, _)| {
            lua.app_data_ref::<SongLuaCompileFrames>()
                .is_some_and(|frames| frame > frames.frame)
        }) {
            // Keep both the Lua command and its clock until Actor::UpdateInternal
            // reaches this frame. Capturing setters early must not run Lua state
            // changes, broadcasts or child commands before their actual dispatch.
            return Ok(());
        }
        if time.is_some()
            && lua
                .app_data_ref::<SongLuaCompileUpdatePhase>()
                .is_some_and(|phase| {
                    phase.active
                        && queue_actor_order(lua, actor.to_pointer() as usize) > phase.order
                })
        {
            // A broadcast can queue a command on a later sibling. Run it
            // when that actor advances, after the broadcasting body finishes
            // updating Lua state, rather than draining it inside the message.
            return Ok(());
        }
        if time.is_some() {
            flush_actor_capture(actor)?;
            let tail = lua
                .app_data_ref::<SongLuaCompileFrames>()
                .and_then(|frames| {
                    frames
                        .clocks
                        .get(&(actor.to_pointer() as usize))
                        .map(|clock| clock.steps.iter().flatten().sum::<f64>())
                })
                .unwrap_or(0.0);
            actor.set("__songlua_capture_cursor", tail as f32)?;
            if let Some(mut frames) = lua.app_data_mut::<SongLuaCompileFrames>() {
                let epoch = frames.epoch;
                if let Some(clock) = frames.clocks.get_mut(&(actor.to_pointer() as usize)) {
                    clock.dispatch = None;
                    clock.epoch = epoch;
                }
            }
        } else if let Some(steps) = actor.get::<Option<Table>>("__songlua_command_queue_steps")? {
            // Startup can dispatch zero-time commands before replay begins.
            // Consume their original tween durations with the command.
            while steps.raw_len() > 0 {
                let step: Value = steps.raw_get(1)?;
                steps.raw_remove(1_i64)?;
                if step == Value::Boolean(false) {
                    break;
                }
            }
        }
        // Shift inside Lua, avoiding a Rust handle and two Lua API crossings
        // per remaining item. Remove before dispatch so callbacks observe the
        // same queue and can append, replace, or recursively drain it.
        if queue.raw_len() == 1 {
            queue.raw_set(1, Value::Nil)?;
        } else {
            queue.raw_remove(1_i64)?;
        }
        let starts = actor.get::<Option<Table>>("__songlua_command_queue_starts")?;
        let start = starts
            .as_ref()
            .map(|starts| starts.raw_get::<Option<f32>>(1))
            .transpose()?
            .flatten()
            .unwrap_or(
                actor
                    .get::<Option<f32>>("__songlua_capture_cursor")?
                    .unwrap_or(0.0),
            );
        // Deferred commands now run at the current replay frame, rather than
        // recording another delay relative to the callback that queued them.
        let start = if time.is_some() { 0.0 } else { start };
        if let Some(starts) = starts {
            if starts.raw_len() > 1 {
                starts.raw_remove(1_i64)?;
            } else {
                starts.raw_set(1, Value::Nil)?;
            }
        }
        // Effect setters bypass the tween queue, but a queued command itself
        // begins at the current queue cursor. Preserve that command-local now.
        let immediate_start_key = "__songlua_capture_immediate_start";
        let previous_immediate_start = actor.get::<Value>(immediate_start_key)?;
        actor.set(immediate_start_key, start)?;
        let previous_start = lua
            .app_data_mut::<SongLuaQueuedStartup>()
            .map(|mut scope| std::mem::replace(&mut scope.0, start));
        let prior_queue = lua.remove_app_data::<SongLuaQueuedCommand>();
        let recurring = actor
            .get::<Option<String>>("__songlua_recurring_update_command")?
            .is_some();
        lua.set_app_data(SongLuaQueuedCommand {
            start,
            actor: actor.to_pointer() as usize,
            time,
        });
        if let Some(mut capture) = lua.app_data_mut::<SongLuaOverlayUpdateCapture>() {
            capture.queued_depth += 1;
        }
        let result = if let Some(message) = name.strip_prefix('!') {
            // Actor::UpdateTweening broadcasts directly from C++, bypassing the
            // Lua binding. Listener-authored Broadcast calls still use it.
            crate::host::broadcast_queued_message(lua, message)
        } else {
            // This entry passed the recursion check when it was enqueued. An
            // earlier dispatch with the same name may still be draining siblings;
            // allow this body while preserving ancestor guards for cyclic queues.
            let active = actor_active_commands(lua, actor)?;
            let command = ActorCommandName::new(&name, "Command");
            let previous_active = active.get::<Value>(command.as_str())?;
            active.set(command.as_str(), Value::Nil)?;
            let result = run_actor_message_with_params(lua, actor, &command, None);
            active.set(command.as_str(), previous_active)?;
            result
        };
        if let Some(mut capture) = lua.app_data_mut::<SongLuaOverlayUpdateCapture>() {
            capture.queued_depth -= 1;
        }
        lua.remove_app_data::<SongLuaQueuedCommand>();
        if let Some(prior) = prior_queue {
            lua.set_app_data(prior);
        }
        actor.set(immediate_start_key, previous_immediate_start)?;
        if let Some(previous) = previous_start {
            lua.app_data_mut::<SongLuaQueuedStartup>()
                .expect("queued startup scope remains installed during dispatch")
                .0 = previous;
        }
        result?;
        advance_capture_position(lua, actor)?;
        if !recurring
            && actor
                .get::<Option<String>>("__songlua_recurring_update_command")?
                .is_some()
            && let Some((frame, remaining)) = time
        {
            run_recurring_update(lua, actor, remaining, true)?;
            // A recurring callback replaces its queue clock. Mark that new
            // clock after dispatch so this actor cannot consume the frame's
            // delta again when its next update job is reached.
            if let Some(mut frames) = lua.app_data_mut::<SongLuaCompileFrames>() {
                if let Some(clock) = frames.clocks.get_mut(&(actor.to_pointer() as usize)) {
                    clock.recurring_frame = Some(frame);
                }
            }
        }
    }
    Ok(())
}

pub fn actor_debug_label(actor: &Table) -> String {
    let actor_type = actor
        .get::<Option<String>>("__songlua_actor_type")
        .ok()
        .flatten()
        .unwrap_or_else(|| "Actor".to_string());
    let name = actor.get::<Option<String>>("Name").ok().flatten();
    match name {
        Some(name) if !name.trim().is_empty() => format!("{actor_type} '{name}'"),
        _ => actor_type,
    }
}

pub fn install_actor_metatable(
    lua: &Lua,
    actor: &Table,
    install_methods: fn(&Lua, &Table) -> mlua::Result<()>,
) -> mlua::Result<()> {
    crate::actor_classes::bind(lua, actor)?;
    let mt = lua.create_table_with_capacity(0, 4)?;
    mt.set("__index", lua.create_function(crate::actor_classes::lookup)?)?;
    mt.set("__newindex", lua.create_function(crate::actor_classes::assign)?)?;
    mt.set(
        "__concat",
        lua.create_function(move |lua, (lhs, rhs): (Table, Table)| {
            // ActorDef.MergeTables makes a fresh definition and keeps the
            // original metatable. Bound methods must address that new table.
            let actor = clone_actor_def(lua, &lhs, install_methods, false)?;
            merge_actor_concat(lua, &actor, &rhs)?;
            actor.set_metatable(lhs.metatable())?;
            Ok(actor)
        })?,
    )?;
    mt.set(
        "__tostring",
        lua.create_function(|_, actor: Table| {
            let kind = actor
                .raw_get::<Option<String>>("__songlua_actor_type")?
                .unwrap_or_else(|| "Actor".into());
            let kind = match kind.as_str() {
                "Quad" => "Sprite",
                "WrapperState" => "ActorFrame",
                "TopScreen" => "ScreenGameplay",
                kind => kind,
            };
            // Luna::tostring_T exposes only type and opaque object identity.
            #[cfg(target_os = "windows")]
            let identity = format!("{:016X}", actor.to_pointer() as usize);
            #[cfg(not(target_os = "windows"))]
            let identity = format!("{:p}", actor.to_pointer());
            Ok(format!("{kind} ({identity})"))
        })?,
    )?;
    let _ = actor.set_metatable(Some(mt));
    Ok(())
}

/// Background.cpp loads Lua files through UpperLeft.lua, whose declaration
/// concatenates native callbacks with the authored root before actor creation.
pub(crate) fn install_background_upper_left(lua: &Lua, actor: &Table, color: [f32; 4]) -> mlua::Result<Table> {
    let factory: Function = lua.load(r#"
return function(color)
    return {
        OnCommand = function(self)
            self:diffuse(color):effectclock('music')
            if self.GetTexture then self:GetTexture():rate(self:GetParent():GetUpdateRate()) end
        end,
        GainFocusCommand = function(self) self:play() end,
        LoseFocusCommand = function(self) self:pause() end,
    }
end
"#).set_name("@theme:/BackgroundEffects/UpperLeft.lua").eval()?;
    merge_actor_concat(lua, actor, &factory.call::<Table>(color.to_vec())?)?;
    // UpperLeft.lua returns an ActorFrame owning the authored actor. The
    // texture rate callback reads that real parent's update rate.
    let wrap: Function = lua.load("return function(child)\n\n\nreturn Def.ActorFrame{child}\nend")
        .set_name("@theme:/BackgroundEffects/UpperLeft.lua").eval()?;
    wrap.call(actor.clone())
}

fn merge_actor_concat(lua: &Lua, actor: &Table, rhs: &Table) -> mlua::Result<()> {
    for pair in rhs.pairs::<Value, Value>() {
        let (key, value) = pair?;
        if matches!(
            &key,
            Value::String(text)
                if text.to_str().ok().is_some_and(|name|
                    name == "__songlua_actor_type"
                        || name == "__songlua_script_dir"
                        || name == "__songlua_song_dir"
                        || name == crate::actor_classes::METHODS_KEY)
        ) {
            continue;
        }
        let value = match (actor.raw_get::<Value>(key.clone())?, value) {
            (Value::Function(first), Value::Function(second)) => {
                // Keep this a Lua function: arbitrary user callbacks must not
                // be classified as installed C methods by actor_classes.
                let compose: Function = lua.load(
                    "return function(first, second) return function(...) first(...); return second(...) end end",
                ).eval()?;
                Value::Function(compose.call::<Function>((first, second))?)
            }
            (_, value) => value,
        };
        actor.set(key, value)?;
    }
    Ok(())
}

pub fn actor_shadow_len(lua: &Lua, actor: &Table) -> mlua::Result<[f32; 2]> {
    if !live_actor_playback(lua)
        && let Some(value) = actor_current_capture_block(lua, actor)?
            .get::<Option<Table>>("shadow_len")?.and_then(|value| table_vec2(&value))
    {
        return Ok(value);
    }
    Ok(actor
        .get::<Option<ActorVectorValue<2>>>("__songlua_state_shadow_len")?
        .and_then(|value| value.0)
        .unwrap_or([0.0, 0.0]))
}

fn capture_actor_command(
    lua: &Lua,
    actor: &Table,
    command_name: &str,
) -> Result<Vec<SongLuaOverlayCommandBlock>, String> {
    let Some(command) = actor
        .get::<Option<Function>>(command_name)
        .map_err(|err| err.to_string())?
    else {
        return Ok(Vec::new());
    };
    reset_actor_capture(lua, actor).map_err(|err| err.to_string())?;
    let params =
        default_message_command_params(lua, command_name).map_err(|err| err.to_string())?;
    run_guarded_actor_command(lua, actor, command_name, &command, true, params)
        .map_err(|err| err.to_string())?;
    flush_actor_capture(actor).map_err(|err| err.to_string())?;
    read_actor_capture_blocks(actor)
}

pub fn capture_actor_command_preserving_state(
    lua: &Lua,
    actor: &Table,
    command_name: &str,
) -> Result<Vec<SongLuaOverlayCommandBlock>, String> {
    let Some(command) = actor
        .get::<Option<Function>>(command_name)
        .map_err(|err| err.to_string())?
    else {
        return Ok(Vec::new());
    };
    let locals = lua
        .create_sequence_from([command.clone()])
        .and_then(|functions| snapshot_function_action_tables(lua, &functions))
        .map_err(|err| err.to_string())?;
    let snapshot = snapshot_actor_mutable_state(lua, actor).map_err(|err| err.to_string())?;
    let globals_snapshot = snapshot_scalar_globals(lua).map_err(|err| err.to_string())?;
    let capture_scope = begin_action_capture_scope(lua).map_err(|err| err.to_string())?;
    let captured = capture_actor_command(lua, actor, command_name);
    let touched = capture_scope_actor_tables(&capture_scope.actors).map_err(|err| err.to_string());
    let touched_snapshots =
        capture_scope_snapshots(&capture_scope.snapshots).map_err(|err| err.to_string());
    restore_action_capture_scope(lua, capture_scope).map_err(|err| err.to_string())?;
    reset_actor_capture_tables(lua, &touched?)?;
    restore_actors_semantic_state(touched_snapshots?).map_err(|err| err.to_string())?;
    restore_scalar_globals(lua, globals_snapshot).map_err(|err| err.to_string())?;
    restore_actor_mutable_state(actor, snapshot).map_err(|err| err.to_string())?;
    restore_function_action_tables(lua, locals).map_err(|err| err.to_string())?;
    captured
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SongLuaCapturedMessageCommands {
    pub commands: Vec<SongLuaOverlayMessageCommand>,
    pub sounds: Vec<(String, PathBuf)>,
    pub skipped: Vec<String>,
}

fn for_each_actor_message_command(
    actor: &Table,
    mut visit: impl FnMut(String, Function),
) -> mlua::Result<()> {
    // Filter values before borrowing/allocating names. Keep keys on Lua's stack
    // and copy only selected names into the caller's pre-execution snapshot.
    let lua = actor.weak_lua().upgrade();
    actor.for_each::<Value, Value>(|key, value| {
        let (Value::String(key), Value::Function(command)) = (key, value) else {
            return Ok(());
        };
        let Ok(Some(name)) = retained_key_text(&lua, &key, |name| name.ends_with("MessageCommand")) else {
            return Ok(());
        };
        visit(name, command);
        Ok(())
    })
}

pub fn capture_actor_message_commands(
    lua: &Lua,
    actor: &Table,
) -> Result<SongLuaCapturedMessageCommands, String> {
    if crate::live::active(lua) {
        return Ok(SongLuaCapturedMessageCommands::default());
    }
    let mut out = SongLuaCapturedMessageCommands::default();
    let mut command_names = Vec::new();
    for_each_actor_message_command(actor, |name, _| command_names.push(name))
        .map_err(|err| err.to_string())?;
    command_names.sort_unstable();

    // Capturing a command resets and restores fields on the actor table. Do not
    // mutate that table while a Lua `pairs` iterator is still traversing it:
    // Lua's next-key order is unspecified and can otherwise skip commands.
    for name in command_names {
        let message = name.trim_end_matches("MessageCommand").to_string();
        let globals = lua.globals();
        let previous_sound_calls = globals
            .get::<Value>(SONG_LUA_SOUND_CALLS_KEY)
            .map_err(|err| err.to_string())?;
        let sound_calls = lua.create_table().map_err(|err| err.to_string())?;
        globals
            .set(SONG_LUA_SOUND_CALLS_KEY, sound_calls.clone())
            .map_err(|err| err.to_string())?;
        let blocks = capture_actor_command_preserving_state(lua, actor, name.as_str());
        let sounds = read_path_table(&sound_calls);
        globals
            .set(SONG_LUA_SOUND_CALLS_KEY, previous_sound_calls)
            .map_err(|err| err.to_string())?;
        let blocks = match blocks {
            Ok(blocks) => blocks,
            Err(err) => {
                let detail = defer_actor_message(
                    lua,
                    actor,
                    &name,
                    format!("{}.{}: {err}", actor_debug_label(actor), name),
                );
                push_unique_compile_detail(&mut out.skipped, detail);
                continue;
            }
        };
        out.sounds
            .extend(sounds?.into_iter().map(|path| (message.clone(), path)));
        if !blocks.is_empty() {
            out.commands.push(SongLuaOverlayMessageCommand {
                frame_advance: 0.0,
                message,
                blocks,
            });
        }
    }
    flush_actor_capture(actor).map_err(|err| err.to_string())?;
    let startup_sound_blocks: Vec<_> = read_actor_capture_blocks(actor)?
        .into_iter()
        .filter(|block| block.delta.sound_play == Some(true))
        .collect();
    if !startup_sound_blocks.is_empty() {
        out.commands.push(SongLuaOverlayMessageCommand {
            frame_advance: 0.0,
            message: SONG_LUA_STARTUP_MESSAGE.to_string(),
            blocks: startup_sound_blocks,
        });
    }
    Ok(out)
}

pub fn actor_current_capture_block(lua: &Lua, actor: &Table) -> mlua::Result<Table> {
    debug_assert!(!live_actor_playback(lua), "live actor requested compiler capture history");
    record_probe_actor_call(lua, actor)?;
    prepare_capture_scope_actor(lua, actor)?;
    if let Some(block) = actor.get::<Option<Table>>("__songlua_capture_block")? {
        return Ok(block);
    }
    let block = lua.create_table()?;
    let start = actor
        .get::<Option<f32>>("__songlua_capture_cursor")?
        .unwrap_or(0.0);
    let duration = actor
        .get::<Option<f32>>("__songlua_capture_duration")?
        .unwrap_or(0.0)
        .max(0.0);
    block.set("start", start)?;
    block.set("duration", duration)?;
    if let Some(steps) = actor.get::<Option<Table>>("__songlua_command_queue_steps")? {
        block.set("__songlua_tween_step", steps.raw_len())?;
    }
    block.set(
        "__songlua_queued_block",
        lua.app_data_ref::<SongLuaQueuedCommand>().is_some(),
    )?;
    block.set("easing", actor.get::<Value>("__songlua_capture_easing")?)?;
    block.set("opt1", actor.get::<Value>("__songlua_capture_opt1")?)?;
    block.set("opt2", actor.get::<Value>("__songlua_capture_opt2")?)?;
    block.set("__songlua_has_changes", false)?;
    actor.set("__songlua_capture_block", block.clone())?;
    Ok(block)
}

fn actor_immediate_capture_block(lua: &Lua, actor: &Table) -> mlua::Result<Table> {
    record_probe_actor_call(lua, actor)?;
    prepare_capture_scope_actor(lua, actor)?;
    let start = lua
        .app_data_ref::<SongLuaQueuedStartup>()
        .map(|scope| scope.0)
        .or_else(|| {
            lua.app_data_ref::<SongLuaQueuedCommand>()
                .map(|scope| scope.start)
        })
        .or(actor.get::<Option<f32>>("__songlua_capture_immediate_start")?)
        .unwrap_or(0.0);
    // Direct setters share the initial immediate state, as native Actor does.
    // Once a timed state is queued, immediate properties use their own block.
    if start == 0.0
        && actor
            .get::<Option<f32>>("__songlua_capture_cursor")?
            .unwrap_or(0.0)
            == 0.0
        && actor
            .get::<Option<f32>>("__songlua_capture_duration")?
            .unwrap_or(0.0)
            == 0.0
        && actor
            .get::<Option<Table>>("__songlua_command_queue_steps")?
            .is_none_or(|steps| steps.raw_len() == 0)
    {
        return actor_current_capture_block(lua, actor);
    }
    if let Some(block) = actor.get::<Option<Table>>("__songlua_capture_immediate_block")?
        && (block.get::<f32>("start")? - start).abs() <= f32::EPSILON
    {
        return Ok(block);
    }
    let block = lua.create_table()?;
    block.set("start", start)?;
    block.set("duration", 0.0_f32)?;
    block.set(
        "__songlua_queued_block",
        lua.app_data_ref::<SongLuaQueuedCommand>().is_some(),
    )?;
    block.set("easing", Value::Nil)?;
    block.set("opt1", Value::Nil)?;
    block.set("opt2", Value::Nil)?;
    block.set("__songlua_has_changes", false)?;
    let blocks: Table = actor.get("__songlua_capture_blocks")?;
    // A direct effect setter must be evaluated before an in-flight tween that
    // starts at the same time or overlay_state_after_blocks would return first.
    let insert_at = (1..=blocks.raw_len())
        .find(|&index| {
            blocks
                .raw_get::<Table>(index)
                .and_then(|existing| existing.get::<f32>("start"))
                .is_ok_and(|existing_start| existing_start >= start)
        })
        .unwrap_or_else(|| blocks.raw_len() + 1);
    for index in (insert_at..=blocks.raw_len()).rev() {
        let value = blocks.raw_get::<Value>(index)?;
        blocks.raw_set(index + 1, value)?;
    }
    blocks.raw_set(insert_at, block.clone())?;
    actor.set("__songlua_capture_immediate_block", block.clone())?;
    Ok(block)
}

fn capture_immediate_bool(lua: &Lua, actor: &Table, key: &str, value: bool) -> mlua::Result<()> {
    if !record_overlay_update_capture_immediate(
        lua,
        actor,
        key,
        SongLuaOverlayUpdateValue::Bool(value),
    ) {
        let block = actor_immediate_capture_block(lua, actor)?;
        block.set(key, value)?;
        block.set("__songlua_has_changes", true)?;
    }
    set_actor_capture_state(actor, key, value)
}

fn capture_immediate_f32(lua: &Lua, actor: &Table, key: &str, value: f32) -> mlua::Result<()> {
    if set_live_base_field(lua, actor, key, value) { return Ok(()); }
    if !record_overlay_update_capture_immediate(
        lua,
        actor,
        key,
        SongLuaOverlayUpdateValue::F32(value),
    ) {
        let block = actor_immediate_capture_block(lua, actor)?;
        block.set(key, value)?;
        block.set("__songlua_has_changes", true)?;
    }
    set_actor_capture_state(actor, key, value)
}

fn capture_immediate_u32(lua: &Lua, actor: &Table, key: &str, value: u32) -> mlua::Result<()> {
    if !record_overlay_update_capture_immediate(
        lua,
        actor,
        key,
        SongLuaOverlayUpdateValue::U32(value),
    ) {
        let block = actor_immediate_capture_block(lua, actor)?;
        block.set(key, value)?;
        block.set("__songlua_has_changes", true)?;
    }
    set_actor_capture_state(actor, key, value)
}

fn capture_immediate_vec2(lua: &Lua, actor: &Table, key: &str, size: [f32; 2]) -> mlua::Result<()> {
    if live_actor_playback(lua) {
        return crate::native_vector::store(lua, actor, actor_capture_state_key(key).as_ref(), size);
    }
    let table = lua.create_table_from(
        size.into_iter()
            .enumerate()
            .map(|(i, value)| (i + 1, value)),
    )?;
    if !record_overlay_update_capture_immediate(
        lua,
        actor,
        key,
        SongLuaOverlayUpdateValue::Vec2(size),
    ) {
        let block = actor_immediate_capture_block(lua, actor)?;
        block.set(key, table.clone())?;
        block.set("__songlua_has_changes", true)?;
    }
    set_actor_capture_state(actor, key, table)
}

fn capture_immediate_vec3(
    lua: &Lua,
    actor: &Table,
    key: &str,
    value: [f32; 3],
) -> mlua::Result<()> {
    if live_actor_playback(lua) {
        return crate::native_vector::store(lua, actor, actor_capture_state_key(key).as_ref(), value);
    }
    let table = lua.create_table_with_capacity(3, 0)?;
    for (index, component) in value.into_iter().enumerate() {
        table.raw_set(index + 1, component)?;
    }
    if !record_overlay_update_capture_immediate(
        lua,
        actor,
        key,
        SongLuaOverlayUpdateValue::Vec3(value),
    ) {
        let block = actor_immediate_capture_block(lua, actor)?;
        block.set(key, table.clone())?;
        block.set("__songlua_has_changes", true)?;
    }
    set_actor_capture_state(actor, key, table)
}

fn capture_immediate_vec4(
    lua: &Lua,
    actor: &Table,
    key: &str,
    value: [f32; 4],
) -> mlua::Result<()> {
    if live_actor_playback(lua) {
        return crate::native_vector::store(lua, actor, actor_capture_state_key(key).as_ref(), value);
    }
    let table = lua.create_table_with_capacity(4, 0)?;
    for (index, component) in value.into_iter().enumerate() {
        table.raw_set(index + 1, component)?;
    }
    if !record_overlay_update_capture_immediate(
        lua,
        actor,
        key,
        SongLuaOverlayUpdateValue::Vec4(value),
    ) {
        let block = actor_immediate_capture_block(lua, actor)?;
        block.set(key, table.clone())?;
        block.set("__songlua_has_changes", true)?;
    }
    set_actor_capture_state(actor, key, table)
}

fn capture_immediate_vec5(
    lua: &Lua,
    actor: &Table,
    key: &str,
    value: [f32; 5],
) -> mlua::Result<()> {
    if live_actor_playback(lua) {
        return crate::native_vector::store(lua, actor, actor_capture_state_key(key).as_ref(), value);
    }
    let table = lua.create_table_with_capacity(5, 0)?;
    for (index, component) in value.into_iter().enumerate() {
        table.raw_set(index + 1, component)?;
    }
    if !record_overlay_update_capture_immediate(
        lua,
        actor,
        key,
        SongLuaOverlayUpdateValue::Vec5(value),
    ) {
        let block = actor_immediate_capture_block(lua, actor)?;
        block.set(key, table.clone())?;
        block.set("__songlua_has_changes", true)?;
    }
    set_actor_capture_state(actor, key, table)
}

fn capture_immediate_string(lua: &Lua, actor: &Table, key: &str, value: &str) -> mlua::Result<()> {
    let update = capture_target_for_key(key).and_then(|target| match target {
        SongLuaOverlayUpdateTarget::EffectClock => {
            parse_overlay_effect_clock(value).map(SongLuaOverlayUpdateValue::EffectClock)
        }
        SongLuaOverlayUpdateTarget::EffectMode => {
            parse_overlay_effect_mode(value).map(SongLuaOverlayUpdateValue::EffectMode)
        }
        _ => None,
    });
    if !live_actor_playback(lua)
        && !update.is_some_and(|value| record_overlay_update_capture_immediate(lua, actor, key, value))
    {
        let block = actor_immediate_capture_block(lua, actor)?;
        block.set(key, value)?;
        block.set("__songlua_has_changes", true)?;
    }
    set_actor_capture_state(actor, key, value)
}

pub fn capture_block_set_f32(lua: &Lua, actor: &Table, key: &str, value: f32) -> mlua::Result<()> {
    if set_live_base_field(lua, actor, key, value) { return Ok(()); }
    if live_destination_scalar(lua, actor, key).is_some() {
        record_overlay_update_capture(lua, actor, key, SongLuaOverlayUpdateValue::F32(value));
        return Ok(());
    }
    if let Some(axis) = ["rot_x_deg", "rot_y_deg", "rot_z_deg"]
        .iter()
        .position(|axis| *axis == key)
    {
        prepare_capture_scope_actor(lua, actor)?;
        if let Some(pose) = actor.raw_get::<Option<Table>>("__songlua_state_spin")? {
            let tail: usize = pose.get("tail")?;
            let rotation: Table = if tail == 0 {
                pose.get("rotation")?
            } else {
                pose.get::<Table>("queue")?
                    .raw_get::<Table>(tail)?
                    .get("to")?
            };
            rotation.raw_set(axis + 1, value)?;
        }
    }
    let current_pos_key = match key {
        "aux" => Some("__songlua_aux"),
        "x" => Some("__songlua_current_x"),
        "y" => Some("__songlua_current_y"),
        "z" => Some("__songlua_current_z"),
        _ => None,
    };
    if current_pos_key.is_some() && live_current_pose(lua, actor).is_some() {
        // Native current state is authoritative during live playback. The
        // setter changes the queue tail; GetX/Y/Z/getaux read current directly.
        // Keep the destination field for destination getters and the offline
        // presentation adapter, without mirroring current back into Lua.
        record_overlay_update_capture(lua, actor, key, SongLuaOverlayUpdateValue::F32(value));
        return set_actor_capture_state(actor, key, value);
    }
    let prior_pos = current_pos_key
        .map(|_| actor.get::<Option<f32>>(actor_capture_state_key(key).as_ref()))
        .transpose()?
        .flatten()
        .unwrap_or(0.0);
    let scheduled = current_pos_key.is_some() && (actor
        .get::<Option<f32>>("__songlua_capture_cursor")?
        .unwrap_or(0.0)
        .max(0.0)
        > f32::EPSILON
        || actor
            .get::<Option<f32>>("__songlua_capture_duration")?
            .unwrap_or(0.0)
            .max(0.0)
            > f32::EPSILON
        || queued_zero_progress(lua, actor).is_some());
    if !record_overlay_update_capture(lua, actor, key, SongLuaOverlayUpdateValue::F32(value)) {
        let block = actor_current_capture_block(lua, actor)?;
        block.set(key, value)?;
        block.set("__songlua_has_changes", true)?;
    }
    set_actor_capture_state(actor, key, value)?;
    if let Some(current_key) = current_pos_key {
        if scheduled {
            if actor.get::<Option<f32>>(current_key)?.is_none() {
                actor.set(current_key, prior_pos)?;
            }
        } else {
            actor.set(current_key, value)?;
            if key != "aux"
                && let Some(mut capture) = lua.app_data_mut::<SongLuaOverlayUpdateCapture>()
            {
                if let Some(&index) = capture.actor_indices.get(&(actor.to_pointer() as usize)) {
                    if let Some(position) = &mut capture.prior_positions[index] {
                        position[match key {
                            "x" => 0,
                            "y" => 1,
                            _ => 2,
                        }] = value;
                    }
                }
            }
        }
    }
    // This setter already published the one changed destination field. Other
    // destination fields and color tables are unchanged. Only current-position
    // getters need reconciliation when the authored write targets a queue tail.
    if let Some(current_key) = current_pos_key {
        let current = lua.app_data_ref::<ActorTweenReplays>()
            .and_then(|replays| replays.0.get(&(actor.to_pointer() as usize)).map(|replay| replay.current));
        if let Some(current) = current {
            let value = match key { "aux" => current.aux, "x" => current.x, "y" => current.y, _ => current.z };
            actor.raw_set(current_key, value)?;
        }
    }
    Ok(())
}

pub fn capture_block_set_bool(
    lua: &Lua,
    actor: &Table,
    key: &str,
    value: bool,
) -> mlua::Result<()> {
    if set_live_render_flag(lua, actor, key, value) { return Ok(()); }
    if key == "visible"
        && (lua.app_data_ref::<SongLuaStartupQueues>().is_some()
            || lua.app_data_ref::<SongLuaQueuedStartup>().is_some()
            || lua.app_data_ref::<SongLuaQueuedCommand>().is_some()
            || lua.app_data_ref::<SongLuaCompileFrames>().is_some())
    {
        return capture_immediate_bool(lua, actor, key, value);
    }
    if !record_overlay_update_capture(lua, actor, key, SongLuaOverlayUpdateValue::Bool(value)) {
        let block = actor_current_capture_block(lua, actor)?;
        block.set(key, value)?;
        block.set("__songlua_has_changes", true)?;
    }
    set_actor_capture_state(actor, key, value)?;
    Ok(())
}

fn actor_capture_state_key(key: &str) -> std::borrow::Cow<'static, str> {
    let state_key = match key {
        "aux" => "__songlua_state_aux",
        "hibernating" => "__songlua_state_hibernating",
        "x" => "__songlua_state_x",
        "y" => "__songlua_state_y",
        "z" => "__songlua_state_z",
        "z_bias" => "__songlua_state_z_bias",
        "draw_order" => "__songlua_state_draw_order",
        "draw_by_z_position" => "__songlua_state_draw_by_z_position",
        "aft_preserve" => "__songlua_state_aft_preserve",
        "aft_created" => "__songlua_state_aft_created",
        "halign" => "__songlua_state_halign",
        "valign" => "__songlua_state_valign",
        "uppercase" => "__songlua_state_uppercase",
        "visible" => "__songlua_state_visible",
        "cropleft" => "__songlua_state_cropleft",
        "cropright" => "__songlua_state_cropright",
        "croptop" => "__songlua_state_croptop",
        "cropbottom" => "__songlua_state_cropbottom",
        "fadeleft" => "__songlua_state_fadeleft",
        "faderight" => "__songlua_state_faderight",
        "fadetop" => "__songlua_state_fadetop",
        "fadebottom" => "__songlua_state_fadebottom",
        "mask_source" => "__songlua_state_mask_source",
        "mask_dest" => "__songlua_state_mask_dest",
        "depth_test" => "__songlua_state_depth_test",
        "depth_write" => "__songlua_state_depth_write",
        "depth_test_on_fail" => "__songlua_state_depth_test_on_fail",
        "cull_mode" => "__songlua_state_cull_mode",
        "zoom" => "__songlua_state_zoom",
        "zoom_x" => "__songlua_state_zoom_x",
        "zoom_y" => "__songlua_state_zoom_y",
        "zoom_z" => "__songlua_state_zoom_z",
        "basezoom" => "__songlua_state_basezoom",
        "basezoom_x" => "__songlua_state_basezoom_x",
        "basezoom_y" => "__songlua_state_basezoom_y",
        "basezoom_z" => "__songlua_state_basezoom_z",
        "base_rotation" => "__songlua_state_base_rotation",
        "rot_x_deg" => "__songlua_state_rot_x_deg",
        "rot_y_deg" => "__songlua_state_rot_y_deg",
        "rot_z_deg" => "__songlua_state_rot_z_deg",
        "skew_x" => "__songlua_state_skew_x",
        "skew_y" => "__songlua_state_skew_y",
        "vibrate" => "__songlua_state_vibrate",
        "effect_period" => "__songlua_state_effect_period",
        "effect_offset" => "__songlua_state_effect_offset",
        "rainbow" => "__songlua_state_rainbow",
        "rainbow_scroll" => "__songlua_state_rainbow_scroll",
        "text_jitter" => "__songlua_state_text_jitter",
        "text_distortion" => "__songlua_state_text_distortion",
        "mult_attrs_with_diffuse" => "__songlua_state_mult_attrs_with_diffuse",
        "sprite_texture" => "__songlua_state_sprite_texture",
        "sprite_animate" => "__songlua_state_sprite_animate",
        "sprite_loop" => "__songlua_state_sprite_loop",
        "sprite_playback_rate" => "__songlua_state_sprite_playback_rate",
        "sprite_state_delay" => "__songlua_state_sprite_state_delay",
        "max_w_pre_zoom" => "__songlua_state_max_w_pre_zoom",
        "max_h_pre_zoom" => "__songlua_state_max_h_pre_zoom",
        "max_dimension_uses_zoom" => "__songlua_state_max_dimension_uses_zoom",
        "texture_filtering" => "__songlua_state_texture_filtering",
        "texture_wrapping" => "__songlua_state_texture_wrapping",
        "banner_scroll_percent" => "__songlua_state_banner_scroll_percent",
        "banner_scrolling" => "__songlua_state_banner_scrolling",
        "blend" => "__songlua_state_blend",
        "custom_texture_rect" => "__songlua_state_custom_texture_rect",
        "decode_movie" => "__songlua_state_decode_movie",
        "diffuse" => "__songlua_state_diffuse",
        "effect_clock" => "__songlua_state_effect_clock",
        "effect_timer" => "__songlua_state_effect_timer",
        "effect_color1" => "__songlua_state_effect_color1",
        "effect_color2" => "__songlua_state_effect_color2",
        "effect_magnitude" => "__songlua_state_effect_magnitude",
        "effect_mode" => "__songlua_state_effect_mode",
        "effect_timing" => "__songlua_state_effect_timing",
        "fov" => "__songlua_state_fov",
        "glow" => "__songlua_state_glow",
        "max_height" => "__songlua_state_max_height",
        "max_width" => "__songlua_state_max_width",
        "shadow_color" => "__songlua_state_shadow_color",
        "shadow_len" => "__songlua_state_shadow_len",
        "size" => "__songlua_state_size",
        "sprite_state_index" => "__songlua_state_sprite_state_index",
        "stretch_rect" => "__songlua_state_stretch_rect",
        "texcoord_offset" => "__songlua_state_texcoord_offset",
        "texcoord_velocity" => "__songlua_state_texcoord_velocity",
        "text_align" => "__songlua_state_text_align",
        "text_glow_mode" => "__songlua_state_text_glow_mode",
        "vanishpoint" => "__songlua_state_vanishpoint",
        "vert_spacing" => "__songlua_state_vert_spacing",
        "vertex_colors" => "__songlua_state_vertex_colors",
        "wrap_width_pixels" => "__songlua_state_wrap_width_pixels",
        _ => return std::borrow::Cow::Owned(format!("__songlua_state_{key}")),
    };
    std::borrow::Cow::Borrowed(state_key)
}

fn set_actor_capture_state(
    actor: &Table,
    key: &str,
    value: impl mlua::IntoLua,
) -> mlua::Result<()> {
    actor.set(actor_capture_state_key(key).as_ref(), value)
}

pub fn capture_block_set_color(lua: &Lua, actor: &Table, color: [f32; 4]) -> mlua::Result<()> {
    if live_actor_playback(lua) && has_tween_replay(lua, actor) {
        record_overlay_update_capture(lua, actor, "diffuse", SongLuaOverlayUpdateValue::Vec4(color));
        return Ok(());
    }
    let value = lua.create_table_with_capacity(4, 0)?;
    value.raw_set(1, color[0])?;
    value.raw_set(2, color[1])?;
    value.raw_set(3, color[2])?;
    value.raw_set(4, color[3])?;
    if !record_overlay_update_capture(
        lua,
        actor,
        "diffuse",
        SongLuaOverlayUpdateValue::Vec4(color),
    ) {
        let block = actor_current_capture_block(lua, actor)?;
        block.set("diffuse", value.clone())?;
        block.set("__songlua_has_changes", true)?;
    }
    actor.set("__songlua_diffuse", value.clone())?;
    actor.set("__songlua_state_diffuse", value)?;
    sync_tween_getters(lua, actor)
}

pub fn capture_actor_vertex_diffuse(
    lua: &Lua,
    actor: &Table,
    args: &MultiValue,
    corner_mask: u8,
) -> mlua::Result<()> {
    let Some(color) = read_color_args(args) else {
        return Ok(());
    };
    if write_live_vertex_colors(lua, actor, |colors| {
        for (index, vertex_color) in colors.iter_mut().enumerate() {
            if corner_mask & (1 << index) != 0 { *vertex_color = color; }
        }
    }) {
        return Ok(());
    }
    let mut colors = actor_vertex_colors(lua, actor)?;
    for (index, vertex_color) in colors.iter_mut().enumerate() {
        if corner_mask & (1 << index) != 0 {
            *vertex_color = color;
        }
    }
    capture_block_set_vertex_colors(lua, actor, colors)
}

fn actor_text_attributes_table(lua: &Lua, actor: &Table) -> mlua::Result<Table> {
    if let Some(attributes) = actor.get::<Option<Table>>("__songlua_text_attributes")? {
        return Ok(attributes);
    }
    let attributes = lua.create_table()?;
    actor.set("__songlua_text_attributes", attributes.clone())?;
    Ok(attributes)
}

pub fn capture_actor_text_attribute(
    lua: &Lua,
    actor: &Table,
    args: &MultiValue,
) -> mlua::Result<()> {
    let Some(start) = method_arg(args, 0).cloned().and_then(read_i32_value) else {
        return Ok(());
    };
    let Some(Value::Table(params)) = method_arg(args, 1).cloned() else {
        return Ok(());
    };
    let length = text_attribute_value(&params, &["Length", "length"])?
        .and_then(read_i32_value)
        .unwrap_or(1);
    if length == 0 {
        return Ok(());
    }
    let mut vertex_colors = text_attribute_value(&params, &["Diffuses", "diffuses"])?
        .and_then(read_vertex_colors_value)
        .unwrap_or([[1.0, 1.0, 1.0, 1.0]; 4]);
    if let Some(color) = text_attribute_value(
        &params,
        &[
            "Diffuse",
            "diffuse",
            "DiffuseColor",
            "diffusecolor",
            "Color",
            "color",
        ],
    )?
    .and_then(read_color_value)
    {
        vertex_colors = [color; 4];
    }
    let color = vertex_colors[0];
    let glow = text_attribute_value(&params, &["Glow", "glow"])?.and_then(read_color_value);

    let attr = lua.create_table()?;
    attr.raw_set("start", start.max(0))?;
    attr.raw_set("length", length)?;
    attr.raw_set("color", make_color_table(lua, color)?)?;
    if vertex_colors != [color; 4] {
        attr.raw_set(
            "vertex_colors",
            make_vertex_color_table(lua, vertex_colors)?,
        )?;
    }
    if let Some(glow) = glow {
        attr.raw_set("glow", make_color_table(lua, glow)?)?;
    }
    let attributes = actor_text_attributes_table(lua, actor)?;
    for existing in attributes.sequence_values::<Table>() {
        if text_attribute_matches(&existing?, start.max(0), length, color, vertex_colors, glow)? {
            return Ok(());
        }
    }
    attributes.raw_set(attributes.raw_len() + 1, attr)?;
    Ok(())
}

pub fn capture_block_set_vec4(
    lua: &Lua,
    actor: &Table,
    key: &str,
    value4: [f32; 4],
) -> mlua::Result<()> {
    if key == "glow" && live_actor_playback(lua) && has_tween_replay(lua, actor) {
        record_overlay_update_capture(lua, actor, key, SongLuaOverlayUpdateValue::Vec4(value4));
        return Ok(());
    }
    let value = lua.create_table_with_capacity(4, 0)?;
    value.raw_set(1, value4[0])?;
    value.raw_set(2, value4[1])?;
    value.raw_set(3, value4[2])?;
    value.raw_set(4, value4[3])?;
    if !record_overlay_update_capture(lua, actor, key, SongLuaOverlayUpdateValue::Vec4(value4)) {
        let block = actor_current_capture_block(lua, actor)?;
        block.set(key, value.clone())?;
        block.set("__songlua_has_changes", true)?;
    }
    set_actor_capture_state(actor, key, value)?;
    Ok(())
}

pub fn capture_block_set_vertex_colors(
    lua: &Lua,
    actor: &Table,
    colors: [[f32; 4]; 4],
) -> mlua::Result<()> {
    if write_live_vertex_colors(lua, actor, |dest| *dest = colors) {
        return Ok(());
    }
    let value = make_vertex_color_table(lua, colors)?;
    if !record_overlay_update_capture(
        lua,
        actor,
        "vertex_colors",
        SongLuaOverlayUpdateValue::VertexColors(Arc::new(colors)),
    ) {
        let block = actor_current_capture_block(lua, actor)?;
        block.set("vertex_colors", value.clone())?;
        block.set("__songlua_has_changes", true)?;
    }
    actor.set("__songlua_state_vertex_colors", value)?;
    Ok(())
}

// Actor::SetDiffuses and its corner/edge variants mutate the back tween state.
// Live playback owns that fixed-size array already; only capture observers need
// an independently owned update value.
fn write_live_vertex_colors(
    lua: &Lua,
    actor: &Table,
    write: impl FnOnce(&mut [[f32; 4]; 4]),
) -> bool {
    if !live_actor_playback(lua) { return false; }
    let Some(mut replays) = lua.app_data_mut::<ActorTweenReplays>() else { return false; };
    let Some(replay) = replays.0.get_mut(&(actor.to_pointer() as usize)) else { return false; };
    let target = SongLuaOverlayUpdateTarget::VertexColors;
    replay.targets |= 1u128 << target as usize;
    #[cfg(feature = "test-support")]
    let queued = !replay.queue.is_empty();
    let dest = replay.queue.back_mut().map_or(&mut replay.current, |step| &mut step.to);
    // Preserve actor_vertex_colors' neutral fallback. Overlay diffuse is stored
    // separately from these optional corner colors.
    let colors = dest.vertex_colors.get_or_insert([[1.0; 4]; 4]);
    write(colors);
    #[cfg(feature = "test-support")]
    let colors = *colors;
    drop(replays);
    #[cfg(feature = "test-support")]
    observe_tween_write(lua, actor, target, queued,
        || SongLuaOverlayUpdateValue::VertexColors(Arc::new(colors)));
    true
}

pub fn capture_block_set_vec5(
    lua: &Lua,
    actor: &Table,
    key: &str,
    value5: [f32; 5],
) -> mlua::Result<()> {
    let value = lua.create_table_with_capacity(5, 0)?;
    value.raw_set(1, value5[0])?;
    value.raw_set(2, value5[1])?;
    value.raw_set(3, value5[2])?;
    value.raw_set(4, value5[3])?;
    value.raw_set(5, value5[4])?;
    if !record_overlay_update_capture(lua, actor, key, SongLuaOverlayUpdateValue::Vec5(value5)) {
        let block = actor_current_capture_block(lua, actor)?;
        block.set(key, value.clone())?;
        block.set("__songlua_has_changes", true)?;
    }
    set_actor_capture_state(actor, key, value)?;
    Ok(())
}

pub fn capture_block_set_u32(lua: &Lua, actor: &Table, key: &str, value: u32) -> mlua::Result<()> {
    if !record_overlay_update_capture(lua, actor, key, SongLuaOverlayUpdateValue::U32(value)) {
        let block = actor_current_capture_block(lua, actor)?;
        block.set(key, value)?;
        block.set("__songlua_has_changes", true)?;
    }
    set_actor_capture_state(actor, key, value)?;
    Ok(())
}

pub fn capture_block_set_i32(lua: &Lua, actor: &Table, key: &str, value: i32) -> mlua::Result<()> {
    if !record_overlay_update_capture(lua, actor, key, SongLuaOverlayUpdateValue::I32(value)) {
        let block = actor_current_capture_block(lua, actor)?;
        block.set(key, value)?;
        block.set("__songlua_has_changes", true)?;
    }
    set_actor_capture_state(actor, key, value)?;
    Ok(())
}

pub fn capture_block_set_vec2(
    lua: &Lua,
    actor: &Table,
    key: &str,
    value2: [f32; 2],
) -> mlua::Result<()> {
    let value = lua.create_table_with_capacity(2, 0)?;
    value.raw_set(1, value2[0])?;
    value.raw_set(2, value2[1])?;
    if !record_overlay_update_capture(lua, actor, key, SongLuaOverlayUpdateValue::Vec2(value2)) {
        let block = actor_current_capture_block(lua, actor)?;
        block.set(key, value.clone())?;
        block.set("__songlua_has_changes", true)?;
    }
    set_actor_capture_state(actor, key, value)?;
    Ok(())
}

pub fn capture_block_set_stretch(lua: &Lua, actor: &Table, rect: [f32; 4]) -> mlua::Result<()> {
    // Actor::StretchTo keeps m_size and sets the destination scale on each axis.
    let (base_width, base_height) = actor_base_size(actor)?;
    let width = rect[2] - rect[0];
    let height = rect[3] - rect[1];
    capture_block_set_f32(lua, actor, "x", rect[0] + width / 2.0)?;
    capture_block_set_f32(lua, actor, "y", rect[1] + height / 2.0)?;
    capture_block_set_f32(lua, actor, "zoom_x", width / base_width)?;
    capture_block_set_f32(lua, actor, "zoom_y", height / base_height)
}

pub fn capture_block_set_size(lua: &Lua, actor: &Table, size: [f32; 2]) -> mlua::Result<()> {
    if live_actor_playback(lua) {
        return crate::native_vector::store(lua, actor, "__songlua_state_size", size);
    }
    let value = lua.create_table_with_capacity(2, 0)?;
    value.raw_set(1, size[0])?;
    value.raw_set(2, size[1])?;
    if !record_overlay_update_capture(lua, actor, "size", SongLuaOverlayUpdateValue::Vec2(size)) {
        let block = actor_current_capture_block(lua, actor)?;
        block.set("size", value.clone())?;
        block.set("__songlua_has_changes", true)?;
    }
    actor.set("__songlua_state_size", value)?;
    Ok(())
}

pub fn capture_block_set_zoom_axes(
    lua: &Lua,
    actor: &Table,
    zoom: f32,
    zoom_x_key: &str,
    zoom_y_key: &str,
    zoom_z_key: &str,
) -> mlua::Result<()> {
    capture_block_set_f32(lua, actor, zoom_x_key, zoom)?;
    capture_block_set_f32(lua, actor, zoom_y_key, zoom)?;
    capture_block_set_f32(lua, actor, zoom_z_key, zoom)?;
    Ok(())
}

pub fn capture_block_set_vec3(
    lua: &Lua,
    actor: &Table,
    key: &str,
    value3: [f32; 3],
) -> mlua::Result<()> {
    let value = lua.create_table_with_capacity(3, 0)?;
    value.raw_set(1, value3[0])?;
    value.raw_set(2, value3[1])?;
    value.raw_set(3, value3[2])?;
    if !record_overlay_update_capture(lua, actor, key, SongLuaOverlayUpdateValue::Vec3(value3)) {
        let block = actor_current_capture_block(lua, actor)?;
        block.set(key, value.clone())?;
        block.set("__songlua_has_changes", true)?;
    }
    set_actor_capture_state(actor, key, value)?;
    Ok(())
}

pub fn capture_block_set_string(
    lua: &Lua,
    actor: &Table,
    key: &str,
    value: &str,
) -> mlua::Result<()> {
    let update_value = capture_target_for_key(key).and_then(|target| match target {
        SongLuaOverlayUpdateTarget::TextAlign => {
            parse_overlay_text_align(value).map(SongLuaOverlayUpdateValue::TextAlign)
        }
        SongLuaOverlayUpdateTarget::Blend => {
            parse_overlay_blend_mode(value).map(SongLuaOverlayUpdateValue::Blend)
        }
        SongLuaOverlayUpdateTarget::EffectClock => {
            parse_overlay_effect_clock(value).map(SongLuaOverlayUpdateValue::EffectClock)
        }
        SongLuaOverlayUpdateTarget::EffectMode => {
            parse_overlay_effect_mode(value).map(SongLuaOverlayUpdateValue::EffectMode)
        }
        SongLuaOverlayUpdateTarget::TextGlowMode => {
            parse_overlay_text_glow_mode(value).map(SongLuaOverlayUpdateValue::TextGlowMode)
        }
        _ => None,
    });
    if !live_actor_playback(lua)
        && !update_value.is_some_and(|value| record_overlay_update_capture(lua, actor, key, value)) {
        let block = actor_current_capture_block(lua, actor)?;
        block.set(key, value)?;
        block.set("__songlua_has_changes", true)?;
    }
    set_actor_capture_state(actor, key, value)?;
    Ok(())
}

pub fn capture_texture_rect(lua: &Lua, actor: &Table, rect: [f32; 4]) -> mlua::Result<()> {
    capture_block_set_u32(
        lua,
        actor,
        "sprite_state_index",
        SONG_LUA_SPRITE_STATE_CLEAR,
    )?;
    capture_block_set_vec4(lua, actor, "custom_texture_rect", rect)
}

pub fn actor_halign(lua: &Lua, actor: &Table) -> mlua::Result<f32> {
    if let Some(base) = live_base_transform(lua, actor) { return Ok(base.get(BaseField::HAlign).unwrap_or(0.5)); }
    Ok(actor
        .get::<Option<f32>>("__songlua_state_halign")?
        .unwrap_or(0.5))
}

pub fn actor_valign(lua: &Lua, actor: &Table) -> mlua::Result<f32> {
    if let Some(base) = live_base_transform(lua, actor) { return Ok(base.get(BaseField::VAlign).unwrap_or(0.5)); }
    Ok(actor
        .get::<Option<f32>>("__songlua_state_valign")?
        .unwrap_or(0.5))
}

pub fn set_actor_sprite_state(lua: &Lua, actor: &Table, state_index: u32) -> mlua::Result<()> {
    crate::model_texture::set_state(lua, actor, state_index.min(i32::MAX as u32) as i32);
    // Sprite::SetState changes animation state immediately, outside Actor's
    // tween queue. A same-command state selection must follow Load's reset.
    capture_immediate_u32(lua, actor, "sprite_state_index", state_index)?;
    if !live_actor_playback(lua) {
        let block = actor_current_capture_block(lua, actor)?;
        block.set("custom_texture_rect", Value::Nil)?;
        block.set("__songlua_has_changes", true)?;
    }
    actor.set("__songlua_state_custom_texture_rect", Value::Nil)?;
    Ok(())
}

pub fn set_actor_effect_mode(lua: &Lua, actor: &Table, mode: &str) -> mlua::Result<()> {
    // Actor owns one effect selector; the separate render flags are aliases.
    capture_immediate_bool(lua, actor, "vibrate", mode == "vibrate")?;
    capture_immediate_bool(lua, actor, "rainbow", mode == "rainbow")?;
    capture_immediate_string(
        lua,
        actor,
        "effect_mode",
        if matches!(mode, "vibrate" | "rainbow") {
            "none"
        } else {
            mode
        },
    )
}

fn capture_effect_period(lua: &Lua, actor: &Table, period: f32) -> mlua::Result<()> {
    capture_immediate_f32(lua, actor, "effect_period", period)?;
    if actor
        .raw_get::<Option<ActorVectorValue<5>>>("__songlua_state_effect_timing")?
        .is_some()
    {
        capture_immediate_vec5(
            lua,
            actor,
            "effect_timing",
            [period / 2.0, 0.0, period / 2.0, 0.0, 0.0],
        )?;
    }
    Ok(())
}

pub fn set_actor_effect_defaults(
    lua: &Lua,
    actor: &Table,
    mode: &str,
    period: Option<f32>,
    magnitude: Option<[f32; 3]>,
    color1: Option<[f32; 4]>,
    color2: Option<[f32; 4]>,
) -> mlua::Result<()> {
    prepare_capture_scope_actor(lua, actor)?;
    let previous = actor.raw_get::<Option<String>>("__songlua_state_effect_mode")?;
    let previous_period = actor
        .raw_get::<Option<f32>>("__songlua_state_effect_period")?
        .unwrap_or(1.0);
    let motion = matches!(mode, "bob" | "bounce" | "wag");
    let color = matches!(
        mode,
        "diffuseblink" | "diffuseshift" | "diffuseramp" | "glowblink" | "glowshift" | "glowramp"
    );
    let reset = (motion
        && (mode == "bounce"
            || previous.as_deref() != Some(mode)
            || (mode == "bob" && period.is_some_and(|value| value != previous_period))))
        || (color && previous.as_deref() != Some(mode))
        || (mode == "rainbow"
            && !actor
                .raw_get::<Option<bool>>("__songlua_state_rainbow")?
                .unwrap_or(false));
    if motion || color || matches!(mode, "pulse" | "rainbow") {
        let clock =
            if let Some(clock) = actor.raw_get::<Option<Table>>("__songlua_state_motion_clock")? {
                clock
            } else {
                let clock = lua.create_table()?;
                clock.set("units", 0.0_f32)?;
                actor.raw_set("__songlua_state_motion_clock", clock.clone())?;
                clock
            };
        if reset {
            clock.set("units", 0.0_f32)?;
        }
    }
    set_actor_effect_mode(lua, actor, mode)?;
    if mode == "spin"
        && actor
            .raw_get::<Option<Table>>("__songlua_state_spin")?
            .is_none()
    {
        let pose = lua.create_table()?;
        let rotation = lua.create_table()?;
        for (index, key) in ["rot_x_deg", "rot_y_deg", "rot_z_deg"].iter().enumerate() {
            rotation.raw_set(
                index + 1,
                actor
                    .raw_get::<Option<f32>>(actor_capture_state_key(key).as_ref())?
                    .unwrap_or(0.0),
            )?;
        }
        pose.set("rotation", rotation)?;
        pose.set("queue", lua.create_table()?)?;
        pose.set("head", 1usize)?;
        pose.set("tail", 0usize)?;
        actor.raw_set("__songlua_state_spin", pose)?;
    }
    if let Some(value) = period.filter(|_| mode != "bob" || reset) {
        capture_effect_period(lua, actor, value)?;
    }
    if let Some(value) = magnitude {
        capture_immediate_vec3(lua, actor, "effect_magnitude", value)?;
    }
    if let Some(value) = color1 {
        capture_immediate_vec4(lua, actor, "effect_color1", value)?;
    }
    if let Some(value) = color2 {
        capture_immediate_vec4(lua, actor, "effect_color2", value)?;
    }
    Ok(())
}

pub fn set_actor_texture_from_path(actor: &Table, path: &str) -> mlua::Result<bool> {
    let path = path.trim();
    if path.is_empty() {
        return Ok(false);
    }
    actor.set("Texture", path.to_string())?;
    actor.raw_set("__songlua_amv_texture_handle", Value::Nil)?;
    actor.raw_set("__songlua_default_texture", false)?;
    actor.set("__songlua_aft_capture_name", Value::Nil)?;
    set_actor_decode_movie_for_texture(actor)?;
    Ok(true)
}

pub fn set_actor_texture_from_path_methods(
    actor: &Table,
    value: Option<&Value>,
    method_names: &[&str],
) -> mlua::Result<bool> {
    let Some(Value::Table(source)) = value else {
        return Ok(false);
    };
    for method_name in method_names {
        if let Value::String(path) = call_table_method(source, method_name)?
            && set_actor_texture_from_path(actor, &path.to_str()?)?
        {
            return Ok(true);
        }
    }
    Ok(false)
}

pub fn set_actor_texture_from_path_methods_or_fallback(
    actor: &Table,
    value: Option<&Value>,
    method_names: &[&str],
    fallback_path: &str,
) -> mlua::Result<()> {
    if !set_actor_texture_from_path_methods(actor, value, method_names)? {
        set_actor_texture_from_path(actor, fallback_path)?;
    }
    Ok(())
}

pub fn set_actor_texture_from_value(
    actor: &Table,
    value: Option<&Value>,
    clear_on_nil: bool,
) -> mlua::Result<()> {
    if actor_type_is(actor, "ActorMultiVertex")? {
        // Native SetTexture retains the copied RageTexture, including its
        // allocation and frame metadata, independently of this actor's size.
        match value {
            Some(Value::Table(texture))
                if texture.raw_get::<Option<String>>("__songlua_native_type")?.as_deref()
                    == Some("RageTexture") =>
            {
                actor.raw_set("__songlua_amv_texture_handle", texture.clone())?;
            }
            Some(Value::String(_)) => actor.raw_set("__songlua_amv_texture_handle", Value::Nil)?,
            Some(Value::Nil) | None if clear_on_nil => {
                actor.raw_set("__songlua_amv_texture_handle", Value::Nil)?;
            }
            _ => (),
        }
    }
    match value {
        Some(Value::String(texture)) => {
            actor.set("Texture", texture.to_str()?.to_string())?;
            actor.raw_set("__songlua_default_texture", false)?;
            actor.set("__songlua_aft_capture_name", Value::Nil)?;
            set_actor_decode_movie_for_texture(actor)?;
        }
        Some(Value::Table(texture)) => {
            if texture.raw_get::<bool>("__songlua_default_texture")? {
                actor.raw_set("__songlua_default_texture", true)?;
                actor.set("Texture", Value::Nil)?;
                actor.set("__songlua_aft_capture_name", Value::Nil)?;
                actor.set("__songlua_state_decode_movie", false)?;
            } else if let Some(capture_name) =
                texture.get::<Option<String>>("__songlua_aft_capture_name")?
            {
                actor.raw_set("__songlua_default_texture", false)?;
                actor.set("__songlua_aft_capture_name", capture_name)?;
                actor.set("Texture", Value::Nil)?;
                set_actor_decode_movie_for_texture(actor)?;
            } else if let Some(texture_path) = texture
                .get::<Option<String>>("__songlua_texture_path")?
                .filter(|path| !path.is_empty())
            {
                actor.set("Texture", texture_path)?;
                actor.raw_set("__songlua_default_texture", false)?;
                actor.set("__songlua_aft_capture_name", Value::Nil)?;
                set_actor_decode_movie_for_texture(actor)?;
            }
        }
        Some(Value::Nil) | None if clear_on_nil => {
            actor.raw_set("__songlua_default_texture", false)?;
            actor.set("Texture", Value::Nil)?;
            actor.set("__songlua_aft_capture_name", Value::Nil)?;
            actor.set("__songlua_state_decode_movie", false)?;
        }
        _ => {}
    }
    Ok(())
}

pub fn actor_sprite_sheet_dims(actor: &Table) -> mlua::Result<Option<(u32, u32)>> {
    if actor.raw_get::<bool>("__songlua_default_texture")? {
        return Ok(Some((1, 1)));
    }
    if let Some(path) = actor_texture_path(actor)? {
        return Ok(Some(parse_sprite_sheet_dims(
            path.to_string_lossy().as_ref(),
        )));
    }
    let Some(texture) = actor.get::<Option<String>>("Texture")? else {
        return Ok(None);
    };
    let texture = texture.trim();
    if texture.is_empty() {
        return Ok(None);
    }
    Ok(Some(parse_sprite_sheet_dims(texture)))
}

pub fn actor_texture_rect(actor: &Table) -> mlua::Result<[f32; 4]> {
    let custom_rect = actor
        .get::<Option<Table>>("__songlua_state_custom_texture_rect")?
        .and_then(|value| table_vec4(&value));
    let state_index = actor.get::<Option<u32>>("__songlua_state_sprite_state_index")?;
    Ok(sprite_texture_rect(
        custom_rect,
        state_index,
        actor_sprite_sheet_dims(actor)?,
    ))
}

pub fn offset_actor_texture_rect(lua: &Lua, actor: &Table, dx: f32, dy: f32) -> mlua::Result<()> {
    let rect = song_lua_offset_texture_rect(actor_texture_rect(actor)?, [dx, dy]);
    capture_texture_rect(lua, actor, rect)
}

pub fn actor_base_size_with_image(
    actor: &Table,
    image_frame_size: Option<(f32, f32)>,
) -> mlua::Result<(f32, f32)> {
    if let Some(size) = actor
        .get::<Option<ActorVectorValue<2>>>("__songlua_state_size")?
        .and_then(|value| value.0)
    {
        return Ok((size[0].abs(), size[1].abs()));
    }
    if let Some(rect) = actor
        .get::<Option<Table>>("__songlua_state_stretch_rect")?
        .and_then(|value| table_vec4(&value))
    {
        return Ok(((rect[2] - rect[0]).abs(), (rect[3] - rect[1]).abs()));
    }
    if let Some(size) = image_frame_size {
        return Ok(size);
    }
    if actor_type_is(actor, "GraphDisplay")? {
        return Ok((
            theme_metric_number("GraphDisplay", "BodyWidth").unwrap_or(300.0),
            theme_metric_number("GraphDisplay", "BodyHeight").unwrap_or(64.0),
        ));
    }
    Ok((1.0, 1.0))
}

pub fn actor_base_size(actor: &Table) -> mlua::Result<(f32, f32)> {
    actor_base_size_with_image(actor, actor_image_frame_size(actor)?)
}

pub fn actor_image_texture_size(actor: &Table) -> mlua::Result<Option<(f32, f32)>> {
    if let Some(path) = actor_texture_path(actor)?
        && let Some(texture) = deadlib_assets::generated_texture(&path.to_string_lossy())
    {
        return Ok(Some((texture.source_size[0] as f32, texture.source_size[1] as f32)));
    }
    if let Some(path) = actor_texture_path(actor)?
        && is_song_lua_image_path(&path)
        && let Ok((width, height)) = deadlib_assets::texture_source_size(&path)
    {
        return Ok(Some((width as f32, height as f32)));
    }
    Ok(None)
}

pub fn actor_image_frame_size(actor: &Table) -> mlua::Result<Option<(f32, f32)>> {
    let path = actor_texture_path(actor)?;
    let sheet_dims = path
        .as_ref()
        .map(|path| parse_sprite_sheet_dims(path.to_string_lossy().as_ref()));
    let source = actor
        .get::<Option<Table>>("__songlua_movie_size")?
        .and_then(|value| table_vec2(&value))
        .map(|size| (size[0], size[1]))
        .or_else(|| path.as_deref().and_then(image_source_size));
    Ok(sprite_image_frame_size(
        source,
        actor
            .get::<Option<bool>>("__songlua_state_sprite_animate")?
            .unwrap_or(false),
        actor.get::<Option<u32>>("__songlua_state_sprite_state_index")?,
        sheet_dims,
    ))
}

// Compile-thread-only, actor-lifetime metadata: one cached size (or failed probe)
// per movie actor. Populated at load/getter time, never from a gameplay frame.
// No eviction or shared locking; the VM frees it after compilation. Probe failure
// is logged once and retains the existing screen-size fallback. Worst-case work
// is one ffprobe invocation per loaded movie actor, including duplicate sources.
fn movie_source_size(lua: &Lua, actor: &Table) -> mlua::Result<Option<(f32, f32)>> {
    let Some(path) = actor_texture_path(actor)?.filter(|path| is_song_lua_video_path(path)) else {
        return Ok(None);
    };
    let key = path.to_string_lossy();
    if actor
        .get::<Option<String>>("__songlua_movie_path")?
        .as_deref()
        == Some(key.as_ref())
    {
        return Ok(actor
            .get::<Option<Table>>("__songlua_movie_size")?
            .and_then(|value| table_vec2(&value))
            .map(|size| (size[0], size[1])));
    }
    actor.set("__songlua_movie_path", key.as_ref())?;
    actor.set("__songlua_movie_size", Value::Nil)?;
    match deadlib_video::dimensions(&path) {
        Ok((width, height)) => {
            actor.set(
                "__songlua_movie_size",
                lua.create_sequence_from([width as f32, height as f32])?,
            )?;
            Ok(Some((width as f32, height as f32)))
        }
        Err(error) => {
            log::warn!("Song Lua movie metadata: {error}");
            Ok(None)
        }
    }
}

fn image_source_size(path: &Path) -> Option<(f32, f32)> {
    if !is_song_lua_image_path(path) {
        return None;
    }
    let (width, height) = if let Some(texture) = deadlib_assets::generated_texture(&path.to_string_lossy()) {
        (texture.source_size[0], texture.source_size[1])
    } else {
        deadlib_assets::texture_source_size(path).ok()?
    };
    // RageBitmapTexture applies the res override before halving doubleres.
    let (width, height) = deadlib_assets::texture_source_dims_from_real(
        path.to_string_lossy().as_ref(),
        width,
        height,
    );
    Some((width as f32, height as f32))
}

pub fn actor_crop_source_size(lua: &Lua, actor: &Table) -> mlua::Result<Option<(f32, f32)>> {
    if let Some(size) = actor_image_frame_size(actor)? {
        return Ok(Some(size));
    }
    let Some(path) = actor_texture_path(actor)? else {
        return Ok(None);
    };
    if is_song_lua_video_path(&path) {
        return movie_source_size(lua, actor);
    }
    Ok(None)
}

pub fn crop_actor_to(lua: &Lua, actor: &Table, width: f32, height: f32) -> mlua::Result<()> {
    let target = [width, height];
    if !target.iter().all(|value| value.is_finite() && *value > 0.0) {
        return Ok(());
    }
    let Some((source_width, source_height)) = actor_crop_source_size(lua, actor)? else {
        return Ok(());
    };
    crop_actor_to_source_size(lua, actor, target, [source_width, source_height])
}

pub fn texture_source_size(lua: &Lua, actor: &Table, path: &Path) -> mlua::Result<(f32, f32)> {
    if let Some(size) = image_source_size(path) {
        return Ok(size);
    }
    if is_song_lua_video_path(path) {
        return Ok(movie_source_size(lua, actor)?.unwrap_or(song_lua_screen_size(lua)?));
    }
    actor_base_size(actor)
}

pub fn scale_actor_to_rect_with_base_size(
    lua: &Lua,
    actor: &Table,
    rect: [f32; 4],
    cover: bool,
    base_size: [f32; 2],
) -> mlua::Result<()> {
    let align = [actor_halign(lua, actor)?, actor_valign(lua, actor)?];
    let Some(plan) = scale_to_rect_plan(rect, base_size, align, cover) else {
        return Ok(());
    };
    capture_block_set_f32(lua, actor, "x", plan.pos[0])?;
    capture_block_set_f32(lua, actor, "y", plan.pos[1])?;
    capture_block_set_f32(lua, actor, "zoom", plan.zoom)?;
    capture_block_set_zoom_axes(lua, actor, plan.zoom, "zoom_x", "zoom_y", "zoom_z")?;
    actor_update_text_pre_zoom_flags(lua, actor, true, true)?;
    if plan.flip_x {
        capture_block_set_f32(lua, actor, "rot_y_deg", 180.0)?;
    }
    if plan.flip_y {
        capture_block_set_f32(lua, actor, "rot_x_deg", 180.0)?;
    }
    Ok(())
}

fn scale_actor_to_rect(lua: &Lua, actor: &Table, rect: [f32; 4], cover: bool) -> mlua::Result<()> {
    let (base_width, base_height) = actor_base_size(actor)?;
    scale_actor_to_rect_with_base_size(lua, actor, rect, cover, [base_width, base_height])
}

pub fn crop_actor_to_source_size(
    lua: &Lua,
    actor: &Table,
    target: [f32; 2],
    source_size: [f32; 2],
) -> mlua::Result<()> {
    if !target.iter().all(|value| value.is_finite() && *value > 0.0) {
        return Ok(());
    }
    let Some(rect) = crop_texture_rect(source_size, target) else {
        return Ok(());
    };
    capture_block_set_size(lua, actor, target)?;
    capture_block_set_u32(
        lua,
        actor,
        "sprite_state_index",
        SONG_LUA_SPRITE_STATE_CLEAR,
    )?;
    capture_block_set_vec4(lua, actor, "custom_texture_rect", rect)?;
    capture_block_set_f32(lua, actor, "zoom", 1.0)?;
    capture_block_set_zoom_axes(lua, actor, 1.0, "zoom_x", "zoom_y", "zoom_z")?;
    Ok(())
}

fn scale_background(lua: &Lua, actor: &Table, center: bool) -> mlua::Result<()> {
    let globals = lua.globals();
    let width: f32 = globals.get("SCREEN_WIDTH")?;
    let height: f32 = globals.get("SCREEN_HEIGHT")?;
    let preferences: Table = globals.get("PREFSMAN")?;
    // _fallback/02 Actor.lua reads this inside the theme helper. Calling the
    // public Lua binding from Rust attributes that internal query to the song.
    let values: Table = preferences.raw_get("__songlua_preference_values")?;
    let mode = values.raw_get::<Option<String>>("BackgroundFitMode")?
        .unwrap_or_else(|| "BackgroundFitMode_CoverPreserve".into());
    let (base_width, base_height) = actor_base_size(actor)?;
    if base_width > 0.0 && base_height > 0.0 {
        let x = width / base_width;
        let y = height / base_height;
        let zoom = match mode.as_str() {
            "BackgroundFitMode_CoverDistort" => None,
            "BackgroundFitMode_CoverPreserve" => Some(x.max(y)),
            "BackgroundFitMode_FitInside" => Some(x.min(y)),
            "BackgroundFitMode_FitInsideAvoidLetter" => Some(y),
            "BackgroundFitMode_FitInsideAvoidPillar" => Some(x),
            _ => {
                scale_actor_to_rect(lua, actor, [0.0, 0.0, width, height], true)?;
                None
            }
        };
        if let Some(zoom) = zoom {
            capture_block_set_f32(lua, actor, "zoom", zoom)?;
            capture_block_set_zoom_axes(lua, actor, zoom, "zoom_x", "zoom_y", "zoom_z")?;
        } else if mode == "BackgroundFitMode_CoverDistort" {
            capture_block_set_f32(lua, actor, "zoom_x", x)?;
            capture_block_set_f32(lua, actor, "zoom_y", y)?;
        }
        actor_update_text_pre_zoom_flags(lua, actor, true, true)?;
    }
    if center {
        capture_block_set_f32(lua, actor, "x", width * 0.5)?;
        capture_block_set_f32(lua, actor, "y", height * 0.5)?;
    }
    Ok(())
}

pub fn install_actor_scale_size_methods(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    for (name, center) in [
        ("scale_or_crop_background", true),
        ("scale_or_crop_background_no_move", false),
    ] {
        actor.set(
            name,
            lua.create_function({
                let actor = actor.clone();
                move |lua, _args: IgnoredArgs| {
                    scale_background(lua, &actor, center)?;
                    Ok(actor.clone())
                }
            })?,
        )?;
    }
    for (name, cover) in [("scaletofit", false), ("scaletocover", true)] {
        actor.set(
            name,
            lua.create_function({
                let actor = actor.clone();
                move |lua, args: MultiValue| {
                    let Some(left) = method_arg(&args, 0).cloned().and_then(read_f32) else {
                        return Ok(actor.clone());
                    };
                    let Some(top) = method_arg(&args, 1).cloned().and_then(read_f32) else {
                        return Ok(actor.clone());
                    };
                    let Some(right) = method_arg(&args, 2).cloned().and_then(read_f32) else {
                        return Ok(actor.clone());
                    };
                    let Some(bottom) = method_arg(&args, 3).cloned().and_then(read_f32) else {
                        return Ok(actor.clone());
                    };
                    scale_actor_to_rect(lua, &actor, [left, top, right, bottom], cover)?;
                    Ok(actor.clone())
                }
            })?,
        )?;
    }
    actor.set(
        "CropTo",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(width) = method_arg(&args, 0).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                let Some(height) = method_arg(&args, 1).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                crop_actor_to(lua, &actor, width, height)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "zoomtowidth",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(width) = args.get(1).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                let (base_width, height) = actor_base_size(&actor)?;
                if actor_type_is(&actor, "BitmapText")? {
                    // Text fit bounds resolve against font metrics during playback.
                    capture_block_set_size(lua, &actor, [width, height])?;
                } else {
                    capture_block_set_f32(lua, &actor, "zoom_x", width / base_width)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "zoomtoheight",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(height) = args.get(1).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                let (width, base_height) = actor_base_size(&actor)?;
                if actor_type_is(&actor, "BitmapText")?
                    && !lua_text_value(actor.get::<Value>("Text")?)?.is_empty()
                {
                    capture_block_set_size(lua, &actor, [width, height])?;
                } else {
                    // BitmapText::BuildChars leaves the constructor height (1)
                    // while its text lines are empty. ZoomToHeight must update
                    // the Lua-visible zoom before text is first assigned.
                    capture_block_set_f32(lua, &actor, "zoom_y", height / base_height)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "SetSize",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(width) = args.get(1).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                let Some(height) = args.get(2).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                capture_block_set_size(lua, &actor, [width, height])?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set("setsize", make_actor_set_size_method(lua, actor)?)?;
    actor.set(
        "SetWidth",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(width) = args.get(1).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                let (_, height) = actor_base_size(&actor)?;
                capture_block_set_size(lua, &actor, [width, height])?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "SetHeight",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(height) = args.get(1).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                let (width, _) = actor_base_size(&actor)?;
                capture_block_set_size(lua, &actor, [width, height])?;
                Ok(actor.clone())
            }
        })?,
    )?;
    Ok(())
}

pub fn install_actor_image_coord_methods(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    actor.set(
        "addimagecoords",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some([width, height]) = actor_upload_image_size(&actor)? else {
                    return Ok(actor.clone());
                };
                let dx = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_f32)
                    .unwrap_or(0.0);
                let dy = method_arg(&args, 1)
                    .cloned()
                    .and_then(read_f32)
                    .unwrap_or(0.0);
                let Some(rect) = texture_pixel_offset_rect(
                    actor_texture_rect(&actor)?,
                    [width, height],
                    [dx, dy],
                ) else {
                    return Ok(actor.clone());
                };
                capture_texture_rect(lua, &actor, rect)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    Ok(())
}

fn actor_upload_image_size(actor: &Table) -> mlua::Result<Option<[f32; 2]>> {
    let Some((width, height)) = actor_image_texture_size(actor)? else { return Ok(None); };
    let path = actor_texture_path(actor)?.unwrap_or_default();
    let mut hints = deadlib_assets::parse_texture_hints(&path.to_string_lossy());
    hints.max_size = Some(2048);
    hints.min_size = Some(8);
    deadlib_assets::texture_image_size([width as u32, height as u32], &hints)
        .map(|size| Some(size.map(|dimension| dimension as f32)))
        .map_err(mlua::Error::external)
}

pub fn install_actor_size_getter_methods(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    actor.set(
        "GetWidth",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: IgnoredArgs| Ok(actor_base_size(&actor)?.0)
        })?,
    )?;
    actor.set(
        "GetZoomedWidth",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: IgnoredArgs| {
                let width = actor_base_size(&actor)?.0;
                Ok(width
                    * actor_zoom_axis(
                        lua,
                        &actor,
                        "__songlua_state_zoom_x",
                        "__songlua_state_basezoom_x",
                    )?)
            }
        })?,
    )?;
    actor.set(
        "GetNumStates",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: IgnoredArgs| Ok(i64::from(actor_sprite_frame_count(&actor)?))
        })?,
    )?;
    actor.set(
        "GetHeight",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: IgnoredArgs| Ok(actor_base_size(&actor)?.1)
        })?,
    )?;
    actor.set(
        "GetZoomedHeight",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: IgnoredArgs| {
                let height = actor_base_size(&actor)?.1;
                Ok(height
                    * actor_zoom_axis(
                        lua,
                        &actor,
                        "__songlua_state_zoom_y",
                        "__songlua_state_basezoom_y",
                    )?)
            }
        })?,
    )?;
    Ok(())
}

pub fn set_actor_seconds_into_animation(
    lua: &Lua,
    actor: &Table,
    seconds: f32,
) -> mlua::Result<()> {
    let delay = actor
        .get::<Option<f32>>("__songlua_state_sprite_state_delay")?
        .unwrap_or(0.1);
    let frame_count = actor_sprite_frame_count(actor)?;
    let state = sprite_animation_state_at(seconds, delay, frame_count);
    set_actor_sprite_state(lua, actor, state)
}

pub fn actor_sprite_frame_count(actor: &Table) -> mlua::Result<u32> {
    Ok(sprite_frame_count(actor_sprite_sheet_dims(actor)?))
}

pub fn create_texture_proxy(lua: &Lua, actor: &Table) -> mlua::Result<Table> {
    if let Some(texture) = actor.raw_get::<Option<Table>>("__songlua_amv_texture_handle")? {
        return Ok(texture);
    }
    let texture = lua.create_table()?;
    texture.raw_set("__songlua_native_type", "RageTexture")?;
    if actor.raw_get::<bool>("__songlua_default_texture")? {
        texture.raw_set("__songlua_default_texture", true)?;
        install_texture_proxy_methods(
            lua, &texture, actor, String::new(), 1.0, 1.0, 1.0, 1.0, (1.0, 1.0), 1,
        )?;
        return Ok(texture);
    }
    let frame_count = actor_sprite_frame_count(actor)?;
    if actor
        .get::<Option<String>>("__songlua_actor_type")?
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("ActorFrameTexture"))
    {
        if let Some(capture_name) = actor_aft_capture_name(actor)? {
            texture.set("__songlua_aft_capture_name", capture_name)?;
        }
        let globals = lua.globals();
        let meta = if let Some(meta) =
            globals.raw_get::<Option<Table>>("__songlua_aft_texture_meta")?
        {
            meta
        } else {
            let meta = lua.create_table()?;
            meta.set(
                "__eq",
                lua.create_function(|_, (left, right): (Table, Table)| {
                    let name = left.raw_get::<Option<String>>("__songlua_aft_capture_name")?;
                    Ok(name.is_some()
                        && name == right.raw_get::<Option<String>>("__songlua_aft_capture_name")?)
                })?,
            )?;
            globals.raw_set("__songlua_aft_texture_meta", meta.clone())?;
            meta
        };
        texture.set_metatable(Some(meta))?;
        let allocation = actor.get::<Table>("__songlua_aft_allocation")?;
        let size = allocation.get::<Table>("size")?;
        let width: f32 = size.raw_get(1)?;
        let height: f32 = size.raw_get(2)?;
        install_texture_proxy_methods(
            lua,
            &texture,
            actor,
            allocation.get("name")?,
            width,
            height,
            (width as u32)
                .checked_next_power_of_two()
                .unwrap_or(u32::MAX) as f32,
            (height as u32)
                .checked_next_power_of_two()
                .unwrap_or(u32::MAX) as f32,
            (width, height),
            1,
        )?;
        texture.set(
            "BeginRenderingTo",
            lua.create_function({
                let actor = actor.clone();
                let texture = texture.clone();
                move |lua, args: MultiValue| {
                    crate::draw_capture::begin(
                        lua,
                        &actor,
                        method_arg(&args, 0).is_some_and(truthy),
                    )?;
                    Ok(texture.clone())
                }
            })?,
        )?;
        texture.set(
            "FinishRenderingTo",
            lua.create_function({
                let actor = actor.clone();
                let texture = texture.clone();
                move |lua, _: MultiValue| {
                    crate::draw_capture::finish(lua, &actor)?;
                    Ok(texture.clone())
                }
            })?,
        )?;
        return Ok(texture);
    }

    let raw_texture = actor.get::<Option<String>>("Texture")?.unwrap_or_default();
    let resolved = actor_texture_path(actor)?;
    let path = resolved
        .as_deref()
        .map(file_path_string)
        .unwrap_or_else(|| raw_texture.clone());
    let (source_width, source_height) = texture_source_size(
        lua,
        actor,
        resolved
            .as_deref()
            .unwrap_or_else(|| Path::new(&raw_texture)),
    )?;
    // RageBitmapTexture caps the image at the native game preference (2048),
    // then stretches both axes if either allocation is under eight pixels.
    // Logical source hints do not change this prepared image/allocation pair.
    let bitmap_sizes = actor_upload_image_size(actor)?
        .map(|image| {
            let allocation = image.map(|dimension| {
                (dimension as u32)
                    .checked_next_power_of_two()
                    .unwrap_or(u32::MAX)
                    .max(8)
            });
            (image, allocation.map(|value| value as f32))
        });
    let ([image_width, image_height], [texture_width, texture_height]) =
        bitmap_sizes.unwrap_or(([source_width, source_height], [source_width, source_height]));
    install_texture_proxy_methods(
        lua,
        &texture,
        actor,
        path,
        source_width,
        source_height,
        texture_width,
        texture_height,
        (image_width, image_height),
        frame_count,
    )?;
    Ok(texture)
}

pub fn install_actor_texture_proxy_getter_methods(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    actor.set(
        "GetTexture",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _self: Table| {
                let created = if actor_type_is(&actor, "ActorFrameTexture")? {
                    actor
                        .raw_get::<Option<Table>>("__songlua_aft_allocation")?
                        .is_some()
                } else {
                    actor.raw_get::<bool>("__songlua_default_texture")?
                        || !matches!(actor.get::<Value>("Texture")?, Value::Nil)
                };
                if created {
                    Ok(Value::Table(create_texture_proxy(lua, &actor)?))
                } else {
                    Ok(Value::Nil)
                }
            }
        })?,
    )?;
    Ok(())
}

pub fn install_texture_proxy_methods(
    lua: &Lua,
    texture: &Table,
    actor: &Table,
    path: String,
    source_width: f32,
    source_height: f32,
    texture_width: f32,
    texture_height: f32,
    image_size: (f32, f32),
    frame_count: u32,
) -> mlua::Result<()> {
    let path = collapse_texture_name(&path);
    texture.set("__songlua_texture_path", path.clone())?;
    // SetTexture uses RageTexture's frame size, independently of the Sprite's
    // existing states. Capture it on this handle so a later Load on its owner
    // cannot change a retained texture's dimensions.
    let (cols, rows) = if actor_type_is(actor, "ActorFrameTexture")? {
        (1, 1)
    } else {
        actor_sprite_sheet_dims(actor)?.unwrap_or((1, 1))
    };
    texture.set(
        "__songlua_source_frame_size",
        lua.create_sequence_from([
            (source_width / cols.max(1) as f32).floor(),
            (source_height / rows.max(1) as f32).floor(),
        ])?,
    )?;
    texture.set(
        "GetPath",
        lua.create_function(move |lua, _args: IgnoredArgs| {
            Ok(Value::String(lua.create_string(&path)?))
        })?,
    )?;
    texture.set(
        "GetSourceWidth",
        lua.create_function(move |_, _args: IgnoredArgs| Ok(source_width))?,
    )?;
    texture.set(
        "GetSourceHeight",
        lua.create_function(move |_, _args: IgnoredArgs| Ok(source_height))?,
    )?;
    texture.set(
        "GetTextureWidth",
        lua.create_function(move |_, _args: IgnoredArgs| Ok(texture_width))?,
    )?;
    texture.set(
        "GetTextureHeight",
        lua.create_function(move |_, _args: IgnoredArgs| Ok(texture_height))?,
    )?;
    texture.set(
        "GetImageWidth",
        lua.create_function(move |_, _args: IgnoredArgs| Ok(image_size.0))?,
    )?;
    texture.set(
        "GetImageHeight",
        lua.create_function(move |_, _args: IgnoredArgs| Ok(image_size.1))?,
    )?;
    texture.set(
        "GetNumFrames",
        lua.create_function(move |_, _args: IgnoredArgs| Ok(i64::from(frame_count)))?,
    )?;
    texture.set(
        "loop",
        lua.create_function({
            let actor = actor.clone();
            let texture = texture.clone();
            move |lua, args: MultiValue| {
                let value = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_boolish)
                    .unwrap_or(true);
                capture_block_set_bool(lua, &actor, "sprite_loop", value)?;
                Ok(Value::Table(texture.clone()))
            }
        })?,
    )?;
    texture.set(
        "rate",
        lua.create_function({
            let actor = actor.clone();
            let texture = texture.clone();
            move |lua, args: MultiValue| {
                if let Some(value) = method_arg(&args, 0).cloned().and_then(read_f32) {
                    capture_block_set_f32(lua, &actor, "sprite_playback_rate", value)?;
                }
                Ok(Value::Table(texture.clone()))
            }
        })?,
    )?;
    Ok(())
}

pub fn set_actor_sound_file_from_value(
    actor: &Table,
    value: Option<&Value>,
    clear_on_nil: bool,
) -> mlua::Result<()> {
    match value {
        Some(Value::String(file)) => actor.set("File", file.to_str()?.to_string())?,
        Some(Value::Nil) | None if clear_on_nil => actor.set("File", Value::Nil)?,
        _ => {}
    }
    Ok(())
}

fn play_sound_actor(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    if crate::live::active(lua) {
        if let Some(file) = actor.get::<Option<String>>("File")? {
            let path = resolve_actor_asset_path(actor, &file).map_err(mlua::Error::external)?;
            crate::live::actor_sound(lua, crate::SongLuaActorSound::Play {
                owner: actor.to_pointer() as u64, path,
            });
        }
        Ok(())
    } else {
        capture_block_set_bool(lua, actor, "sound_play", true)
    }
}

fn install_sound_methods(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    if !actor_type_is(actor, "Sound")? {
        return Ok(());
    }
    // ActorSound calls are immediate (not tween properties). Preserve their
    // order and actor identity, including two actors loading the same file.
    actor.set("stop", lua.create_function({
        let actor = actor.clone();
        move |lua, _: MultiValue| {
            crate::live::actor_sound(lua, crate::SongLuaActorSound::Stop {
                owner: actor.to_pointer() as u64,
            });
            Ok(actor.clone())
        }
    })?)?;
    actor.set("pause", lua.create_function({
        let actor = actor.clone();
        move |lua, args: MultiValue| {
            let paused = method_arg(&args, 0).cloned().and_then(read_boolish).unwrap_or(false);
            crate::live::actor_sound(lua, crate::SongLuaActorSound::Pause {
                owner: actor.to_pointer() as u64, paused,
            });
            Ok(actor.clone())
        }
    })?)?;
    let sound = lua.create_table()?;
    sound.raw_set("__songlua_volume", 1.0_f32)?;
    sound.raw_set("__songlua_pitch", 1.0_f32)?;
    sound.set("pitch", lua.create_function({
        let sound = sound.clone();
        move |_, args: MultiValue| {
            // LunaRageSound::pitch copies the native params, writes FArg(1),
            // and returns the same sound. Keep the other parameters intact.
            let pitch = method_arg(&args, 0).cloned().and_then(read_f32)
                .ok_or_else(|| mlua::Error::runtime("Expected pitch number"))?;
            sound.raw_set("__songlua_pitch", pitch)?;
            Ok(sound.clone())
        }
    })?)?;
    sound.set(
        "volume",
        lua.create_function({
            let sound = sound.clone();
            move |_, args: MultiValue| {
                // LunaRageSound::volume stores FArg(1) without clamping and
                // returns the same RageSound, rather than its owning Actor.
                let volume = method_arg(&args, 0).cloned().and_then(read_f32)
                    .ok_or_else(|| mlua::Error::runtime("Expected volume number"))?;
                sound.raw_set("__songlua_volume", volume)?;
                Ok(sound.clone())
            }
        })?,
    )?;
    sound.set(
        "get_length",
        lua.create_function({
            let actor = actor.clone();
            // Per-actor metadata lives with the compile-time Lua VM. Probe once on
            // first use (or after load changes File), never on a gameplay frame.
            let cached = std::cell::RefCell::new(None::<(Option<String>, f32)>);
            move |_, _args: IgnoredArgs| {
                let file = actor.get::<Option<String>>("File")?;
                let mut cached = cached.borrow_mut();
                if let Some((previous, length)) = cached.as_ref() {
                    if previous == &file {
                        return Ok(*length);
                    }
                }
                let length = file
                    .as_deref()
                    .and_then(|file| resolve_actor_asset_path(&actor, file).ok())
                    .and_then(|path| {
                        deadlib_audio_decode::file_length_seconds(&path, Default::default()).ok()
                    })
                    // RageSound::GetLengthSeconds converts its reader's whole
                    // milliseconds to seconds, rather than exposing samples.
                    .map(|seconds| (seconds * 1000.0).trunc() / 1000.0)
                    .unwrap_or(-1.0);
                *cached = Some((file, length));
                Ok(length)
            }
        })?,
    )?;
    // RageSound returns whether its reader accepts the property. The current
    // scheduled-SFX path has no mutable reader properties; report that honestly.
    sound.set(
        "SetProperty",
        lua.create_function(|_, _args: IgnoredArgs| Ok(false))?,
    )?;
    actor.set(
        "get",
        lua.create_function(move |_, _args: IgnoredArgs| Ok(sound.clone()))?,
    )?;
    Ok(())
}

pub fn set_rolling_numbers_metric(actor: &Table, metric: &str) -> mlua::Result<()> {
    actor.set("__songlua_rolling_numbers_metric", metric)?;
    actor.set(
        "__songlua_rolling_numbers_format",
        rolling_numbers_format(metric),
    )?;
    Ok(())
}

pub fn rolling_numbers_text(actor: &Table, number: f32) -> mlua::Result<String> {
    let format = actor
        .get::<Option<String>>("__songlua_rolling_numbers_format")?
        .unwrap_or_else(|| "%.0f".to_string());
    Ok(format_rolling_number(&format, number))
}

pub fn actor_update_text_pre_zoom_flags(
    lua: &Lua,
    actor: &Table,
    update_width: bool,
    update_height: bool,
) -> mlua::Result<()> {
    if !actor_is_bitmap_text(actor)? {
        return Ok(());
    }
    if update_width && actor.get::<Option<bool>>("__songlua_text_saw_max_width")? == Some(true) {
        capture_block_set_bool(lua, actor, "max_w_pre_zoom", true)?;
    }
    if update_height && actor.get::<Option<bool>>("__songlua_text_saw_max_height")? == Some(true) {
        capture_block_set_bool(lua, actor, "max_h_pre_zoom", true)?;
    }
    Ok(())
}

pub fn run_command_on_leaves(lua: &Lua, actor: &Table, command: &Function) -> mlua::Result<()> {
    let mut saw_child = false;
    for child in actor_direct_children(lua, actor)? {
        saw_child = true;
        run_command_on_leaves(lua, &child, command)?;
    }
    if !saw_child {
        let _ = command.call::<Value>(actor.clone())?;
    }
    Ok(())
}

pub fn run_named_command_on_leaves(
    lua: &Lua,
    actor: &Table,
    command: &str,
    params: Option<Value>,
) -> mlua::Result<()> {
    let mut saw_child = false;
    for child in actor_direct_children(lua, actor)? {
        saw_child = true;
        run_named_command_on_leaves(lua, &child, command, params.clone())?;
    }
    if !saw_child {
        run_actor_named_command_with_drain_and_params(lua, actor, command, true, params)?;
    }
    Ok(())
}

pub fn run_named_command_on_children_recursively(
    lua: &Lua,
    actor: &Table,
    command: &str,
    params: Option<Value>,
) -> mlua::Result<()> {
    for child in actor_direct_children(lua, actor)? {
        run_actor_named_command_with_drain_and_params(lua, &child, command, true, params.clone())?;
        run_named_command_on_children_recursively(lua, &child, command, params.clone())?;
    }
    Ok(())
}

// ActorFrame overrides SetZTestMode and SetZWrite to apply the setter to
// every owned child. Capture these writes on the children themselves so a
// child's later ztest(0) still overrides its parent's earlier ztest(1).
fn capture_actor_depth_recursive(lua: &Lua, actor: &Table, field: &str, enabled: bool) -> mlua::Result<()> {
    capture_block_set_bool(lua, actor, field, enabled)?;
    for child in actor_direct_children(lua, actor)? {
        capture_actor_depth_recursive(lua, &child, field, enabled)?;
    }
    Ok(())
}

pub fn make_actor_chain_method(lua: &Lua, actor: &Table) -> mlua::Result<Function> {
    let actor = actor.clone();
    lua.create_function(move |_, _args: IgnoredArgs| Ok(actor.clone()))
}

pub fn make_actor_stop_tweening_method(lua: &Lua, actor: &Table) -> mlua::Result<Function> {
    let actor = actor.clone();
    lua.create_function(move |lua, _args: IgnoredArgs| {
        prepare_capture_scope_actor(lua, &actor)?;
        let position = [
            actor_current_position(lua, &actor, 0)?,
            actor_current_position(lua, &actor, 1)?,
            actor_current_position(lua, &actor, 2)?,
        ];
        clear_spin_queue(&actor, false)?;
        clear_replay_tweens(lua, &actor, false)?;
        flush_actor_capture(&actor)?;
        clear_actor_queue(lua, &actor)?;
        stop_pending_tweens(lua, &actor, position);
        reset_actor_capture(lua, &actor)?;
        // StopTweening removes the back state; relative setters now start at
        // the current position, including a later child's pre-update pose.
        if !live_actor_playback(lua) || !has_tween_replay(lua, &actor) {
            for (key, value) in POSITION_DEST_KEYS.into_iter().zip(position) {
                actor.set(key, value)?;
            }
        }
        Ok(actor.clone())
    })
}

pub fn make_actor_finish_tweening_method(lua: &Lua, actor: &Table) -> mlua::Result<Function> {
    let actor = actor.clone();
    lua.create_function(move |lua, _args: IgnoredArgs| {
        finish_actor_tree_tweening(lua, &actor)?;
        Ok(actor.clone())
    })
}

fn finish_actor_tree_tweening(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    prepare_capture_scope_actor(lua, actor)?;
    clear_spin_queue(actor, true)?;
    clear_replay_tweens(lua, actor, true)?;
    clear_actor_queue(lua, actor)?;
    finish_actor_tweening(lua, actor)?;
    // ActorFrame::FinishTweening finishes self, then each direct child.
    // Queued commands are discarded, never dispatched by FinishTweening.
    for child in actor_direct_children(lua, actor)? {
        finish_actor_tree_tweening(lua, &child)?;
    }
    Ok(())
}

fn clear_actor_queue(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    if let Some(mut frames) = lua.app_data_mut::<SongLuaCompileFrames>() {
        frames.clocks.remove(&(actor.to_pointer() as usize));
    }
    for key in [
        "__songlua_command_queue",
        "__songlua_command_queue_starts",
        "__songlua_command_queue_steps",
    ] {
        if let Some(queue) = actor.get::<Option<Table>>(key)? {
            queue.clear()?;
        }
    }
    actor.set("__songlua_command_queue_deferred", Value::Nil)?;
    actor.set("__songlua_recurring_update_command", Value::Nil)?;
    actor.set("__songlua_recurring_update_time_left", Value::Nil)?;
    actor.set("__songlua_startup_queue_seconds", Value::Nil)?;
    invalidate_compile_update_plan(lua);
    Ok(())
}

pub fn make_actor_wrap_width_method(lua: &Lua, actor: &Table) -> mlua::Result<Function> {
    let actor = actor.clone();
    lua.create_function(move |lua, CheckedPrefix((first, second, _third, _rest)): NumericArgs| {
        let value = if matches!(first, Value::Table(_)) { &second } else { &first };
        let Some(value) = read_f32(value.clone()) else {
            return Ok(actor.clone());
        };
        let wrap = value as i32;
        if wrap >= 0 {
            capture_block_set_i32(lua, &actor, "wrap_width_pixels", wrap)?;
        }
        Ok(actor.clone())
    })
}

pub fn make_actor_set_size_method(lua: &Lua, actor: &Table) -> mlua::Result<Function> {
    let actor = actor.clone();
    lua.create_function(move |lua, CheckedPrefix((first, second, third, _rest)): NumericArgs| {
        let (width, height) = if matches!(first, Value::Table(_)) {
            (&second, &third)
        } else {
            (&first, &second)
        };
        let Some(width) = read_f32(width.clone()) else {
            return Ok(actor.clone());
        };
        let Some(height) = read_f32(height.clone()) else {
            return Ok(actor.clone());
        };
        capture_block_set_size(lua, &actor, [width, height])?;
        Ok(actor.clone())
    })
}

// Actor.cpp's FArg property setters narrow Lua numbers to float without
// discarding NaN or infinity. Keep duration and other validated inputs on read_f32.
fn read_actor_scalar_f32(value: Value) -> Option<f32> {
    match value { Value::Number(value) => Some(value as f32), other => read_f32(other) }
}

pub fn make_actor_capture_f32_method(
    lua: &Lua,
    actor: &Table,
    key: &'static str,
    probe_name: Option<&'static str>,
) -> mlua::Result<Function> {
    let actor = actor.clone();
    lua.create_function(move |lua, (_self, value): (Option<Value>, Option<Value>)| {
        if let Some(probe_name) = probe_name {
            record_probe_method_call(lua, &actor, probe_name)?;
        }
        if let Some(value) = value.and_then(read_actor_scalar_f32) {
            capture_block_set_f32(lua, &actor, key, value)?;
        }
        Ok(actor.clone())
    })
}

pub fn make_actor_add_f32_method(
    lua: &Lua,
    actor: &Table,
    key: &'static str,
) -> mlua::Result<Function> {
    let actor = actor.clone();
    let state_key = actor_capture_state_key(key);
    lua.create_function(move |lua, (_self, delta): (Option<Value>, Option<Value>)| {
        let Some(delta) = delta.and_then(read_f32) else {
            return Ok(actor.clone());
        };
        let captured = if live_actor_playback(lua) { None }
            else { actor_current_capture_block(lua, &actor)?.get::<Option<f32>>(key)? };
        let current = match live_destination_scalar(lua, &actor, key) {
            Some(value) => value,
            None => captured.or(actor.get::<Option<f32>>(state_key.as_ref())?).unwrap_or(0.0),
        };
        capture_block_set_f32(lua, &actor, key, current + delta)?;
        Ok(actor.clone())
    })
}

pub fn make_actor_tween_method(
    lua: &Lua,
    actor: &Table,
    easing: Option<&'static str>,
) -> mlua::Result<Function> {
    let actor = actor.clone();
    lua.create_function(move |lua, CheckedPrefix((_first, value, _third, _rest)): NumericArgs| {
        let duration = read_f32(value.clone())
            .unwrap_or(0.0)
            .max(0.0);
        let exact_duration = read_f64(value.clone())
            .unwrap_or(0.0)
            .max(0.0);
        begin_actor_tween(lua, &actor, duration, exact_duration, easing)
    })
}

// Actor::BeginTweening finishes an overflowing queue before adding its 52nd
// state. Sleep adds two states; queued commands add one zero-duration state.
fn finish_overflowing_tween_queue(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    let retained = lua.app_data_ref::<ActorTweenReplays>().and_then(|replays|
        replays.0.get(&(actor.to_pointer() as usize)).map(|replay| replay.queue.len()));
    let count = if let Some(count) = retained { count }
        else if let Some(frames) = lua.app_data_ref::<SongLuaCompileFrames>() {
            frames.clocks.get(&(actor.to_pointer() as usize)).map_or(0, |clock| clock.steps.len())
        } else {
            actor.raw_get::<Option<Table>>("__songlua_command_queue_steps")?
                .map_or(0, |steps| steps.raw_len())
        };
    if count > 50 {
        log::warn!("Lua: tween overflow; finishing actor queue");
        finish_actor_tree_tweening(lua, actor)?;
    }
    Ok(())
}

fn begin_actor_tween(lua: &Lua, actor: &Table, duration: f32, exact_duration: f64,
    easing: Option<&str>) -> mlua::Result<Table> {
    prepare_capture_scope_actor(lua, actor)?;
    finish_overflowing_tween_queue(lua, actor)?;
    if live_actor_playback(lua) {
        queue_spin_tween(lua, actor, exact_duration, easing)?;
        queue_replay_tween(lua, actor, exact_duration, easing);
        return Ok(actor.clone());
    }
    flush_actor_capture(actor)?;
    let mut cursor = actor.get::<Option<f32>>("__songlua_capture_cursor")?.unwrap_or(0.0);
    queue_spin_tween(lua, actor, exact_duration, easing)?;
    queue_replay_tween(lua, actor, exact_duration, easing);
    if let Some(delay) = record_queue_step(lua, actor, Some(exact_duration))? {
        cursor = delay;
        actor.set("__songlua_capture_cursor", cursor)?;
    }
    actor.set("__songlua_capture_duration", duration)?;
    actor.set("__songlua_capture_tween_time_left", cursor + duration)?;
    actor.set("__songlua_capture_easing", easing)?;
    actor.set("__songlua_capture_opt1", Value::Nil)?;
    actor.set("__songlua_capture_opt2", Value::Nil)?;
    // BeginTweening immediately owns an empty destination state as well.
    actor_current_capture_block(lua, actor)?;
    Ok(actor.clone())
}

fn make_native_actor_tween_method(lua: &Lua, actor: &Table) -> mlua::Result<Function> {
    let actor = actor.clone();
    lua.create_function(move |lua, args: MultiValue| {
        let duration = lua.coerce_number(args.get(1).cloned().unwrap_or(Value::Nil))?
            .ok_or_else(|| mlua::Error::runtime("tween duration must be a number"))? as f32;
        if duration < 0.0 {
            log::warn!("Lua: tween({duration}): tween time must not be negative");
            return Ok(actor.clone());
        }
        if duration.is_nan() { return Err(mlua::Error::runtime("invalid tween duration")); }
        let kind = crate::song_tables::option_enum(args.get(2),
            &["TweenType_Linear", "TweenType_Accelerate", "TweenType_Decelerate",
                "TweenType_Spring", "TweenType_Bezier"], "TweenType")?;
        let bezier;
        let easing = match kind {
            0 => "linear", 1 => "inQuad", 2 => "outQuad", 3 => "spring",
            _ => {
                let Some(Value::Table(controls)) = args.get(3) else {
                    return Err(mlua::Error::runtime("Bezier tween requires a table"));
                };
                let count = controls.raw_len();
                if count != 4 && count != 8 {
                    log::warn!("Tween::CreateFromStack: table argument must have 4 or 8 entries");
                    return Ok(actor.clone());
                }
                let mut points = Vec::with_capacity(count);
                for index in 1..=count {
                    // ITween uses lua_tonumber: nonnumeric entries become 0.
                    let value = lua.coerce_number(controls.raw_get::<Value>(index)?)?
                        .unwrap_or(0.0) as f32;
                    points.push(value.to_string());
                }
                bezier = format!("bezier({})", points.join(","));
                &bezier
            }
        };
        begin_actor_tween(lua, &actor, duration, f64::from(duration), Some(easing))
    })
}

// Most actor command keys fit here. Long keys get one exact-sized allocation.
enum ActorCommandName {
    Inline { bytes: [u8; 128], len: usize },
    Heap(String),
}

impl ActorCommandName {
    fn new(name: &str, suffix: &str) -> Self {
        let len = name.len() + suffix.len();
        if len <= 128 {
            let mut bytes = [0; 128];
            bytes[..name.len()].copy_from_slice(name.as_bytes());
            bytes[name.len()..len].copy_from_slice(suffix.as_bytes());
            Self::Inline { bytes, len }
        } else {
            Self::Heap(Self::owned(name, suffix))
        }
    }

    fn owned(name: &str, suffix: &str) -> String {
        let mut key = String::with_capacity(name.len() + suffix.len());
        key.push_str(name);
        key.push_str(suffix);
        key
    }

    fn as_str(&self) -> &str {
        match self {
            Self::Inline { bytes, len } => {
                std::str::from_utf8(&bytes[..*len]).expect("concatenated UTF-8 strings")
            }
            Self::Heap(key) => key.as_str(),
        }
    }
}

impl std::ops::Deref for ActorCommandName {
    type Target = str;

    fn deref(&self) -> &str {
        self.as_str()
    }
}

impl mlua::IntoLua for ActorCommandName {
    fn into_lua(self, lua: &Lua) -> mlua::Result<Value> {
        lua.create_string(self.as_str()).map(Value::String)
    }
}

pub fn install_actor_command_methods(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    actor.set(
        "SetTarget",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                let Some(Value::Table(target)) = args.get(1) else {
                    return Err(mlua::Error::RuntimeError("ActorProxy.SetTarget requires an Actor".into()));
                };
                if target.raw_get::<Option<String>>("__songlua_actor_type")?.is_none() {
                    return Err(mlua::Error::RuntimeError("ActorProxy.SetTarget requires an Actor".into()));
                }
                set_proxy_target_fields(&actor, target)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    if actor_type_is(actor, "ActorProxy")? {
        actor.set(
            "GetTarget",
            lua.create_function({
                let actor = actor.clone();
                move |_, _args: IgnoredArgs| actor.raw_get::<Value>("__songlua_proxy_target_actor")
            })?,
        )?;
    }
    actor.set(
        "visible",
        lua.create_function({
            let actor = actor.clone();
            move |lua, (_self, value): (Option<Value>, Option<Value>)| {
                if let Some(value) = value.and_then(read_boolish) {
                    prepare_capture_scope_actor(lua, &actor)?;
                    actor.set("__songlua_visible", value)?;
                    capture_block_set_bool(lua, &actor, "visible", value)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "addcommand",
        lua.create_function({
            let actor = actor.clone();
            move |lua, (_self, name, function): (Table, String, Function)| {
                let command_name = ActorCommandName::new(&name, "Command");
                if crate::live::active(lua) {
                    let commands = match actor.raw_get::<Option<Table>>("__songlua_dynamic_commands")? {
                        Some(commands) => commands,
                        None => {
                            let commands = lua.create_table()?;
                            actor.raw_set("__songlua_dynamic_commands", commands.clone())?;
                            commands
                        }
                    };
                    commands.raw_set(command_name.as_str(), function.clone())?;
                }
                actor.set(command_name, function)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "removecommand",
        lua.create_function({
            let actor = actor.clone();
            move |_, (_self, name): (Table, String)| {
                let command_name = ActorCommandName::new(&name, "Command");
                if let Some(commands) = actor.raw_get::<Option<Table>>("__songlua_dynamic_commands")? {
                    commands.raw_set(command_name.as_str(), Value::Nil)?;
                }
                actor.set(command_name, Value::Nil)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "GetCommand",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                let Some(name) = method_arg(&args, 0).cloned().and_then(read_string) else {
                    return Ok(Value::Nil);
                };
                Ok(lookup_actor_named_command(&actor, &ActorCommandName::new(&name, "Command"))?
                    .map_or(Value::Nil, |(command, _)| Value::Function(command)))
            }
        })?,
    )?;
    actor.set(
        "GetTweenTimeLeft",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: IgnoredArgs| actor_tween_time_left(lua, &actor)
        })?,
    )?;
    actor.set(
        "sleep",
        lua.create_function({
            let actor = actor.clone();
            move |lua, (_self, duration): (Option<Value>, Option<Value>)| {
                let trace_arg = if log::log_enabled!(target: "deadsync_song_lua::trace", log::Level::Trace)
                    && crate::live::active(lua) { duration.clone() } else { None };
                prepare_capture_scope_actor(lua, &actor)?;
                flush_actor_capture(&actor)?;
                // Actor::Sleep retains positive infinite native float durations.
                let exact_duration = duration
                    .as_ref()
                    .cloned()
                    .and_then(|value| match value {
                        Value::Number(value) if value == f64::INFINITY => Some(value),
                        value => read_f64(value),
                    })
                    .unwrap_or(0.0)
                    .max(0.0);
                begin_actor_tween(lua, &actor, exact_duration as f32, exact_duration, Some("linear"))?;
                begin_actor_tween(lua, &actor, 0.0, 0.0, Some("linear"))?;
                crate::live::trace_actor_call(lua, &actor, "Actor.sleep", trace_arg.as_ref());
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "hibernate",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                // A direct hibernate call replaces the theme's initial sleep,
                // including hibernate(0), which wakes an engine HUD actor.
                actor.raw_set("__songlua_theme_hibernating", false)?;
                let duration = args
                    .get(1)
                    .cloned()
                    .and_then(|value| match value {
                        // Simply Love uses math.huge for engine HUD actors.
                        Value::Number(value) => Some(value as f32),
                        value => read_f32(value),
                    })
                    .unwrap_or(0.0);
                actor.raw_set("__songlua_hibernate_seconds", duration)?;
                // Actor::SetHibernate replaces an independent timer; it does not
                // append a tween or change the actor's own visibility.
                capture_immediate_bool(lua, &actor, "hibernating", duration > 0.0)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "linear",
        make_actor_tween_method(lua, actor, Some("linear"))?,
    )?;
    actor.set(
        "accelerate",
        make_actor_tween_method(lua, actor, Some("inQuad"))?,
    )?;
    actor.set(
        "decelerate",
        make_actor_tween_method(lua, actor, Some("outQuad"))?,
    )?;
    actor.set(
        "smooth",
        make_actor_tween_method(lua, actor, Some("smooth"))?,
    )?;
    actor.set(
        "spring",
        make_actor_tween_method(lua, actor, Some("spring"))?,
    )?;
    actor.set("tween", make_native_actor_tween_method(lua, actor)?)?;
    actor.set(
        "bouncebegin",
        make_actor_tween_method(lua, actor, Some("bouncebegin"))?,
    )?;
    actor.set(
        "bounceend",
        make_actor_tween_method(lua, actor, Some("bounceend"))?,
    )?;
    actor.set(
        "queuecommand",
        lua.create_function({
            let actor = actor.clone();
            move |lua, (_self, name): (Option<Value>, Option<Value>)| {
                let trace_arg = if log::log_enabled!(target: "deadsync_song_lua::trace", log::Level::Trace)
                    && crate::live::active(lua) { name.clone() } else { None };
                let Some(name) = name.and_then(read_string) else {
                    return Ok(actor.clone());
                };
                prepare_capture_scope_actor(lua, &actor)?;
                // Init/On queues become native tween replay steps. Preserve a
                // self-queue there too; an interval alone loses its command
                // when replay takes ownership of the preceding sleep.
                if has_tween_replay(lua, &actor)
                    || lua.app_data_ref::<SongLuaStartupQueues>().is_some()
                {
                    enqueue_actor_command(lua, &actor, &name)?;
                    crate::live::trace_actor_call(lua, &actor, "Actor.queuecommand", trace_arg.as_ref());
                    return Ok(actor.clone());
                }
                let active = actor_active_commands(lua, &actor)?;
                let command = ActorCommandName::new(&name, "Command");
                if active
                    .get::<Option<bool>>(command.as_str())?
                    .unwrap_or(false)
                {
                    flush_actor_capture(&actor)?;
                    actor.set("__songlua_recurring_update_command", command)?;
                    invalidate_compile_update_plan(lua);
                    let cursor = actor
                        .get::<Option<f32>>("__songlua_capture_cursor")?
                        .unwrap_or(0.0);
                    let start = actor
                        .get::<Option<f32>>("__songlua_recurring_update_start_cursor")?
                        .unwrap_or(cursor);
                    let interval = actor
                        .get::<Option<f64>>("__songlua_recurring_update_exact_interval")?
                        .filter(|interval| *interval > f64::EPSILON)
                        .unwrap_or_else(|| f64::from((cursor - start).max(0.0)));
                    if interval > f64::EPSILON {
                        actor.set("__songlua_recurring_update_interval", interval)?;
                        if lua.app_data_ref::<SongLuaCompileFrames>().is_none()
                            && actor
                                .get::<Option<f64>>("__songlua_recurring_update_time_left")?
                                .is_none()
                        {
                            // Startup capture has already run the first queued
                            // callback. Its next deadline includes the delay
                            // before that callback, not just the new interval.
                            actor.set(
                                "__songlua_recurring_update_time_left",
                                actor
                                    .get::<Option<f64>>("__songlua_startup_queue_seconds")?
                                    .unwrap_or_else(|| f64::from(start.max(0.0)) + interval),
                            )?;
                        }
                    }
                    crate::live::trace_actor_call(lua, &actor, "Actor.queuecommand", trace_arg.as_ref());
                    return Ok(actor.clone());
                }
                enqueue_actor_command(lua, &actor, &name)?;
                crate::live::trace_actor_call(lua, &actor, "Actor.queuecommand", trace_arg.as_ref());
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "queuemessage",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if let Some(message) = args.get(1).cloned().and_then(read_string) {
                    enqueue_actor_command(lua, &actor, &format!("!{message}"))?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "playcommand",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(name) = method_arg(&args, 0).cloned().and_then(read_string) else {
                    return Ok(actor.clone());
                };
                let params = method_arg(&args, 1).cloned();
                let command_name = ActorCommandName::owned(&name, "Command");
                if actor
                    .get::<Option<bool>>("__songlua_propagate_commands")?
                    .unwrap_or(false)
                {
                    run_named_command_on_children_recursively(lua, &actor, &command_name, params)?;
                } else {
                    run_actor_message_with_params(lua, &actor, &command_name, params)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "propagate",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                actor.set(
                    "__songlua_propagate_commands",
                    method_arg(&args, 0).is_some_and(truthy),
                )?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "propagatecommand",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(name) = method_arg(&args, 0).cloned().and_then(read_string) else {
                    return Ok(actor.clone());
                };
                let command = ActorCommandName::new(&name, "Command");
                run_named_command_on_children_recursively(
                    lua,
                    &actor,
                    &command,
                    method_arg(&args, 1).cloned(),
                )?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "playcommandonchildren",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(name) = method_arg(&args, 0).cloned().and_then(read_string) else {
                    return Ok(actor.clone());
                };
                let params = method_arg(&args, 1).cloned();
                let command = ActorCommandName::new(&name, "Command");
                for child in actor_direct_children(lua, &actor)? {
                    run_actor_named_command_with_drain_and_params(
                        lua,
                        &child,
                        &command,
                        true,
                        params.clone(),
                    )?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "playcommandonleaves",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(name) = method_arg(&args, 0).cloned().and_then(read_string) else {
                    return Ok(actor.clone());
                };
                let params = method_arg(&args, 1).cloned();
                run_named_command_on_leaves(
                    lua,
                    &actor,
                    &ActorCommandName::new(&name, "Command"),
                    params,
                )?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "RemoveChild",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(name) = method_arg(&args, 0).cloned().and_then(read_string) else {
                    return Ok(actor.clone());
                };
                remove_actor_child(lua, &actor, &name)?;
                invalidate_compile_update_plan(lua);
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "RemoveAllChildren",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: IgnoredArgs| {
                remove_all_actor_children(lua, &actor)?;
                invalidate_compile_update_plan(lua);
                Ok(actor.clone())
            }
        })?,
    )?;
    Ok(())
}

fn load_actor_texture(
    lua: &Lua,
    actor: &Table,
    value: Option<&Value>,
    clear_on_nil: bool,
) -> mlua::Result<()> {
    prepare_capture_scope_actor(lua, actor)?;
    let previous = actor_texture_path(actor)?;
    set_actor_texture_from_value(actor, value, clear_on_nil)?;
    // Sprite::Load replaces custom states; SetTexture preserves them.
    actor.raw_set("__songlua_state_sprite_default_states", true)?;
    if let Some((cols, rows)) = actor_sprite_sheet_dims(actor)? {
        actor.raw_set(
            "__songlua_state_sprite_frame_sheet",
            lua.create_sequence_from([cols, rows])?,
        )?;
    }
    if let Some((width, height)) = actor_image_frame_size(actor)? {
        capture_immediate_vec2(lua, actor, "size", [width, height])?;
        capture_immediate_bool(lua, actor, "sprite_texture", true)?;
    }
    capture_sprite_texture(lua, actor, previous)
}

pub fn install_actor_texture_load_methods(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    actor.set(
        "LoadTexture",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                prepare_capture_scope_actor(lua, &actor)?;
                let value = method_arg(&args, 0).cloned().unwrap_or(Value::Nil);
                if value.is_nil() {
                    // LunaActorMultiVertex::LoadTexture unloads the resource and
                    // loads RageTexture_Default without changing the mesh.
                    set_actor_texture_from_value(&actor, Some(&Value::Nil), true)?;
                    actor.raw_set("__songlua_default_texture", true)?;
                } else {
                    let path = lua.coerce_string(value)?
                        .ok_or_else(|| mlua::Error::runtime("LoadTexture: string expected"))?;
                    set_actor_texture_from_value(&actor, Some(&Value::String(path)), false)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "SetTexture",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                prepare_capture_scope_actor(lua, &actor)?;
                let previous = actor_texture_path(&actor)?;
                if actor
                    .raw_get::<Option<Table>>("__songlua_state_sprite_frame_sheet")?
                    .is_none()
                    && let Some((cols, rows)) = actor_sprite_sheet_dims(&actor)?
                {
                    // SetTexture preserves existing state's UV rectangles.
                    actor.raw_set(
                        "__songlua_state_sprite_frame_sheet",
                        lua.create_sequence_from([cols, rows])?,
                    )?;
                }
                set_actor_texture_from_value(&actor, method_arg(&args, 0), false)?;
                let size = match method_arg(&args, 0) {
                    Some(Value::Table(texture)) => {
                        if let Some(size) =
                            texture.get::<Option<Table>>("__songlua_source_frame_size")?
                        {
                            table_vec2(&size).map(|[width, height]| (width, height))
                        } else {
                            actor_image_frame_size(&actor)?
                        }
                    }
                    _ => actor_image_frame_size(&actor)?,
                };
                if let Some((width, height)) = size {
                    capture_immediate_vec2(lua, &actor, "size", [width, height])?;
                }
                let bound = actor.raw_get::<bool>("__songlua_default_texture")?
                    || actor_aft_capture_name(&actor)?.is_some()
                    || actor_texture_path(&actor)?.is_some();
                capture_immediate_bool(lua, &actor, "sprite_texture", bound)?;
                capture_sprite_texture(lua, &actor, previous)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    for name in ["Load", "LoadBanner", "LoadBackground"] {
        actor.set(
            name,
            lua.create_function({
                let actor = actor.clone();
                let method_name = name;
                move |lua, args: MultiValue| {
                    if method_name == "Load" && actor_type_is(&actor, "RollingNumbers")? {
                        if let Some(metric) = method_arg(&args, 0).cloned().and_then(read_string) {
                            set_rolling_numbers_metric(&actor, &metric)?;
                        }
                        return Ok(Value::Table(actor.clone()));
                    }
                    if method_name == "Load" && actor_type_is(&actor, "GraphDisplay")? {
                        if let Some(metric) = method_arg(&args, 0).cloned().and_then(read_string) {
                            actor.set("__songlua_graph_display_metric", metric)?;
                        }
                        return Ok(Value::Table(actor.clone()));
                    }
                    if method_name == "Load" && actor_type_is(&actor, "Sound")? {
                        set_actor_sound_file_from_value(&actor, method_arg(&args, 0), true)?;
                        return Ok(Value::Table(actor.clone()));
                    }
                    if method_name == "Load" {
                        load_actor_texture(lua, &actor, method_arg(&args, 0), true)?;
                        return Ok(Value::Table(actor.clone()));
                    }
                    let value = method_arg(&args, 0).cloned().unwrap_or(Value::Nil);
                    let Some(path) = lua.coerce_string(value)? else {
                        return Err(mlua::Error::runtime(format!(
                            "{method_name}: string expected"
                        )));
                    };
                    load_actor_texture(lua, &actor, Some(&Value::String(path)), false)?;
                    // LunaSprite returns 1 without pushing self, so the
                    // existing last argument is the result, including nil.
                    Ok(args.back().cloned().unwrap_or(Value::Nil))
                }
            })?,
        )?;
    }
    actor.set(
        "LoadFromCached",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let path = method_arg(&args, 1)
                    .or_else(|| method_arg(&args, 0))
                    .filter(|value| !matches!(value, Value::Nil));
                load_actor_texture(lua, &actor, path, true)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    for name in [
        "LoadFromCachedBanner",
        "LoadFromCachedBackground",
        "LoadFromCachedJacket",
    ] {
        actor.set(
            name,
            lua.create_function({
                let actor = actor.clone();
                move |_, args: MultiValue| {
                    set_actor_texture_from_value(&actor, method_arg(&args, 0), true)?;
                    Ok(actor.clone())
                }
            })?,
        )?;
    }
    actor.set(
        "LoadFromCurrentSongBackground",
        // This is a theme Lua helper, not a Sprite C binding. Preserve its
        // real caller provenance for the queries it performs internally.
        lua.load(r#"
return function(self)
    local song = GAMESTATE:GetCurrentSong()
    if not song then
        local trail = GAMESTATE:GetCurrentTrail(GAMESTATE:GetMasterPlayerNumber())
        local entries = trail:GetTrailEntries()
        if #entries > 0 then song = entries[1]:GetSong() end
    end
    if not song then return self end
    self:LoadFromSongBackground(song)
    return self
end
"#).set_name("@theme:/Scripts/02 Sprite.lua").eval::<Function>()?,
    )?;
    actor.set("LoadFromSong", lua.create_function({
        let actor = actor.clone();
        move |lua, args: MultiValue| {
            let path = if let Some(Value::Table(song)) = method_arg(&args, 0)
                && truthy(&call_table_method(song, "HasBanner")?)
            {
                call_table_method(song, "GetBannerPath")?.as_string()
                    .map(|path| path.to_string_lossy())
            } else { None };
            let path = match path.filter(|path| !path.is_empty()) {
                Some(path) => path,
                None => load_native_blank_texture()?.to_owned(),
            };
            load_actor_texture(lua, &actor, Some(&Value::String(lua.create_string(&path)?)), false)?;
            actor.set("__songlua_state_banner_scrolling", false)?;
            actor.set("__songlua_state_banner_scroll_percent", 0.0_f32)?;
            Ok(actor.clone())
        }
    })?)?;
    for (name, path_methods) in [
        (
            "LoadFromCourse",
            &["GetBannerPath", "GetBackgroundPath"][..],
        ),
        ("LoadFromSongBackground", &["GetBackgroundPath"][..]),
    ] {
        actor.set(
            name,
            lua.create_function({
                let actor = actor.clone();
                move |_, args: MultiValue| {
                    set_actor_texture_from_path_methods(
                        &actor,
                        method_arg(&args, 0),
                        path_methods,
                    )?;
                    Ok(actor.clone())
                }
            })?,
        )?;
    }
    actor.set(
        "LoadFromSongGroup",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let group = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_string)
                    .unwrap_or_default();
                let Some(songman) = lua.globals().get::<Option<Table>>("SONGMAN")? else {
                    return Ok(actor.clone());
                };
                let Some(method) = songman.get::<Option<Function>>("GetSongGroupBannerPath")?
                else {
                    return Ok(actor.clone());
                };
                if let Value::String(path) = method.call::<Value>((songman, group))? {
                    if !set_actor_texture_from_path(&actor, &path.to_str()?)? {
                        set_actor_texture_from_path(
                            &actor,
                            &theme_path("G", "Banner", "group fallback"),
                        )?;
                    }
                } else {
                    set_actor_texture_from_path(
                        &actor,
                        &theme_path("G", "Banner", "group fallback"),
                    )?;
                }
                actor.set("__songlua_state_banner_scrolling", false)?;
                actor.set("__songlua_state_banner_scroll_percent", 0.0_f32)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "LoadFromSortOrder",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                let sort_order = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_string)
                    .unwrap_or_else(|| "SortOrder_Invalid".to_string());
                if let Some(path) = banner_sort_order_path(&sort_order) {
                    set_actor_texture_from_path(&actor, &path)?;
                }
                actor.set("__songlua_state_banner_scrolling", false)?;
                actor.set("__songlua_state_banner_scroll_percent", 0.0_f32)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    const CHARACTER_ICON_PATH_METHODS: &[&str] = &["GetIconPath"];
    const CHARACTER_CARD_PATH_METHODS: &[&str] = &["GetCardPath"];
    const UNLOCK_BANNER_PATH_METHODS: &[&str] = &["GetBannerFile"];
    const UNLOCK_BACKGROUND_PATH_METHODS: &[&str] = &["GetBackgroundFile"];
    for (name, path_methods, fallback_path) in [
        (
            "LoadIconFromCharacter",
            CHARACTER_ICON_PATH_METHODS,
            theme_path("G", "Common", "fallback banner"),
        ),
        (
            "LoadCardFromCharacter",
            CHARACTER_CARD_PATH_METHODS,
            theme_path("G", "Common", "fallback banner"),
        ),
        (
            "LoadBannerFromUnlockEntry",
            UNLOCK_BANNER_PATH_METHODS,
            theme_path("G", "Common", "fallback banner"),
        ),
        (
            "LoadBackgroundFromUnlockEntry",
            UNLOCK_BACKGROUND_PATH_METHODS,
            theme_path("G", "Common", "fallback banner"),
        ),
    ] {
        actor.set(
            name,
            lua.create_function({
                let actor = actor.clone();
                move |_, args: MultiValue| {
                    set_actor_texture_from_path_methods_or_fallback(
                        &actor,
                        method_arg(&args, 0),
                        path_methods,
                        &fallback_path,
                    )?;
                    actor.set("__songlua_state_banner_scrolling", false)?;
                    actor.set("__songlua_state_banner_scroll_percent", 0.0_f32)?;
                    Ok(actor.clone())
                }
            })?,
        )?;
    }
    actor.set(
        "SetScrolling",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                let scrolling = method_arg(&args, 0).is_some_and(truthy);
                let percent = method_arg(&args, 1)
                    .cloned()
                    .and_then(read_f32)
                    .unwrap_or(0.0);
                actor.set("__songlua_state_banner_scrolling", scrolling)?;
                actor.set("__songlua_state_banner_scroll_percent", percent)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "GetScrolling",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: IgnoredArgs| {
                Ok(actor
                    .get::<Option<bool>>("__songlua_state_banner_scrolling")?
                    .unwrap_or(false))
            }
        })?,
    )?;
    actor.set(
        "GetPercentScrolling",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: IgnoredArgs| {
                Ok(actor
                    .get::<Option<f32>>("__songlua_state_banner_scroll_percent")?
                    .unwrap_or(0.0))
            }
        })?,
    )?;
    actor.set(
        "SetTextureName",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let value = method_arg(&args, 0).cloned().unwrap_or(Value::Nil);
                let name = mlua::LuaString::from_lua(value, lua)?;
                actor.set("__songlua_aft_capture_name", name)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    Ok(())
}

pub fn install_actor_transform_methods(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    actor.set(
        "x",
        lua.create_function({
            let actor = actor.clone();
            move |lua, (_self, value): (Option<Value>, Option<Value>)| {
                record_probe_method_call(lua, &actor, "x")?;
                if let Some(value) = value.and_then(read_actor_scalar_f32) {
                    capture_block_set_f32(lua, &actor, "x", value)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "y",
        lua.create_function({
            let actor = actor.clone();
            move |lua, (_self, value): (Option<Value>, Option<Value>)| {
                record_probe_method_call(lua, &actor, "y")?;
                if let Some(value) = value.and_then(read_actor_scalar_f32) {
                    capture_block_set_f32(lua, &actor, "y", value)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "fov",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if let Some(value) = args.get(1).cloned().and_then(read_f32) {
                    capture_block_set_f32(lua, &actor, "fov", value)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "SetFOV",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if let Some(value) = method_arg(&args, 0).cloned().and_then(read_f32) {
                    capture_block_set_f32(lua, &actor, "fov", value)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "SetUpdateRate",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let number = method_arg(&args, 0).cloned().unwrap_or(Value::Nil);
                let value = lua.coerce_number(number)?.ok_or_else(|| {
                    mlua::Error::runtime("ActorFrame:SetUpdateRate: number expected")
                })? as f32;
                if value <= 0.0 {
                    return Err(mlua::Error::runtime(format!(
                        "ActorFrame:SetUpdateRate({value:.6}) Update rate must be greater than 0."
                    )));
                }
                // ActorFrame's Lua binding rejects nonpositive values, while
                // its C++ setter ignores NaN and accepts positive infinity.
                if value > 0.0 {
                    actor.set("__songlua_update_rate", value)?;
                    invalidate_compile_update_plan(lua);
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "GetUpdateRate",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: IgnoredArgs| {
                Ok(actor
                    .get::<Option<f32>>("__songlua_update_rate")?
                    .unwrap_or(1.0))
            }
        })?,
    )?;
    actor.set(
        "vanishpoint",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(x) = args.get(1).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                let Some(y) = args.get(2).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                capture_block_set_vec2(lua, &actor, "vanishpoint", [x, y])?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "xy",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if let Some(x) = args.get(1).cloned().and_then(read_f32) {
                    capture_block_set_f32(lua, &actor, "x", x)?;
                }
                if let Some(y) = args.get(2).cloned().and_then(read_f32) {
                    capture_block_set_f32(lua, &actor, "y", y)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "halign",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if let Some(value) = method_arg(&args, 0).and_then(song_lua_halign_value) {
                    capture_block_set_f32(lua, &actor, "halign", value)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "valign",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if let Some(value) = method_arg(&args, 0).and_then(song_lua_valign_value) {
                    capture_block_set_f32(lua, &actor, "valign", value)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "align",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if let Some(value) = method_arg(&args, 0).and_then(song_lua_halign_value) {
                    capture_block_set_f32(lua, &actor, "halign", value)?;
                }
                if let Some(value) = method_arg(&args, 1).and_then(song_lua_valign_value) {
                    capture_block_set_f32(lua, &actor, "valign", value)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "vertalign",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if let Some(value) = method_arg(&args, 0).and_then(song_lua_valign_value) {
                    capture_block_set_f32(lua, &actor, "valign", value)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "horizalign",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(raw) = method_arg(&args, 0) else {
                    return Ok(actor.clone());
                };
                if let Some(value) = song_lua_halign_value(raw) {
                    capture_block_set_f32(lua, &actor, "halign", value)?;
                }
                if let Some(value) = song_lua_text_align_value(raw) {
                    capture_block_set_string(
                        lua,
                        &actor,
                        "text_align",
                        overlay_text_align_label(value),
                    )?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "Center",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: IgnoredArgs| {
                let (center_x, center_y) = song_lua_screen_center(lua)?;
                capture_block_set_f32(lua, &actor, "x", center_x)?;
                capture_block_set_f32(lua, &actor, "y", center_y)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "CenterX",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: IgnoredArgs| {
                let (center_x, _) = song_lua_screen_center(lua)?;
                capture_block_set_f32(lua, &actor, "x", center_x)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "CenterY",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: IgnoredArgs| {
                let (_, center_y) = song_lua_screen_center(lua)?;
                capture_block_set_f32(lua, &actor, "y", center_y)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "stretchto",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let rect = [
                    args.get(1).cloned().and_then(read_f32).unwrap_or(0.0),
                    args.get(2).cloned().and_then(read_f32).unwrap_or(0.0),
                    args.get(3).cloned().and_then(read_f32).unwrap_or(0.0),
                    args.get(4).cloned().and_then(read_f32).unwrap_or(0.0),
                ];
                capture_block_set_stretch(lua, &actor, rect)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "FullScreen",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: IgnoredArgs| {
                let (width, height) = song_lua_screen_size(lua)?;
                capture_block_set_stretch(lua, &actor, [0.0, 0.0, width, height])?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "cropright",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if let Some(value) = args.get(1).cloned().and_then(read_f32) {
                    capture_block_set_f32(lua, &actor, "cropright", value)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "cropleft",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if let Some(value) = args.get(1).cloned().and_then(read_f32) {
                    capture_block_set_f32(lua, &actor, "cropleft", value)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "diffusealpha",
        lua.create_function({
            let actor = actor.clone();
            move |lua, (_self, alpha): (Option<Value>, Option<Value>)| {
                // Actor::SetDiffuseAlpha retains infinities. Keep the raw
                // value for getters and captured state; rendering clamps it.
                let alpha = alpha.and_then(|value| match value {
                    Value::Number(value) if value.is_infinite() => Some(value as f32),
                    value => read_f32(value),
                });
                if let Some(alpha) = alpha {
                    let mut diffuse = actor_diffuse(lua, &actor)?;
                    diffuse[3] = alpha;
                    capture_block_set_color(lua, &actor, diffuse)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "basezoom",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if let Some(value) = args.get(1).cloned().and_then(read_f32) {
                    capture_block_set_f32(lua, &actor, "basezoom", value)?;
                    capture_block_set_zoom_axes(
                        lua,
                        &actor,
                        value,
                        "basezoom_x",
                        "basezoom_y",
                        "basezoom_z",
                    )?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "basezoomx",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if let Some(value) = args.get(1).cloned().and_then(read_f32) {
                    capture_block_set_f32(lua, &actor, "basezoom_x", value)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "basezoomy",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if let Some(value) = args.get(1).cloned().and_then(read_f32) {
                    capture_block_set_f32(lua, &actor, "basezoom_y", value)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "basezoomz",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if let Some(value) = args.get(1).cloned().and_then(read_f32) {
                    capture_block_set_f32(lua, &actor, "basezoom_z", value)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "zoom",
        lua.create_function({
            let actor = actor.clone();
            move |lua, (_self, value): (Option<Value>, Option<Value>)| {
                record_probe_method_call(lua, &actor, "zoom")?;
                if let Some(value) = value.and_then(read_actor_scalar_f32) {
                    capture_block_set_f32(lua, &actor, "zoom", value)?;
                    capture_block_set_zoom_axes(lua, &actor, value, "zoom_x", "zoom_y", "zoom_z")?;
                    actor_update_text_pre_zoom_flags(lua, &actor, true, true)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "SetUpdateFunction",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if !matches!(args.get(1), None | Some(Value::Nil | Value::Function(_))) {
                    return Err(mlua::Error::RuntimeError("SetUpdateFunction requires a function or nil".into()));
                }
                // Discovery visits queued commands before their native update.
                // Keep the currently installed callback active until the real
                // queue dispatch installs its replacement.
                let pending = lua.app_data_ref::<SongLuaQueuedStartup>().is_some();
                if pending
                    && !actor
                        .raw_get::<Option<bool>>("__songlua_update_pending")?
                        .unwrap_or(false)
                {
                    actor.raw_set(
                        "__songlua_update_previous",
                        actor.raw_get::<Value>("__songlua_update_function")?,
                    )?;
                }
                actor.raw_set("__songlua_update_pending", pending)?;
                if !pending {
                    actor.raw_set("__songlua_update_previous", Value::Nil)?;
                }
                match args.get(1).cloned() {
                    Some(Value::Function(function)) => {
                        actor.set("__songlua_update_function", function)?;
                    }
                    _ => {
                        actor.set("__songlua_update_function", Value::Nil)?;
                    }
                }
                invalidate_compile_update_plan(lua);
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "aux",
        lua.create_function({
            let actor = actor.clone();
            move |lua, (_self, value): (Option<Value>, Option<Value>)| {
                if let Some(value) = value.and_then(read_f32) {
                    capture_block_set_f32(lua, &actor, "aux", value)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "getaux",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _self: Option<Value>| {
                if let Some(aux) = live_current_value(lua, &actor, |current| current.aux) { return Ok(aux); }
                Ok(actor.get::<Option<f32>>("__songlua_aux")?.unwrap_or(0.0))
            }
        })?,
    )?;
    actor.set(
        "draworder",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if let Some(value) = method_arg(&args, 0).cloned().and_then(read_i32_value) {
                    capture_block_set_i32(lua, &actor, "draw_order", value)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "SetDrawFunction",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if !matches!(args.get(1), None | Some(Value::Nil | Value::Function(_))) {
                    return Err(mlua::Error::RuntimeError("SetDrawFunction requires a function or nil".into()));
                }
                if let Some(Value::Function(function)) = args.get(1).cloned() {
                    lua.globals()
                        .raw_set(crate::draw_capture::DRAW_INSTALLED_KEY, true)?;
                    actor.set("__songlua_draw_function", function)?;
                } else {
                    actor.set("__songlua_draw_function", Value::Nil)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "GetDrawFunction",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: IgnoredArgs| {
                Ok(actor
                    .get::<Option<Function>>("__songlua_draw_function")?
                    .map_or(Value::Nil, Value::Function))
            }
        })?,
    )?;
    Ok(())
}

pub fn install_actor_display_state_methods(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    actor.set(
        "Set",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if actor_type_is(&actor, "GraphDisplay")? {
                    actor.set("__songlua_graph_display_set", true)?;
                    let stage_stats = method_arg(&args, 0).cloned();
                    let player_stats = method_arg(&args, 1).cloned();
                    if let Some(stage_stats) = stage_stats.clone() {
                        actor.set("__songlua_graph_display_stage_stats", stage_stats)?;
                    }
                    if let Some(player_stats) = player_stats.clone() {
                        actor.set("__songlua_graph_display_player_stats", player_stats)?;
                    }
                    capture_graph_display_values(lua, &actor, stage_stats, player_stats)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "SetStreamWidth",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                if actor_type_is(&actor, "SongMeterDisplay")?
                    && let Some(width) = method_arg(&args, 0).cloned().and_then(read_f32)
                {
                    actor.set("StreamWidth", width)?;
                    actor.set("__songlua_stream_width", width)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "GetStreamWidth",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: IgnoredArgs| {
                Ok(actor
                    .get::<Option<f32>>("__songlua_stream_width")?
                    .unwrap_or(0.0))
            }
        })?,
    )?;
    actor.set(
        "SetFromGameState",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: IgnoredArgs| {
                if actor_type_is(&actor, "CourseContentsList")? {
                    actor.set("__songlua_course_contents_from_gamestate", true)?;
                    actor.set("__songlua_scroller_num_items", 1_i64)?;
                    populate_course_contents_display(lua, &actor)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "SetCurrentAndDestinationItem",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                if let Some(item) = method_arg(&args, 0).cloned().and_then(read_f32) {
                    actor.set("__songlua_scroller_current_item", item)?;
                    actor.set("__songlua_scroller_destination_item", item)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "SetCurrentItem",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                if let Some(item) = method_arg(&args, 0).cloned().and_then(read_f32) {
                    actor.set("__songlua_scroller_current_item", item)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "GetCurrentItem",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: IgnoredArgs| {
                Ok(actor
                    .get::<Option<f32>>("__songlua_scroller_current_item")?
                    .unwrap_or(0.0))
            }
        })?,
    )?;
    actor.set(
        "SetDestinationItem",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                if let Some(item) = method_arg(&args, 0).cloned().and_then(read_f32) {
                    actor.set("__songlua_scroller_destination_item", item.max(0.0))?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "GetDestinationItem",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: IgnoredArgs| {
                Ok(actor
                    .get::<Option<f32>>("__songlua_scroller_destination_item")?
                    .unwrap_or(0.0))
            }
        })?,
    )?;
    actor.set(
        "GetNumItems",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: IgnoredArgs| {
                Ok(actor
                    .get::<Option<i64>>("__songlua_scroller_num_items")?
                    .unwrap_or(1)
                    .max(0))
            }
        })?,
    )?;
    actor.set(
        "SetTransformFromFunction",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                if let Some(Value::Function(function)) = method_arg(&args, 0).cloned() {
                    actor.set("__songlua_scroller_transform_function", function)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "SetTransformFromHeight",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                if let Some(height) = method_arg(&args, 0).cloned().and_then(read_f32) {
                    actor.set("__songlua_scroller_item_height", height)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "SetTransformFromWidth",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                if let Some(width) = method_arg(&args, 0).cloned().and_then(read_f32) {
                    actor.set("__songlua_scroller_item_width", width)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "PositionItems",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: IgnoredArgs| {
                position_scroller_items(lua, &actor)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "GetSecondsToDestination",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: IgnoredArgs| {
                let current = actor
                    .get::<Option<f32>>("__songlua_scroller_current_item")?
                    .unwrap_or(0.0);
                let destination = actor
                    .get::<Option<f32>>("__songlua_scroller_destination_item")?
                    .unwrap_or(current);
                let seconds_per_item = actor
                    .get::<Option<f32>>("__songlua_scroller_seconds_per_item")?
                    .unwrap_or(0.0);
                Ok((destination - current).abs() * seconds_per_item)
            }
        })?,
    )?;
    actor.set(
        "SetSecondsPerItem",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                if let Some(seconds) = method_arg(&args, 0).cloned().and_then(read_f32) {
                    actor.set("__songlua_scroller_seconds_per_item", seconds.max(0.0))?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "SetLoop",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                if let Some(value) = method_arg(&args, 0).map(truthy) {
                    actor.set("__songlua_scroller_loop", value)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "GetLoop",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: IgnoredArgs| {
                Ok(actor
                    .get::<Option<bool>>("__songlua_scroller_loop")?
                    .unwrap_or(false))
            }
        })?,
    )?;
    actor.set(
        "SetPauseCountdownSeconds",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                if let Some(seconds) = method_arg(&args, 0).cloned().and_then(read_f32) {
                    actor.set("__songlua_scroller_pause_countdown", seconds.max(0.0))?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "SetSecondsPauseBetweenItems",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                if let Some(seconds) = method_arg(&args, 0).cloned().and_then(read_f32) {
                    actor.set("__songlua_scroller_pause_between", seconds.max(0.0))?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "GetSecondsPauseBetweenItems",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: IgnoredArgs| {
                Ok(actor
                    .get::<Option<f32>>("__songlua_scroller_pause_between")?
                    .unwrap_or(0.0))
            }
        })?,
    )?;
    actor.set(
        "SetNumSubdivisions",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                if let Some(count) = method_arg(&args, 0).cloned().and_then(read_i32_value) {
                    actor.set("__songlua_scroller_num_subdivisions", count.max(0))?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "ScrollThroughAllItems",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: IgnoredArgs| {
                let last_item = actor
                    .get::<Option<i64>>("__songlua_scroller_num_items")?
                    .unwrap_or(1)
                    .saturating_sub(1)
                    .max(0) as f32;
                actor.set("__songlua_scroller_destination_item", last_item)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "ScrollWithPadding",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                let before = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_f32)
                    .unwrap_or(0.0)
                    .max(0.0);
                let after = method_arg(&args, 1)
                    .cloned()
                    .and_then(read_f32)
                    .unwrap_or(0.0)
                    .max(0.0);
                let last_item = actor
                    .get::<Option<i64>>("__songlua_scroller_num_items")?
                    .unwrap_or(1)
                    .saturating_sub(1)
                    .max(0) as f32;
                actor.set("__songlua_scroller_current_item", -before)?;
                actor.set("__songlua_scroller_destination_item", last_item + after)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    for (name, key) in [
        ("SetFastCatchup", "__songlua_scroller_fast_catchup"),
        ("SetWrap", "__songlua_scroller_wrap"),
        ("SetMask", "__songlua_scroller_mask"),
    ] {
        actor.set(
            name,
            lua.create_function({
                let actor = actor.clone();
                move |_, args: MultiValue| {
                    actor.set(key, method_arg(&args, 0).is_some_and(truthy))?;
                    Ok(actor.clone())
                }
            })?,
        )?;
    }
    actor.set(
        "SetNumItemsToDraw",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                if let Some(count) = method_arg(&args, 0).cloned().and_then(read_i32_value) {
                    actor.set("__songlua_scroller_num_items_to_draw", count.max(0))?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "GetFullScrollLengthSeconds",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: IgnoredArgs| {
                let num_items = actor
                    .get::<Option<i64>>("__songlua_scroller_num_items")?
                    .unwrap_or(1)
                    .max(0) as f32;
                let seconds_per_item = actor
                    .get::<Option<f32>>("__songlua_scroller_seconds_per_item")?
                    .unwrap_or(0.0);
                let pause = actor
                    .get::<Option<f32>>("__songlua_scroller_pause_between")?
                    .unwrap_or(0.0);
                Ok((num_items - 1.0)
                    .max(0.0)
                    .mul_add(pause, num_items * seconds_per_item))
            }
        })?,
    )?;
    actor.set(
        "SetDrawState",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if let Some(Value::Table(state)) = method_arg(&args, 0).cloned() {
                    let destination = match actor.get::<Option<Table>>("__songlua_draw_state")? {
                        Some(destination) => destination,
                        None => lua.create_table()?,
                    };
                    for key in ["Mode", "First", "Num"] {
                        let value = state.get::<Value>(key)?;
                        if !value.is_nil() {
                            destination.set(key, value)?;
                        }
                    }
                    actor.set("__songlua_draw_state", destination)?;
                    if let Some(mode) = state.get::<Option<String>>("Mode")? {
                        actor.set("__songlua_draw_state_mode", mode)?;
                    }
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "GetDestDrawMode",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: IgnoredArgs| {
                Ok(actor.get::<Option<String>>("__songlua_draw_state_mode")?)
            }
        })?,
    )?;
    actor.set(
        "SetLineWidth",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                if let Some(width) = method_arg(&args, 0).cloned().and_then(read_f32) {
                    if width >= 0.0 {
                        actor.set("__songlua_line_width", width)?;
                    }
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "SetNumVertices",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                let count = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_i32_value)
                    .unwrap_or(0)
                    .max(0);
                actor.set("__songlua_vertex_count", i64::from(count))?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "SetVertices",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if let Some(Value::Table(vertices)) = method_arg(&args, 0).cloned() {
                    actor.set("__songlua_vertex_count", vertices.raw_len() as i64)?;
                    // Native AMV copies scalar vertices into its own buffer.
                    // Deep-cloning Lua's nested tables creates thousands of VM
                    // allocations per upload and makes catch-up frames stall.
                    let points = read_actor_multi_vertex_upload(lua, &vertices)?;
                    actor.set("__songlua_vertices", lua.create_any_userdata(points)?)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "GetNumVertices",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: IgnoredArgs| {
                if let Some(count) = actor.get::<Option<i64>>("__songlua_vertex_count")? {
                    return Ok(count.max(0));
                }
                Ok(actor
                    .get::<Option<Table>>("__songlua_vertices")?
                    .map(|vertices| vertices.raw_len() as i64)
                    .unwrap_or(0))
            }
        })?,
    )?;
    Ok(())
}

pub fn install_actor_crop_shadow_methods(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    actor.set(
        "cropbottom",
        make_actor_capture_f32_method(lua, actor, "cropbottom", None)?,
    )?;
    actor.set(
        "croptop",
        make_actor_capture_f32_method(lua, actor, "croptop", None)?,
    )?;
    actor.set(
        "fadeleft",
        make_actor_capture_f32_method(lua, actor, "fadeleft", None)?,
    )?;
    actor.set(
        "faderight",
        make_actor_capture_f32_method(lua, actor, "faderight", None)?,
    )?;
    actor.set(
        "fadetop",
        make_actor_capture_f32_method(lua, actor, "fadetop", None)?,
    )?;
    actor.set(
        "fadebottom",
        make_actor_capture_f32_method(lua, actor, "fadebottom", None)?,
    )?;
    actor.set(
        "shadowlength",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(value) = method_arg(&args, 0).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                capture_immediate_vec2(lua, &actor, "shadow_len", [value, -value])?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "shadowlengthx",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(value) = method_arg(&args, 0).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                let mut len = actor_shadow_len(lua, &actor)?;
                len[0] = value;
                capture_immediate_vec2(lua, &actor, "shadow_len", len)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "shadowlengthy",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(value) = method_arg(&args, 0).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                let mut len = actor_shadow_len(lua, &actor)?;
                len[1] = -value;
                capture_immediate_vec2(lua, &actor, "shadow_len", len)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "shadowcolor",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(color) = read_color_args(&args) else {
                    return Ok(actor.clone());
                };
                capture_immediate_vec4(lua, &actor, "shadow_color", color)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "MaskSource",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: IgnoredArgs| {
                capture_block_set_bool(lua, &actor, "mask_source", true)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "MaskDest",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: IgnoredArgs| {
                capture_block_set_bool(lua, &actor, "mask_dest", true)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "rotationx",
        make_actor_capture_f32_method(lua, actor, "rot_x_deg", Some("rotationx"))?,
    )?;
    actor.set(
        "rotationy",
        make_actor_capture_f32_method(lua, actor, "rot_y_deg", Some("rotationy"))?,
    )?;
    actor.set(
        "rotationz",
        make_actor_capture_f32_method(lua, actor, "rot_z_deg", Some("rotationz"))?,
    )?;
    for (axis, method) in ["baserotationx", "baserotationy", "baserotationz"]
        .into_iter()
        .enumerate()
    {
        let actor = actor.clone();
        actor.clone().set(
            method,
            lua.create_function(move |lua, (_self, value): (Option<Value>, Option<Value>)| {
                record_probe_method_call(lua, &actor, method)?;
                if let Some(value) = value.and_then(read_f32) {
                    let mut rotation = actor
                        .get::<Option<ActorVectorValue<3>>>("__songlua_state_base_rotation")?
                        .and_then(|table| table.0)
                        .unwrap_or([0.0; 3]);
                    rotation[axis] = value;
                    capture_immediate_vec3(lua, &actor, "base_rotation", rotation)?;
                }
                Ok(actor.clone())
            })?,
        )?;
    }
    actor.set(
        "zoomx",
        lua.create_function({
            let actor = actor.clone();
            move |lua, (_self, value): (Option<Value>, Option<Value>)| {
                record_probe_method_call(lua, &actor, "zoomx")?;
                if let Some(value) = value.and_then(read_actor_scalar_f32) {
                    capture_block_set_f32(lua, &actor, "zoom_x", value)?;
                    actor_update_text_pre_zoom_flags(lua, &actor, true, false)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "zoomy",
        lua.create_function({
            let actor = actor.clone();
            move |lua, (_self, value): (Option<Value>, Option<Value>)| {
                record_probe_method_call(lua, &actor, "zoomy")?;
                if let Some(value) = value.and_then(read_actor_scalar_f32) {
                    capture_block_set_f32(lua, &actor, "zoom_y", value)?;
                    actor_update_text_pre_zoom_flags(lua, &actor, false, true)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "zoomto",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(width) = args.get(1).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                let Some(height) = args.get(2).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                if actor_type_is(&actor, "BitmapText")? {
                    capture_block_set_size(lua, &actor, [width, height])?;
                } else {
                    let (base_width, base_height) = actor_base_size(&actor)?;
                    capture_block_set_f32(lua, &actor, "zoom_x", width / base_width)?;
                    capture_block_set_f32(lua, &actor, "zoom_y", height / base_height)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set("scaletoclipped", make_actor_set_size_method(lua, actor)?)?;
    actor.set("ScaleToClipped", make_actor_set_size_method(lua, actor)?)?;
    Ok(())
}

pub fn install_actor_extra_transform_methods(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    actor.set(
        "z",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                record_probe_method_call(lua, &actor, "z")?;
                if let Some(value) = args.get(1).cloned().and_then(read_f32) {
                    capture_block_set_f32(lua, &actor, "z", value)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set("addx", make_actor_add_f32_method(lua, actor, "x")?)?;
    actor.set("addy", make_actor_add_f32_method(lua, actor, "y")?)?;
    actor.set("addz", make_actor_add_f32_method(lua, actor, "z")?)?;
    actor.set(
        "addrotationx",
        make_actor_add_f32_method(lua, actor, "rot_x_deg")?,
    )?;
    actor.set(
        "addrotationy",
        make_actor_add_f32_method(lua, actor, "rot_y_deg")?,
    )?;
    actor.set(
        "addrotationz",
        make_actor_add_f32_method(lua, actor, "rot_z_deg")?,
    )?;
    for (name, key) in [("skewx", "skew_x"), ("skewy", "skew_y")] {
        actor.set(
            name,
            lua.create_function({
                let actor = actor.clone();
                let method_name = name.to_string();
                move |lua, (_self, value): (Option<Value>, Option<Value>)| {
                    record_probe_method_call(lua, &actor, &method_name)?;
                    if let Some(value) = value.and_then(read_f32) {
                        if !live_actor_playback(lua) || !has_tween_replay(lua, &actor) {
                            // Keep the compiler's pre-capture write ordering.
                            set_actor_capture_state(&actor, key, value)?;
                        }
                        capture_block_set_f32(lua, &actor, key, value)?;
                    }
                    Ok(actor.clone())
                }
            })?,
        )?;
    }
    actor.set(
        "zoomz",
        make_actor_capture_f32_method(lua, actor, "zoom_z", Some("zoomz"))?,
    )?;
    actor.set(
        "blend",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if let Some(raw) = args.get(1).cloned().and_then(read_string)
                    && let Some(blend) = parse_overlay_blend_mode(raw.as_str())
                {
                    let label = match blend {
                        SongLuaOverlayBlendMode::NoEffect => "noeffect",
                        SongLuaOverlayBlendMode::Alpha => "alpha",
                        SongLuaOverlayBlendMode::Add => "add",
                        SongLuaOverlayBlendMode::Multiply => "multiply",
                        SongLuaOverlayBlendMode::Subtract => "subtract",
                    };
                    capture_block_set_string(lua, &actor, "blend", label)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "glow",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(color) = read_color_args(&args) else {
                    return Ok(actor.clone());
                };
                capture_block_set_vec4(lua, &actor, "glow", color)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "effectmagnitude",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                capture_immediate_vec3(
                    lua,
                    &actor,
                    "effect_magnitude",
                    [
                        args.get(1).cloned().and_then(read_f32).unwrap_or(0.0),
                        args.get(2).cloned().and_then(read_f32).unwrap_or(0.0),
                        args.get(3).cloned().and_then(read_f32).unwrap_or(0.0),
                    ],
                )?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "customtexturerect",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let rect = [
                    method_arg(&args, 0)
                        .cloned()
                        .and_then(read_f32)
                        .unwrap_or(0.0),
                    method_arg(&args, 1)
                        .cloned()
                        .and_then(read_f32)
                        .unwrap_or(0.0),
                    method_arg(&args, 2)
                        .cloned()
                        .and_then(read_f32)
                        .unwrap_or(0.0),
                    method_arg(&args, 3)
                        .cloned()
                        .and_then(read_f32)
                        .unwrap_or(0.0),
                ];
                capture_texture_rect(lua, &actor, rect)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "SetCustomImageRect",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let rect = [
                    method_arg(&args, 0)
                        .cloned()
                        .and_then(read_f32)
                        .unwrap_or(0.0),
                    method_arg(&args, 1)
                        .cloned()
                        .and_then(read_f32)
                        .unwrap_or(0.0),
                    method_arg(&args, 2)
                        .cloned()
                        .and_then(read_f32)
                        .unwrap_or(0.0),
                    method_arg(&args, 3)
                        .cloned()
                        .and_then(read_f32)
                        .unwrap_or(0.0),
                ];
                capture_texture_rect(lua, &actor, rect)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "stretchtexcoords",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let dx = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_f32)
                    .unwrap_or(0.0);
                let dy = method_arg(&args, 1)
                    .cloned()
                    .and_then(read_f32)
                    .unwrap_or(0.0);
                offset_actor_texture_rect(lua, &actor, dx, dy)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    Ok(())
}

pub fn install_actor_sprite_animation_methods(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    actor.set(
        "setstate",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(state_index) = method_arg(&args, 0).cloned().and_then(read_u32_value)
                else {
                    return Ok(actor.clone());
                };
                set_actor_sprite_state(lua, &actor, state_index)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "GetState",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: IgnoredArgs| {
                Ok(i64::from(
                    song_lua_valid_sprite_state_index(
                        actor.get::<Option<u32>>("__songlua_state_sprite_state_index")?,
                    )
                    .unwrap_or(0),
                ))
            }
        })?,
    )?;
    actor.set(
        "SetStateProperties",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(Value::Table(states)) = method_arg(&args, 0) else {
                    return Ok(actor.clone());
                };
                let state_count = states.raw_len();
                if state_count == 0 {
                    return Ok(actor.clone());
                }

                let mut animation_length = 0.0_f32;
                let mut first_frame = None;
                let mut first_delay = None;
                for state in states.sequence_values::<Table>() {
                    let state = state?;
                    if first_frame.is_none() {
                        first_frame = state.get::<Option<u32>>("Frame")?;
                    }
                    let delay = state.get::<Option<f32>>("Delay")?.unwrap_or(0.0).max(0.0);
                    if first_delay.is_none() {
                        first_delay = Some(delay);
                    }
                    animation_length += delay;
                }

                actor.set(
                    "__songlua_sprite_animation_length_seconds",
                    animation_length,
                )?;
                if let Some(delay) = first_delay {
                    capture_block_set_f32(lua, &actor, "sprite_state_delay", delay)?;
                }
                set_actor_sprite_state(lua, &actor, first_frame.unwrap_or(0))?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "GetAnimationLengthSeconds",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: IgnoredArgs| {
                if let Some(value) =
                    actor.get::<Option<f32>>("__songlua_sprite_animation_length_seconds")?
                {
                    return Ok(value);
                }
                let delay = actor
                    .get::<Option<f32>>("__songlua_state_sprite_state_delay")?
                    .unwrap_or(0.1)
                    .max(0.0);
                Ok(actor_sprite_frame_count(&actor)? as f32 * delay)
            }
        })?,
    )?;
    actor.set(
        "SetSecondsIntoAnimation",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(seconds) = method_arg(&args, 0).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                set_actor_seconds_into_animation(lua, &actor, seconds)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "SetEffectMode",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                if let Some(mode) = method_arg(&args, 0).cloned().and_then(read_string) {
                    actor.set("__songlua_sprite_effect_mode", mode)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "animate",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let value = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_boolish)
                    .unwrap_or(true);
                capture_block_set_bool(lua, &actor, "sprite_animate", value)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "play",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: IgnoredArgs| {
                if actor_type_is(&actor, "Sound")? {
                    play_sound_actor(lua, &actor)?;
                } else {
                    capture_block_set_bool(lua, &actor, "sprite_animate", true)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "playforplayer",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: IgnoredArgs| {
                if actor_type_is(&actor, "Sound")? {
                    play_sound_actor(lua, &actor)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "pause",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: IgnoredArgs| {
                if !actor_type_is(&actor, "Sound")? {
                    capture_block_set_bool(lua, &actor, "sprite_animate", false)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "loop",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let value = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_boolish)
                    .unwrap_or(true);
                capture_block_set_bool(lua, &actor, "sprite_loop", value)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "rate",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if let Some(value) = method_arg(&args, 0).cloned().and_then(read_f32) {
                    capture_block_set_f32(lua, &actor, "sprite_playback_rate", value)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    for name in ["SetAllStateDelays", "setallstatedelays"] {
        actor.set(
            name,
            lua.create_function({
                let actor = actor.clone();
                move |lua, args: MultiValue| {
                    if let Some(value) = method_arg(&args, 0).cloned().and_then(read_f32) {
                        capture_block_set_f32(lua, &actor, "sprite_state_delay", value.max(0.0))?;
                    }
                    Ok(actor.clone())
                }
            })?,
        )?;
    }
    actor.set(
        "texcoordvelocity",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let velocity = [
                    args.get(1).cloned().and_then(read_f32).unwrap_or(0.0),
                    args.get(2).cloned().and_then(read_f32).unwrap_or(0.0),
                ];
                capture_block_set_vec2(lua, &actor, "texcoord_velocity", velocity)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "texturetranslate",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let offset = [
                    args.get(1).cloned().and_then(read_f32).unwrap_or(0.0),
                    args.get(2).cloned().and_then(read_f32).unwrap_or(0.0),
                ];
                capture_block_set_vec2(lua, &actor, "texcoord_offset", offset)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "texturewrapping",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let value = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_boolish)
                    .unwrap_or(true);
                capture_block_set_bool(lua, &actor, "texture_wrapping", value)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "SetDecodeMovie",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                let value = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_boolish)
                    .unwrap_or(true);
                actor.set("__songlua_state_decode_movie", value)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "GetDecodeMovie",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: IgnoredArgs| actor_decode_movie(&actor)
        })?,
    )?;
    Ok(())
}

pub fn install_actor_effect_methods(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    actor.set(
        "glowshift",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: IgnoredArgs| {
                set_actor_effect_defaults(
                    lua,
                    &actor,
                    "glowshift",
                    Some(1.0),
                    None,
                    Some([1.0, 1.0, 1.0, 0.2]),
                    Some([1.0, 1.0, 1.0, 0.8]),
                )?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "glowblink",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: IgnoredArgs| {
                set_actor_effect_defaults(
                    lua,
                    &actor,
                    "glowblink",
                    Some(1.0),
                    None,
                    Some([1.0, 1.0, 1.0, 0.2]),
                    Some([1.0, 1.0, 1.0, 0.8]),
                )?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "diffuseshift",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: IgnoredArgs| {
                set_actor_effect_defaults(
                    lua,
                    &actor,
                    "diffuseshift",
                    Some(1.0),
                    None,
                    Some([0.0, 0.0, 0.0, 1.0]),
                    Some([1.0, 1.0, 1.0, 1.0]),
                )?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "diffuseblink",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: IgnoredArgs| {
                set_actor_effect_defaults(
                    lua,
                    &actor,
                    "diffuseblink",
                    Some(1.0),
                    None,
                    Some([0.5, 0.5, 0.5, 0.5]),
                    Some([1.0, 1.0, 1.0, 1.0]),
                )?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "diffuseramp",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: IgnoredArgs| {
                set_actor_effect_defaults(
                    lua,
                    &actor,
                    "diffuseramp",
                    Some(1.0),
                    None,
                    Some([0.0, 0.0, 0.0, 1.0]),
                    Some([1.0, 1.0, 1.0, 1.0]),
                )?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "glowramp",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: IgnoredArgs| {
                set_actor_effect_defaults(
                    lua,
                    &actor,
                    "glowramp",
                    Some(1.0),
                    None,
                    Some([1.0, 1.0, 1.0, 0.2]),
                    Some([1.0, 1.0, 1.0, 0.8]),
                )?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "pulse",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: IgnoredArgs| {
                set_actor_effect_defaults(
                    lua,
                    &actor,
                    "pulse",
                    Some(2.0),
                    Some([0.5, 1.0, 1.0]),
                    None,
                    None,
                )?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "bob",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: IgnoredArgs| {
                set_actor_effect_defaults(
                    lua,
                    &actor,
                    "bob",
                    Some(2.0),
                    Some([0.0, 20.0, 0.0]),
                    None,
                    None,
                )?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "bounce",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: IgnoredArgs| {
                set_actor_effect_defaults(
                    lua,
                    &actor,
                    "bounce",
                    Some(2.0),
                    Some([0.0, 20.0, 0.0]),
                    None,
                    None,
                )?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "wag",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: IgnoredArgs| {
                set_actor_effect_defaults(
                    lua,
                    &actor,
                    "wag",
                    Some(2.0),
                    Some([0.0, 0.0, 20.0]),
                    None,
                    None,
                )?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "spin",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: IgnoredArgs| {
                set_actor_effect_defaults(
                    lua,
                    &actor,
                    "spin",
                    None,
                    Some([0.0, 0.0, 180.0]),
                    None,
                    None,
                )?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "effectcolor1",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let color = read_color_args(&args).unwrap_or([1.0, 1.0, 1.0, 1.0]);
                capture_immediate_vec4(lua, &actor, "effect_color1", color)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "effectcolor2",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let color = read_color_args(&args).unwrap_or([1.0, 1.0, 1.0, 1.0]);
                capture_immediate_vec4(lua, &actor, "effect_color2", color)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "effectperiod",
        lua.create_function({
            let actor = actor.clone();
            move |lua, (_self, value): (Option<Value>, Option<Value>)| {
                if let Some(value) = value.and_then(read_f32) {
                    capture_effect_period(lua, &actor, value)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "effectoffset",
        lua.create_function({
            let actor = actor.clone();
            move |lua, (_self, value): (Option<Value>, Option<Value>)| {
                if let Some(value) = value.and_then(read_f32) {
                    capture_immediate_f32(lua, &actor, "effect_offset", value)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "effecttiming",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(a) = method_arg(&args, 0).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                let Some(b) = method_arg(&args, 1).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                let Some(c) = method_arg(&args, 2).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                let Some(d) = method_arg(&args, 3).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                let timing = if let Some(e) = method_arg(&args, 4).cloned().and_then(read_f32) {
                    [a.max(0.0), b.max(0.0), c.max(0.0), e.max(0.0), d.max(0.0)]
                } else {
                    [a.max(0.0), b.max(0.0), c.max(0.0), 0.0, d.max(0.0)]
                };
                let total = timing.iter().sum::<f32>();
                if total > 0.0 {
                    capture_immediate_vec5(lua, &actor, "effect_timing", timing)?;
                    capture_immediate_f32(lua, &actor, "effect_period", total)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "effect_hold_at_full",
        lua.create_function({
            let actor = actor.clone();
            move |lua, (_self, hold): (Table, f32)| {
                let period = actor
                    .get::<Option<f32>>("__songlua_state_effect_period")?
                    .unwrap_or(1.0);
                let mut timing = actor
                    .get::<Option<ActorVectorValue<5>>>("__songlua_state_effect_timing")?
                    .and_then(|value| value.0)
                    .unwrap_or([period / 2.0, 0.0, period / 2.0, 0.0, 0.0]);
                timing[3] = hold;
                if hold < 0.0 || timing.iter().sum::<f32>() <= 0.0 {
                    return Err(mlua::Error::runtime(
                        "effect timings must be nonnegative and not all zero",
                    ));
                }
                capture_immediate_vec5(lua, &actor, "effect_timing", timing)?;
                capture_immediate_f32(lua, &actor, "effect_period", timing.iter().sum())?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "effectclock",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(raw) = method_arg(&args, 0).cloned().and_then(read_string) else {
                    return Ok(actor.clone());
                };
                let Some(clock) = parse_overlay_effect_clock(raw.as_str()) else {
                    return Ok(actor.clone());
                };
                capture_immediate_string(lua, &actor, "effect_clock", effect_clock_label(clock))?;
                capture_immediate_bool(
                    lua,
                    &actor,
                    "effect_timer",
                    raw.eq_ignore_ascii_case("timer"),
                )?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "vibrate",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _self: Option<Value>| {
                set_actor_effect_mode(lua, &actor, "vibrate")?;
                capture_immediate_vec3(lua, &actor, "effect_magnitude", [10.0, 10.0, 10.0])?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "stopeffect",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: IgnoredArgs| {
                set_actor_effect_mode(lua, &actor, "none")?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "hurrytweening",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let factor = lua.coerce_number(method_arg(&args, 0).cloned().unwrap_or(Value::Nil))?
                    .ok_or_else(|| mlua::Error::runtime("hurrytweening factor must be a number"))? as f32;
                if factor < 0.0 { return Err(mlua::Error::runtime("Tweening hurry factor cannot be negative")); }
                prepare_capture_scope_actor(lua, &actor)?;
                hurry_actor_tweening(lua, &actor, factor)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    Ok(())
}

pub fn install_actor_render_compat_methods(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    for name in [
        "clearzbuffer",
        "SetAmbientLightColor",
        "SetDiffuseLightColor",
        "SetLightDirection",
        "SetSpecularLightColor",
        "SortByDrawOrder",
        "fardistz",
        "StartTransitioningScreen",
        "stop",
        "volume",
    ] {
        actor.set(name, make_actor_chain_method(lua, actor)?)?;
    }
    if actor_type_is(actor, "ActorFrameTexture")? {
        actor.set(
            "Create",
            lua.create_function({
                let actor = actor.clone();
                move |lua, _: MultiValue| {
                    create_aft(lua, &actor)?;
                    Ok(actor.clone())
                }
            })?,
        )?;
        for (name, key) in [
            ("EnableAlphaBuffer", "__songlua_aft_alpha_buffer"),
            ("EnableDepthBuffer", "__songlua_aft_depth_buffer"),
            ("EnableFloat", "__songlua_aft_float_buffer"),
            ("EnablePreserveTexture", "__songlua_state_aft_preserve"),
        ] {
            actor.set(
                name,
                lua.create_function({
                    let actor = actor.clone();
                    move |lua, args: MultiValue| {
                        let Some(Value::Boolean(value)) = method_arg(&args, 0) else {
                            return Err(mlua::Error::RuntimeError(format!(
                                "{name}: boolean expected"
                            )));
                        };
                        if name == "EnablePreserveTexture" {
                            capture_immediate_bool(lua, &actor, "aft_preserve", *value)?;
                        } else {
                            actor.set(key, *value)?;
                        }
                        Ok(actor.clone())
                    }
                })?,
            )?;
        }
    }
    actor.set(
        "Draw",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: IgnoredArgs| {
                let in_draw_callback = lua
                    .globals()
                    .get::<Option<bool>>(DRAW_CALLBACK_ACTIVE_KEY)?
                    .unwrap_or(false);
                let visible = actor
                    .get::<Option<bool>>("__songlua_visible")?
                    .unwrap_or(true);
                if visible {
                    let player = actor
                        .raw_get::<Option<String>>("__songlua_top_screen_child_name")?
                        .is_some_and(|name| matches!(name.as_str(), "PlayerP1" | "PlayerP2"));
                    if in_draw_callback && !player {
                        let state = lua.create_table()?;
                        for (key, value) in snapshot_actor_semantic_state(lua, &actor)? {
                            state.set(key, value)?;
                        }
                        actor.set(MANUAL_DRAW_STATE_KEY, state)?;
                    }
                    crate::draw_capture::draw(lua, &actor)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "position",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(seconds) = method_arg(&args, 0).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                set_actor_seconds_into_animation(lua, &actor, seconds)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "zbias",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(value) = method_arg(&args, 0).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                capture_block_set_f32(lua, &actor, "z_bias", value)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "SetDrawByZPosition",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let enabled = method_arg(&args, 0).is_none_or(truthy);
                capture_block_set_bool(lua, &actor, "draw_by_z_position", enabled)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    Ok(())
}

pub fn install_actor_visual_text_methods(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    actor.set(
        "load",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                if actor_type_is(&actor, "Sound")? {
                    set_actor_sound_file_from_value(&actor, method_arg(&args, 0), true)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "rainbow",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: IgnoredArgs| {
                set_actor_effect_defaults(lua, &actor, "rainbow", Some(2.0), None, None, None)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    for (name, key) in [
        ("rainbowscroll", "rainbow_scroll"),
        ("jitter", "text_jitter"),
        ("uppercase", "uppercase"),
    ] {
        actor.set(
            name,
            lua.create_function({
                let actor = actor.clone();
                move |lua, args: MultiValue| {
                    // These LunaBitmapText methods use BArg, not BIArg.
                    let Some(Value::Boolean(enabled)) = method_arg(&args, 0) else {
                        return Err(mlua::Error::RuntimeError(format!(
                            "{name}: boolean expected"
                        )));
                    };
                    capture_block_set_bool(lua, &actor, key, *enabled)?;
                    Ok(actor.clone())
                }
            })?,
        )?;
    }
    actor.set(
        "distort",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let amount = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_f32)
                    .unwrap_or(0.0)
                    .max(0.0);
                capture_block_set_f32(lua, &actor, "text_distortion", amount)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "undistort",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: IgnoredArgs| {
                capture_block_set_f32(lua, &actor, "text_distortion", 0.0)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "AddAttribute",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                capture_actor_text_attribute(lua, &actor, &args)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "ClearAttributes",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: IgnoredArgs| {
                actor.set("__songlua_text_attributes", lua.create_table()?)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    for (name, corner_mask) in [
        ("diffuseupperleft", 1 << 0),
        ("diffuseupperright", 1 << 1),
        ("diffuselowerright", 1 << 2),
        ("diffuselowerleft", 1 << 3),
        ("diffusetopedge", (1 << 0) | (1 << 1)),
        ("diffuserightedge", (1 << 1) | (1 << 2)),
        ("diffusebottomedge", (1 << 2) | (1 << 3)),
        ("diffuseleftedge", (1 << 0) | (1 << 3)),
    ] {
        actor.set(
            name,
            lua.create_function({
                let actor = actor.clone();
                move |lua, args: MultiValue| {
                    capture_actor_vertex_diffuse(lua, &actor, &args, corner_mask)?;
                    Ok(actor.clone())
                }
            })?,
        )?;
    }
    actor.set(
        "SetTextureFiltering",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let enabled = method_arg(&args, 0).is_none_or(truthy);
                capture_block_set_bool(lua, &actor, "texture_filtering", enabled)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    for name in ["backfacecull", "cullmode"] {
        actor.set(
            name,
            lua.create_function({
                let actor = actor.clone();
                move |lua, args: MultiValue| {
                    use deadlib_render_core::CullMode;
                    let mode = if name == "backfacecull" {
                        // Actor::backfacecull uses BIArg, including numeric
                        // truncation and strict boolean/number type checks.
                        let enabled = match method_arg(&args, 0) {
                            Some(Value::Boolean(value)) => *value,
                            Some(Value::Integer(value)) => *value != 0,
                            Some(Value::Number(value)) => value.trunc() != 0.0,
                            _ => return Err(mlua::Error::runtime("backfacecull expects a boolean or number")),
                        };
                        if enabled {
                            CullMode::Back
                        } else {
                            CullMode::None
                        }
                    } else {
                        match method_arg(&args, 0) {
                            Some(Value::Integer(0)) | Some(Value::Number(0.0)) => CullMode::Back,
                            Some(Value::Integer(1)) | Some(Value::Number(1.0)) => CullMode::Front,
                            Some(Value::Integer(2)) | Some(Value::Number(2.0)) => CullMode::None,
                            Some(Value::String(value)) => match value.to_str()?.as_ref() {
                                "CullMode_Back" => CullMode::Back,
                                "CullMode_Front" => CullMode::Front,
                                "CullMode_None" => CullMode::None,
                                legacy if legacy.eq_ignore_ascii_case("back") => CullMode::Back,
                                legacy if legacy.eq_ignore_ascii_case("front") => CullMode::Front,
                                legacy if legacy.eq_ignore_ascii_case("none") => CullMode::None,
                                _ => return Err(mlua::Error::runtime("invalid CullMode")),
                            },
                            _ => return Err(mlua::Error::runtime("invalid CullMode")),
                        }
                    };
                    if !record_overlay_update_capture_immediate(
                        lua,
                        &actor,
                        "cull_mode",
                        SongLuaOverlayUpdateValue::CullMode(mode),
                    ) {
                        let block = actor_immediate_capture_block(lua, &actor)?;
                        block.set("cull_mode", mode as u8)?;
                        block.set("__songlua_has_changes", true)?;
                    }
                    set_actor_capture_state(&actor, "cull_mode", mode as u8)?;
                    Ok(actor.clone())
                }
            })?,
        )?;
    }
    for name in ["zbuffer", "ztest", "zwrite"] {
        actor.set(
            name,
            lua.create_function({
                let actor = actor.clone();
                move |lua, args: MultiValue| {
                    let enabled = match method_arg(&args, 0) {
                        Some(Value::Boolean(value)) => *value,
                        Some(Value::Integer(value)) => *value != 0,
                        Some(Value::Number(value)) => value.trunc() != 0.0,
                        _ => return Err(mlua::Error::runtime(format!("{name} expects a boolean or number"))),
                    };
                    if name != "zwrite" {
                        capture_actor_depth_recursive(lua, &actor, "depth_test", enabled)?;
                        capture_actor_depth_recursive(lua, &actor, "depth_test_on_fail", false)?;
                    }
                    if name != "ztest" {
                        capture_actor_depth_recursive(lua, &actor, "depth_write", enabled)?;
                    }
                    Ok(actor.clone())
                }
            })?,
        )?;
    }
    actor.set(
        "ztestmode",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let mode = method_arg(&args, 0).cloned().and_then(read_string)
                    .ok_or_else(|| mlua::Error::runtime("ztestmode expects a ZTestMode"))?;
                let normalized = mode.trim().trim_start_matches("ZTestMode_").to_ascii_lowercase();
                let (enabled, on_fail) = match normalized.as_str() {
                    "off" => (false, false),
                    "writeonpass" => (true, false),
                    "writeonfail" => (true, true),
                    _ => return Err(mlua::Error::runtime(format!("invalid ZTestMode: {mode}"))),
                };
                capture_actor_depth_recursive(lua, &actor, "depth_test", enabled)?;
                capture_actor_depth_recursive(lua, &actor, "depth_test_on_fail", on_fail)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "finishtweening",
        make_actor_finish_tweening_method(lua, actor)?,
    )?;
    actor.set("stoptweening", make_actor_stop_tweening_method(lua, actor)?)?;
    actor.set(
        "strokecolor",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(color) = read_color_args(&args) else {
                    return Ok(actor.clone());
                };
                actor.set("__songlua_stroke_color", make_color_table(lua, color)?)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "getstrokecolor",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: IgnoredArgs| {
                let color = read_actor_color_field(&actor, "__songlua_stroke_color")
                    .map_err(mlua::Error::external)?
                    .or_else(|| read_actor_color_field(&actor, "StrokeColor").ok().flatten())
                    .unwrap_or([0.0, 0.0, 0.0, 0.0]);
                make_color_table(lua, color)
            }
        })?,
    )?;
    for name in ["set_mult_attrs_with_diffuse", "mult_attrs_with_diffuse"] {
        actor.set(
            name,
            lua.create_function({
                let actor = actor.clone();
                move |lua, args: MultiValue| {
                    // GETTER_SETTER_BOOL_METHOD uses lua_toboolean, including 0.
                    let value = !matches!(
                        method_arg(&args, 0),
                        None | Some(Value::Nil | Value::Boolean(false))
                    );
                    capture_block_set_bool(lua, &actor, "mult_attrs_with_diffuse", value)?;
                    Ok(actor.clone())
                }
            })?,
        )?;
    }
    actor.set(
        "get_mult_attrs_with_diffuse",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: IgnoredArgs| {
                Ok(actor
                    .get::<Option<bool>>("__songlua_state_mult_attrs_with_diffuse")?
                    .unwrap_or(false))
            }
        })?,
    )?;
    actor.set(
        "textglowmode",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if let Some(mode) = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_string)
                    .and_then(|mode| parse_overlay_text_glow_mode(mode.as_str()))
                {
                    capture_block_set_string(
                        lua,
                        &actor,
                        "text_glow_mode",
                        text_glow_mode_label(mode),
                    )?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "settext",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                let text = args
                    .get(1)
                    .cloned()
                    .map(lua_text_value)
                    .transpose()?
                    .unwrap_or_default();
                actor.set("Text", text)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "settextf",
        lua.create_function({
            let actor = actor.clone();
            move |lua, mut args: MultiValue| {
                actor.set("Text", lua_format_text_in_place(lua, &mut args)?)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    // LunaBitmapText owns GetText; its presence is observable in Lua.
    if actor_is_bitmap_text(actor)? {
        actor.set(
            "GetText",
            lua.create_function({
                let actor = actor.clone();
                move |_, _args: IgnoredArgs| lua_text_value(actor.get::<Value>("Text")?)
            })?,
        )?;
    }
    for name in ["targetnumber", "SetTargetNumber"] {
        actor.set(
            name,
            lua.create_function({
                let actor = actor.clone();
                move |_, args: MultiValue| {
                    if let Some(number) = method_arg(&args, 0).cloned().and_then(read_f32) {
                        actor.set("__songlua_target_number", number)?;
                        actor.set("Text", rolling_numbers_text(&actor, number)?)?;
                    }
                    Ok(actor.clone())
                }
            })?,
        )?;
    }
    actor.set(
        "GetTargetNumber",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: IgnoredArgs| {
                Ok(actor
                    .get::<Option<f32>>("__songlua_target_number")?
                    .unwrap_or(0.0))
            }
        })?,
    )?;
    actor.set("wrapwidthpixels", make_actor_wrap_width_method(lua, actor)?)?;
    actor.set(
        "vertspacing",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(value) = method_arg(&args, 0).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                capture_block_set_i32(lua, &actor, "vert_spacing", value.round() as i32)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "maxwidth",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(value) = method_arg(&args, 0).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                capture_block_set_f32(lua, &actor, "max_width", value)?;
                capture_block_set_bool(lua, &actor, "max_w_pre_zoom", false)?;
                actor.set("__songlua_text_saw_max_width", true)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "maxheight",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(value) = method_arg(&args, 0).cloned().and_then(read_f32) else {
                    return Ok(actor.clone());
                };
                capture_block_set_f32(lua, &actor, "max_height", value)?;
                capture_block_set_bool(lua, &actor, "max_h_pre_zoom", false)?;
                actor.set("__songlua_text_saw_max_height", true)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "max_dimension_use_zoom",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let value = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_boolish)
                    .unwrap_or(true);
                capture_block_set_bool(lua, &actor, "max_dimension_uses_zoom", value)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "diffuse",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(color) = read_color_args(&args) else {
                    return Ok(actor.clone());
                };
                capture_block_set_color(lua, &actor, color)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "diffusecolor",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(mut color) = read_color_args(&args) else {
                    return Ok(actor.clone());
                };
                // Actor::SetDiffuseColor replaces RGB without changing alpha.
                color[3] = actor_diffuse(lua, &actor)?[3];
                capture_block_set_color(lua, &actor, color)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    Ok(())
}

pub fn install_actor_tap_note_methods(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    actor.set(
        "SetDidTapNoteCallback",
        lua.create_function({
            let actor = actor.clone();
            move |_, args: MultiValue| {
                match args.get(1) {
                    Some(Value::Function(function)) => {
                        actor.set("__songlua_did_tap_note_callback", function.clone())?;
                    }
                    _ => {
                        actor.set("__songlua_did_tap_note_callback", Value::Nil)?;
                    }
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    let set_did_tap_note_callback = actor.get::<Function>("SetDidTapNoteCallback")?;
    actor.set("set_did_tap_note_callback", set_did_tap_note_callback)?;
    actor.set(
        "did_tap_note",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let column = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_i32_value)
                    .unwrap_or(0);
                let score = method_arg(&args, 1).cloned().unwrap_or(Value::Nil);
                let bright = method_arg(&args, 2).is_some_and(truthy);
                actor.set("__songlua_last_tap_note_column", column)?;
                actor.set("__songlua_last_tap_note_score", score.clone())?;
                actor.set("__songlua_last_tap_note_bright", bright)?;
                if let Some(callback) =
                    actor.get::<Option<Function>>("__songlua_did_tap_note_callback")?
                {
                    let mut callback_args = MultiValue::new();
                    callback_args.push_back(Value::Integer(i64::from(column)));
                    callback_args.push_back(score);
                    callback_args.push_back(Value::Boolean(bright));
                    let _ = callback.call::<Value>(callback_args)?;
                }
                note_song_lua_side_effect(lua)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    Ok(())
}

pub fn install_actor_child_command_methods(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    actor.set(
        "RunCommandsOnChildren",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(command) = method_arg(&args, 0).cloned().and_then(|value| match value {
                    Value::Function(function) => Some(function),
                    _ => None,
                }) else {
                    return Ok(actor.clone());
                };
                let params = method_arg(&args, 1).cloned();
                for child in actor_direct_children(lua, &actor)? {
                    let _ = match params.clone() {
                        Some(params) => command.call::<Value>((child, params))?,
                        None => command.call::<Value>(child)?,
                    };
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "runcommandsonleaves",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(command) = method_arg(&args, 0).cloned().and_then(|value| match value {
                    Value::Function(function) => Some(function),
                    _ => None,
                }) else {
                    return Ok(actor.clone());
                };
                run_command_on_leaves(lua, &actor, &command)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    Ok(())
}

pub fn install_actor_path_child_methods(
    lua: &Lua,
    actor: &Table,
    add_actor_child_from_path: fn(&Lua, &Table, &str) -> mlua::Result<()>,
) -> mlua::Result<()> {
    actor.set(
        "AddChildFromPath",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(path) = method_arg(&args, 0).cloned().and_then(read_string) else {
                    return Ok(actor.clone());
                };
                add_actor_child_from_path(lua, &actor, &path)?;
                invalidate_compile_update_plan(lua);
                Ok(actor.clone())
            }
        })?,
    )?;
    Ok(())
}

pub fn add_actor_child_from_path(
    lua: &Lua,
    actor: &Table,
    path: &str,
    create_dummy_actor: fn(&Lua, &'static str) -> mlua::Result<Table>,
) -> mlua::Result<()> {
    let Some(song_dir) =
        actor
            .get::<Option<String>>("__songlua_song_dir")?
            .or(lua.globals().get::<Option<String>>("__songlua_song_dir")?)
    else {
        return Ok(());
    };
    let Ok(Value::Table(child)) = load_actor_path(
        lua,
        Path::new(&song_dir),
        path,
        MultiValue::new(),
        create_dummy_actor,
    ) else {
        return Ok(());
    };
    child.set("__songlua_parent", actor.clone())?;
    if !push_sequence_child_once(actor, child.clone())? {
        return Ok(());
    }
    run_added_actor_child_commands(lua, actor, &child)
}

pub fn install_actor_runtime_child_methods(
    lua: &Lua,
    actor: &Table,
    note_field_column_actors: fn(&Lua, &Table) -> mlua::Result<Table>,
    create_named_child_actor: fn(&Lua, &Table, &str) -> mlua::Result<Table>,
    create_dummy_actor: fn(&Lua, &'static str) -> mlua::Result<Table>,
) -> mlua::Result<()> {
    actor.set(
        "GetColumnActors",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: IgnoredArgs| {
                let is_note_field = actor
                    .get::<Option<String>>("__songlua_player_child_name")?
                    .as_deref()
                    .is_some_and(|name| name.eq_ignore_ascii_case("NoteField"))
                    || actor
                        .get::<Option<String>>("__songlua_actor_type")?
                        .as_deref()
                        .is_some_and(|kind| kind.eq_ignore_ascii_case("NoteField"));
                if !is_note_field {
                    return Ok(Value::Nil);
                }
                Ok(Value::Table(note_field_column_actors(lua, &actor)?))
            }
        })?,
    )?;
    actor.set(
        "get_column_actors",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: IgnoredArgs| {
                let Value::Function(method) = actor.get::<Value>("GetColumnActors")? else {
                    return Ok(Value::Nil);
                };
                method.call::<Value>(MultiValue::from_vec(vec![Value::Table(actor.clone())]))
            }
        })?,
    )?;
    actor.set(
        "GetChild",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(name) = method_arg(&args, 0).cloned().and_then(read_string) else {
                    return Ok(Value::Nil);
                };
                let children = actor_named_children(lua, &actor)?;
                if let Some(child) = children.get::<Option<Table>>(name.as_str())? {
                    return Ok(Value::Table(child));
                }
                if !can_create_named_child_actor(&actor, &name)? {
                    return Ok(Value::Nil);
                }
                let child = create_named_child_actor(lua, &actor, &name)?;
                actor_children(lua, &actor)?.set(name.as_str(), child.clone())?;
                Ok(Value::Table(child))
            }
        })?,
    )?;
    install_actor_child_command_methods(lua, actor)?;
    install_actor_wrapper_query_methods(lua, actor)?;
    actor.set(
        "AddWrapperState",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: IgnoredArgs| {
                prepare_capture_scope_actor(lua, &actor)?;
                let wrapper = create_dummy_actor(lua, "WrapperState")?;
                copy_dummy_actor_tags(&actor, &wrapper)?;
                wrapper.raw_set("__songlua_parent", actor.clone())?;
                let wrappers = actor_wrappers(lua, &actor)?;
                let next_index = wrappers.raw_len() + 1;
                wrappers.raw_set(next_index, wrapper.clone())?;
                capture_new_wrapper(lua, &actor, &wrapper)?;
                invalidate_compile_update_plan(lua);
                Ok(wrapper)
            }
        })?,
    )?;
    install_actor_child_query_methods(lua, actor)?;
    Ok(())
}

pub fn install_actor_methods(
    lua: &Lua,
    actor: &Table,
    add_actor_child_from_path: fn(&Lua, &Table, &str) -> mlua::Result<()>,
    note_field_column_actors: fn(&Lua, &Table) -> mlua::Result<Table>,
    create_named_child_actor: fn(&Lua, &Table, &str) -> mlua::Result<Table>,
    create_dummy_actor: fn(&Lua, &'static str) -> mlua::Result<Table>,
) -> mlua::Result<()> {
    install_actor_command_methods(lua, actor)?;
    install_actor_texture_load_methods(lua, actor)?;
    install_actor_texture_proxy_getter_methods(lua, actor)?;
    install_actor_transform_methods(lua, actor)?;
    install_actor_display_state_methods(lua, actor)?;
    install_actor_crop_shadow_methods(lua, actor)?;
    install_actor_scale_size_methods(lua, actor)?;
    install_actor_extra_transform_methods(lua, actor)?;
    install_actor_image_coord_methods(lua, actor)?;
    install_actor_sprite_animation_methods(lua, actor)?;
    install_actor_effect_methods(lua, actor)?;
    install_actor_render_compat_methods(lua, actor)?;
    install_sound_methods(lua, actor)?;
    install_actor_path_child_methods(lua, actor, add_actor_child_from_path)?;
    install_actor_visual_text_methods(lua, actor)?;
    install_actor_tap_note_methods(lua, actor)?;
    install_actor_runtime_child_methods(
        lua,
        actor,
        note_field_column_actors,
        create_named_child_actor,
        create_dummy_actor,
    )?;
    install_actor_basic_getter_methods(lua, actor)?;
    install_actor_size_getter_methods(lua, actor)?;
    install_actor_transform_getter_methods(lua, actor)?;
    install_actor_effect_time_getter_methods(lua, actor)?;
    install_actor_parent_methods(lua, actor)?;
    Ok(())
}

pub fn install_actor_wrapper_query_methods(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    actor.set(
        "GetWrapperState",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let Some(index) = method_arg(&args, 0).cloned().and_then(|value| match value {
                    Value::Integer(value) => Some(value),
                    Value::Number(value) => Some(value as i64),
                    _ => None,
                }) else {
                    return Ok(Value::Nil);
                };
                let Some(wrapper) = actor_wrappers(lua, &actor)?.get::<Option<Table>>(index)?
                else {
                    return Ok(Value::Nil);
                };
                Ok(Value::Table(wrapper))
            }
        })?,
    )?;
    actor.set(
        "GetNumWrapperStates",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: IgnoredArgs| Ok(actor_wrappers(lua, &actor)?.raw_len() as i64)
        })?,
    )?;
    Ok(())
}

pub fn install_actor_child_query_methods(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    actor.set(
        "GetChildren",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: IgnoredArgs| actor_named_children(lua, &actor)
        })?,
    )?;
    actor.set(
        "GetNumChildren",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: IgnoredArgs| {
                // Native GetNumChildren counts actors, including siblings
                // with duplicate or empty names, rather than table keys.
                Ok(actor_direct_children(lua, &actor)?.len() as i64)
            }
        })?,
    )?;
    Ok(())
}

pub fn install_actor_basic_getter_methods(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    actor.set(
        "name",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                // LunaActor::name uses SArg and returns the same actor.
                let value = method_arg(&args, 0).cloned().unwrap_or(Value::Nil);
                let Some(name) = lua.coerce_string(value)? else {
                    return Err(mlua::Error::runtime("name: string expected"));
                };
                set_actor_name(&actor, name)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "GetX",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _self: Option<Value>| actor_current_position(lua, &actor, 0)
        })?,
    )?;
    actor.set(
        "GetName",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: IgnoredArgs| {
                let name = actor_name(&actor)?;
                Ok(Value::String(lua.create_string(name)?))
            }
        })?,
    )?;
    actor.set(
        "GetY",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _self: Option<Value>| actor_current_position(lua, &actor, 1)
        })?,
    )?;
    actor.set(
        "GetDestX",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _self: Option<Value>| actor_destination_position(lua, &actor, 0)
        })?,
    )?;
    actor.set(
        "GetDestY",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _self: Option<Value>| actor_destination_position(lua, &actor, 1)
        })?,
    )?;
    actor.set(
        "GetDestZ",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _self: Option<Value>| actor_destination_position(lua, &actor, 2)
        })?,
    )?;
    actor.set(
        "GetVisible",
        lua.create_function({
            let actor = actor.clone();
            move |_, _self: Option<Value>| {
                Ok(actor
                    .get::<Option<bool>>("__songlua_visible")?
                    .unwrap_or(true))
            }
        })?,
    )?;
    actor.set(
        "GetZoom",
        lua.create_function({
            let actor = actor.clone();
            // Actor::GetZoom aliases the destination X scale.
            move |lua, _self: Option<Value>| actor_destination_zoom(lua, &actor, "zoom_x")
        })?,
    )?;
    Ok(())
}

pub fn install_actor_transform_getter_methods(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    actor.set(
        "GetZ",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _self: Option<Value>| actor_current_position(lua, &actor, 2)
        })?,
    )?;
    actor.set(
        "GetRotationX",
        lua.create_function({
            let actor = actor.clone();
            move |_, _self: Option<Value>| {
                Ok(actor
                    .get::<Option<f32>>("__songlua_state_rot_x_deg")?
                    .unwrap_or(0.0_f32))
            }
        })?,
    )?;
    actor.set(
        "GetRotationY",
        lua.create_function({
            let actor = actor.clone();
            move |_, _self: Option<Value>| {
                Ok(actor
                    .get::<Option<f32>>("__songlua_state_rot_y_deg")?
                    .unwrap_or(0.0_f32))
            }
        })?,
    )?;
    actor.set(
        "GetRotationZ",
        lua.create_function({
            let actor = actor.clone();
            move |_, _self: Option<Value>| {
                Ok(actor
                    .get::<Option<f32>>("__songlua_state_rot_z_deg")?
                    .unwrap_or(0.0_f32))
            }
        })?,
    )?;
    actor.set(
        "getrotation",
        lua.create_function({
            let actor = actor.clone();
            move |_, _self: Option<Value>| {
                let x = actor
                    .get::<Option<f32>>("__songlua_state_rot_x_deg")?
                    .unwrap_or(0.0_f32);
                let y = actor
                    .get::<Option<f32>>("__songlua_state_rot_y_deg")?
                    .unwrap_or(0.0_f32);
                let z = actor
                    .get::<Option<f32>>("__songlua_state_rot_z_deg")?
                    .unwrap_or(0.0_f32);
                Ok((x, y, z))
            }
        })?,
    )?;
    actor.set(
        "GetZoomX",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _self: Option<Value>| actor_destination_zoom(lua, &actor, "zoom_x")
        })?,
    )?;
    actor.set(
        "GetZoomY",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _self: Option<Value>| actor_destination_zoom(lua, &actor, "zoom_y")
        })?,
    )?;
    actor.set(
        "GetZoomZ",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _self: Option<Value>| actor_destination_zoom(lua, &actor, "zoom_z")
        })?,
    )?;
    actor.set(
        "GetBaseZoomX",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: IgnoredArgs| actor_base_zoom_axis(lua, &actor, "__songlua_state_basezoom_x")
        })?,
    )?;
    actor.set(
        "GetBaseZoomY",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: IgnoredArgs| actor_base_zoom_axis(lua, &actor, "__songlua_state_basezoom_y")
        })?,
    )?;
    actor.set(
        "GetBaseZoomZ",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: IgnoredArgs| actor_base_zoom_axis(lua, &actor, "__songlua_state_basezoom_z")
        })?,
    )?;
    actor.set(
        "GetAlpha",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: IgnoredArgs| Ok(actor_diffuse(lua, &actor)?[3])
        })?,
    )?;
    actor.set(
        "GetDiffuse",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: IgnoredArgs| make_color_table(lua, actor_diffuse(lua, &actor)?)
        })?,
    )?;
    actor.set(
        "GetGlow",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: IgnoredArgs| make_color_table(lua, actor_glow(lua, &actor)?)
        })?,
    )?;
    actor.set(
        "GetDiffuseAlpha",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: IgnoredArgs| Ok(actor_diffuse(lua, &actor)?[3])
        })?,
    )?;
    actor.set(
        "GetHAlign",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: IgnoredArgs| actor_halign(lua, &actor)
        })?,
    )?;
    actor.set(
        "GetVAlign",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: IgnoredArgs| actor_valign(lua, &actor)
        })?,
    )?;
    actor.set(
        "geteffectmagnitude",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: IgnoredArgs| {
                let [x, y, z] = actor_effect_magnitude(&actor)?;
                Ok((x, y, z))
            }
        })?,
    )?;
    Ok(())
}

pub fn install_actor_effect_time_getter_methods(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    actor.set(
        "GetSecsIntoEffect",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: IgnoredArgs| {
                if let Some(clock) =
                    actor.raw_get::<Option<Table>>("__songlua_state_motion_clock")?
                {
                    return clock.get::<f32>("units");
                }
                let (beat, seconds) = compile_song_runtime_values(lua)?;
                let clock = actor
                    .get::<Option<String>>("__songlua_state_effect_clock")?
                    .as_deref()
                    .and_then(parse_overlay_effect_clock)
                    .unwrap_or(EffectClock::Time);
                Ok(match clock {
                    EffectClock::Time => seconds,
                    EffectClock::Beat => beat,
                })
            }
        })?,
    )?;
    actor.set(
        "GetEffectDelta",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: IgnoredArgs| {
                if let Some(clock) =
                    actor.raw_get::<Option<Table>>("__songlua_state_motion_clock")?
                {
                    return Ok(clock.get::<Option<f32>>("delta")?.unwrap_or(0.0));
                }
                let (delta_beat, delta_seconds) = compile_song_runtime_delta_values(lua)?;
                let clock = actor
                    .get::<Option<String>>("__songlua_state_effect_clock")?
                    .as_deref()
                    .and_then(parse_overlay_effect_clock)
                    .unwrap_or(EffectClock::Time);
                Ok(match clock {
                    EffectClock::Time => delta_seconds,
                    EffectClock::Beat => delta_beat,
                })
            }
        })?,
    )?;
    Ok(())
}

pub fn install_actor_parent_methods(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    actor.set(
        "GetParent",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: IgnoredArgs| actor.raw_get::<Option<Table>>("__songlua_parent")
        })?,
    )?;
    Ok(())
}

fn adopt_actor_child(parent: &Table, child: &Table) -> mlua::Result<()> {
    // Session root lists group loaders; they are not native ActorFrames and
    // must not replace a song actor's SongForeground/SongBackground parent.
    if parent.raw_get::<Option<String>>("__songlua_actor_type")?.is_some() {
        child.raw_set("__songlua_parent", parent.clone())?;
    }
    Ok(())
}

fn compact_actor_children(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    let (_, children) = actor_def_children(lua, actor)?;
    let mut indices = smallvec::SmallVec::<[i64; 16]>::new();
    actor.for_each::<Value, Value>(|key, _value| {
        if let Value::Integer(index) = key
            && index > 0
        {
            indices.push(index);
        }
        Ok(())
    })?;
    // Runtime sequence walks use the selected native load order. In particular,
    // excluded direct children must not receive Init or enter the actor tree.
    for index in indices {
        actor.raw_set(index, Value::Nil)?;
    }
    let mut index = 1;
    for (_, child) in children {
        // ActorUtil::LoadFromNode returns null for Condition=false before
        // constructing or initializing any actor in that subtree.
        if matches!(child.raw_get::<Value>("Condition")?, Value::Boolean(false)) {
            continue;
        }
        actor.raw_set(index, child)?;
        index += 1;
    }
    Ok(())
}

pub fn run_actor_init_commands_for_table(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    if actor
        .get::<Option<bool>>("__songlua_init_commands_ran")?
        .unwrap_or(false)
    {
        return Ok(());
    }
    compact_actor_children(lua, actor)?;
    // ActorFrame loads its children before Actor::LoadFromNode applies Name.
    // Keep the authored attribute separate from the observable loading name;
    // a child's Init may rename its parent before that attribute is applied.
    let name = actor.raw_get::<Value>("Name")?;
    // Actor::m_sName is separate from userdata attributes such as self.Name.
    // Retain that native name after loading so later attribute writes cannot
    // rename actors or change GetChild/GetChildren and observed ancestry.
    actor.raw_set("__songlua_native_name", "")?;
    actor.raw_set("__songlua_load_name", "")?;
    // Def tables are constructed children first. Native actors subscribe as
    // the definition tree loads, with parents before their children.
    register_song_lua_actor(lua, actor)?;
    for (index, child) in actor.sequence_values::<Value>().enumerate() {
        let Value::Table(child) = child? else {
            continue;
        };
        adopt_actor_child(actor, &child)?;
        #[cfg(feature = "test-support")]
        if let Some(path) = actor.raw_get::<Option<String>>("__songlua_message_path")? {
            child.raw_set("__songlua_message_path", format!("{path}/{}", index + 1))?;
        }
        let _ = index;
        run_actor_init_commands_for_table(lua, &child)?;
    }
    if !matches!(name, Value::Nil) {
        actor.raw_set("__songlua_native_name", name.clone())?;
        actor.raw_set("Name", name)?;
    } else {
        actor.raw_set("__songlua_native_name", actor.raw_get::<Value>("__songlua_load_name")?)?;
    }
    actor.raw_set("__songlua_load_name", Value::Nil)?;
    // ActorFrame::LoadFromNode initializes its children before Actor runs Init.
    // Native QueueCommand appends a tween; Init does not advance its queue.
    run_actor_named_command_with_drain(lua, actor, "InitCommand", false)?;
    run_song_meter_stream_init_command(lua, actor)?;
    actor.set("__songlua_init_commands_ran", true)?;
    Ok(())
}

pub fn run_actor_startup_commands_for_table(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    if actor.raw_get::<Option<bool>>("__songlua_defer_on")?.unwrap_or(false) {
        return Ok(());
    }
    if actor
        .get::<Option<bool>>("__songlua_startup_commands_ran")?
        .unwrap_or(false)
    {
        return Ok(());
    }
    actor.set("__songlua_startup_command_started", true)?;
    actor.set("__songlua_startup_children_walked", false)?;
    if actor_runs_startup_commands(actor)? {
        run_actor_named_command_with_drain(lua, actor, "OnCommand", false)?;
    }
    for child in actor.sequence_values::<Value>() {
        let Value::Table(child) = child? else {
            continue;
        };
        adopt_actor_child(actor, &child)?;
        run_actor_startup_commands_for_table(lua, &child)?;
    }
    run_song_meter_stream_startup_command(lua, actor)?;
    actor.set("__songlua_startup_children_walked", true)?;
    drain_actor_command_queue(lua, actor)?;
    actor.set("__songlua_startup_commands_ran", true)?;
    Ok(())
}

pub(crate) fn init_actor_native_state(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    let previous_flags = live_render_flags(lua, actor);
    // ActorFrame::InitState resets children and base Actor state, preserving
    // names, Lua fields, command handlers, callbacks and loaded resources.
    for child in actor.sequence_values::<Table>() { init_actor_native_state(lua, &child?)?; }
    let keys = actor.clone().pairs::<Value, Value>().filter_map(|pair| {
        let (Value::String(key), _) = pair.ok()? else { return None; };
        let name = key.to_str().ok()?;
        let native_state = name.strip_prefix("__songlua_state_").is_some_and(|name|
            matches!(name, "aux" | "x" | "y" | "z" | "z_bias" | "draw_order"
                | "halign" | "valign" | "visible" | "zoom" | "zoom_x" | "zoom_y" | "zoom_z"
                | "basezoom" | "basezoom_x" | "basezoom_y" | "basezoom_z" | "base_rotation"
                | "rot_x_deg" | "rot_y_deg" | "rot_z_deg" | "skew_x" | "skew_y"
                | "cropleft" | "cropright" | "croptop" | "cropbottom"
                | "fadeleft" | "faderight" | "fadetop" | "fadebottom" | "stretch_rect"
                | "diffuse" | "vertex_colors" | "glow" | "blend" | "shadow_len" | "shadow_color"
                | "depth_test" | "depth_write" | "depth_test_on_fail" | "cull_mode" | "texture_filtering" | "texture_wrapping"
                | "texcoord_offset" | "vibrate" | "rainbow" | "rainbow_scroll" | "spin"
                | "motion_clock" | "hibernating") || name.starts_with("effect_"));
        (native_state || matches!(name.as_ref(), "__songlua_visible" | "__songlua_current_x"
            | "__songlua_current_y" | "__songlua_current_z" | "__songlua_diffuse" | "__songlua_aux"
            | "__songlua_hibernate_seconds" | "__songlua_startup_commands_ran"
            | "__songlua_startup_command_started" | "__songlua_startup_children_walked")
            || name.starts_with("__songlua_command_queue")).then(|| key.clone())
    }).collect::<Vec<_>>();
    for key in keys { actor.raw_set(key, Value::Nil)?; }
    if actor_type_is(actor, "Sprite")? {
        actor.raw_set("__songlua_state_sprite_state_index", 0u32)?;
        actor.raw_set("__songlua_state_sprite_animate", true)?;
    }
    let current = actor_overlay_initial_state(actor).map_err(mlua::Error::external)?;
    let render_flags = (previous_flags.is_some() || live_actor_playback(lua)).then(|| {
        let mut flags = RenderFlags::from(current);
        if let Some(previous) = previous_flags { flags.preserve_on_reset(previous); }
        flags
    });
    let native_base = live_actor_playback(lua);
    let base_transform = native_base.then(|| BaseTransform::read(actor).map(Box::new)).transpose()?;
    if let Some(mut replays) = lua.app_data_mut::<ActorTweenReplays>() {
        replays.0.insert(actor.to_pointer() as usize, ActorTweenReplay {
            current: current.into(), render_flags, base_transform, queue: Default::default(), progress: None, targets: 0,
        });
    }
    if render_flags.is_some() {
        for flag in RenderFlag::ALL { actor.raw_set(flag.state_key(), Value::Nil)?; }
    }
    if native_base {
        for field in BaseField::ALL { actor.raw_set(field.state_key(), Value::Nil)?; }
    }
    reset_actor_capture(lua, actor)?;
    sync_tween_getters(lua, actor)
}

pub(crate) fn run_background_transition(lua: &Lua, actor: &Table, transition: &str) -> mlua::Result<()> {
    let duration = match transition {
        "" => return Ok(()),
        "CrossFade" => 1.0,
        "CrossFade_Faster" => 0.75,
        "CrossFade_Fastest" => 0.5,
        _ => return Err(mlua::Error::runtime(format!("unsupported background transition: {transition}"))),
    };
    fn leaves(lua: &Lua, actor: &Table, duration: f32) -> mlua::Result<()> {
        let children = actor_direct_children(lua, actor)?;
        if children.is_empty() {
            actor.get::<Function>("stoptweening")?.call::<()>(actor.clone())?;
            actor.get::<Function>("linear")?.call::<()>((actor.clone(), duration))?;
            actor.get::<Function>("diffusealpha")?.call::<()>((actor.clone(), 0.0))?;
        } else { for child in children { leaves(lua, &child, duration)?; } }
        Ok(())
    }
    leaves(lua, actor, duration)
}

pub fn run_added_actor_child_commands(
    lua: &Lua,
    parent: &Table,
    child: &Table,
) -> mlua::Result<()> {
    // AddChildFromPath's MakeActor initializes the loaded child immediately,
    // including when it is called from the parent's own InitCommand.
    run_actor_init_commands_for_table(lua, child)?;
    let startup_already_needs_child = parent
        .get::<Option<bool>>("__songlua_startup_commands_ran")?
        .unwrap_or(false)
        || parent
            .get::<Option<bool>>("__songlua_startup_children_walked")?
            .unwrap_or(false);
    if startup_already_needs_child {
        run_actor_startup_commands_for_table(lua, child)?;
    }
    Ok(())
}

pub fn run_actor_init_commands(lua: &Lua, root: &Value) -> mlua::Result<()> {
    let Value::Table(root) = root else {
        return Ok(());
    };
    // Retain queues across all layer Init passes, including nested playcommand
    // calls. Startup releases them only after every layer's On pass.
    if lua.app_data_ref::<SongLuaStartupQueues>().is_none() {
        lua.set_app_data(SongLuaStartupQueues(Vec::new()));
    }
    run_actor_init_commands_for_table(lua, root)
}

struct SongLuaStartupQueues(Vec<Table>);
struct SongLuaZeroUpdate;
struct SongLuaQueuedStartup(f32);
#[derive(Clone, Copy)]
struct SongLuaQueuedCommand {
    start: f32,
    actor: usize,
    time: Option<(usize, f64)>,
}

// Load-time queue clocks use the same replay frames as callbacks. Storage lives
// for one compilation; it never exists on a gameplay frame.
struct SongLuaQueueClock {
    // One shared back-tween sample array per actor. Appending a queue step
    // invalidates it; sampling is bounded by the song's canonical frame count.
    progress: Option<std::sync::Arc<[[f32; 2]]>>,
    epoch: usize,
    frame: usize,
    remaining: f64,
    // A zero-time state becomes visible on its actor update. Following tweens
    // inherit its destination but cannot render that pose on an earlier frame.
    zero_frame: Option<usize>,
    steps: std::collections::VecDeque<Option<f64>>,
    dispatch: Option<(usize, f64)>,
    recurring_frame: Option<usize>,
}

pub(crate) struct SongLuaCompileFrames {
    times: Vec<f64>,
    // Optional native actor delta and its active span in the global frame.
    actor_frames: Option<Vec<[f32; 2]>>,
    beats: Vec<f32>,
    frame: usize,
    epoch: usize,
    clocks: FxHashMap<usize, SongLuaQueueClock>,
}

impl SongLuaCompileFrames {
    fn time(&self, frame: usize) -> f64 {
        self.times.get(frame).copied().unwrap_or_else(|| {
            self.times.last().copied().unwrap_or(0.0) + (frame + 1 - self.times.len()) as f64 / 60.0
        })
    }
    fn delta(&self, frame: usize) -> f64 {
        if let Some(delta) = self
            .actor_frames
            .as_ref()
            .and_then(|frames| frames.get(frame))
        {
            return f64::from(delta[0]);
        }
        if frame == 0 {
            0.0
        } else {
            f64::from(self.time(frame) as f32 - self.time(frame - 1) as f32)
        }
    }
    fn global_remainder(&self, frame: usize, remaining: f64) -> f64 {
        self.actor_frames
            .as_ref()
            .and_then(|frames| frames.get(frame))
            .map_or(remaining, |[delta, span]| {
                if *delta > 0.0 {
                    remaining * f64::from(*span) / f64::from(*delta)
                } else {
                    0.0
                }
            })
    }
}

pub(crate) fn set_compile_frames(
    lua: &Lua,
    frames: impl Iterator<Item = (f64, f64)>,
) -> mlua::Result<()> {
    let mut seconds = 0.0;
    let (beats, times) = frames
        .map(|(beat, delta)| {
            seconds += delta;
            (beat as f32, seconds)
        })
        .unzip();
    lua.set_app_data(SongLuaCompileFrames {
        times,
        actor_frames: None,
        beats,
        frame: 0,
        epoch: 0,
        clocks: FxHashMap::default(),
    });
    for actor in song_lua_actor_registry(lua)?.sequence_values::<Table>() {
        let actor = actor?;
        let deferred = actor
            .raw_get::<Option<bool>>("__songlua_command_queue_deferred")?
            .unwrap_or(false);
        let recurring = actor
            .raw_get::<Option<String>>("__songlua_recurring_update_command")?
            .is_some();
        if !deferred && !recurring {
            continue;
        }
        let captured: Table = actor.get("__songlua_command_queue_steps")?;
        let steps = captured
            .sequence_values::<Value>()
            .map(|step| step.map(read_f64))
            .collect::<mlua::Result<std::collections::VecDeque<_>>>()?;
        actor.set("__songlua_command_queue_steps", Value::Nil)?;
        lua.app_data_mut::<SongLuaCompileFrames>()
            .expect("frames installed")
            .clocks
            .insert(
                actor.to_pointer() as usize,
                SongLuaQueueClock {
                    progress: None,
                    epoch: 0,
                    frame: 0,
                    remaining: 0.0,
                    zero_frame: None,
                    steps,
                    dispatch: None,
                    recurring_frame: None,
                },
            );
        if deferred {
            drain_queue_clock(lua, &actor);
        }
        actor.set("__songlua_command_queue_deferred", Value::Nil)?;
    }
    Ok(())
}

pub(crate) fn set_compile_frame(lua: &Lua, frame: usize) {
    if let Some(mut frames) = lua.app_data_mut::<SongLuaCompileFrames>() {
        frames.frame = frame;
    }
}

fn queue_actor_order(lua: &Lua, actor: usize) -> usize {
    lua.app_data_ref::<SongLuaCompileUpdateOrders>()
        .and_then(|orders| orders.0.get(&actor).copied())
        .unwrap_or(0)
}

fn advance_queue_clock(
    frames: &SongLuaCompileFrames,
    clock: &mut SongLuaQueueClock,
    duration: f64,
    mut samples: Option<&mut Vec<[f32; 2]>>,
) {
    // Actor::UpdateTweening subtracts native floats on each frame. A double
    // clock can release a zero-time command early and change later RNG calls.
    let mut left = duration as f32;
    loop {
        while clock.remaining <= 0.0 {
            clock.frame += 1;
            clock.remaining = frames.delta(clock.frame);
        }
        if duration as f32 == 0.0 {
            clock.zero_frame = Some(clock.frame);
        }
        // A callback after its child can enqueue a zero-time state on the final
        // captured frame. Its start still belongs to the next update, even when
        // that update lies beyond the replay. Omitting this sample makes the
        // pending destination look immediate and overwrites the last draw.
        if let Some(samples) = &mut samples
            && samples.is_empty()
        {
            let start = frames.time(clock.frame)
                - if left == 0.0 {
                    0.0
                } else {
                    frames.global_remainder(clock.frame, clock.remaining)
                };
            let mut start = start as f32;
            if left > 0.0 && clock.frame > 0 {
                let prior = frames.time(clock.frame - 1) as f32;
                // A tiny remainder can put the next tween strictly after the
                // prior update while both timestamps round to the same float.
                // Preserve queue order: it cannot draw on that earlier frame.
                if frames.time(clock.frame) - frames.global_remainder(clock.frame, clock.remaining)
                    > f64::from(prior)
                    && start <= prior
                {
                    start = prior.next_up();
                }
            }
            if let Some(frame) = clock.zero_frame {
                start = start.max(frames.time(frame) as f32);
            }
            samples.push([start, left]);
        }
        let elapsed = left.min(clock.remaining as f32);
        left -= elapsed;
        clock.remaining = f64::from(clock.remaining as f32 - elapsed);
        if let Some(samples) = &mut samples
            && clock.frame < frames.times.len()
            && left != duration as f32
        {
            samples.push([frames.time(clock.frame) as f32, left]);
        }
        if left == 0.0 {
            break;
        }
        if clock.frame >= frames.times.len() {
            let count = (left * 60.0).floor() as usize;
            if count > 1 {
                clock.frame += count - 1;
                left -= (count - 1) as f32 / 60.0;
            }
        }
    }
}

fn record_queue_step(lua: &Lua, actor: &Table, duration: Option<f64>) -> mlua::Result<Option<f32>> {
    if has_tween_replay(lua, actor) {
        return Ok(None);
    }
    // Queue creation spans startup and replay; retain its clock and dispatch
    // tail together so changing phases cannot advance the same time twice.
    if lua.app_data_ref::<SongLuaCompileFrames>().is_none() {
        let steps = match actor.get::<Option<Table>>("__songlua_command_queue_steps")? {
            Some(steps) => steps,
            None => {
                let steps = lua.create_table()?;
                actor.set("__songlua_command_queue_steps", steps.clone())?;
                steps
            }
        };
        // Keep each native float duration. Subtracting rounded cumulative
        // cursors changes individual sleeps and can defer a command one frame.
        steps.raw_set(
            steps.raw_len() + 1,
            duration.map_or(Value::Boolean(false), |seconds| {
                Value::Number(f64::from(seconds as f32))
            }),
        )?;
    }
    if let Some(duration) = duration
        && actor_has_active_command(lua, actor)?
    {
        // A self-queued command waits for every tween and sleep in its cycle.
        // Keep their exact durations instead of the rounded render cursor.
        let interval = actor
            .get::<Option<f64>>("__songlua_recurring_update_exact_interval")?
            .unwrap_or(0.0);
        actor.set(
            "__songlua_recurring_update_exact_interval",
            interval + duration,
        )?;
    }
    if lua.app_data_ref::<SongLuaCompileFrames>().is_none()
        && let Some(duration) = duration
    {
        // Keep startup queue time at the same precision as replay clocks.
        // Summing the render cursor's f32 values can move a frame boundary.
        let seconds = actor
            .get::<Option<f64>>("__songlua_startup_queue_seconds")?
            .unwrap_or(0.0);
        actor.set("__songlua_startup_queue_seconds", seconds + duration)?;
    }
    if !lua
        .app_data_ref::<SongLuaCompileUpdatePhase>()
        .is_some_and(|phase| phase.active)
    {
        return Ok(None);
    }
    let actor = actor.to_pointer() as usize;
    let scope = lua
        .app_data_ref::<SongLuaQueuedCommand>()
        .map(|scope| *scope);
    let phase = *lua
        .app_data_ref::<SongLuaCompileUpdatePhase>()
        .expect("phase is active");
    let later = queue_actor_order(lua, actor) > phase.order;
    let owner_later = scope
        .is_some_and(|scope| queue_actor_order(lua, actor) > queue_actor_order(lua, scope.actor));
    let Some(mut frames) = lua.app_data_mut::<SongLuaCompileFrames>() else {
        return Ok(None);
    };
    let (frame, remaining) = scope
        .and_then(|scope| {
            scope.time.map(|(frame, remaining)| {
                (
                    frame,
                    if scope.actor == actor {
                        remaining
                    } else if owner_later {
                        frames.delta(frame)
                    } else {
                        0.0
                    },
                )
            })
        })
        .unwrap_or((frames.frame, if later { phase.delta_seconds } else { 0.0 }));
    let epoch = frames.epoch;
    let mut clock = frames
        .clocks
        .remove(&actor)
        .unwrap_or_else(|| SongLuaQueueClock {
            progress: None,
            epoch,
            frame,
            remaining,
            zero_frame: None,
            steps: std::collections::VecDeque::new(),
            dispatch: None,
            recurring_frame: None,
        });
    let changed = clock.epoch != epoch;
    if changed && clock.dispatch.is_none() {
        while let Some(step) = clock.steps.pop_front() {
            advance_queue_clock(&frames, &mut clock, step.unwrap_or(0.0), None);
        }
        clock.epoch = epoch;
    }
    // Nested queued commands can reach an actor whose earlier queue has already
    // finished. Advance its clock to the caller, while retaining a live tail.
    if clock.steps.is_empty()
        && (clock.frame < frame || (clock.frame == frame && clock.remaining > remaining))
    {
        clock.frame = frame;
        clock.remaining = remaining;
    }
    let delay = changed.then(|| {
        (frames.time(clock.frame) - clock.remaining - (frames.time(frame) - remaining)).max(0.0)
            as f32
    });
    clock.steps.push_back(duration);
    clock.progress = None;
    frames.clocks.insert(actor, clock);
    Ok(delay)
}

fn actor_tween_progress(lua: &Lua, actor: &Table) -> Option<std::sync::Arc<[[f32; 2]]>> {
    let actor = actor.to_pointer() as usize;
    let mut frames = lua.app_data_mut::<SongLuaCompileFrames>()?;
    let mut clock = frames.clocks.remove(&actor)?;
    if clock.progress.is_none() && !clock.steps.is_empty() {
        let mut sample_clock = SongLuaQueueClock {
            progress: None,
            epoch: clock.epoch,
            frame: clock.frame,
            remaining: clock.remaining,
            zero_frame: clock.zero_frame,
            steps: std::collections::VecDeque::new(),
            dispatch: None,
            recurring_frame: None,
        };
        let mut samples = Vec::new();
        for (index, step) in clock.steps.iter().enumerate() {
            advance_queue_clock(
                &frames,
                &mut sample_clock,
                step.unwrap_or(0.0),
                (index + 1 == clock.steps.len()).then_some(&mut samples),
            );
        }
        if !samples.is_empty() {
            clock.progress = Some(samples.into());
        }
    }
    let progress = clock.progress.clone();
    frames.clocks.insert(actor, clock);
    progress
}

fn queued_zero_progress(lua: &Lua, actor: &Table) -> Option<std::sync::Arc<[[f32; 2]]>> {
    let empty_tail = lua
        .app_data_ref::<SongLuaCompileFrames>()?
        .clocks
        .get(&(actor.to_pointer() as usize))
        .is_some_and(|clock| clock.steps.back() == Some(&Some(0.0)));
    if !empty_tail {
        return None;
    }
    let progress = actor_tween_progress(lua, actor)?;
    let frames = lua.app_data_ref::<SongLuaCompileFrames>()?;
    // Even sleep(0) waits for a positive actor update. Its queued destination
    // cannot become a current position during another callback in this frame.
    // Both clocks use native float timestamps, including long song times.
    let later = lua
        .app_data_ref::<SongLuaCompileUpdatePhase>()
        .is_some_and(|phase| {
            phase.active && queue_actor_order(lua, actor.to_pointer() as usize) > phase.order
        });
    (later
        || progress
            .first()
            .is_some_and(|sample| sample[0] > frames.time(frames.frame) as f32))
    .then_some(progress)
}

fn drain_queue_clock(lua: &Lua, actor: &Table) -> Option<(usize, f64)> {
    let actor = actor.to_pointer() as usize;
    let mut frames = lua.app_data_mut::<SongLuaCompileFrames>()?;
    let mut clock = frames.clocks.remove(&actor)?;
    if let Some(time) = clock.dispatch {
        frames.clocks.insert(actor, clock);
        return Some(time);
    }
    while let Some(step) = clock.steps.pop_front() {
        advance_queue_clock(&frames, &mut clock, step.unwrap_or(0.0), None);
        if step.is_none() {
            let time = (clock.frame, clock.remaining);
            clock.dispatch = Some(time);
            frames.clocks.insert(actor, clock);
            return Some(time);
        }
    }
    frames.clocks.insert(actor, clock);
    None
}

fn queued_dispatch_seconds(lua: &Lua) -> Option<f64> {
    let scope = *lua.app_data_ref::<SongLuaQueuedCommand>()?;
    let (frame, _) = scope.time?;
    Some(lua.app_data_ref::<SongLuaCompileFrames>()?.time(frame))
}

fn queued_render_advance(
    lua: &Lua,
    scope: SongLuaQueuedCommand,
    actor: &Table,
    immediate: bool,
) -> f32 {
    let Some((frame, remaining)) = scope.time else {
        return actor_frame_advance(lua, actor);
    };
    let later =
        queue_actor_order(lua, actor.to_pointer() as usize) > queue_actor_order(lua, scope.actor);
    let Some(frames) = lua.app_data_ref::<SongLuaCompileFrames>() else {
        return 0.0;
    };
    let advance = if immediate {
        0.0
    } else if scope.actor == actor.to_pointer() as usize {
        remaining
    } else if later {
        frames.delta(frame)
    } else {
        0.0
    };
    (frames.time(frames.frame) + f64::from(scope.start) - frames.time(frame) + advance) as f32
}

pub struct SongLuaStartupState {
    pub initial: SongLuaOverlayState,
    pub blocks: Vec<SongLuaOverlayCommandBlock>,
    steps: Vec<f32>,
    commands: Vec<Option<String>>,
    block_steps: Vec<Option<usize>>,
    clock_path: Vec<[f32; 2]>,
}

// Load-time Actor::UpdateTweening state. Each queue consumes the actual actor
// delta after ancestor hibernation, wrappers and ActorFrame update rates. Its
// native float time-left samples become immutable startup playback blocks.
struct ActorTweenReplay {
    current: TweenPose,
    render_flags: Option<RenderFlags>,
    base_transform: Option<Box<BaseTransform>>,
    queue: std::collections::VecDeque<ActorTweenStep>,
    progress: Option<Vec<Vec<[f32; 2]>>>,
    targets: u128,
}

struct ActorTweenStep {
    index: usize,
    duration: f32,
    left: f32,
    from: Option<TweenPose>,
    to: TweenPose,
    easing: Option<String>,
    opt1: Option<f32>,
    command: Option<String>,
}

struct ActorTweenReplays(FxHashMap<usize, ActorTweenReplay>);

// Preparation can still need compiler captures while constructing visual
// bindings. After ownership transfers to the live scene, pending native queues
// and immediate actor properties are sufficient; audit history is disposable.
pub(crate) struct LiveActorPlayback;

fn live_actor_playback(lua: &Lua) -> bool {
    lua.app_data_ref::<LiveActorPlayback>().is_some()
}

pub(crate) fn begin_live_actor_playback(lua: &Lua, actors: &[Table]) -> mlua::Result<()> {
    for actor in actors {
        ensure_tween_replay(lua, actor)?;
        // Promotion must not re-read unrelated properties such as Sprite's
        // Texture fallback, which can invoke authored __index code.
        let model = actor.raw_get::<Option<LuaFieldText<32>>>("__songlua_actor_type")?
            .is_some_and(|kind| kind.as_str().eq_ignore_ascii_case("Model"));
        let mut flags = RenderFlags::from(SongLuaOverlayState {
            depth_test: model, depth_write: model, texture_wrapping: model,
            ..Default::default()
        });
        for flag in RenderFlag::ALL {
            if let Some(value) = actor.raw_get::<Option<bool>>(flag.state_key())? {
                flags.set(flag, value);
            }
        }
        let base_transform = BaseTransform::read(actor)?;
        if let Some(mut replays) = lua.app_data_mut::<ActorTweenReplays>()
            && let Some(replay) = replays.0.get_mut(&(actor.to_pointer() as usize)) {
            replay.render_flags = Some(flags);
            replay.base_transform = Some(Box::new(base_transform));
        }
        for flag in RenderFlag::ALL { actor.raw_set(flag.state_key(), Value::Nil)?; }
        for field in BaseField::ALL { actor.raw_set(field.state_key(), Value::Nil)?; }
        reset_actor_capture(lua, actor)?;
        actor.raw_set("__songlua_capture_blocks", Value::Nil)?;
    }
    lua.set_app_data(LiveActorPlayback);
    Ok(())
}

fn live_base_transform(lua: &Lua, actor: &Table) -> Option<BaseTransform> {
    lua.app_data_ref::<ActorTweenReplays>()?
        .0.get(&(actor.to_pointer() as usize))?.base_transform.as_deref().copied()
}

fn set_live_base_field(lua: &Lua, actor: &Table, key: &str, value: f32) -> bool {
    let Some(field) = BaseField::from_key(key) else { return false; };
    let Some(mut replays) = lua.app_data_mut::<ActorTweenReplays>() else { return false; };
    let Some(base) = replays.0.get_mut(&(actor.to_pointer() as usize))
        .and_then(|replay| replay.base_transform.as_mut()) else { return false; };
    base.set(field, value);
    true
}

#[cfg(test)]
pub(crate) fn use_table_base_transform(lua: &Lua, actors: &[Table]) -> mlua::Result<()> {
    for actor in actors {
        let base = lua.app_data_mut::<ActorTweenReplays>().and_then(|mut replays|
            replays.0.get_mut(&(actor.to_pointer() as usize))?.base_transform.take());
        if let Some(base) = base {
            for (key, value) in base.snapshot() { actor.raw_set(key, value)?; }
        }
    }
    Ok(())
}

fn live_render_flags(lua: &Lua, actor: &Table) -> Option<RenderFlags> {
    lua.app_data_ref::<ActorTweenReplays>()?
        .0.get(&(actor.to_pointer() as usize))?.render_flags
}

fn set_live_render_flag(lua: &Lua, actor: &Table, key: &str, value: bool) -> bool {
    let Some(flag) = RenderFlag::from_key(key) else { return false; };
    let Some(mut replays) = lua.app_data_mut::<ActorTweenReplays>() else { return false; };
    let Some(flags) = replays.0.get_mut(&(actor.to_pointer() as usize))
        .and_then(|replay| replay.render_flags.as_mut()) else { return false; };
    flags.set(flag, value);
    true
}

#[cfg(test)]
pub(crate) fn use_table_render_flags(lua: &Lua, actors: &[Table]) -> mlua::Result<()> {
    for actor in actors {
        let flags = lua.app_data_mut::<ActorTweenReplays>().and_then(|mut replays|
            replays.0.get_mut(&(actor.to_pointer() as usize))?.render_flags.take());
        if let Some(flags) = flags {
            for (key, value) in flags.snapshot() { actor.raw_set(key, value)?; }
        }
    }
    Ok(())
}

fn live_current_value<T>(lua: &Lua, actor: &Table, read: impl FnOnce(&TweenPose) -> T) -> Option<T> {
    if !crate::live::active(lua) { return None; }
    lua.app_data_ref::<ActorTweenReplays>()?
        .0.get(&(actor.to_pointer() as usize)).map(|replay| read(&replay.current))
}

fn live_current_pose(lua: &Lua, actor: &Table) -> Option<TweenPose> {
    live_current_value(lua, actor, |current| *current)
}

// Tween setters and destination getters address the queue tail, while drawing reads
// current. During playback the replay owns both; Lua tables are only exported
// for public getter results and explicit definition/state snapshots.
fn live_destination_value<T>(
    lua: &Lua,
    actor: &Table,
    read: impl FnOnce(&TweenPose) -> T,
) -> Option<T> {
    if !live_actor_playback(lua) { return None; }
    lua.app_data_ref::<ActorTweenReplays>()?
        .0.get(&(actor.to_pointer() as usize))
        .map(|replay| read(replay.queue.back().map_or(&replay.current, |step| &step.to)))
}

fn live_destination_scalar(lua: &Lua, actor: &Table, key: &str) -> Option<f32> {
    live_destination_value(lua, actor, |dest| Some(match key {
        "aux" => dest.aux,
        "x" => dest.x,
        "y" => dest.y,
        "z" => dest.z,
        "zoom" => dest.zoom,
        "zoom_x" => dest.zoom_x,
        "zoom_y" => dest.zoom_y,
        "zoom_z" => dest.zoom_z,
        "skew_x" => dest.skew_x,
        "skew_y" => dest.skew_y,
        "cropleft" => dest.cropleft,
        "cropright" => dest.cropright,
        "croptop" => dest.croptop,
        "cropbottom" => dest.cropbottom,
        "fadeleft" => dest.fadeleft,
        "faderight" => dest.faderight,
        "fadetop" => dest.fadetop,
        "fadebottom" => dest.fadebottom,
        // Spin still owns a separate rotation queue. Keep its table-backed
        // getter synchronization until that queue shares the native pose.
        _ => return None,
    })).flatten()
}

fn actor_destination_position(lua: &Lua, actor: &Table, axis: usize) -> mlua::Result<f32> {
    if let Some(value) = live_destination_value(lua, actor, |dest| [dest.x, dest.y, dest.z][axis]) {
        return Ok(value);
    }
    Ok(actor.get::<Option<f32>>(POSITION_DEST_KEYS[axis])?.unwrap_or(0.0))
}

fn actor_destination_zoom(lua: &Lua, actor: &Table, key: &str) -> mlua::Result<f32> {
    if let Some(value) = live_destination_scalar(lua, actor, key) { return Ok(value); }
    Ok(actor.get::<Option<f32>>(actor_capture_state_key(key).as_ref())?
        .or(actor.get::<Option<f32>>("__songlua_state_zoom")?).unwrap_or(1.0))
}

fn live_pose_snapshot(lua: &Lua, actor: &Table) -> mlua::Result<Option<[(&'static str, Value); 26]>> {
    let Some(dest) = live_destination_value(lua, actor, |dest| *dest) else { return Ok(None); };
    let current = live_current_pose(lua, actor).expect("live destination has a current pose");
    // Release the native state borrow before allocating or calling Lua.
    Ok(Some([
        ("__songlua_diffuse", Value::Table(make_color_table(lua, dest.diffuse)?)),
        ("__songlua_state_diffuse", Value::Table(make_color_table(lua, dest.diffuse)?)),
        ("__songlua_state_glow", Value::Table(make_color_table(lua, dest.glow)?)),
        ("__songlua_state_vertex_colors", dest.vertex_colors
            .map(|colors| make_vertex_color_table(lua, colors).map(Value::Table))
            .transpose()?.unwrap_or(Value::Nil)),
        ("__songlua_state_aux", Value::Number(dest.aux.into())),
        ("__songlua_state_x", Value::Number(dest.x.into())),
        ("__songlua_state_y", Value::Number(dest.y.into())),
        ("__songlua_state_z", Value::Number(dest.z.into())),
        ("__songlua_state_zoom", Value::Number(dest.zoom.into())),
        ("__songlua_state_zoom_x", Value::Number(dest.zoom_x.into())),
        ("__songlua_state_zoom_y", Value::Number(dest.zoom_y.into())),
        ("__songlua_state_zoom_z", Value::Number(dest.zoom_z.into())),
        ("__songlua_state_skew_x", Value::Number(dest.skew_x.into())),
        ("__songlua_state_skew_y", Value::Number(dest.skew_y.into())),
        ("__songlua_state_cropleft", Value::Number(dest.cropleft.into())),
        ("__songlua_state_cropright", Value::Number(dest.cropright.into())),
        ("__songlua_state_croptop", Value::Number(dest.croptop.into())),
        ("__songlua_state_cropbottom", Value::Number(dest.cropbottom.into())),
        ("__songlua_state_fadeleft", Value::Number(dest.fadeleft.into())),
        ("__songlua_state_faderight", Value::Number(dest.faderight.into())),
        ("__songlua_state_fadetop", Value::Number(dest.fadetop.into())),
        ("__songlua_state_fadebottom", Value::Number(dest.fadebottom.into())),
        ("__songlua_aux", Value::Number(current.aux.into())),
        ("__songlua_current_x", Value::Number(current.x.into())),
        ("__songlua_current_y", Value::Number(current.y.into())),
        ("__songlua_current_z", Value::Number(current.z.into())),
    ]))
}

pub(crate) fn copy_live_actor_pose(lua: &Lua, source: &Table, into: &Table) -> mlua::Result<()> {
    if let Some(pose) = live_pose_snapshot(lua, source)? {
        for (key, value) in pose { into.raw_set(key, value)?; }
    }
    if let Some(flags) = live_render_flags(lua, source) {
        for (key, value) in flags.snapshot() { into.raw_set(key, value)?; }
    }
    if let Some(base) = live_base_transform(lua, source) {
        for (key, value) in base.snapshot() { into.raw_set(key, value)?; }
    }
    Ok(())
}

struct LiveBroadcasts(Vec<(f32, String, bool)>);
struct LiveSourceBroadcasts(Vec<crate::live::SongLuaLiveBroadcast>);

pub(crate) fn begin_live_runtime(lua: &Lua) -> mlua::Result<()> {
    lua.remove_app_data::<LiveActorPlayback>();
    lua.remove_app_data::<CanonicalLiveActorHandles>();
    // The process VM survives ScreenGameplay reentry; the actor execution
    // plan belongs to the outgoing screen and must release its old tables.
    invalidate_compile_update_plan(lua);
    lua.set_app_data(LiveBroadcasts(Vec::new()));
    lua.set_app_data(LiveSourceBroadcasts(Vec::new()));
    if lua.app_data_ref::<crate::live::SourceRequests>().is_none() {
        lua.set_app_data(crate::live::SourceRequests(Vec::new()));
    }
    let scene = lua.app_data_ref::<crate::live::LiveMode>().map_or(1, |mode| mode.0 + 1);
    lua.set_app_data(crate::live::LiveMode(scene));
    lua.remove_app_data::<crate::live::LivePlayers>();
    lua.set_app_data(ActorTweenReplays(FxHashMap::default()));
    set_compile_frames(lua, [(0.0, 0.0)].into_iter())?;
    if let Some(mut frames) = lua.app_data_mut::<SongLuaCompileFrames>() {
        frames.actor_frames = Some(vec![[0.0, 0.0]]);
    }
    Ok(())
}

pub(crate) fn set_live_frame(lua: &Lua, seconds: f64, beat: f64, delta: f64) {
    if let Some(mut frames) = lua.app_data_mut::<SongLuaCompileFrames>() {
        frames.times[0] = seconds;
        frames.beats[0] = beat as f32;
        if let Some(actor_frames) = &mut frames.actor_frames {
            actor_frames[0] = [delta as f32, delta as f32];
        }
    }
}

pub(crate) use crate::tween_pose::TARGETS as TWEEN_POSE_TARGETS;

pub(crate) fn begin_tween_replay<Kind>(
    lua: &Lua,
    overlays: &[SongLuaOverlayCompileActor<Kind>],
    states: &SongLuaStartupStates,
) {
    let mut replays = FxHashMap::default();
    // A queue can belong to a callback-only ActorFrame which has no drawable
    // overlay. Its commands still own native time and must dispatch normally.
    // The Lua actor registry owns the live actors. Startup states need only
    // their existing identity keys and baked replay data; retaining fresh Rust
    // Table handles in each capture map exhausts Lua 5.1's auxiliary stack.
    for (&pointer, startup) in states.iter().filter(|(_, state)| !state.steps.is_empty()) {
        let current = overlays
            .iter()
            .find(|overlay| overlay.table.to_pointer() as usize == pointer)
            .map_or_else(
                || crate::overlay_state_after_blocks(startup.initial, &startup.blocks, 0.0),
                |overlay| overlay.actor.initial_state,
            );
        let mut to = current;
        let mut targets = 0;
        let mut queue = std::collections::VecDeque::new();
        for (index, &duration) in startup.steps.iter().enumerate() {
            let mut easing = None;
            let mut opt1 = None;
            for (block, step) in startup.blocks.iter().zip(&startup.block_steps) {
                if *step != Some(index + 1) {
                    continue;
                }
                crate::apply_overlay_delta(&mut to, &block.delta);
                for target in TWEEN_POSE_TARGETS {
                    if block.delta.has_update_target(target) {
                        targets |= 1u128 << target as usize;
                    }
                }
                easing.clone_from(&block.easing);
                opt1 = block.opt1;
            }
            queue.push_back(ActorTweenStep {
                index,
                duration,
                left: duration,
                from: None,
                to: to.into(),
                easing,
                opt1,
                command: startup.commands.get(index).cloned().flatten(),
            });
        }
        replays.insert(
            pointer,
            ActorTweenReplay {
                current: current.into(),
                render_flags: None,
                base_transform: None,
                queue,
                progress: Some(vec![Vec::new(); startup.steps.len()]),
                targets,
            },
        );
    }
    lua.set_app_data(ActorTweenReplays(replays));
}

fn queue_replay_tween(lua: &Lua, actor: &Table, duration: f64, easing: Option<&str>) {
    let Some(mut replays) = lua.app_data_mut::<ActorTweenReplays>() else {
        return;
    };
    let Some(replay) = replays.0.get_mut(&(actor.to_pointer() as usize)) else {
        return;
    };
    let to = replay.queue.back().map_or(replay.current, |step| step.to);
    let duration = duration as f32;
    let index = replay.progress.as_ref().map_or(0, Vec::len);
    if let Some(progress) = &mut replay.progress {
        progress.push(Vec::new());
    }
    replay.queue.push_back(ActorTweenStep {
        index,
        duration,
        left: duration,
        from: None,
        to,
        easing: easing.map(str::to_owned),
        opt1: None,
        command: None,
    });
}

fn record_tween_write(
    lua: &Lua,
    actor: &Table,
    target: SongLuaOverlayUpdateTarget,
    value: &SongLuaOverlayUpdateValue,
) -> Option<bool> {
    if !TWEEN_POSE_TARGETS.contains(&target) {
        return None;
    }
    let mut replays = lua.app_data_mut::<ActorTweenReplays>()?;
    let replay = replays.0.get_mut(&(actor.to_pointer() as usize))?;
    replay.targets |= 1u128 << target as usize;
    let queued = !replay.queue.is_empty();
    let dest = replay
        .queue
        .back_mut()
        .map_or(&mut replay.current, |step| &mut step.to);
    dest.set(target, value);
    drop(replays);
    // Pose replay owns the queue, but message provenance must retain its
    // actual delay. Static command blocks cannot describe an existing tail.
    let broadcast = lua
        .app_data_ref::<SongLuaOverlayUpdateCapture>()
        .is_some_and(|capture| capture.active_broadcast.is_some());
    if broadcast && !live_actor_playback(lua) {
        let beat = compile_song_runtime_values(lua).map_or(0.0, |(beat, _)| beat);
        let duration = actor
            .get::<Option<f32>>("__songlua_capture_duration")
            .ok()
            .flatten()
            .unwrap_or(0.0);
        let delay = if duration > 0.0 {
            let hibernate = actor
                .raw_get::<Option<f32>>("__songlua_hibernate_seconds")
                .ok()
                .flatten()
                .unwrap_or(0.0);
            lua.app_data_ref::<ActorTweenReplays>()
                .and_then(|replays| {
                    replays.0.get(&(actor.to_pointer() as usize)).map(|replay| {
                        replay
                            .queue
                            .iter()
                            .take(replay.queue.len().saturating_sub(1))
                            .fold(hibernate, |total, step| total + step.left)
                    })
                })
                .unwrap_or(0.0)
        } else {
            0.0
        };
        let easing = actor
            .get::<Option<String>>("__songlua_capture_easing")
            .ok()
            .flatten();
        let opt1 = actor
            .get::<Option<f32>>("__songlua_capture_opt1")
            .ok()
            .flatten();
        if let Some(mut capture) = lua.app_data_mut::<SongLuaOverlayUpdateCapture>() {
            capture.record_stateful_message(
                actor,
                beat,
                target,
                value.clone(),
                delay,
                duration,
                easing,
                opt1,
            );
        }
    }
    #[cfg(feature = "test-support")]
    observe_tween_write(lua, actor, target, queued, || value.clone());
    #[cfg(not(feature = "test-support"))]
    let _ = queued;
    Some(true)
}

#[cfg(feature = "test-support")]
fn observe_tween_write(
    lua: &Lua,
    actor: &Table,
    target: SongLuaOverlayUpdateTarget,
    queued: bool,
    value: impl FnOnce() -> SongLuaOverlayUpdateValue,
) {
    if let Some(mut capture) = lua.app_data_mut::<SongLuaOverlayUpdateCapture>()
        && let Some(index) = capture.touch(actor) {
        let beat = compile_song_runtime_values(lua).map_or(0.0, |(beat, _)| beat);
        capture.record_write(index, beat, target, &value(), queued);
    }
}

fn sync_tween_getters(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    if live_actor_playback(lua) {
        // Rotation remains shared with the spin queue. Other tween properties
        // are native-owned and exported only at explicit snapshot boundaries.
        if let Some(rotation) = live_destination_value(lua, actor,
            |dest| [dest.rot_x_deg, dest.rot_y_deg, dest.rot_z_deg]) {
            for (key, value) in ["rot_x_deg", "rot_y_deg", "rot_z_deg"].into_iter().zip(rotation) {
                set_actor_capture_state(actor, key, value)?;
            }
        }
        return Ok(());
    }
    let poses = lua.app_data_ref::<ActorTweenReplays>().and_then(|replays| {
        replays.0.get(&(actor.to_pointer() as usize)).map(|replay| {
            (
                replay.current,
                replay.queue.back().map_or(replay.current, |step| step.to),
            )
        })
    });
    let Some((current, dest)) = poses else {
        return Ok(());
    };
    for (key, value) in [
        ("aux", dest.aux),
        ("x", dest.x),
        ("y", dest.y),
        ("z", dest.z),
        ("zoom", dest.zoom),
        ("zoom_x", dest.zoom_x),
        ("zoom_y", dest.zoom_y),
        ("zoom_z", dest.zoom_z),
        ("rot_x_deg", dest.rot_x_deg),
        ("rot_y_deg", dest.rot_y_deg),
        ("rot_z_deg", dest.rot_z_deg),
        ("skew_x", dest.skew_x),
        ("skew_y", dest.skew_y),
        ("cropleft", dest.cropleft),
        ("cropright", dest.cropright),
        ("croptop", dest.croptop),
        ("cropbottom", dest.cropbottom),
        ("fadeleft", dest.fadeleft),
        ("faderight", dest.faderight),
        ("fadetop", dest.fadetop),
        ("fadebottom", dest.fadebottom),
    ] {
        set_actor_capture_state(actor, key, value)?;
    }
    actor.raw_set("__songlua_diffuse", make_color_table(lua, dest.diffuse)?)?;
    actor.raw_set(
        "__songlua_state_diffuse",
        make_color_table(lua, dest.diffuse)?,
    )?;
    actor.raw_set("__songlua_state_glow", make_color_table(lua, dest.glow)?)?;
    actor.raw_set(
        "__songlua_state_vertex_colors",
        dest.vertex_colors
            .map(|colors| make_vertex_color_table(lua, colors))
            .transpose()?,
    )?;
    actor.raw_set("__songlua_aux", current.aux)?;
    for (key, value) in POSITION_CURRENT_KEYS
        .into_iter()
        .zip([current.x, current.y, current.z])
    {
        actor.raw_set(key, value)?;
    }
    Ok(())
}

fn clear_replay_tweens(lua: &Lua, actor: &Table, finish: bool) -> mlua::Result<()> {
    if let Some(mut replays) = lua.app_data_mut::<ActorTweenReplays>()
        && let Some(replay) = replays.0.get_mut(&(actor.to_pointer() as usize))
    {
        if finish && let Some(step) = replay.queue.back() {
            replay.current = step.to;
        }
        replay.queue.clear();
    }
    sync_tween_getters(lua, actor)
}

pub(crate) fn replay_tween_pose(lua: &Lua, actor: &Table, state: &mut SongLuaOverlayState) -> bool {
    if let Some(replays) = lua.app_data_ref::<ActorTweenReplays>()
        && let Some(replay) = replays.0.get(&(actor.to_pointer() as usize))
    {
        replay.current.write_to(state);
        return true;
    }
    false
}

pub(crate) fn ensure_tween_replay(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    if has_tween_replay(lua, actor) {
        return Ok(());
    }
    let current = actor_overlay_initial_state(actor).map_err(mlua::Error::external)?;
    let render_flags = live_actor_playback(lua).then(|| RenderFlags::from(current));
    let native_base = live_actor_playback(lua);
    let base_transform = native_base.then(|| BaseTransform::read(actor).map(Box::new)).transpose()?;
    if let Some(mut replays) = lua.app_data_mut::<ActorTweenReplays>() {
        replays.0.insert(actor.to_pointer() as usize, ActorTweenReplay {
            current: current.into(), render_flags, base_transform, queue: Default::default(),
            progress: (!crate::live::active(lua)).then(Vec::new), targets: 0,
        });
    }
    if render_flags.is_some() {
        for flag in RenderFlag::ALL { actor.raw_set(flag.state_key(), Value::Nil)?; }
    }
    if native_base {
        for field in BaseField::ALL { actor.raw_set(field.state_key(), Value::Nil)?; }
    }
    Ok(())
}

fn has_tween_replay(lua: &Lua, actor: &Table) -> bool {
    lua.app_data_ref::<ActorTweenReplays>()
        .is_some_and(|replays| replays.0.contains_key(&(actor.to_pointer() as usize)))
}

fn queue_replay_command(lua: &Lua, actor: &Table, name: &str) -> bool {
    if !has_tween_replay(lua, actor) {
        return false;
    }
    queue_replay_tween(lua, actor, 0.0, Some("linear"));
    lua.app_data_mut::<ActorTweenReplays>()
        .expect("actor replay exists")
        .0
        .get_mut(&(actor.to_pointer() as usize))
        .expect("actor replay exists")
        .queue
        .back_mut()
        .expect("queued a zero-time state")
        .command = Some(name.to_owned());
    true
}

fn dispatch_replay_command(
    lua: &Lua,
    actor: &Table,
    name: &str,
    remaining: f32,
) -> mlua::Result<()> {
    reset_actor_capture(lua, actor)?;
    let frame = lua
        .app_data_ref::<SongLuaCompileFrames>()
        .map_or(0, |frames| frames.frame);
    let prior = lua.remove_app_data::<SongLuaQueuedCommand>();
    lua.set_app_data(SongLuaQueuedCommand {
        start: 0.0,
        actor: actor.to_pointer() as usize,
        time: Some((frame, f64::from(remaining))),
    });
    if let Some(mut capture) = lua.app_data_mut::<SongLuaOverlayUpdateCapture>() {
        capture.queued_depth += 1;
    }
    let result = if let Some(message) = name.strip_prefix('!') {
        crate::host::broadcast_queued_message(lua, message)
    } else {
        // Actor::PlayCommand goes through virtual HandleMessage. ActorFrame
        // forwards non-broadcast commands to its children, even without an
        // identically named command on the frame itself.
        run_actor_message_with_params(lua, actor, &ActorCommandName::new(name, "Command"), None)
    };
    if let Some(mut capture) = lua.app_data_mut::<SongLuaOverlayUpdateCapture>() {
        capture.queued_depth -= 1;
    }
    lua.remove_app_data::<SongLuaQueuedCommand>();
    if let Some(prior) = prior {
        lua.set_app_data(prior);
    }
    result
}

fn advance_tween_replay(lua: &Lua, actor: &Table, delta: f64, span: f32) -> mlua::Result<()> {
    if lua.app_data_ref::<ActorTweenReplays>()
        .and_then(|replays| replays.0.get(&(actor.to_pointer() as usize)).map(|replay| replay.queue.is_empty()))
        .unwrap_or(true)
    {
        return Ok(());
    }
    let time = lua
        .app_data_ref::<SongLuaCompileFrames>()
        .map_or(0.0, |frames| frames.time(frames.frame) as f32);
    let mut remaining = delta as f32;
    let mut dispatched = 0;
    // Actor::UpdateTweening can execute a command which replaces its queue.
    // Release the queue borrow before dispatch, then consume the actual float
    // remainder in whatever queue the Lua body left behind.
    while remaining > 0.0 {
        let command = {
            let mut replays = lua
                .app_data_mut::<ActorTweenReplays>()
                .expect("actor replay exists");
            let replay = replays
                .0
                .get_mut(&(actor.to_pointer() as usize))
                .expect("actor replay exists");
            let Some(step) = replay.queue.front_mut() else {
                break;
            };
            let beginning = step.from.is_none();
            if beginning {
                step.from = Some(replay.current);
                let start = if step.duration == 0.0 {
                    time
                } else {
                    time - span * (remaining / delta as f32)
                };
                if let Some(progress) = &mut replay.progress {
                    progress[step.index].push([start, step.duration]);
                }
            }
            let command = beginning.then(|| step.command.clone()).flatten();
            let used = remaining.min(step.left);
            step.left -= used;
            remaining -= used;
            if let Some(progress) = &mut replay.progress {
                progress[step.index].push([time, step.left]);
            }
            if step.left == 0.0 {
                replay.current = step.to;
                replay.queue.pop_front();
            } else {
                let factor = crate::overlay_command_ease_factor(
                    step.easing.as_deref(),
                    1.0 - step.left / step.duration,
                    step.opt1,
                );
                replay.current = step
                    .from
                    .expect("the queue head captured its starting pose")
                    .lerp(step.to, factor);
            }
            command
        };
        // Advancing time changes current, never the back tween destination.
        // Live current getters and rendering read the native pose directly;
        // rewriting every destination scalar/color here only serializes the
        // same data back to Lua (including fresh color tables every tick).
        // Queue replacement and InitState still synchronize destinations.
        if !crate::live::active(lua) { sync_tween_getters(lua, actor)?; }
        if let Some(command) = command {
            dispatched += 1;
            if dispatched > 10000 {
                return Err(mlua::Error::runtime(
                    "nonterminating zero-time actor command queue",
                ));
            }
            dispatch_replay_command(lua, actor, &command, remaining)?;
        }
    }
    Ok(())
}

pub(crate) fn sample_tween_replays(lua: &Lua) {
    let time = lua
        .app_data_ref::<SongLuaCompileFrames>()
        .map_or(0.0, |frames| frames.time(frames.frame) as f32);
    if let Some(mut replays) = lua.app_data_mut::<ActorTweenReplays>() {
        for replay in replays.0.values_mut() {
            let Some(progress) = &mut replay.progress else { continue; };
            if let Some(step) = replay.queue.front()
                && step.from.is_some()
                && progress[step.index]
                    .last()
                    .is_none_or(|sample| sample[0] != time)
            {
                progress[step.index].push([time, step.left]);
            }
        }
    }
}

pub(crate) fn capture_tween_poses(lua: &Lua, actors: &[Table]) {
    let Some(replays) = lua.app_data_ref::<ActorTweenReplays>() else {
        return;
    };
    let Some(mut capture) = lua.app_data_mut::<SongLuaOverlayUpdateCapture>() else {
        return;
    };
    for actor in actors {
        let Some(replay) = replays.0.get(&(actor.to_pointer() as usize)) else {
            continue;
        };
        let Some(index) = capture.touch(actor) else {
            continue;
        };
        for target in TWEEN_POSE_TARGETS {
            if replay.targets & (1u128 << target as usize) == 0 {
                continue;
            }
            let value = replay.current.get(target);
            SongLuaOverlayUpdateCapture::replace_value(
                &mut capture.values[index],
                target,
                value.clone(),
            );
            SongLuaOverlayUpdateCapture::replace_value(
                &mut capture.final_values[index],
                target,
                value,
            );
        }
    }
}

pub(crate) fn finish_tween_replay<Kind>(
    lua: &Lua,
    overlays: &mut [SongLuaOverlayCompileActor<Kind>],
    states: &SongLuaStartupStates,
) {
    let Some(replays) = lua.remove_app_data::<ActorTweenReplays>() else {
        return;
    };
    for overlay in overlays {
        let pointer = overlay.table.to_pointer() as usize;
        let (Some(replay), Some(startup)) = (replays.0.get(&pointer), states.get(&pointer)) else {
            continue;
        };
        let Some(command) = overlay
            .actor
            .message_commands
            .iter_mut()
            .find(|command| command.message == "__songlua_actor_startup")
        else {
            continue;
        };
        for (block, step) in command.blocks.iter_mut().zip(&startup.block_steps) {
            let Some(index) = step.and_then(|step| step.checked_sub(1)) else {
                continue;
            };
            let Some(progress) = &replay.progress else { continue; };
            let samples = &progress[index];
            if !samples.is_empty() {
                block.progress = Some(samples.clone().into());
            }
        }
    }
}

// Snapshot the native update ancestry before speculative queued-body captures.
// A wrapper receives its owner's wake remainder before the owner applies its rate.
fn startup_clock_path(actor: &Table) -> mlua::Result<Vec<[f32; 2]>> {
    let mut path = Vec::new();
    let mut next = Some(actor.clone());
    let mut wrapper_child = false;
    let mut visited = HashSet::new();
    while let Some(actor) = next {
        if !visited.insert(actor.to_pointer() as usize) {
            return Err(mlua::Error::external("cycle in actor update ancestry"));
        }
        let hibernate = if actor
            .raw_get::<Option<bool>>("__songlua_theme_hibernating")?
            .unwrap_or(false)
        {
            f32::INFINITY
        } else {
            actor
                .raw_get::<Option<f32>>("__songlua_hibernate_seconds")?
                .unwrap_or(0.0)
        };
        path.push([
            hibernate,
            if wrapper_child {
                1.0
            } else {
                actor_update_rate(&actor)? as f32
            },
        ]);
        wrapper_child = actor_type_is(&actor, "WrapperState")?;
        next = actor.raw_get::<Option<Table>>("__songlua_parent")?;
    }
    path.reverse();
    Ok(path)
}

fn startup_clock_frames(times: &[f64], path: &[[f32; 2]]) -> Vec<[f32; 2]> {
    let mut path = path.to_vec();
    times
        .iter()
        .enumerate()
        .map(|(frame, &time)| {
            let mut delta = if frame == 0 {
                0.0
            } else {
                time as f32 - times[frame - 1] as f32
            };
            let mut span = delta;
            for [hibernate, rate] in &mut path {
                if *hibernate > 0.0 {
                    *hibernate -= delta;
                    if *hibernate > 0.0 {
                        return [0.0, 0.0];
                    }
                    let remainder = -*hibernate;
                    span = if delta > 0.0 {
                        span * (remainder / delta)
                    } else {
                        0.0
                    };
                    delta = remainder;
                    *hibernate = 0.0;
                }
                delta *= *rate;
            }
            [delta, span]
        })
        .collect()
}

pub(crate) fn bake_startup_tweens<'a>(
    states: impl Iterator<Item = &'a mut SongLuaStartupState>,
    replay: impl Iterator<Item = (f64, f64)>,
) {
    let mut seconds = 0.0;
    let times = replay
        .map(|(_, delta)| {
            seconds += delta;
            seconds
        })
        .collect::<Vec<_>>();
    if times.len() < 2 {
        return;
    }
    let mut frames = SongLuaCompileFrames {
        times,
        actor_frames: None,
        beats: Vec::new(),
        frame: 0,
        epoch: 0,
        clocks: FxHashMap::default(),
    };
    for state in states {
        if state.steps.is_empty() {
            continue;
        }
        frames.actor_frames = state
            .clock_path
            .iter()
            .any(|[sleep, rate]| *sleep > 0.0 || *rate != 1.0)
            .then(|| startup_clock_frames(&frames.times, &state.clock_path));
        let mut clock = SongLuaQueueClock {
            progress: None,
            epoch: 0,
            frame: 0,
            remaining: 0.0,
            zero_frame: None,
            steps: std::collections::VecDeque::new(),
            dispatch: None,
            recurring_frame: None,
        };
        for (index, duration) in state.steps.iter().enumerate() {
            let capture = state
                .block_steps
                .iter()
                .any(|step| *step == Some(index + 1));
            let mut samples = Vec::new();
            advance_queue_clock(
                &frames,
                &mut clock,
                f64::from(*duration),
                capture.then_some(&mut samples),
            );
            if samples.is_empty() {
                continue;
            }
            let samples: std::sync::Arc<[[f32; 2]]> = samples.into();
            for (block, step) in state.blocks.iter_mut().zip(&state.block_steps) {
                if *step == Some(index + 1) {
                    block.progress = Some(samples.clone());
                }
            }
        }
    }
}
pub(crate) type SongLuaStartupStates = HashMap<usize, SongLuaStartupState>;

pub(crate) fn collect_initial_states(
    root: &Value,
    states: &mut HashMap<usize, (Table, SongLuaOverlayState)>,
) -> mlua::Result<()> {
    let Value::Table(actor) = root else {
        return Ok(());
    };
    if actor
        .get::<Option<String>>("__songlua_actor_type")?
        .is_some()
    {
        let initial = actor_overlay_initial_state(actor).map_err(mlua::Error::external)?;
        states.insert(actor.to_pointer() as usize, (actor.clone(), initial));
    }
    for child in actor.sequence_values::<Value>() {
        collect_initial_states(&child?, states)?;
    }
    Ok(())
}

pub(crate) fn screen_layer_states(
    lua: &Lua,
) -> mlua::Result<HashMap<usize, (Table, SongLuaOverlayState)>> {
    let top = lua.globals().get::<Table>("__songlua_top_screen")?;
    let children = actor_children(lua, &top)?;
    let mut states = HashMap::new();
    for name in ["Underlay", "Overlay", "SongBackground", "In"] {
        if let Some(actor) = children.get::<Option<Table>>(name)? {
            let initial = actor_overlay_initial_state(&actor).map_err(mlua::Error::external)?;
            states.insert(actor.to_pointer() as usize, (actor, initial));
        }
    }
    Ok(states)
}

pub(crate) fn capture_startup_states(
    states: HashMap<usize, (Table, SongLuaOverlayState)>,
) -> Result<HashMap<usize, SongLuaStartupState>, String> {
    states
        .into_iter()
        .map(|(pointer, (actor, initial))| {
            flush_actor_capture(&actor).map_err(|err| err.to_string())?;
            let blocks = read_actor_capture_blocks(&actor)?;
            let steps = actor
                .get::<Option<Table>>("__songlua_command_queue_steps")
                .map_err(|err| err.to_string())?
                .map(|steps| {
                    steps
                        .sequence_values::<Value>()
                        .map(|step| step.map(|step| read_f32(step).unwrap_or(0.0)))
                        .collect::<mlua::Result<Vec<_>>>()
                })
                .transpose()
                .map_err(|err| err.to_string())?
                .unwrap_or_default();
            let pending = actor
                .get::<Option<Table>>("__songlua_command_queue")
                .map_err(|err| err.to_string())?;
            let mut command_index = 1;
            let commands = actor
                .get::<Option<Table>>("__songlua_command_queue_steps")
                .map_err(|err| err.to_string())?
                .map(|captured| {
                    captured
                        .sequence_values::<Value>()
                        .map(|step| {
                            if step? != Value::Boolean(false) {
                                return Ok(None);
                            }
                            let name = pending
                                .as_ref()
                                .map(|queue| queue.raw_get::<Option<String>>(command_index))
                                .transpose()?
                                .flatten();
                            command_index += 1;
                            Ok(name)
                        })
                        .collect::<mlua::Result<Vec<_>>>()
                })
                .transpose()
                .map_err(|err| err.to_string())?
                .unwrap_or_default();
            let captured: Table = actor
                .get("__songlua_capture_blocks")
                .map_err(|err| err.to_string())?;
            let block_steps = captured
                .sequence_values::<Table>()
                .map(|block| block?.get::<Option<usize>>("__songlua_tween_step"))
                .collect::<mlua::Result<Vec<_>>>()
                .map_err(|err| err.to_string())?;
            Ok((
                pointer,
                SongLuaStartupState {
                    initial,
                    blocks,
                    steps,
                    commands,
                    block_steps,
                    clock_path: startup_clock_path(&actor).map_err(|err| err.to_string())?,
                },
            ))
        })
        .collect()
}

fn collect_startup_states(
    lua: &Lua,
    actor: &Table,
    states: &mut HashMap<usize, (Table, SongLuaOverlayState)>,
) -> mlua::Result<()> {
    if actor
        .get::<Option<String>>("__songlua_actor_type")?
        .is_some()
    {
        states.insert(
            actor.to_pointer() as usize,
            (
                actor.clone(),
                actor_overlay_initial_state(actor).map_err(mlua::Error::external)?,
            ),
        );
        // Capture queued writes separately from OnCommand, without changing
        // its tween cursor or the state used for target discovery.
        flush_actor_capture(actor)?;
        actor.set("__songlua_capture_blocks", lua.create_table()?)?;
        actor.set("__songlua_capture_immediate_block", Value::Nil)?;
        if actor_type_is(actor, "Sprite")? {
            let binding = lua.create_table()?;
            actor.for_each::<Value, Value>(|key, value| {
                if let Value::String(name) = &key {
                    let name = name.to_str()?;
                    if matches!(
                        name.as_ref(),
                        "Texture"
                            | "Frames"
                            | "__songlua_state_sprite_default_states"
                            | "__songlua_state_sprite_texture_epoch"
                            | "__songlua_state_sprite_frame_sheet"
                    ) || name.starts_with("Frame")
                        || name.starts_with("Delay")
                    {
                        binding.raw_set(key, clone_lua_value(lua, value)?)?;
                    }
                }
                Ok(())
            })?;
            actor.raw_set("__songlua_startup_sprite", binding)?;
        }
    }
    if let Some(wrappers) = actor.get::<Option<Table>>("__songlua_wrappers")? {
        for wrapper in wrappers.sequence_values::<Table>() {
            collect_startup_states(lua, &wrapper?, states)?;
        }
    }
    for child in actor.sequence_values::<Value>() {
        if let Value::Table(child) = child? {
            collect_startup_states(lua, &child, states)?;
        }
    }
    Ok(())
}

fn collect_wrapper_initials(
    actor: &Table,
    states: &mut HashMap<usize, (Table, SongLuaOverlayState)>,
) -> mlua::Result<()> {
    if let Some(wrappers) = actor.get::<Option<Table>>("__songlua_wrappers")? {
        for wrapper in wrappers.sequence_values::<Table>() {
            let wrapper = wrapper?;
            states
                .entry(wrapper.to_pointer() as usize)
                .or_insert_with(|| (wrapper.clone(), SongLuaOverlayState::default()));
            collect_wrapper_initials(&wrapper, states)?;
        }
    }
    for child in actor.sequence_values::<Value>() {
        if let Value::Table(child) = child? {
            collect_wrapper_initials(&child, states)?;
        }
    }
    Ok(())
}

fn read_startup(lua: &Lua, context: &SongLuaCompileContext) -> mlua::Result<crate::SongLuaStartup> {
    let globals = lua.globals();
    let mut startup = crate::SongLuaStartup::default();
    for (index, key) in crate::SONG_LUA_PLAYER_OPTIONS_KEYS.iter().enumerate() {
        if context.players[index].enabled {
            let options = globals.get::<Table>(*key)?;
            startup.noteskins[index] = options.raw_get("__songlua_noteskin_override")?;
        }
    }
    let screen = globals.get::<Table>("__songlua_top_screen")?;
    startup.min_seconds_to_music = screen.raw_get("__songlua_min_seconds_to_music")?;
    let children = crate::actor_children(lua, &screen)?;
    if let Some(actor) = children.get::<Option<Table>>("In")? {
        startup.hide_in = actor.get::<Option<bool>>("__songlua_visible")? == Some(false);
    }
    Ok(startup)
}

#[derive(Default)]
pub(crate) struct SongLuaStartupCapture {
    pub queued: SongLuaStartupStates,
    pub tweens: SongLuaStartupStates,
    pub updates: SongLuaStartupStates,
    pub options: crate::SongLuaStartup,
    pub broadcasts: Vec<(f32, String, bool)>,
}

fn capture_initial_update(
    lua: &Lua,
    root: &Table,
    context: &SongLuaCompileContext,
) -> mlua::Result<SongLuaStartupStates> {
    if context.live && context.defer_initial_live_update {
        return Ok(HashMap::new());
    }
    let root = Value::Table(root.clone());
    let mut states = HashMap::new();
    collect_initial_states(&root, &mut states)?;
    let mut prefixes = HashMap::new();
    for (&pointer, (actor, _)) in &states {
        flush_actor_capture(actor)?;
        prefixes.insert(
            pointer,
            read_actor_capture_blocks(actor).map_err(mlua::Error::external)?,
        );
    }
    // On uses beat zero; Update(0) uses the song position at elapsed zero,
    // including an opening warp. Its callbacks run, but no queue advances.
    crate::set_compile_song_runtime_values(
        lua,
        crate::song_beat_at_elapsed_seconds(0.0, context),
        0.0,
    )?;
    lua.set_app_data(SongLuaZeroUpdate);
    crate::song_tables::advance_option_levels(lua, 0.0)?;
    let result = run_actor_update_functions_with_delta(lua, &root, 0.0);
    lua.remove_app_data::<SongLuaZeroUpdate>();
    result?;
    let mut updates = capture_startup_states(states).map_err(mlua::Error::external)?;
    for (pointer, update) in &mut updates {
        let prefix = prefixes.get(pointer).map_or(0, |blocks| {
            if update.blocks.starts_with(blocks) {
                blocks.len()
            } else {
                0
            }
        });
        update.blocks.drain(..prefix);
        update.block_steps.drain(..prefix);
    }
    Ok(updates)
}

pub(crate) fn run_actor_startup_commands(
    lua: &Lua,
    root: &Value,
    mut initial_states: HashMap<usize, (Table, SongLuaOverlayState)>,
    context: &SongLuaCompileContext,
) -> mlua::Result<SongLuaStartupCapture> {
    let Value::Table(root) = root else {
        return Ok(SongLuaStartupCapture::default());
    };
    if lua.app_data_ref::<SongLuaStartupQueues>().is_none() {
        lua.set_app_data(SongLuaStartupQueues(Vec::new()));
    }
    // Screen::BeginScreen propagates Begin before Foreground::Update sends On.
    // This root is the session loader list, so dispatch to its actor children.
    for child in actor_direct_children(lua, root)? {
        // Native Background keeps its cached actors outside the screen's
        // propagated command children. Foreground actors receive Begin.
        if child.raw_get::<Option<f32>>("__songlua_background_start_beat")?.is_some() {
            continue;
        }
        run_actor_named_command(lua, &child, "BeginCommand")?;
        run_named_command_on_children_recursively(lua, &child, "BeginCommand", None)?;
    }
    if let Some(screen) = lua.globals().raw_get::<Option<Table>>("__songlua_top_screen")?
        && let Some(transition) = actor_children(lua, &screen)?.raw_get::<Option<Table>>("In")?
    {
        start_actor_transition(lua, &transition)?;
    }
    // LoadDelayedScreen broadcasts after activation, before Update(0) sends On.
    crate::record_song_lua_broadcast(lua, "ScreenChanged", false)?;
    broadcast_song_lua_message(lua, "ScreenChanged", None)?;
    run_actor_startup_commands_for_table(lua, root)?;
    // Wrappers created by Init/On begin at the default ActorFrame state.
    collect_wrapper_initials(root, &mut initial_states)?;
    // Transition settings belong to Init/On, before Update(0) can change them.
    let options = read_startup(lua, context)?;
    let initial_updates = capture_initial_update(lua, root, context)?;
    // ScreenManager consumes requests after the real zero-delta update.
    // Queue discovery below may execute future commands; those requests are
    // observations of future frames and must not trigger a startup reload.
    let screen_request = lua.remove_app_data::<crate::host::SongLuaScreenRequest>();
    // The chronological replay starts at the first positive frame, so retain
    // real Init/On and Update(0), then exclude queued discovery probes below.
    #[cfg(feature = "test-support")]
    if let Some(mut capture) = lua.app_data_mut::<crate::song_tables::SongLuaSkinWrites>() {
        capture.active = false;
    }
    for (actor, _) in initial_states.values() {
        if actor_type_is(actor, "BitmapText")? || actor_type_is(actor, "RollingNumbers")? {
            actor.raw_set("__songlua_startup_text", actor.raw_get::<Value>("Text")?)?;
        }
    }
    let broadcasts = take_runtime_broadcasts(lua);
    collect_wrapper_initials(root, &mut initial_states)?;
    let queued = lua
        .remove_app_data::<SongLuaStartupQueues>()
        .expect("startup queue scope was installed above");
    if context.live {
        if let Some(request) = screen_request {
            lua.set_app_data(request);
        }
        // ActorTweenReplays already owns the native Init/On queue. No future
        // command discovery, closure rewinding or timeline baking is allowed.
        return Ok(SongLuaStartupCapture {
            updates: initial_updates, options, broadcasts,
            ..SongLuaStartupCapture::default()
        });
    }
    if let Some(request) = screen_request {
        // This screen is being deleted now. Its future queue must neither run
        // nor mutate globals that the replacement screen will inherit.
        lua.set_app_data(request);
        return Ok(SongLuaStartupCapture {
            updates: initial_updates,
            options,
            broadcasts,
            ..SongLuaStartupCapture::default()
        });
    }
    // Native queued commands do not execute on the zero-delta startup frame.
    // Discovery probes must leave shared Lua locals and globals at that frame,
    // otherwise chronological replay begins with counters already advanced.
    let locals = if queued.0.is_empty() {
        None
    } else {
        let functions = lua.create_table()?;
        let mut function_count = 0;
        let mut seen = HashSet::new();
        for (actor, _) in initial_states.values() {
            actor.for_each::<Value, Value>(|_, value| {
                if let Value::Function(function) = value
                    && function.environment().is_some()
                    && seen.insert(function.to_pointer() as usize)
                {
                    function_count += 1;
                    functions.raw_set(function_count, function)?;
                }
                Ok(())
            })?;
        }
        // The queued probes share one pre-probe Lua state. Capture each
        // global, local table and closure once across all command roots.
        Some(snapshot_function_action_tables(lua, &functions)?)
    };
    // Keep Init/On tweens before queued-command capture resets the song tree.
    let startup_tweens = capture_startup_states(initial_states).map_err(mlua::Error::external)?;
    let mut states = HashMap::new();
    if !queued.0.is_empty() {
        // Actor::UpdateTweening requires positive delta time. Keep the state
        // after On but before queued commands for the compiled beat-zero frame.
        let mut initial_states = HashMap::new();
        collect_startup_states(lua, root, &mut initial_states)?;
        lua.set_app_data(SongLuaQueuedStartup(0.0));
        let result = queued
            .0
            .into_iter()
            .try_for_each(|actor| drain_actor_command_queue(lua, &actor));
        lua.remove_app_data::<SongLuaQueuedStartup>();
        result?;
        for (pointer, (actor, initial)) in initial_states {
            flush_actor_capture(&actor)?;
            let blocks = read_actor_capture_blocks(&actor).map_err(mlua::Error::external)?;
            // Discovery can write otherwise untouched siblings. Keep their
            // real frame-zero state; startup_command filters unchanged actors.
            states.insert(
                pointer,
                SongLuaStartupState {
                    initial,
                    blocks,
                    steps: Vec::new(),
                    commands: Vec::new(),
                    block_steps: Vec::new(),
                    clock_path: startup_clock_path(&actor)?,
                },
            );
        }
    }
    // Keep the discovered command shape until the compiler has collected its
    // declarations. Rewind script state only when chronological replay starts.
    lua.set_app_data(SongLuaStartupLocals(locals));
    lua.remove_app_data::<crate::host::SongLuaScreenRequest>();
    Ok(SongLuaStartupCapture {
        queued: states,
        tweens: startup_tweens,
        updates: initial_updates,
        options,
        broadcasts,
    })
}

struct SongLuaStartupLocals(Option<FunctionActionSnapshot>);

pub(crate) fn end_actor_scene(lua: &Lua, root: &Table) -> mlua::Result<()> {
    lua.globals().raw_set("__songlua_top_screen", Value::Nil)?;
    // EndScreen dispatches commands without advancing the outgoing actors'
    // tween queues. Drop their queues after End, including queues it creates.
    lua.set_app_data(SongLuaStartupQueues(Vec::new()));
    let result = run_named_command_on_children_recursively(lua, root, "EndCommand", None);
    lua.remove_app_data::<SongLuaStartupQueues>();
    result
}

pub(crate) fn restore_startup_locals(lua: &Lua) -> mlua::Result<()> {
    if let Some(snapshot) = lua
        .remove_app_data::<SongLuaStartupLocals>()
        .and_then(|locals| locals.0)
    {
        restore_function_action_tables(lua, snapshot)?;
    }
    Ok(())
}

pub fn run_actor_update_functions(lua: &Lua, root: &Value) -> mlua::Result<()> {
    run_actor_update_functions_with_delta(lua, root, 1.0_f64 / 60.0)
}

pub fn run_actor_update_functions_with_delta(
    lua: &Lua,
    root: &Value,
    delta_seconds: f64,
) -> mlua::Result<()> {
    let Value::Table(root) = root else {
        return Ok(());
    };
    run_actor_update_functions_for_table(lua, root, delta_seconds)
}

#[derive(Clone, Copy)]
enum SongLuaCompileParentDelta {
    Children,
    Wrappers,
    NoteColumns,
}

#[derive(Clone)]
enum SongLuaCompileUpdateJob {
    Transition { actor: Table },
    PlayerFeedback {
        actor: Table,
    },
    Enter {
        actor: Table,
        parent: Option<Table>,
        delta_source: SongLuaCompileParentDelta,
    },
    Advance {
        actor: Table,
        order: usize,
        recurring: bool,
    },
    Recurring {
        actor: Table,
        order: usize,
    },
    Callback {
        actor: Table,
        order: usize,
    },
    Texture {
        actor: Table,
    },
}

#[derive(Clone, Default)]
struct SongLuaCompileUpdatePlan {
    jobs: Arc<[SongLuaCompileUpdateJob]>,
    slots: Arc<[CompileUpdateSlots]>,
    actor_count: usize,
}

#[derive(Clone, Copy)]
struct CompileUpdateSlots {
    actor: usize,
    parent: Option<usize>,
}

impl SongLuaCompileUpdatePlan {
    fn new(jobs: Vec<SongLuaCompileUpdateJob>) -> Self {
        let mut actors = FxHashMap::default();
        let slots = jobs.iter().map(|job| {
            let actor = match job {
                SongLuaCompileUpdateJob::Enter { actor, .. }
                | SongLuaCompileUpdateJob::Advance { actor, .. }
                | SongLuaCompileUpdateJob::Recurring { actor, .. }
                | SongLuaCompileUpdateJob::Callback { actor, .. }
                | SongLuaCompileUpdateJob::Texture { actor }
                | SongLuaCompileUpdateJob::PlayerFeedback { actor }
                | SongLuaCompileUpdateJob::Transition { actor } => actor,
            };
            // Aliases and repeated visits retain the same delta slot, just as
            // the pointer-keyed map did. Only the immutable plan resolves IDs.
            let next = actors.len();
            let actor = *actors.entry(actor.to_pointer() as usize).or_insert(next);
            let parent = if let SongLuaCompileUpdateJob::Enter { parent: Some(parent), .. } = job {
                Some(*actors.get(&(parent.to_pointer() as usize)).expect("parent entered before child"))
            } else { None };
            CompileUpdateSlots { actor, parent }
        }).collect::<Vec<_>>();
        Self { jobs: jobs.into(), slots: slots.into(), actor_count: actors.len() }
    }

    fn iter(&self) -> std::slice::Iter<'_, SongLuaCompileUpdateJob> {
        self.jobs.iter()
    }
}

pub(crate) fn invalidate_compile_update_plan(lua: &Lua) {
    lua.remove_app_data::<SongLuaCompileUpdatePlan>();
}

fn collect_compile_update_jobs(
    lua: &Lua,
    actor: &Table,
    parent: Option<&Table>,
    delta_source: SongLuaCompileParentDelta,
    jobs: &mut Vec<SongLuaCompileUpdateJob>,
    order: &mut usize,
) -> mlua::Result<()> {
    let canonical = lua.app_data_mut::<CanonicalLiveActorHandles>()
        .map(|mut cache| cache.0.entry(actor.to_pointer() as usize)
            .or_insert_with(|| actor.clone()).clone());
    let actor = canonical.as_ref().unwrap_or(actor);

    collect_compile_update_self_jobs(lua, actor, parent, delta_source, jobs, order)?;
    for child in actor.sequence_values::<Value>() {
        let Value::Table(child) = child? else {
            continue;
        };
        adopt_actor_child(actor, &child)?;
        collect_compile_update_jobs(lua, &child, Some(actor), SongLuaCompileParentDelta::Children, jobs, order)?;
    }
    if let Some(stream) = song_meter_stream_child(lua, actor)? {
        collect_compile_update_jobs(lua, &stream, Some(actor), SongLuaCompileParentDelta::Children, jobs, order)?;
    }
    if crate::model_texture::contains(lua, actor) {
        jobs.push(SongLuaCompileUpdateJob::Texture { actor: actor.clone() });
    }
    // ActorFrame checks its current update function after its children. On
    // (or an earlier actor) can install one after this frame's plan is built.
    // Retain that slot even when the function is initially absent.
    jobs.push(SongLuaCompileUpdateJob::Callback {
        actor: actor.clone(),
        order: *order,
    });
    // NoteField::Update runs its column vector after ActorFrame::Update.
    // These Actor owners receive the original field delta, outside that
    // ActorFrame's update rate and hibernation gate, and are not its children.
    if let Some(columns) = actor.raw_get::<Option<Table>>("__songlua_note_columns")? {
        for column in columns.sequence_values::<Table>() {
            collect_compile_update_jobs(lua, &column?, Some(actor),
                SongLuaCompileParentDelta::NoteColumns, jobs, order)?;
        }
    }
    Ok(())
}

fn collect_compile_update_self_jobs(
    lua: &Lua,
    actor: &Table,
    parent: Option<&Table>,
    delta_source: SongLuaCompileParentDelta,
    jobs: &mut Vec<SongLuaCompileUpdateJob>,
    order: &mut usize,
) -> mlua::Result<()> {
    // Actor::Update checks hibernation before its wrappers. Retain that entry
    // phase separately from UpdateInternal so paused owners block the tree.
    jobs.push(SongLuaCompileUpdateJob::Enter {
        actor: actor.clone(),
        parent: parent.cloned(),
        delta_source,
    });
    // Actor::Update advances wrappers before the wrapped actor. A wrapper
    // receives its owner's delta, before ActorFrame's child update rate.
    if let Some(wrappers) = actor.get::<Option<Table>>("__songlua_wrappers")? {
        for wrapper in wrappers.sequence_values::<Table>() {
            collect_compile_update_jobs(lua, &wrapper?, Some(actor), SongLuaCompileParentDelta::Wrappers, jobs, order)?;
        }
    }
    *order += 1;
    if lua.app_data_ref::<SongLuaCompileUpdateOrders>().is_none() {
        lua.set_app_data(SongLuaCompileUpdateOrders(FxHashMap::default()));
    }
    lua.app_data_mut::<SongLuaCompileUpdateOrders>()
        .expect("orders installed")
        .0
        .insert(actor.to_pointer() as usize, *order);
    let recurring = actor
        .get::<Option<String>>("__songlua_recurring_update_command")?
        .is_some();
    jobs.push(SongLuaCompileUpdateJob::Advance {
        actor: actor.clone(),
        order: *order,
        recurring,
    });
    if recurring {
        jobs.push(SongLuaCompileUpdateJob::Recurring {
            actor: actor.clone(),
            order: *order,
        });
    }
    if actor.raw_get::<Option<u8>>("__songlua_transition_state")?.is_some() {
        jobs.push(SongLuaCompileUpdateJob::Transition { actor: actor.clone() });
    }
    Ok(())
}

fn compile_update_jobs(lua: &Lua, root: &Table) -> mlua::Result<SongLuaCompileUpdatePlan> {
    if lua.app_data_ref::<SongLuaCompileUpdatePlan>().is_none() {
        let mut jobs = Vec::new();
        let mut order = 0;
        let mut after_song = Vec::new();
        // ScreenWithMenuElements owns these actors outside the authored song
        // roots. Their own tween queues still advance every screen Update.
        // GetChild returns this same owner; adding a later tween must not keep
        // an earlier completed tween in its queue.
        if let Some(screen) = lua.globals().raw_get::<Option<Table>>("__songlua_top_screen")? {
            // Screen's own Actor queue precedes its children. The song and
            // theme owners are visited separately below, each exactly once.
            collect_compile_update_self_jobs(lua, &screen, None,
                SongLuaCompileParentDelta::Children, &mut jobs, &mut order)?;
            let children = actor_children(lua, &screen)?;
            // SongBackground's entries are already visited through the song
            // root below; traversing its child aliases would update them twice.
            for name in ["Underlay", "Overlay", "Debug"] {
                if let Some(actor) = children.raw_get::<Option<Table>>(name)? {
                    collect_compile_update_jobs(lua, &actor, None, SongLuaCompileParentDelta::Children, &mut jobs, &mut order)?;
                }
            }
            // ScreenGameplay sorts SongForeground at OVERLAY + 1 and the
            // transitions at TRANSITIONS. Foreground On
            // and Update callbacks therefore see their preceding-frame queues.
            for name in ["In", "Out", "Cancel"] {
                if let Some(actor) = children.raw_get::<Option<Table>>(name)? {
                    after_song.push(actor);
                }
            }
            for actor in screen.sequence_values::<Table>() {
                let actor = actor?;
                if actor.raw_get::<Option<String>>("__songlua_screen_actor_role")?.is_some() {
                    collect_compile_update_jobs(lua, &actor, None, SongLuaCompileParentDelta::Children, &mut jobs, &mut order)?;
                }
            }
        }
        for key in ["__songlua_top_screen_player_1", "__songlua_top_screen_player_2"] {
            // Player actors advance native time even without NoteField wrappers.
            if let Some(player) = lua.globals().raw_get::<Option<Table>>(key)? {
                collect_compile_update_jobs(lua, &player, None, SongLuaCompileParentDelta::Children, &mut jobs, &mut order)?;
                // Player::Update runs ActorFrame::Update first, then emits
                // Judgment while processing rows. Song callbacks follow it.
                if crate::live::active(lua) {
                    jobs.push(SongLuaCompileUpdateJob::PlayerFeedback { actor: player });
                }
            }
        }
        collect_compile_update_jobs(lua, root, None, SongLuaCompileParentDelta::Children, &mut jobs, &mut order)?;
        for actor in after_song {
            collect_compile_update_jobs(lua, &actor, None, SongLuaCompileParentDelta::Children, &mut jobs, &mut order)?;
        }
        if let Some(screen) = lua.globals().raw_get::<Option<Table>>("__songlua_top_screen")? {
            jobs.push(SongLuaCompileUpdateJob::Callback { actor: screen, order });
        }
        lua.set_app_data(SongLuaCompileUpdatePlan::new(jobs));
    }
    Ok(lua
        .app_data_ref::<SongLuaCompileUpdatePlan>()
        .map(|plan| (*plan).clone())
        .unwrap_or_default())
}

// mlua Table clones share one auxiliary stack reference. Keep one retained
// handle per actor across the live registry and update jobs, instead of reading
// the same Lua object into a new auxiliary reference at every tree traversal.
struct CanonicalLiveActorHandles(FxHashMap<usize, Table>);

fn canonical_live_actor_handle(lua: &Lua, actor: Table) -> Table {
    lua.app_data_ref::<CanonicalLiveActorHandles>()
        .and_then(|cache| cache.0.get(&(actor.to_pointer() as usize)).cloned())
        .unwrap_or(actor)
}

pub(crate) fn prepare_live_actor_handle_cache(
    lua: &Lua,
    actors: impl IntoIterator<Item = Table>,
) -> mlua::Result<Vec<Table>> {
    let mut canonical = actors.into_iter()
        .map(|actor| (actor.to_pointer() as usize, actor))
        .collect::<FxHashMap<_, _>>();
    let mut registry = Vec::new();
    for actor in song_lua_actor_registry(lua)?.sequence_values::<Table>() {
        let actor = actor?;
        registry.push(canonical.entry(actor.to_pointer() as usize).or_insert(actor).clone());
    }
    lua.set_app_data(CanonicalLiveActorHandles(canonical));
    Ok(registry)
}

pub(crate) fn prepare_live_updates(lua: &Lua, root: &Value) -> mlua::Result<()> {
    if let Value::Table(root) = root {
        compile_update_jobs(lua, root)?;
    }
    Ok(())
}

pub(crate) fn actor_tree_update_only(lua: &Lua, root: &Value, name: &str) -> mlua::Result<bool> {
    let Value::Table(root) = root else {
        return Ok(false);
    };
    let jobs = compile_update_jobs(lua, root)?;
    for job in jobs.iter() {
        if let SongLuaCompileUpdateJob::Advance { actor, .. } = job
            && (actor
                .raw_get::<Option<f32>>("__songlua_hibernate_seconds")?
                .is_some_and(|left| left > 0.0)
                || actor
                    .raw_get::<Option<bool>>("__songlua_command_queue_deferred")?
                    .unwrap_or(false))
        {
            return Ok(false);
        }
    }
    let mut active_jobs = Vec::new();
    for job in jobs.iter() {
        if matches!(
            job,
            SongLuaCompileUpdateJob::Advance { .. } | SongLuaCompileUpdateJob::Enter { .. }
                | SongLuaCompileUpdateJob::Texture { .. } | SongLuaCompileUpdateJob::PlayerFeedback { .. }
                | SongLuaCompileUpdateJob::Transition { .. }
        ) { continue; }
        if let SongLuaCompileUpdateJob::Callback { actor, .. } = job
            && actor.get::<Option<Function>>("__songlua_update_function")?.is_none()
            && actor.get::<Option<Function>>("__songlua_update_previous")?.is_none() {
            continue;
        }
        active_jobs.push(job);
    }
    let mut jobs = active_jobs.into_iter();
    match (jobs.next(), jobs.next()) {
        (None, None) => Ok(true),
        (Some(SongLuaCompileUpdateJob::Callback { actor, .. }), None) => {
            Ok(actor.get::<Option<String>>("Name")?.as_deref() == Some(name))
        }
        _ => Ok(false),
    }
}

// A frame's queued commands run before its children advance. Callbacks run
// after the children. The phase exists only during load-time replay.
// Spin changes the current rotation, while rotation getters expose the back
// tween destination. Keep that small rotation queue inside semantic actor state
// so speculative captures restore it along with the actor. It exists only at
// song load; gameplay consumes baked rotation tracks without Lua allocations.
fn queue_spin_tween(
    lua: &Lua,
    actor: &Table,
    duration: f64,
    easing: Option<&str>,
) -> mlua::Result<()> {
    let Some(pose) = actor.raw_get::<Option<Table>>("__songlua_state_spin")? else {
        return Ok(());
    };
    let queue: Table = pose.get("queue")?;
    let tail: usize = pose.get("tail")?;
    let source: Table = if tail == 0 {
        pose.get("rotation")?
    } else {
        queue.raw_get::<Table>(tail)?.get("to")?
    };
    let tween = lua.create_table()?;
    tween.set("to", clone_lua_value(lua, Value::Table(source))?)?;
    let duration = (duration as f32).max(0.0);
    tween.set("duration", duration)?;
    tween.set("left", duration)?;
    tween.set("easing", easing)?;
    queue.raw_set(tail + 1, tween)?;
    pose.set("tail", tail + 1)
}

fn clear_spin_queue(actor: &Table, finish: bool) -> mlua::Result<()> {
    let Some(pose) = actor.raw_get::<Option<Table>>("__songlua_state_spin")? else {
        return Ok(());
    };
    let queue: Table = pose.get("queue")?;
    let tail: usize = pose.get("tail")?;
    if finish && tail > 0 {
        pose.set(
            "rotation",
            queue.raw_get::<Table>(tail)?.get::<Table>("to")?,
        )?;
    }
    let rotation: Table = pose.get("rotation")?;
    for (index, key) in ["rot_x_deg", "rot_y_deg", "rot_z_deg"].iter().enumerate() {
        actor.raw_set(
            actor_capture_state_key(key).as_ref(),
            rotation.raw_get::<f32>(index + 1)?,
        )?;
    }
    queue.clear()?;
    pose.set("head", 1usize)?;
    pose.set("tail", 0usize)
}

fn advance_motion_clock(lua: &Lua, actor: &Table, delta_seconds: f64) -> mlua::Result<()> {
    // Native Actor advances its timer before any effect has been selected.
    // Retain that history so a later pulse keeps the actor's existing phase.
    let clock =
        if let Some(clock) = actor.raw_get::<Option<Table>>("__songlua_state_motion_clock")? {
            clock
        } else {
            let clock = lua.create_table()?;
            actor.raw_set("__songlua_state_motion_clock", clock.clone())?;
            clock
        };
    let previous = clock.get::<Option<f32>>("units")?.unwrap_or(0.0);
    let timer = actor
        .raw_get::<Option<bool>>("__songlua_state_effect_timer")?
        .unwrap_or(true);
    let (units, delta) = if timer {
        let delta = delta_seconds as f32;
        let period = actor
            .raw_get::<Option<f32>>("__songlua_state_effect_period")?
            .unwrap_or(1.0);
        let mut units = previous + delta;
        if units > period {
            units -= period;
        }
        (units, delta)
    } else {
        let units = if actor
            .raw_get::<Option<String>>("__songlua_state_effect_clock")?
            .as_deref()
            == Some("beat")
        {
            compile_song_runtime_values(lua)?.0
        } else {
            let runtime = lua.globals().get::<Table>(crate::SONG_LUA_RUNTIME_KEY)?;
            f32::from_lua(crate::runtime::song_music_seconds(lua, &runtime)?, lua)?
        };
        (units, units - previous)
    };
    let units = clock.get::<Option<f32>>("pending_units")?.unwrap_or(units);
    clock.set("pending_units", Value::Nil)?;
    clock.set("units", units)?;
    clock.set("delta", delta)
}

#[cfg(test)]
mod actor_clock_tests {
    use super::*;

    #[test]
    fn effect_clock_timer_preserves_itgmania_single_subtraction_and_getters() {
        for native in [false, true] {
            let lua = Lua::new();
            if native { lua.set_app_data(LiveActorPlayback); }
            let actor = lua.create_table().unwrap();
            install_actor_effect_time_getter_methods(&lua, &actor).unwrap();
            let units = actor.get::<Function>("GetSecsIntoEffect").unwrap();
            let delta = actor.get::<Function>("GetEffectDelta").unwrap();
            // Actor.cpp uses >, not >=, and subtracts the period only once.
            for (elapsed, expected) in [(0.5, 0.5), (0.5, 1.0), (0.0, 1.0), (2.5, 2.5), (-0.5, 1.0)] {
                advance_motion_clock(&lua, &actor, elapsed).unwrap();
                assert_eq!(units.call::<f32>(()).unwrap(), expected);
                assert_eq!(delta.call::<f32>(()).unwrap(), elapsed as f32);
            }
            actor.raw_set("__songlua_state_effect_period", 0.0).unwrap();
            advance_motion_clock(&lua, &actor, 2.0).unwrap();
            assert_eq!(units.call::<f32>(()).unwrap(), 3.0);
            actor.raw_set("__songlua_state_effect_period", -1.0).unwrap();
            advance_motion_clock(&lua, &actor, 0.0).unwrap();
            assert_eq!(units.call::<f32>(()).unwrap(), 4.0);
        }
    }
}

pub(crate) fn effect_render_time(
    actor: &Table,
    frame_clock: [f32; 2],
) -> mlua::Result<Option<[f32; 2]>> {
    if !actor
        .raw_get::<Option<bool>>("__songlua_state_rainbow")?
        .unwrap_or(false)
        && !matches!(
            actor
                .raw_get::<Option<String>>("__songlua_state_effect_mode")?
                .as_deref(),
            Some(
                "bob"
                    | "bounce"
                    | "wag"
                    | "pulse"
                    | "diffuseblink"
                    | "diffuseshift"
                    | "diffuseramp"
                    | "glowblink"
                    | "glowshift"
                    | "glowramp"
            )
        )
    {
        return Ok(None);
    }
    let Some(clock) = actor.raw_get::<Option<Table>>("__songlua_state_motion_clock")? else {
        return Ok(None);
    };
    let units = if actor
        .raw_get::<Option<String>>("__songlua_state_effect_clock")?
        .as_deref()
        == Some("beat")
    {
        frame_clock[1]
    } else {
        frame_clock[0]
    };
    Ok(Some([
        units,
        clock.get::<Option<f32>>("units")?.unwrap_or(0.0),
    ]))
}

pub(crate) fn sync_motion_restart(actor: &Table, epoch: f32, seconds: f32) -> mlua::Result<()> {
    let Some(clock) = actor.raw_get::<Option<Table>>("__songlua_state_motion_clock")? else {
        return Ok(());
    };
    if clock.get::<Option<f32>>("replay_epoch")? != Some(epoch) {
        clock.set("replay_epoch", epoch)?;
        // Queued commands run after Actor::UpdateInternal has advanced time.
        clock.set("pending_units", (seconds - epoch).max(0.0))?;
    }
    Ok(())
}

// Mirrors Actor::UpdateInternal: advance spin first, then interpolate the
// complete tween rotation even if the authored tween only changes alpha.
fn advance_spin_pose(lua: &Lua, actor: &Table, delta_seconds: f64) -> mlua::Result<()> {
    let Some(pose) = actor.raw_get::<Option<Table>>("__songlua_state_spin")? else {
        return Ok(());
    };
    let rotation: Table = pose.get("rotation")?;
    // Actor::UpdateInternal uses the same effect delta for every effect.
    // Reuse the already advanced clock, including music origins and resets.
    let clock = actor.raw_get::<Table>("__songlua_state_motion_clock")?;
    let elapsed = clock.get::<f32>("delta")?;
    if actor
        .raw_get::<Option<String>>("__songlua_state_effect_mode")?
        .as_deref()
        == Some("spin")
    {
        let magnitude = actor_effect_magnitude(actor)?;
        for (index, magnitude) in magnitude.into_iter().enumerate() {
            let value = rotation.raw_get::<f32>(index + 1)?;
            rotation.raw_set(index + 1, (value + elapsed * magnitude).rem_euclid(360.0))?;
        }
    }
    let queue: Table = pose.get("queue")?;
    let tail: usize = pose.get("tail")?;
    let mut head: usize = pose.get("head")?;
    let mut remaining = (delta_seconds as f32).max(0.0);
    while head <= tail && remaining > 0.0 {
        let tween: Table = queue.raw_get(head)?;
        let source = if let Some(source) = tween.get::<Option<Table>>("from")? {
            source
        } else {
            let source = clone_lua_value(lua, Value::Table(rotation.clone()))?;
            tween.set("from", source.clone())?;
            let Value::Table(source) = source else {
                unreachable!("cloned rotation is a table")
            };
            source
        };
        let left: f32 = tween.get("left")?;
        let consumed = left.min(remaining);
        let left = left - consumed;
        remaining -= consumed;
        tween.set("left", left)?;
        let dest: Table = tween.get("to")?;
        if left == 0.0 {
            for index in 1..=3 {
                rotation.raw_set(index, dest.raw_get::<f32>(index)?)?;
            }
            queue.raw_set(head, Value::Nil)?;
            head += 1;
        } else {
            let duration: f32 = tween.get("duration")?;
            let percent = 1.0 - left / duration;
            let easing = tween.get::<Option<String>>("easing")?;
            let percent =
                deadsync_gameplay::song_lua_ease_factor(easing.as_deref(), percent, None, None);
            for index in 1..=3 {
                let from = source.raw_get::<f32>(index)?;
                let to = dest.raw_get::<f32>(index)?;
                rotation.raw_set(index, (1.0 - percent) * from + percent * to)?;
            }
        }
    }
    if head > tail {
        pose.set("head", 1usize)?;
        pose.set("tail", 0usize)?;
        for (index, key) in ["rot_x_deg", "rot_y_deg", "rot_z_deg"].iter().enumerate() {
            actor.raw_set(
                actor_capture_state_key(key).as_ref(),
                rotation.raw_get::<f32>(index + 1)?,
            )?;
        }
    } else {
        pose.set("head", head)?;
    }
    pose.set("baked", true)
}

pub(crate) fn spin_render_pose(actor: &Table) -> mlua::Result<Option<[f32; 3]>> {
    let Some(pose) = actor.raw_get::<Option<Table>>("__songlua_state_spin")? else {
        return Ok(None);
    };
    if !pose.get::<Option<bool>>("baked")?.unwrap_or(false) {
        return Ok(None);
    }
    let rotation: Table = pose.get("rotation")?;
    Ok(Some([
        rotation.raw_get(1)?,
        rotation.raw_get(2)?,
        rotation.raw_get(3)?,
    ]))
}

struct SongLuaCompileUpdateOrders(FxHashMap<usize, usize>);

#[derive(Clone, Copy)]
struct SongLuaCompileUpdatePhase {
    active: bool,
    order: usize,
    delta_seconds: f64,
}

fn record_message_advance(lua: &Lua, actor: &Table) {
    let advance = actor_frame_advance(lua, actor);
    if let Some(mut capture) = lua.app_data_mut::<SongLuaOverlayUpdateCapture>() {
        if let Some(index) = capture
            .actor_indices
            .get(&(actor.to_pointer() as usize))
            .copied()
        {
            if let Some(message) = capture.active_broadcast.as_deref() {
                if !capture
                    .message_advances
                    .iter()
                    .any(|((actor_index, name), value)| {
                        *actor_index == index && name == message && *value == advance
                    })
                {
                    let message = message.to_owned();
                    capture.message_advances.insert((index, message), advance);
                }
            }
        }
    }
}

fn actor_frame_advance(lua: &Lua, actor: &Table) -> f32 {
    let Some(phase) = lua.app_data_ref::<SongLuaCompileUpdatePhase>() else {
        return 0.0;
    };
    if !phase.active {
        return 0.0;
    }
    let order = lua
        .app_data_ref::<SongLuaCompileUpdateOrders>()
        .and_then(|orders| orders.0.get(&(actor.to_pointer() as usize)).copied())
        .unwrap_or(0);
    if order > phase.order {
        phase.delta_seconds as f32
    } else {
        0.0
    }
}

pub fn run_actor_compile_update_functions_with_delta(
    lua: &Lua,
    root: &Value,
    delta_seconds: f64,
) -> mlua::Result<()> {
    run_actor_compile_update_functions_with_entry(lua, root, delta_seconds, |_| Ok(()))
}

// Update traversal scratch belongs to the executor, not the authored actor.
// Keep it in Rust so dense scenes do not shuttle six private Lua fields through
// the VM for every Enter/Advance/Callback job. Reuse capacity between frames;
// no app-data borrow survives a Lua callback or plan invalidation.
#[derive(Clone, Copy, Default)]
struct ActorCompileUpdateDeltas {
    input: (Option<f64>, Option<f32>),
    wrappers: (Option<f64>, Option<f32>),
    children: (Option<f64>, Option<f32>),
}

#[derive(Default)]
struct CompileUpdateDeltaScratch(Vec<ActorCompileUpdateDeltas>);

pub(crate) fn run_actor_compile_update_functions_with_entry(
    lua: &Lua,
    root: &Value,
    delta_seconds: f64,
    mut before_enter: impl FnMut(&Table) -> mlua::Result<()>,
) -> mlua::Result<()> {
    let Value::Table(root) = root else {
        return Ok(());
    };
    // Retain the immutable plan without borrowing app data across callbacks:
    // callbacks can invalidate the plan while this frame finishes its jobs.
    let jobs = compile_update_jobs(lua, root)?;
    let mut deltas = lua.remove_app_data::<CompileUpdateDeltaScratch>().unwrap_or_default().0;
    deltas.resize(jobs.actor_count, ActorCompileUpdateDeltas::default());
    deltas.fill(ActorCompileUpdateDeltas::default());
    if lua.app_data_ref::<SongLuaCompileUpdatePhase>().is_none() {
        lua.set_app_data(SongLuaCompileUpdatePhase {
            active: false,
            order: 0,
            delta_seconds: 0.0,
        });
    }
    let prior_phase = *lua
        .app_data_ref::<SongLuaCompileUpdatePhase>()
        .expect("phase installed");
    let result = (|| {
        for (job, slots) in jobs.iter().zip(jobs.slots.iter()) {
            if let SongLuaCompileUpdateJob::Transition { actor } = job {
                // Transition::UpdateInternal gates its child, after the base
                // Actor has advanced its own queue. Completion is checked
                // before the child's last Update, as in Transition.cpp.
                let current = &mut deltas[slots.actor];
                if current.children.0.is_some() {
                    if actor.raw_get::<u8>("__songlua_transition_state")? != 1 {
                        current.children.0 = None;
                    } else if let Some(child) = actor.raw_get::<Option<Table>>("__songlua_transition_child")?
                        && actor_tween_time_left(lua, &child)? == 0.0
                    {
                        actor.raw_set("__songlua_transition_state", 2)?;
                    }
                }
                continue;
            }
            if let SongLuaCompileUpdateJob::PlayerFeedback { actor } = job {
                crate::live::update_player_holds(lua, actor)?;
                crate::live::update_player_judgment(lua, actor)?;
                crate::live::update_player_note_field_zoom(lua, actor)?;
                continue;
            }
            if let SongLuaCompileUpdateJob::Texture { actor } = job {
                crate::model_texture::update(lua, actor);
                continue;
            }
            let (actor, order) = match job {
                SongLuaCompileUpdateJob::Enter {
                    actor,
                    parent: _,
                    delta_source,
                } => {
                    before_enter(actor)?;
                    let (mut incoming, mut span) = match slots.parent {
                        Some(parent) => {
                            let parent = &deltas[parent];
                            match delta_source {
                                SongLuaCompileParentDelta::Children => parent.children,
                                SongLuaCompileParentDelta::Wrappers => parent.wrappers,
                                SongLuaCompileParentDelta::NoteColumns => parent.input,
                            }
                        }
                        None => (Some(delta_seconds), Some(delta_seconds as f32)),
                    };
                    // A scheduled foreground's whole subtree uses its native
                    // music delta. Dormant/finished layers receive no updates.
                    if actor.raw_get::<Option<bool>>("__songlua_live_inactive")?.unwrap_or(false) {
                        incoming = None;
                        span = None;
                    } else if let Some(delta) = actor.raw_get::<Option<f64>>("__songlua_live_delta")? {
                        incoming = Some(delta);
                        span = Some(delta as f32);
                    }
                    let input = (incoming, span);
                    // Model::Update passes the original delta to its materials
                    // after Actor::Update, even if that Actor is hibernating.
                    crate::model_texture::begin(lua, actor, incoming);
                    let delta = incoming
                        .map(|delta| advance_hibernation(lua, actor, delta))
                        .transpose()?
                        .flatten();
                    let span = match (incoming, delta, span) {
                        (Some(incoming), Some(delta), Some(span)) if incoming > 0.0 => {
                            Some(span * (delta as f32 / incoming as f32))
                        }
                        (_, Some(_), Some(_)) => Some(0.0),
                        _ => None,
                    };
                    deltas[slots.actor] = ActorCompileUpdateDeltas {
                        input, wrappers: (delta, span), children: (None, None),
                    };
                    continue;
                }
                SongLuaCompileUpdateJob::Advance { actor, order, .. }
                | SongLuaCompileUpdateJob::Recurring { actor, order, .. }
                | SongLuaCompileUpdateJob::Callback { actor, order, .. } => (actor, *order),
                SongLuaCompileUpdateJob::Texture { .. } | SongLuaCompileUpdateJob::PlayerFeedback { .. }
                | SongLuaCompileUpdateJob::Transition { .. } => unreachable!("host jobs handled above"),
            };
            let current = &mut deltas[slots.actor];
            if matches!(job, SongLuaCompileUpdateJob::Advance { .. }) {
                // Wrappers may have changed the owner's rate since Enter.
                let rate = actor_update_rate(actor)? as f32;
                current.children = (current.wrappers.0.map(|delta| f64::from(delta as f32 * rate)),
                    current.wrappers.1);
            }
            let (delta, span) = current.children;
            let Some(delta_seconds) = delta else {
                continue;
            };
            *lua.app_data_mut::<SongLuaCompileUpdatePhase>()
                .expect("phase installed") = SongLuaCompileUpdatePhase {
                active: true,
                order,
                delta_seconds,
            };
            if let Some(mut frames) = lua.app_data_mut::<SongLuaCompileFrames>() {
                frames.epoch += 1;
            }
            match job {
                SongLuaCompileUpdateJob::Advance {
                    actor, recurring, ..
                } => {
                    advance_motion_clock(lua, actor, delta_seconds)?;
                    advance_spin_pose(lua, actor, delta_seconds)?;
                    advance_tween_replay(lua, actor, delta_seconds, span.unwrap_or(delta_seconds as f32))?;
                    advance_capture_position(lua, actor)?;
                    let ready = lua
                        .app_data_ref::<SongLuaCompileFrames>()
                        .is_some_and(|frames| {
                            frames
                                .clocks
                                .get(&(actor.to_pointer() as usize))
                                .and_then(|clock| clock.dispatch)
                                .is_some_and(|(frame, _)| frame <= frames.frame)
                        });
                    if ready {
                        if let Err(err) = drain_actor_command_queue(lua, actor) {
                            report_update_error(
                                lua,
                                actor,
                                UPDATE_QUEUE_ERROR_KEY,
                                "update queue",
                                &err,
                            )?;
                        }
                    }
                    if !recurring {
                        // A parent's queued command can start a child's cycle
                        // after this frame's plan was built. The child still
                        // consumes its delta when native ActorFrame reaches it.
                        let started =
                            lua.app_data_ref::<SongLuaCompileFrames>()
                                .is_some_and(|frames| {
                                    frames
                                        .clocks
                                        .get(&(actor.to_pointer() as usize))
                                        .is_some_and(|clock| {
                                            clock.recurring_frame == Some(frames.frame)
                                        })
                                });
                        if !started {
                            run_recurring_update(lua, actor, delta_seconds, true)?;
                        }
                    }
                }
                SongLuaCompileUpdateJob::Recurring { actor, .. } => {
                    let started =
                        lua.app_data_ref::<SongLuaCompileFrames>()
                            .is_some_and(|frames| {
                                frames
                                    .clocks
                                    .get(&(actor.to_pointer() as usize))
                                    .is_some_and(|clock| {
                                        clock.recurring_frame == Some(frames.frame)
                                    })
                            });
                    if !started {
                        run_recurring_update(lua, actor, delta_seconds, true)?;
                    }
                }
                SongLuaCompileUpdateJob::Callback { actor, .. } => {
                    run_update_callback(lua, actor, delta_seconds)?;
                    // The native trace probe follows the base Overlay's child
                    // updates and precedes the scheduled song foreground.
                    #[cfg(feature = "test-support")]
                    crate::live::observe_overlay_stage(lua, actor)?;
                }
                SongLuaCompileUpdateJob::Enter { .. } | SongLuaCompileUpdateJob::Texture { .. }
                | SongLuaCompileUpdateJob::PlayerFeedback { .. } | SongLuaCompileUpdateJob::Transition { .. }
                    => unreachable!("actor entry handled above"),
            }
        }
        Ok(())
    })();
    *lua.app_data_mut::<SongLuaCompileUpdatePhase>()
        .expect("phase installed") = prior_phase;
    lua.set_app_data(CompileUpdateDeltaScratch(deltas));
    result
}

pub fn actor_tree_has_update_functions(lua: &Lua, root: &Value) -> mlua::Result<bool> {
    let Value::Table(root) = root else {
        return Ok(false);
    };
    actor_table_has_update_functions(lua, root)
}

pub fn update_tree_reads_global(lua: &Lua, root: &Value, name: &str) -> mlua::Result<bool> {
    let Value::Table(root) = root else {
        return Ok(false);
    };
    update_actor_reads_global(lua, root, name.as_bytes())
}

fn update_actor_reads_global(lua: &Lua, actor: &Table, name: &[u8]) -> mlua::Result<bool> {
    let chunk_has_name = |function: Function| {
        let chunk = function.dump(true);
        chunk.windows(name.len()).any(|window| window == name)
    };
    if actor
        .get::<Option<Function>>("__songlua_update_function")?
        .is_some_and(&chunk_has_name)
        || actor
            .get::<Option<String>>("__songlua_recurring_update_command")?
            .and_then(|name| actor.get::<Option<Function>>(name).ok().flatten())
            .is_some_and(&chunk_has_name)
    {
        return Ok(true);
    }
    for child in actor.sequence_values::<Value>() {
        let Value::Table(child) = child? else {
            continue;
        };
        if update_actor_reads_global(lua, &child, name)? {
            return Ok(true);
        }
    }
    if let Some(stream) = song_meter_stream_child(lua, actor)? {
        return update_actor_reads_global(lua, &stream, name);
    }
    Ok(false)
}

fn song_meter_stream_child(lua: &Lua, actor: &Table) -> mlua::Result<Option<Table>> {
    if !actor_type_is(actor, "SongMeterDisplay")? {
        return Ok(None);
    }
    let Some(stream) = actor_named_children(lua, actor)?.get::<Option<Table>>("Stream")? else {
        return Ok(None);
    };
    stream.set("__songlua_parent", actor.clone())?;
    Ok(Some(stream))
}

fn run_song_meter_stream_init_command(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    let Some(stream) = song_meter_stream_child(lua, actor)? else {
        return Ok(());
    };
    run_actor_init_commands_for_table(lua, &stream)
}

fn run_song_meter_stream_startup_command(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    let Some(stream) = song_meter_stream_child(lua, actor)? else {
        return Ok(());
    };
    run_actor_startup_commands_for_table(lua, &stream)
}

fn actor_update_rate(actor: &Table) -> mlua::Result<f64> {
    Ok(actor
        .raw_get::<Option<f64>>("__songlua_update_rate")?
        .unwrap_or(1.0))
}

fn advance_hibernation(lua: &Lua, actor: &Table, delta_seconds: f64) -> mlua::Result<Option<f64>> {
    if actor
        .raw_get::<Option<bool>>("__songlua_theme_hibernating")?
        .unwrap_or(false)
    {
        return Ok(None);
    }
    let left = actor
        .raw_get::<Option<f32>>("__songlua_hibernate_seconds")?
        .unwrap_or(0.0);
    // Native Actor::Update enters this gate only for a strictly positive
    // remainder. NaN does not sleep, just like zero and negative durations.
    if left <= 0.0 || left.is_nan() {
        return Ok(Some(f64::from(delta_seconds as f32)));
    }
    // Actor.cpp::Update subtracts native floats before wrappers and before the
    // ActorFrame update rate. Its wake-up frame receives only the overshoot.
    let left = left - delta_seconds as f32;
    actor.raw_set("__songlua_hibernate_seconds", left.max(0.0))?;
    if left <= 0.0 {
        capture_immediate_bool(lua, actor, "hibernating", false)?;
    }
    Ok((left <= 0.0).then(|| f64::from(-left)))
}

pub(crate) fn report_update_error(
    lua: &Lua,
    actor: &Table,
    reported_key: &'static str,
    callback: &str,
    err: &mlua::Error,
) -> mlua::Result<()> {
    if actor.get::<Option<bool>>(reported_key)?.unwrap_or(false) {
        return Ok(());
    }
    actor.set(reported_key, true)?;
    let beat = compile_song_runtime_values(lua).map_or(0.0, |(beat, _)| beat);
    let detail = format!(
        "{callback} for {} at beat {beat:.3}: {err}",
        actor_debug_label(actor)
    );
    if let Some(mut errors) = lua.app_data_mut::<SongLuaUpdateErrors>() {
        errors.0.push(detail);
    } else {
        lua.set_app_data(SongLuaUpdateErrors(vec![detail]));
    }
    log::debug!(
        "Song lua {callback} failed for {}; preserving partial update state: {err}",
        actor_debug_label(actor),
    );
    Ok(())
}

fn run_recurring_update(
    lua: &Lua,
    actor: &Table,
    delta_seconds: f64,
    enabled: bool,
) -> mlua::Result<()> {
    if has_tween_replay(lua, actor) {
        return Ok(());
    }
    let Some(command) =
        actor.get::<Option<LuaFieldText<128>>>("__songlua_recurring_update_command")?
    else {
        return Ok(());
    };
    let command = command.as_str();
    if !enabled {
        return Ok(());
    }
    let mut interval = actor
        .get::<Option<f64>>("__songlua_recurring_update_interval")?
        .unwrap_or(0.0);
    if interval <= f64::EPSILON {
        reset_actor_capture(lua, actor)?;
        actor.set("__songlua_recurring_update_command", Value::Nil)?;
        invalidate_compile_update_plan(lua);
        if let Err(err) = run_actor_named_command(lua, actor, command) {
            report_update_error(lua, actor, UPDATE_CMD_ERROR_KEY, command, &err)?;
        }
        return Ok(());
    }

    // Actor::UpdateTweening advances a command behind a sleep only when the
    // frame retains positive delta. Use native float subtraction; rounding
    // can defer the zero-time command and change later Lua state or RNG calls.
    let mut time_left = actor
        .get::<Option<f64>>("__songlua_recurring_update_time_left")?
        .unwrap_or(interval) as f32;
    let mut delta = delta_seconds.max(0.0) as f32;
    let mut runs = 0usize;
    while delta > 0.0 && runs < 64 {
        if time_left > 0.0 {
            let elapsed = time_left.min(delta);
            time_left -= elapsed;
            delta -= elapsed;
            if delta == 0.0 {
                break;
            }
        }

        reset_actor_capture(lua, actor)?;
        // Consume the pending command. Only a new self-queue keeps this loop
        // alive; a conditional command can finish without scheduling a cycle.
        actor.set("__songlua_recurring_update_command", Value::Nil)?;
        invalidate_compile_update_plan(lua);
        // The preceding cycle has drained this actor's queue. Do not carry its
        // captured tail forward and advance the same durations a second time.
        if let Some(mut frames) = lua.app_data_mut::<SongLuaCompileFrames>() {
            frames.clocks.remove(&(actor.to_pointer() as usize));
        }
        // A recurring command is the next queued command. Its new tween must
        // consume the delta left after finishing the preceding cycle.
        let time = lua
            .app_data_ref::<SongLuaCompileFrames>()
            .map(|frames| (frames.frame, f64::from(delta)));
        let prior_queue = lua.remove_app_data::<SongLuaQueuedCommand>();
        lua.set_app_data(SongLuaQueuedCommand {
            start: 0.0,
            actor: actor.to_pointer() as usize,
            time,
        });
        let result = run_actor_named_command(lua, actor, command)
            .and_then(|()| advance_capture_position(lua, actor));
        lua.remove_app_data::<SongLuaQueuedCommand>();
        if let Some(prior) = prior_queue {
            lua.set_app_data(prior);
        }
        if let Err(err) = result {
            report_update_error(lua, actor, UPDATE_CMD_ERROR_KEY, command, &err)?;
        }
        if actor
            .get::<Option<LuaFieldText<128>>>("__songlua_recurring_update_command")?
            .is_none()
        {
            actor.set("__songlua_recurring_update_time_left", Value::Nil)?;
            return Ok(());
        }
        runs += 1;
        interval = actor
            .get::<Option<f64>>("__songlua_recurring_update_interval")?
            .unwrap_or(0.0);
        if interval <= f64::EPSILON {
            break;
        }
        time_left = interval as f32;
    }
    actor.set("__songlua_recurring_update_time_left", f64::from(time_left))?;
    Ok(())
}

fn run_update_callback(lua: &Lua, actor: &Table, delta_seconds: f64) -> mlua::Result<()> {
    let key = if actor
        .raw_get::<Option<bool>>("__songlua_update_pending")?
        .unwrap_or(false)
    {
        "__songlua_update_previous"
    } else {
        "__songlua_update_function"
    };
    let Some(update) = actor.raw_get::<Option<Function>>(key)? else {
        return Ok(());
    };
    let observed = crate::live::observes_song_callback(&update);
    if observed { crate::live::callback_phase(lua, true); }
    let update_result =
        call_actor_function(lua, actor, &update, Some(Value::Number(delta_seconds)));
    if observed { crate::live::callback_phase(lua, false); }
    let drain_result = drain_actor_command_queue(lua, actor);
    if let Err(err) = update_result {
        report_update_error(lua, actor, UPDATE_FN_ERROR_KEY, "update function", &err)?;
    }
    if let Err(err) = drain_result {
        report_update_error(lua, actor, UPDATE_QUEUE_ERROR_KEY, "update queue", &err)?;
    }
    Ok(())
}

pub fn run_actor_update_functions_for_table(
    lua: &Lua,
    actor: &Table,
    parent_delta_seconds: f64,
) -> mlua::Result<()> {
    run_actor_update_functions_for_table_inner(lua, actor, parent_delta_seconds, false)
}

fn run_actor_update_functions_for_table_inner(
    lua: &Lua,
    actor: &Table,
    parent_delta_seconds: f64,
    run_recurring_commands: bool,
) -> mlua::Result<()> {
    // Foreground::Update also skips dormant layers during ScreenGameplay's
    // preparation Update(0), including their descendants and callbacks.
    if actor.raw_get::<Option<bool>>("__songlua_live_inactive")?.unwrap_or(false) {
        return Ok(());
    }
    let Some(parent_delta_seconds) = advance_hibernation(lua, actor, parent_delta_seconds)? else {
        return Ok(());
    };
    if let Some(wrappers) = actor.get::<Option<Table>>("__songlua_wrappers")? {
        for wrapper in wrappers.sequence_values::<Table>() {
            run_actor_update_functions_for_table_inner(
                lua,
                &wrapper?,
                parent_delta_seconds,
                run_recurring_commands,
            )?;
        }
    }
    let delta_seconds = f64::from(parent_delta_seconds as f32 * actor_update_rate(actor)? as f32);
    // Initialize clocks during the existing zero-delta startup update, before
    // its child getters and callback. Chronological replay must not add a second
    // callback at time zero. Actor::UpdateTweening still needs positive delta.
    let effect = actor.raw_get::<Option<String>>("__songlua_state_effect_mode")?;
    let music_clock = actor.raw_get::<Option<String>>("__songlua_state_effect_clock")?
        .as_deref() == Some("time")
        && actor.raw_get::<Option<bool>>("__songlua_state_effect_timer")? == Some(false);
    if delta_seconds == 0.0
        && (music_clock || effect.as_deref().is_some_and(|mode| mode != "none")
            || actor.raw_get::<Option<bool>>("__songlua_state_rainbow")?.unwrap_or(false))
    {
        advance_motion_clock(lua, actor, delta_seconds)?;
        advance_spin_pose(lua, actor, delta_seconds)?;
    }
    // ITGmania runs Actor::UpdateInternal (including queued UpdateCommands),
    // then child updates, then the ActorFrame update function.
    run_recurring_update(lua, actor, delta_seconds, run_recurring_commands)?;
    for child in actor.sequence_values::<Value>() {
        let Value::Table(child) = child? else {
            continue;
        };
        adopt_actor_child(actor, &child)?;
        run_actor_update_functions_for_table_inner(
            lua,
            &child,
            delta_seconds,
            run_recurring_commands,
        )?;
    }
    if let Some(stream) = song_meter_stream_child(lua, actor)? {
        run_actor_update_functions_for_table_inner(
            lua,
            &stream,
            delta_seconds,
            run_recurring_commands,
        )?;
    }
    run_update_callback(lua, actor, delta_seconds)?;
    Ok(())
}

pub fn run_actor_draw_functions(lua: &Lua, root: &Value) {
    let Value::Table(root) = root else {
        return;
    };
    if let Err(err) = run_actor_draw_functions_for_table(lua, root) {
        log::debug!("Skipping song lua draw function capture: {err}");
    }
}

pub fn run_actor_draw_functions_for_table(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    let globals = lua.globals();
    let prior = globals.raw_get::<Value>(DRAW_CALLBACK_ACTIVE_KEY)?;
    globals.raw_set(DRAW_CALLBACK_ACTIVE_KEY, true)?;
    let result = if crate::live::active(lua) {
        crate::draw_capture::run_live(lua, actor)
    } else {
        crate::draw_capture::run(lua, actor)
    };
    globals.raw_set(DRAW_CALLBACK_ACTIVE_KEY, prior)?;
    if let Err(err) = result {
        report_update_error(
            lua,
            actor,
            "__songlua_draw_error_reported",
            "draw callback",
            &err,
        )?;
    }
    Ok(())
}

pub fn actor_table_has_update_functions(lua: &Lua, actor: &Table) -> mlua::Result<bool> {
    // Motion clocks need chronological advancement even without a Lua callback.
    if actor
        .raw_get::<Option<f32>>("__songlua_hibernate_seconds")?
        .is_some_and(|left| left > 0.0)
        || actor
            .get::<Option<Function>>("__songlua_update_function")?
            .is_some()
        || actor
            .get::<Option<Function>>("__songlua_draw_function")?
            .is_some()
        || actor
            .get::<Option<String>>("__songlua_recurring_update_command")?
            .is_some()
        || actor
            .raw_get::<Option<Table>>("__songlua_state_motion_clock")?
            .is_some()
        || actor
            .raw_get::<Option<bool>>("__songlua_command_queue_deferred")?
            .unwrap_or(false)
    {
        return Ok(true);
    }
    if let Some(wrappers) = actor.get::<Option<Table>>("__songlua_wrappers")? {
        for wrapper in wrappers.sequence_values::<Table>() {
            if actor_table_has_update_functions(lua, &wrapper?)? {
                return Ok(true);
            }
        }
    }
    for child in actor.sequence_values::<Value>() {
        let Value::Table(child) = child? else {
            continue;
        };
        if actor_table_has_update_functions(lua, &child)? {
            return Ok(true);
        }
    }
    if let Some(stream) = song_meter_stream_child(lua, actor)? {
        return actor_table_has_update_functions(lua, &stream);
    }
    Ok(false)
}

// Table::get supplies a live result slot under the VM lock. Classify this
// frequent name without copying it; retain String's validation and coercions.
struct ActorIsFrameTexture(bool);
impl mlua::FromLua for ActorIsFrameTexture {
    fn from_lua(value: mlua::Value, lua: &Lua) -> mlua::Result<Self> {
        Ok(Self(
            String::from_lua(value, lua)?.eq_ignore_ascii_case("ActorFrameTexture"),
        ))
    }
    unsafe fn from_stack(index: c_int, lua: &mlua::state::RawLua) -> mlua::Result<Self> {
        // SAFETY: only a type-checked, rooted string is borrowed. Validation and
        // comparison finish without Lua calls, and no pointer or slice escapes.
        // Other types delegate to the original checked String conversion.
        unsafe {
            let state = lua.state();
            if ffi::lua_type(state, index) == ffi::LUA_TSTRING {
                let mut len = 0;
                let ptr = ffi::lua_tolstring(state, index, &mut len);
                let bytes = std::slice::from_raw_parts(ptr.cast::<u8>(), len);
                std::str::from_utf8(bytes).map_err(|e| mlua::Error::FromLuaConversionError {
                    from: "string",
                    to: "String".into(),
                    message: Some(e.to_string()),
                })?;
                return Ok(Self(bytes.eq_ignore_ascii_case(b"ActorFrameTexture")));
            }
            Ok(Self(
                String::from_stack(index, lua)?.eq_ignore_ascii_case("ActorFrameTexture"),
            ))
        }
    }
}

#[inline]
pub fn actor_type_is(actor: &Table, expected: &str) -> mlua::Result<bool> {
    if expected.eq_ignore_ascii_case("ActorFrameTexture") {
        return Ok(actor
            .get::<Option<ActorIsFrameTexture>>("__songlua_actor_type")?
            .is_some_and(|kind| kind.0));
    }
    Ok(actor
        .get::<Option<LuaFieldText<32>>>("__songlua_actor_type")?
        .is_some_and(|kind| {
            kind.as_str().eq_ignore_ascii_case(expected)
                || (expected.eq_ignore_ascii_case("Sprite")
                    && kind.as_str().eq_ignore_ascii_case("Banner"))
        }))
}

pub fn actor_is_bitmap_text(actor: &Table) -> mlua::Result<bool> {
    Ok(actor
        .get::<Option<LuaFieldText<32>>>("__songlua_actor_type")?
        .is_some_and(|kind| {
            ["BitmapText", "RollingNumbers", "BPMDisplay", "HelpDisplay",
                "ActiveAttackList", "ScoreDisplayAliveTime", "ScoreDisplayCalories",
                "DeviceList", "InputList"]
                .iter().any(|name| kind.as_str().eq_ignore_ascii_case(name))
        }))
}

pub fn create_named_actor(
    lua: &Lua,
    actor_type: &'static str,
    name: &str,
    create_dummy_actor: fn(&Lua, &'static str) -> mlua::Result<Table>,
) -> mlua::Result<Table> {
    let actor = create_dummy_actor(lua, actor_type)?;
    set_actor_name(&actor, lua.create_string(name)?)?;
    actor.set("Name", name)?;
    Ok(actor)
}

pub fn create_named_text_actor(
    lua: &Lua,
    actor_type: &'static str,
    name: &str,
    text: String,
    create_dummy_actor: fn(&Lua, &'static str) -> mlua::Result<Table>,
) -> mlua::Result<Table> {
    let actor = create_named_actor(lua, actor_type, name, create_dummy_actor)?;
    actor.set("Text", text)?;
    Ok(actor)
}

fn load_native_combo_milestone(lua: &Lua, thousand: bool) -> mlua::Result<Value> {
    let (name, source) = if thousand {
        ("Combo 1000milestone", include_str!("../assets/simply-love/Graphics/Combo 1000milestone/default.lua"))
    } else {
        ("Combo 100milestone", include_str!("../assets/simply-love/Graphics/Combo 100milestone/default.lua"))
    };
    let environment = lua.create_table()?;
    let mt = lua.create_table()?;
    mt.raw_set("__index", lua.globals())?;
    environment.set_metatable(Some(mt))?;
    environment.raw_set("LoadActor", lua.create_function(|lua, path: String| {
        if path == "../Combo 100milestone" {
            return load_native_combo_milestone(lua, false);
        }
        let key = if path == "explosion.png" {
            "Combo 100milestone/explosion.png"
        } else {
            path.strip_prefix(crate::SONG_LUA_THEME_PATH_PREFIX)
                .and_then(|path| path.strip_prefix("G/")).unwrap_or(&path)
        };
        let definition = lua.create_table()?;
        definition.raw_set("Texture", load_native_gameplay_in_bitmap(key)?)?;
        lua.globals().get::<Table>("Def")?.get::<Function>("Sprite")?.call::<Value>(definition)
    })?)?;
    lua.load(source).set_environment(environment)
        .set_name(format!("@theme:/Graphics/{name}/default.lua")).eval()
}

fn create_native_combo_milestone(lua: &Lua, name: &str, thousand: bool) -> mlua::Result<Table> {
    let install = lua.app_data_ref::<ActorDefinitionHost>()
        .map(|host| host.install_methods)
        .ok_or_else(|| mlua::Error::runtime("combo milestone requires its actor host"))?;
    let definition = load_native_combo_milestone(lua, thousand)?;
    let Value::Table(actor) = own_actor_root(lua, definition, install)? else {
        return Err(mlua::Error::runtime("combo milestone must load an actor"));
    };
    actor.raw_set("Name", name)?;
    native_step_statistics_lookup(lua, &actor)?;
    run_actor_init_commands_for_table(lua, &actor)?;
    Ok(actor)
}

pub(crate) fn install_native_gameplay_combo(lua: &Lua) -> mlua::Result<()> {
    let globals = lua.globals();
    let sl = globals.get::<Table>("SL")?;
    for index in 1..=2 {
        let player = globals.get::<Table>(format!("__songlua_top_screen_player_{index}"))?;
        let combo = actor_children(lua, &player)?.get::<Table>("Combo")?;
        let mods = sl.get::<Table>(format!("P{index}"))?.get::<Table>("ActiveModifiers")?;
        if !actor_type_is(&combo, "ActorFrame")? || truthy(&mods.get::<Value>("HideComboExplosions")?) {
            continue;
        }
        let children = actor_children(lua, &combo)?;
        let number = children.get::<Table>("Number")?;
        for (ordinal, name, thousand) in [(1, "OneHundredMilestone", false), (2, "OneThousandMilestone", true)] {
            let child = create_native_combo_milestone(lua, name, thousand)?;
            child.raw_set("__songlua_parent", combo.clone())?;
            combo.raw_set(ordinal, child.clone())?;
            children.raw_set(name, child)?;
        }
        combo.raw_set(3, number)?;
    }
    invalidate_compile_update_plan(lua);
    Ok(())
}

pub fn create_named_child_actor(
    lua: &Lua,
    parent: &Table,
    name: &str,
    create_dummy_actor: fn(&Lua, &'static str) -> mlua::Result<Table>,
    create_named_child_actor: fn(&Lua, &Table, &str) -> mlua::Result<Table>,
) -> mlua::Result<Table> {
    let parent_type = parent.get::<Option<String>>("__songlua_actor_type")?;
    let player_index = parent.get::<Option<i64>>("__songlua_player_index")?;
    let child = if parent_type
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("PlayerActor"))
        && let Some(player_index) = player_index
    {
        if name.eq_ignore_ascii_case("NoteField") {
            let style_name = parent
                .get::<Option<String>>("__songlua_style_name")?
                .unwrap_or_else(|| current_song_lua_style_name(lua));
            create_note_field_actor(lua, player_index as usize, &style_name, create_dummy_actor)?
        } else if name.eq_ignore_ascii_case("Judgment") {
            // Player::Init names the theme's frame Judgment. Simply Love's
            // replaceable texture belongs to its JudgmentWithOffsets Sprite.
            let frame = create_named_actor(lua, "ActorFrame", name, create_dummy_actor)?;
            let sprite =
                create_named_actor(lua, "Sprite", "JudgmentWithOffsets", create_dummy_actor)?;
            sprite.set("__songlua_parent", frame.clone())?;
            sprite.set("__songlua_sprite_animate", false)?;
            sprite.set("__songlua_visible", false)?;
            sprite.set("__songlua_judgment_sprite_player", player_index)?;
            if let Some(path) = parent.get::<Option<String>>("__songlua_judgment_texture")? {
                set_actor_texture_from_path(&sprite, &path)?;
            }
            push_sequence_child_once(&frame, sprite.clone())?;
            actor_children(lua, &frame)?.set("JudgmentWithOffsets", sprite)?;
            frame
        } else if name.eq_ignore_ascii_case("Combo") {
            // Player combo.lua returns an invisible leaf when hidden or when
            // GetComboFonts finds no valid assets in the current theme.
            let fonts = lua.globals().get::<Function>("GetComboFonts")?.call::<Table>(())?;
            let sl = lua.globals().get::<Table>("SL")?;
            let mods = sl.get::<Table>(format!("P{}", player_index + 1))?
                .get::<Table>("ActiveModifiers")?;
            if truthy(&mods.get::<Value>("HideCombo")?) || fonts.raw_len() == 0 {
                let actor = create_named_actor(lua, "Actor", name, create_dummy_actor)?;
                actor.raw_set("__songlua_visible", false)?;
                actor
            } else {
                let frame = create_named_actor(lua, "ActorFrame", name, create_dummy_actor)?;
                let number = create_named_actor(lua, "BitmapText", "Number", create_dummy_actor)?;
                number.set("__songlua_parent", frame.clone())?;
                push_sequence_child_once(&frame, number.clone())?;
                actor_children(lua, &frame)?.set("Number", number)?;
                frame
            }
        } else {
            create_dummy_actor(lua, "ChildActor")?
        }
    } else if parent_type
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("TopScreen"))
        && name.eq_ignore_ascii_case("Timer")
    {
        create_screen_timer_actor(lua, create_dummy_actor)?
    } else if parent_type
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("TopScreen"))
        && let Some(player_index) = top_screen_score_index(name)
    {
        create_top_screen_score_actor(lua, player_index, create_dummy_actor)?
    } else if parent_type
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("TopScreen"))
        && let Some(child) = create_top_screen_theme_actor(
            lua,
            parent,
            name,
            create_dummy_actor,
            create_named_child_actor,
        )?
    {
        child
    } else if parent
        .get::<Option<String>>("__songlua_top_screen_child_name")?
        .as_deref()
        .is_some_and(|child_name| child_name.eq_ignore_ascii_case("Underlay"))
        && let Some(child) = create_underlay_theme_actor(lua, parent, name, create_dummy_actor)?
    {
        child
    } else {
        create_dummy_actor(lua, "ChildActor")?
    };
    set_actor_name(&child, lua.create_string(name)?)?;
    child.set("Name", name)?;
    copy_dummy_actor_tags(parent, &child)?;
    child.set("__songlua_parent", parent.clone())?;
    // A theme child is a distinct target, not another alias of its Underlay.
    if parent
        .get::<Option<String>>("__songlua_top_screen_child_name")?
        .as_deref()
        == Some("Underlay")
    {
        child.set("__songlua_top_screen_child_name", name)?;
    }
    if parent_type
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("PlayerActor"))
    {
        child.set("__songlua_player_child_name", name)?;
        // Native Player::Init owns these actors in m_SubActors. The named
        // lookup is an alias of that tree, not a separate unadvanced actor.
        actor_children(lua, parent)?.set(name, child.clone())?;
        if push_sequence_child_once(parent, child.clone())? {
            invalidate_compile_update_plan(lua);
        }
    } else if parent_type
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("TopScreen"))
    {
        child.set("__songlua_top_screen_child_name", name)?;
    }
    Ok(child)
}

struct NativeGameplayInBitmap {
    key: &'static str,
    filename: &'static str,
    bytes: &'static [u8],
    prepared: OnceLock<Result<String, String>>,
}

static NATIVE_GAMEPLAY_IN_BITMAPS: [NativeGameplayInBitmap; 57] = [
    NativeGameplayInBitmap { key: "Combo 100milestone/explosion", filename: "Combo 100milestone/explosion.png", bytes: include_bytes!("../assets/simply-love/Graphics/Combo 100milestone/explosion.png"), prepared: OnceLock::new() },
    NativeGameplayInBitmap { key: "_VisualStyles/Arrows/Combo 1000milestone swoosh", filename: "_VisualStyles/Arrows/Combo 1000milestone swoosh.png", bytes: include_bytes!("../assets/simply-love/Graphics/_VisualStyles/Arrows/Combo 1000milestone swoosh.png"), prepared: OnceLock::new() },
    NativeGameplayInBitmap { key: "_VisualStyles/Arrows/Combo 100milestone minisplode", filename: "_VisualStyles/Arrows/Combo 100milestone minisplode (doubleres).png", bytes: include_bytes!("../assets/simply-love/Graphics/_VisualStyles/Arrows/Combo 100milestone minisplode (doubleres).png"), prepared: OnceLock::new() },
    NativeGameplayInBitmap { key: "_VisualStyles/Arrows/Combo 100milestone splode", filename: "_VisualStyles/Arrows/Combo 100milestone splode (doubleres).png", bytes: include_bytes!("../assets/simply-love/Graphics/_VisualStyles/Arrows/Combo 100milestone splode (doubleres).png"), prepared: OnceLock::new() },
    NativeGameplayInBitmap { key: "_VisualStyles/Bears/Combo 1000milestone swoosh", filename: "_VisualStyles/Bears/Combo 1000milestone swoosh.png", bytes: include_bytes!("../assets/simply-love/Graphics/_VisualStyles/Bears/Combo 1000milestone swoosh.png"), prepared: OnceLock::new() },
    NativeGameplayInBitmap { key: "_VisualStyles/Bears/Combo 100milestone minisplode", filename: "_VisualStyles/Bears/Combo 100milestone minisplode (doubleres).png", bytes: include_bytes!("../assets/simply-love/Graphics/_VisualStyles/Bears/Combo 100milestone minisplode (doubleres).png"), prepared: OnceLock::new() },
    NativeGameplayInBitmap { key: "_VisualStyles/Bears/Combo 100milestone splode", filename: "_VisualStyles/Bears/Combo 100milestone splode (doubleres).png", bytes: include_bytes!("../assets/simply-love/Graphics/_VisualStyles/Bears/Combo 100milestone splode (doubleres).png"), prepared: OnceLock::new() },
    NativeGameplayInBitmap { key: "_VisualStyles/Cats/Combo 1000milestone swoosh", filename: "_VisualStyles/Cats/Combo 1000milestone swoosh.png", bytes: include_bytes!("../assets/simply-love/Graphics/_VisualStyles/Cats/Combo 1000milestone swoosh.png"), prepared: OnceLock::new() },
    NativeGameplayInBitmap { key: "_VisualStyles/Cats/Combo 100milestone minisplode", filename: "_VisualStyles/Cats/Combo 100milestone minisplode (doubleres).png", bytes: include_bytes!("../assets/simply-love/Graphics/_VisualStyles/Cats/Combo 100milestone minisplode (doubleres).png"), prepared: OnceLock::new() },
    NativeGameplayInBitmap { key: "_VisualStyles/Cats/Combo 100milestone splode", filename: "_VisualStyles/Cats/Combo 100milestone splode.png", bytes: include_bytes!("../assets/simply-love/Graphics/_VisualStyles/Cats/Combo 100milestone splode.png"), prepared: OnceLock::new() },
    NativeGameplayInBitmap { key: "_VisualStyles/Ducks/Combo 1000milestone swoosh", filename: "_VisualStyles/Ducks/Combo 1000milestone swoosh.png", bytes: include_bytes!("../assets/simply-love/Graphics/_VisualStyles/Ducks/Combo 1000milestone swoosh.png"), prepared: OnceLock::new() },
    NativeGameplayInBitmap { key: "_VisualStyles/Ducks/Combo 100milestone minisplode", filename: "_VisualStyles/Ducks/Combo 100milestone minisplode (doubleres).png", bytes: include_bytes!("../assets/simply-love/Graphics/_VisualStyles/Ducks/Combo 100milestone minisplode (doubleres).png"), prepared: OnceLock::new() },
    NativeGameplayInBitmap { key: "_VisualStyles/Ducks/Combo 100milestone splode", filename: "_VisualStyles/Ducks/Combo 100milestone splode (doubleres).png", bytes: include_bytes!("../assets/simply-love/Graphics/_VisualStyles/Ducks/Combo 100milestone splode (doubleres).png"), prepared: OnceLock::new() },
    NativeGameplayInBitmap { key: "_VisualStyles/Gay/Combo 1000milestone swoosh", filename: "_VisualStyles/Gay/Combo 1000milestone swoosh.png", bytes: include_bytes!("../assets/simply-love/Graphics/_VisualStyles/Gay/Combo 1000milestone swoosh.png"), prepared: OnceLock::new() },
    NativeGameplayInBitmap { key: "_VisualStyles/Gay/Combo 100milestone minisplode", filename: "_VisualStyles/Gay/Combo 100milestone minisplode.png", bytes: include_bytes!("../assets/simply-love/Graphics/_VisualStyles/Gay/Combo 100milestone minisplode.png"), prepared: OnceLock::new() },
    NativeGameplayInBitmap { key: "_VisualStyles/Gay/Combo 100milestone splode", filename: "_VisualStyles/Gay/Combo 100milestone splode.png", bytes: include_bytes!("../assets/simply-love/Graphics/_VisualStyles/Gay/Combo 100milestone splode.png"), prepared: OnceLock::new() },
    NativeGameplayInBitmap { key: "_VisualStyles/Hearts/Combo 1000milestone swoosh", filename: "_VisualStyles/Hearts/Combo 1000milestone swoosh.png", bytes: include_bytes!("../assets/simply-love/Graphics/_VisualStyles/Hearts/Combo 1000milestone swoosh.png"), prepared: OnceLock::new() },
    NativeGameplayInBitmap { key: "_VisualStyles/Hearts/Combo 100milestone minisplode", filename: "_VisualStyles/Hearts/Combo 100milestone minisplode.png", bytes: include_bytes!("../assets/simply-love/Graphics/_VisualStyles/Hearts/Combo 100milestone minisplode.png"), prepared: OnceLock::new() },
    NativeGameplayInBitmap { key: "_VisualStyles/Hearts/Combo 100milestone splode", filename: "_VisualStyles/Hearts/Combo 100milestone splode.png", bytes: include_bytes!("../assets/simply-love/Graphics/_VisualStyles/Hearts/Combo 100milestone splode.png"), prepared: OnceLock::new() },
    NativeGameplayInBitmap { key: "_VisualStyles/Spooky/Combo 1000milestone swoosh", filename: "_VisualStyles/Spooky/Combo 1000milestone swoosh.png", bytes: include_bytes!("../assets/simply-love/Graphics/_VisualStyles/Spooky/Combo 1000milestone swoosh.png"), prepared: OnceLock::new() },
    NativeGameplayInBitmap { key: "_VisualStyles/Spooky/Combo 100milestone minisplode", filename: "_VisualStyles/Spooky/Combo 100milestone minisplode (doubleres).png", bytes: include_bytes!("../assets/simply-love/Graphics/_VisualStyles/Spooky/Combo 100milestone minisplode (doubleres).png"), prepared: OnceLock::new() },
    NativeGameplayInBitmap { key: "_VisualStyles/Spooky/Combo 100milestone splode", filename: "_VisualStyles/Spooky/Combo 100milestone splode (doubleres).png", bytes: include_bytes!("../assets/simply-love/Graphics/_VisualStyles/Spooky/Combo 100milestone splode (doubleres).png"), prepared: OnceLock::new() },
    NativeGameplayInBitmap { key: "_VisualStyles/SRPG10/Combo 1000milestone swoosh", filename: "_VisualStyles/SRPG10/Combo 1000milestone swoosh.png", bytes: include_bytes!("../assets/simply-love/Graphics/_VisualStyles/SRPG10/Combo 1000milestone swoosh.png"), prepared: OnceLock::new() },
    NativeGameplayInBitmap { key: "_VisualStyles/SRPG10/Combo 100milestone minisplode", filename: "_VisualStyles/SRPG10/Combo 100milestone minisplode.png", bytes: include_bytes!("../assets/simply-love/Graphics/_VisualStyles/SRPG10/Combo 100milestone minisplode.png"), prepared: OnceLock::new() },
    NativeGameplayInBitmap { key: "_VisualStyles/SRPG10/Combo 100milestone splode", filename: "_VisualStyles/SRPG10/Combo 100milestone splode.png", bytes: include_bytes!("../assets/simply-love/Graphics/_VisualStyles/SRPG10/Combo 100milestone splode.png"), prepared: OnceLock::new() },
    NativeGameplayInBitmap { key: "_VisualStyles/Stars/Combo 1000milestone swoosh", filename: "_VisualStyles/Stars/Combo 1000milestone swoosh.png", bytes: include_bytes!("../assets/simply-love/Graphics/_VisualStyles/Stars/Combo 1000milestone swoosh.png"), prepared: OnceLock::new() },
    NativeGameplayInBitmap { key: "_VisualStyles/Stars/Combo 100milestone minisplode", filename: "_VisualStyles/Stars/Combo 100milestone minisplode (doubleres).png", bytes: include_bytes!("../assets/simply-love/Graphics/_VisualStyles/Stars/Combo 100milestone minisplode (doubleres).png"), prepared: OnceLock::new() },
    NativeGameplayInBitmap { key: "_VisualStyles/Stars/Combo 100milestone splode", filename: "_VisualStyles/Stars/Combo 100milestone splode (doubleres).png", bytes: include_bytes!("../assets/simply-love/Graphics/_VisualStyles/Stars/Combo 100milestone splode (doubleres).png"), prepared: OnceLock::new() },
    NativeGameplayInBitmap { key: "_VisualStyles/Technique/Combo 1000milestone swoosh", filename: "_VisualStyles/Technique/Combo 1000milestone swoosh.png", bytes: include_bytes!("../assets/simply-love/Graphics/_VisualStyles/Technique/Combo 1000milestone swoosh.png"), prepared: OnceLock::new() },
    NativeGameplayInBitmap { key: "_VisualStyles/Technique/Combo 100milestone minisplode", filename: "_VisualStyles/Technique/Combo 100milestone minisplode (doubleres).png", bytes: include_bytes!("../assets/simply-love/Graphics/_VisualStyles/Technique/Combo 100milestone minisplode (doubleres).png"), prepared: OnceLock::new() },
    NativeGameplayInBitmap { key: "_VisualStyles/Technique/Combo 100milestone splode", filename: "_VisualStyles/Technique/Combo 100milestone splode (doubleres).png", bytes: include_bytes!("../assets/simply-love/Graphics/_VisualStyles/Technique/Combo 100milestone splode (doubleres).png"), prepared: OnceLock::new() },
    NativeGameplayInBitmap { key: "_VisualStyles/Thonk/Combo 1000milestone swoosh", filename: "_VisualStyles/Thonk/Combo 1000milestone swoosh.png", bytes: include_bytes!("../assets/simply-love/Graphics/_VisualStyles/Thonk/Combo 1000milestone swoosh.png"), prepared: OnceLock::new() },
    NativeGameplayInBitmap { key: "_VisualStyles/Thonk/Combo 100milestone minisplode", filename: "_VisualStyles/Thonk/Combo 100milestone minisplode (doubleres).png", bytes: include_bytes!("../assets/simply-love/Graphics/_VisualStyles/Thonk/Combo 100milestone minisplode (doubleres).png"), prepared: OnceLock::new() },
    NativeGameplayInBitmap { key: "_VisualStyles/Thonk/Combo 100milestone splode", filename: "_VisualStyles/Thonk/Combo 100milestone splode (doubleres).png", bytes: include_bytes!("../assets/simply-love/Graphics/_VisualStyles/Thonk/Combo 100milestone splode (doubleres).png"), prepared: OnceLock::new() },
    NativeGameplayInBitmap { key: "_VisualStyles/Arrows/GameplayIn minisplode", filename: "_VisualStyles/Arrows/GameplayIn minisplode (doubleres).png", bytes: include_bytes!("../assets/simply-love/Graphics/_VisualStyles/Arrows/GameplayIn minisplode (doubleres).png"), prepared: OnceLock::new() },
    NativeGameplayInBitmap { key: "_VisualStyles/Arrows/GameplayIn splode", filename: "_VisualStyles/Arrows/GameplayIn splode (doubleres).png", bytes: include_bytes!("../assets/simply-love/Graphics/_VisualStyles/Arrows/GameplayIn splode (doubleres).png"), prepared: OnceLock::new() },
    NativeGameplayInBitmap { key: "_VisualStyles/Bears/GameplayIn minisplode", filename: "_VisualStyles/Bears/GameplayIn minisplode.png", bytes: include_bytes!("../assets/simply-love/Graphics/_VisualStyles/Bears/GameplayIn minisplode.png"), prepared: OnceLock::new() },
    NativeGameplayInBitmap { key: "_VisualStyles/Bears/GameplayIn splode", filename: "_VisualStyles/Bears/GameplayIn splode.png", bytes: include_bytes!("../assets/simply-love/Graphics/_VisualStyles/Bears/GameplayIn splode.png"), prepared: OnceLock::new() },
    NativeGameplayInBitmap { key: "_VisualStyles/Cats/GameplayIn minisplode", filename: "_VisualStyles/Cats/GameplayIn minisplode.png", bytes: include_bytes!("../assets/simply-love/Graphics/_VisualStyles/Cats/GameplayIn minisplode.png"), prepared: OnceLock::new() },
    NativeGameplayInBitmap { key: "_VisualStyles/Cats/GameplayIn splode", filename: "_VisualStyles/Cats/GameplayIn splode.png", bytes: include_bytes!("../assets/simply-love/Graphics/_VisualStyles/Cats/GameplayIn splode.png"), prepared: OnceLock::new() },
    NativeGameplayInBitmap { key: "_VisualStyles/Ducks/GameplayIn minisplode", filename: "_VisualStyles/Ducks/GameplayIn minisplode (doubleres).png", bytes: include_bytes!("../assets/simply-love/Graphics/_VisualStyles/Ducks/GameplayIn minisplode (doubleres).png"), prepared: OnceLock::new() },
    NativeGameplayInBitmap { key: "_VisualStyles/Ducks/GameplayIn splode", filename: "_VisualStyles/Ducks/GameplayIn splode (doubleres).png", bytes: include_bytes!("../assets/simply-love/Graphics/_VisualStyles/Ducks/GameplayIn splode (doubleres).png"), prepared: OnceLock::new() },
    NativeGameplayInBitmap { key: "_VisualStyles/Gay/GameplayIn minisplode", filename: "_VisualStyles/Gay/GameplayIn minisplode.png", bytes: include_bytes!("../assets/simply-love/Graphics/_VisualStyles/Gay/GameplayIn minisplode.png"), prepared: OnceLock::new() },
    NativeGameplayInBitmap { key: "_VisualStyles/Gay/GameplayIn splode", filename: "_VisualStyles/Gay/GameplayIn splode.png", bytes: include_bytes!("../assets/simply-love/Graphics/_VisualStyles/Gay/GameplayIn splode.png"), prepared: OnceLock::new() },
    NativeGameplayInBitmap { key: "_VisualStyles/Hearts/GameplayIn minisplode", filename: "_VisualStyles/Hearts/GameplayIn minisplode.png", bytes: include_bytes!("../assets/simply-love/Graphics/_VisualStyles/Hearts/GameplayIn minisplode.png"), prepared: OnceLock::new() },
    NativeGameplayInBitmap { key: "_VisualStyles/Hearts/GameplayIn splode", filename: "_VisualStyles/Hearts/GameplayIn splode.png", bytes: include_bytes!("../assets/simply-love/Graphics/_VisualStyles/Hearts/GameplayIn splode.png"), prepared: OnceLock::new() },
    NativeGameplayInBitmap { key: "_VisualStyles/Spooky/ExtraSpooky/Bats", filename: "_VisualStyles/Spooky/ExtraSpooky/Bats (doubleres) 2x1.png", bytes: include_bytes!("../assets/simply-love/Graphics/_VisualStyles/Spooky/ExtraSpooky/Bats (doubleres) 2x1.png"), prepared: OnceLock::new() },
    NativeGameplayInBitmap { key: "_VisualStyles/Spooky/GameplayIn minisplode", filename: "_VisualStyles/Spooky/GameplayIn minisplode (doubleres).png", bytes: include_bytes!("../assets/simply-love/Graphics/_VisualStyles/Spooky/GameplayIn minisplode (doubleres).png"), prepared: OnceLock::new() },
    NativeGameplayInBitmap { key: "_VisualStyles/Spooky/GameplayIn splode", filename: "_VisualStyles/Spooky/GameplayIn splode (doubleres).png", bytes: include_bytes!("../assets/simply-love/Graphics/_VisualStyles/Spooky/GameplayIn splode (doubleres).png"), prepared: OnceLock::new() },
    NativeGameplayInBitmap { key: "_VisualStyles/SRPG10/GameplayIn minisplode", filename: "_VisualStyles/SRPG10/GameplayIn minisplode.png", bytes: include_bytes!("../assets/simply-love/Graphics/_VisualStyles/SRPG10/GameplayIn minisplode.png"), prepared: OnceLock::new() },
    NativeGameplayInBitmap { key: "_VisualStyles/SRPG10/GameplayIn splode", filename: "_VisualStyles/SRPG10/GameplayIn splode.png", bytes: include_bytes!("../assets/simply-love/Graphics/_VisualStyles/SRPG10/GameplayIn splode.png"), prepared: OnceLock::new() },
    NativeGameplayInBitmap { key: "_VisualStyles/Stars/GameplayIn minisplode", filename: "_VisualStyles/Stars/GameplayIn minisplode (doubleres).png", bytes: include_bytes!("../assets/simply-love/Graphics/_VisualStyles/Stars/GameplayIn minisplode (doubleres).png"), prepared: OnceLock::new() },
    NativeGameplayInBitmap { key: "_VisualStyles/Stars/GameplayIn splode", filename: "_VisualStyles/Stars/GameplayIn splode (doubleres).png", bytes: include_bytes!("../assets/simply-love/Graphics/_VisualStyles/Stars/GameplayIn splode (doubleres).png"), prepared: OnceLock::new() },
    NativeGameplayInBitmap { key: "_VisualStyles/Technique/GameplayIn minisplode", filename: "_VisualStyles/Technique/GameplayIn minisplode (doubleres).png", bytes: include_bytes!("../assets/simply-love/Graphics/_VisualStyles/Technique/GameplayIn minisplode (doubleres).png"), prepared: OnceLock::new() },
    NativeGameplayInBitmap { key: "_VisualStyles/Technique/GameplayIn splode", filename: "_VisualStyles/Technique/GameplayIn splode (doubleres).png", bytes: include_bytes!("../assets/simply-love/Graphics/_VisualStyles/Technique/GameplayIn splode (doubleres).png"), prepared: OnceLock::new() },
    NativeGameplayInBitmap { key: "_VisualStyles/Thonk/GameplayIn minisplode", filename: "_VisualStyles/Thonk/GameplayIn minisplode (doubleres).png", bytes: include_bytes!("../assets/simply-love/Graphics/_VisualStyles/Thonk/GameplayIn minisplode (doubleres).png"), prepared: OnceLock::new() },
    NativeGameplayInBitmap { key: "_VisualStyles/Thonk/GameplayIn splode", filename: "_VisualStyles/Thonk/GameplayIn splode (doubleres).png", bytes: include_bytes!("../assets/simply-love/Graphics/_VisualStyles/Thonk/GameplayIn splode (doubleres).png"), prepared: OnceLock::new() },
 ];

pub(crate) fn native_gameplay_in_bitmap_filename(name: &str) -> Option<&'static str> {
    NATIVE_GAMEPLAY_IN_BITMAPS.iter().find(|bitmap| bitmap.key == name || bitmap.filename == name)
        .map(|bitmap| bitmap.filename)
}

fn load_native_gameplay_in_bitmap(name: &str) -> mlua::Result<String> {
    let bitmap = NATIVE_GAMEPLAY_IN_BITMAPS.iter()
        .find(|bitmap| bitmap.key == name || bitmap.filename == name)
        .ok_or_else(|| mlua::Error::runtime(format!("missing native gameplay-in bitmap: {name}")))?;
    bitmap.prepared.get_or_init(|| {
        let decoded = image::load_from_memory_with_format(bitmap.bytes, image::ImageFormat::Png)
            .map_err(|error| error.to_string())?;
        let mut image = decoded.into_rgba8();
        let source_size = [image.width(), image.height()];
        let mut hints = deadlib_assets::parse_texture_hints(bitmap.filename);
        hints.min_size = Some(8);
        hints.max_size = Some(2048);
        let size = deadlib_assets::texture_image_size(source_size, &hints)
            .map_err(|error| error.to_string())?;
        if size != source_size {
            image = image::imageops::resize(&image, size[0], size[1], image::imageops::FilterType::Triangle);
        }
        deadlib_assets::apply_texture_hints(&mut image, &hints);
        deadlib_assets::fix_hidden_alpha(&mut image);
        let path = format!("{}G/{}", crate::SONG_LUA_THEME_PATH_PREFIX, bitmap.filename);
        deadlib_assets::register_prepared_generated_texture(&path, image, source_size, hints.sampler_desc());
        Ok(path)
    }).as_ref().cloned().map_err(|error| mlua::Error::external(error.clone()))
}

const NATIVE_BLANK_TEXTURE_PATH: &str = "Themes/_fallback/Graphics/_blank.png";

fn load_native_blank_texture() -> mlua::Result<&'static str> {
    // Bundled native bitmap, decoded/prepared once at screen construction.
    // The generated registry carries real pixels to ordinary renderer prewarm.
    static PREPARED: OnceLock<Result<(), String>> = OnceLock::new();
    PREPARED.get_or_init(|| {
        let decoded = image::load_from_memory_with_format(
            include_bytes!("../assets/itgmania/_blank.png"), image::ImageFormat::Png,
        ).map_err(|error| error.to_string())?;
        let mut bitmap = decoded.into_rgba8();
        let source_size = [bitmap.width(), bitmap.height()];
        let hints = deadlib_assets::TextureHints {
            min_size: Some(8), max_size: Some(2048), ..Default::default()
        };
        let upload = deadlib_assets::texture_image_size(source_size, &hints)
            .map_err(|error| error.to_string())?;
        if upload != source_size {
            bitmap = image::imageops::resize(&bitmap, upload[0], upload[1], image::imageops::FilterType::Triangle);
        }
        deadlib_assets::fix_hidden_alpha(&mut bitmap);
        deadlib_assets::register_prepared_generated_texture(
            NATIVE_BLANK_TEXTURE_PATH, bitmap, source_size, hints.sampler_desc(),
        );
        Ok(())
    }).as_ref().map(|_| NATIVE_BLANK_TEXTURE_PATH)
        .map_err(|error| mlua::Error::external(error.clone()))
}

fn create_lyric_display_actor(
    lua: &Lua,
    name: &str,
    create_dummy_actor: fn(&Lua, &'static str) -> mlua::Result<Table>,
) -> mlua::Result<Table> {
    let actor = create_named_actor(lua, "ActorFrame", name, create_dummy_actor)?;
    // LyricDisplay.cpp loads the inherited font and Changed command for each
    // text layer before Init clears both strings. ScreenGameplay owns position.
    let commands: Table = lua.load(r#"
return {
    Back = function(self) self:LyricCommand('Back') end,
    Front = function(self) self:LyricCommand('Front') end,
    SetNoReverseCommand = function(self) self:x(SCREEN_CENTER_X):y(SCREEN_CENTER_Y + 160) end,
    SetReverseCommand = function(self) self:x(SCREEN_CENTER_X):y(SCREEN_CENTER_Y - 140) end,
    SetOneReverseCommand = function(self) self:x(SCREEN_CENTER_X):y(SCREEN_CENTER_Y - 205) end,
}
"#).set_name("@theme:/metrics.ini:LyricDisplay").eval()?;
    for (name, side) in [("LyricBack", "Back"), ("LyricFront", "Front")] {
        let text = create_named_text_actor(lua, "BitmapText", name, String::new(), create_dummy_actor)?;
        text.raw_set("Font", "Common Normal")?;
        text.raw_set("ChangedCommand", commands.raw_get::<Function>(side)?)?;
        text.raw_set("__songlua_parent", actor.clone())?;
        push_sequence_child_once(&actor, text.clone())?;
        actor_children(lua, &actor)?.set(name, text)?;
    }
    for name in ["SetNoReverseCommand", "SetReverseCommand", "SetOneReverseCommand"] {
        actor.raw_set(name, commands.raw_get::<Function>(name)?)?;
    }
    let initial: String = lua.load(r#"
local allReverse, anyReverse = true, false
for _, player in ipairs(GAMESTATE:GetEnabledPlayers()) do
    local reverse = GAMESTATE:GetPlayerState(player):GetPlayerOptions('ModsLevel_Current'):Reverse() == 1
    allReverse = allReverse and reverse
    anyReverse = anyReverse or reverse
end
return allReverse and 'SetReverse' or anyReverse and 'SetOneReverse' or 'SetNoReverse'
"#).set_name("@theme:/ScreenGameplay:LyricDisplayPosition").eval()?;
    // This is ScreenGameplay's constructor initialization, before authored
    // command capture. Calling the public dispatcher would emit song events.
    commands.raw_get::<Function>(format!("{initial}Command"))?.call::<()>(actor.clone())?;
    Ok(actor)
}

// StepsDisplay.cpp::Load/SetInternal and the pinned StepsDisplayGameplay metrics.
fn create_native_steps_display(
    lua: &Lua, name: &str,
    create_dummy_actor: fn(&Lua, &'static str) -> mlua::Result<Table>,
) -> mlua::Result<Table> {
    let display = create_named_actor(lua, "StepsDisplay", name, create_dummy_actor)?;
    display.raw_set("__songlua_owned_child_lookup", true)?;
    for (name, kind) in [("Frame", "Sprite"), ("Meter", "BitmapText"),
        ("Description", "BitmapText"), ("Autogen", "Sprite")] {
        let child = create_named_actor(lua, kind, name, create_dummy_actor)?;
        if kind == "Sprite" { set_actor_texture_from_path(&child, load_native_blank_texture()?)?; }
        else { child.raw_set("Font", "Common Normal")?; child.raw_set("Text", "")?; }
        child.raw_set("__songlua_parent", display.clone())?;
        push_sequence_child_once(&display, child.clone())?;
        actor_children(lua, &display)?.raw_set(name, child)?;
    }
    for (name, trail) in [("SetFromSteps", false), ("SetFromTrail", true)] {
        display.set(name, lua.create_function(move |lua, (display, value): (Table, Option<Table>)| {
            if let Some(value) = value { native_steps_display_set(lua, &display, Some(value), trail)?; }
            else { native_steps_display_visibility(lua, &display, false)?; }
            note_song_lua_side_effect(lua)?;
            Ok(display)
        })?)?;
    }
    display.set("SetFromGameState", lua.create_function(|lua, (display, player): (Table, Value)| {
        native_steps_display_from_game_state(lua, &display, player)?;
        note_song_lua_side_effect(lua)?;
        Ok(display)
    })?)?;
    Ok(display)
}

fn native_steps_display_visibility(lua: &Lua, actor: &Table, visible: bool) -> mlua::Result<()> {
    // Native SetVisible updates the immediate property, including GetVisible.
    actor.raw_set("__songlua_visible", visible)?;
    capture_immediate_bool(lua, actor, "visible", visible)
}

fn native_steps_display_set(lua: &Lua, display: &Table, value: Option<Table>, trail: bool) -> mlua::Result<()> {
    let (difficulty, steps_type, meter) = if let Some(value) = &value {
        (value.get::<Function>("GetDifficulty")?.call::<String>(value.clone())?,
         value.get::<Function>("GetStepsType")?.call::<String>(value.clone())?,
         value.get::<Function>("GetMeter")?.call::<i32>(value.clone())?)
    } else {
        (display.raw_get::<String>("__songlua_preferred_difficulty")?, "StepsType_Invalid".into(), 0)
    };
    let short = difficulty.strip_prefix("Difficulty_").unwrap_or(&difficulty);
    // Difficulty.cpp's matching CustomDifficulty entries in the fallback theme.
    let custom = match (steps_type.as_str(), short) {
        ("StepsType_Pump_Single", "Hard") => "Crazy",
        ("StepsType_Pump_Double", "Medium") => "Freestyle",
        ("StepsType_Pump_Double", "Hard") => "Nightmare",
        ("StepsType_Pump_Halfdouble", "Medium") => "HalfDouble",
        (_, "Invalid") => "",
        _ => short,
    };
    let description = if !trail && short == "Edit" && value.is_some() {
        let steps = value.as_ref().unwrap();
        steps.get::<Function>("GetDescription")?.call::<String>(steps.clone())?
    } else {
        // _fallback/Languages/en.ini [CustomDifficulty], inherited by Simply Love.
        match custom { "Beginner" => "Novice", "Challenge" => "Expert", _ => custom }.into()
    };
    let params = lua.create_table()?;
    params.raw_set("CustomDifficulty", custom)?;
    params.raw_set("DisplayDescription", description.clone())?;
    params.raw_set("Meter", meter)?;
    params.raw_set("StepsType", steps_type)?;
    if let Some(value) = &value { params.raw_set(if trail { "Trail" } else { "Steps" }, value.clone())?; }
    let params = Some(Value::Table(params));
    native_steps_display_visibility(lua, display, true)?;
    let children = actor_children(lua, display)?;
    let frame = children.raw_get::<Table>("Frame")?;
    run_actor_message_with_params(lua, &frame, "SetCommand", params.clone())?;
    let text = children.raw_get::<Table>("Meter")?;
    text.raw_set("Text", if meter == 0 { "?".into() } else { meter.to_string() })?;
    if meter != 0 { run_actor_message_with_params(lua, &text, "SetCommand", params.clone())?; }
    let text = children.raw_get::<Table>("Description")?;
    text.raw_set("Text", description)?;
    run_actor_message_with_params(lua, &text, "SetCommand", params.clone())?;
    let autogen = children.raw_get::<Table>("Autogen")?;
    run_actor_message_with_params(lua, &autogen, "SetCommand", params.clone())?;
    let is_autogen = !trail && value.as_ref().map(|steps| {
        steps.get::<Function>("IsAutogen")?.call::<bool>(steps.clone())
    }).transpose()?.unwrap_or(false);
    native_steps_display_visibility(lua, &autogen, is_autogen)?;
    // ActorFrame::HandleMessage forwards a direct message, even when command
    // propagation is disabled. SetInternal therefore visits these children twice.
    run_actor_named_command_with_drain_and_params(lua, display, "SetCommand", true, params.clone())?;
    for child in actor_direct_children(lua, display)? {
        run_actor_message_with_params(lua, &child, "SetCommand", params.clone())?;
    }
    Ok(())
}

fn native_steps_display_from_game_state(lua: &Lua, display: &Table, player: Value) -> mlua::Result<()> {
    let game = lua.globals().get::<Table>("GAMESTATE")?;
    let course = game.get::<Function>("IsCourseMode")?.call::<bool>(game.clone())?;
    let value = game.get::<Function>(if course { "GetCurrentTrail" } else { "GetCurrentSteps" })?
        .call::<Option<Table>>((game, player))?;
    native_steps_display_set(lua, display, value, course)
}

fn initialize_native_steps_displays(lua: &Lua, screen: &Table) -> mlua::Result<()> {
    let game = lua.globals().get::<Table>("GAMESTATE")?;
    let commands = lua.load(r#"
return {
    FrameLoadCommand = function(self,param) local bFlip = param.PlayerState and param.PlayerState:GetPlayerNumber() ~= PLAYER_1; self:zoomx(bFlip and -1 or 1); end,
    FrameSetCommand = function(self,param) if param.CustomDifficulty and param.CustomDifficulty ~= "" then self:diffuse(CustomDifficultyToColor(param.CustomDifficulty)) end end,
    MeterOnCommand = function(self) self:visible(false) end,
    MeterSetCommand = function(self,param) MESSAGEMAN:Broadcast("MeterSet") end,
    DescriptionOnCommand = function(self) self:zoom(0.8):horizalign("left"):vertspacing(-9):_wrapwidthpixels(345) end,
    DescriptionSetCommand = function(self,params) self:visible(params.Steps ~= nil and params.Steps:GetDifficulty()=="Difficulty_Edit") end,
}
"#).set_name("@theme:/metrics.ini:StepsDisplayGameplay").eval::<Table>()?;
    for index in 0..LUA_PLAYERS {
        let Some(display) = actor_children(lua, screen)?.raw_get::<Option<Table>>(format!("StepsDisplayP{}", index + 1))? else { continue; };
        for (name, x, y) in [("Frame", 0.0, 0.0), ("Meter", 30.0, 0.0),
            ("Description", -265.0, 0.0), ("Autogen", 40.0, 0.0)] {
            let child = actor_children(lua, &display)?.raw_get::<Table>(name)?;
            capture_block_set_f32(lua, &child, "x", x)?;
            capture_block_set_f32(lua, &child, "y", y)?;
            for command in ["Load", "On", "Set"] {
                if let Some(callback) = commands.raw_get::<Option<Function>>(format!("{name}{command}Command"))? {
                    child.raw_set(format!("{command}Command"), callback)?;
                }
            }
            run_actor_named_command(lua, &child, "OnCommand")?;
        }
        let player = Value::String(lua.create_string(player_number_name(index))?);
        let params = lua.create_table()?;
        params.raw_set("PlayerState", game.get::<Function>("GetPlayerState")?.call::<Table>((game.clone(), player.clone()))?)?;
        for child in actor_direct_children(lua, &display)? {
            run_actor_message_with_params(lua, &child, "LoadCommand", Some(Value::Table(params.clone())))?;
        }
        native_steps_display_visibility(lua, &display, false)?;
        // ScreenGameplay::LoadNextSong calls SetFromSteps on each display.
        let steps = game.get::<Function>("GetCurrentSteps")?.call::<Option<Table>>((game.clone(), player))?;
        if let Some(steps) = steps { native_steps_display_set(lua, &display, Some(steps), false)?; }
    }
    Ok(())
}

pub fn create_top_screen_theme_actor(
    lua: &Lua,
    parent: &Table,
    name: &str,
    create_dummy_actor: fn(&Lua, &'static str) -> mlua::Result<Table>,
    create_named_child_actor: fn(&Lua, &Table, &str) -> mlua::Result<Table>,
) -> mlua::Result<Option<Table>> {
    if name.eq_ignore_ascii_case("LyricDisplay") {
        return create_lyric_display_actor(lua, name, create_dummy_actor).map(Some);
    }
    if ["In", "Out", "Cancel"].iter().any(|known| name.eq_ignore_ascii_case(known)) {
        let actor = create_named_actor(lua, "ActorFrame", name, create_dummy_actor)?;
        actor.raw_set("__songlua_transition_state", 0_u8)?;
        if name.eq_ignore_ascii_case("In") {
            let child = create_named_actor(lua, "ActorFrame", "", create_dummy_actor)?;
            child.raw_set("__songlua_parent", actor.clone())?;
            actor.raw_set("__songlua_transition_child", child.clone())?;
            push_sequence_child_once(&actor, child.clone())?;
            // Simply Love's gameplay-in lead-in owns these actual queues.
            // On precedes StartTransitioning; the wrapper's own tweens
            // never contribute to its virtual GetTweenTimeLeft result.
            let visuals = create_named_actor(lua, "ActorFrame", "", create_dummy_actor)?;
            visuals.raw_set("__songlua_parent", child.clone())?;
            push_sequence_child_once(&child, visuals.clone())?;
            let reload = lua.globals().get::<Table>("SL")?.get::<Table>("Global")?
                .get::<bool>("GameplayReloadCheck")?;
            visuals.raw_set("HideCommand", lua.create_function(|lua, actor: Table| {
                actor.get::<Function>("visible")?.call::<()>((actor.clone(), false))?;
                lua.globals().get::<Table>("SL")?.get::<Table>("Global")?.raw_set("GameplayReloadCheck", true)
            })?)?;
            visuals.raw_set("OffCommand", lua.create_function(|lua, _: Table| {
                lua.globals().get::<Table>("SL")?.get::<Table>("Global")?.raw_set("GameplayReloadCheck", false)
            })?)?;
            if !reload {
                visuals.get::<Function>("sleep")?.call::<()>((visuals.clone(), 2.0))?;
                visuals.get::<Function>("queuecommand")?.call::<()>((visuals.clone(), "Hide"))?;
                let commands = lua.load(r#"
return {
    { InitCommand=function(self) self:diffuse(Color.Black):Center():FullScreen() end,
      OnCommand=function(self) self:sleep(1.4):accelerate(0.6):diffusealpha(0) end },
    { InitCommand=function(self) self:diffuse(GetCurrentColor(true)):Center():rotationz(10):zoom(0):diffusealpha(0.9) end,
      OnCommand=function(self) self:sleep(0.4):linear(0.6):rotationz(0):zoom(1.1):diffusealpha(0) end },
    { InitCommand=function(self) self:diffuse(GetCurrentColor(true)):Center():rotationy(180):rotationz(-10):zoom(0):diffusealpha(0.8) end,
      OnCommand=function(self) self:sleep(0.4):decelerate(0.6):rotationz(0):zoom(1.3):diffusealpha(0) end },
    { InitCommand=function(self) self:diffuse(GetCurrentColor(true)):Center():rotationz(10):zoom(0) end,
      OnCommand=function(self) self:sleep(0.4):decelerate(0.8):rotationz(0):zoom(0.9):diffusealpha(0) end },
}
"#).set_name("@theme:/BGAnimations/ScreenGameplay in/default.lua").eval::<Table>()?;
                let style = lua.globals().get::<Table>("ThemePrefs")?
                    .get::<Function>("Get")?.call::<String>("VisualStyle")?;
                let spooky = lua.globals().get::<Function>("IsSpooky")?.call::<bool>(())?;
                let color = lua.globals().get::<Function>("GetCurrentColor")?.call::<Table>(true)?;
                for index in 0..4 {
                    let sprite = create_named_actor(lua, if index == 0 { "Quad" } else { "Sprite" }, "", create_dummy_actor)?;
                    sprite.raw_set("__songlua_parent", visuals.clone())?;
                    let handlers = commands.raw_get::<Table>(index + 1)?;
                    for command in ["InitCommand", "OnCommand"] {
                        sprite.raw_set(command, handlers.raw_get::<Function>(command)?)?;
                    }
                    sprite.get::<Function>("Center")?.call::<()>(sprite.clone())?;
                    if index == 0 {
                        sprite.get::<Function>("FullScreen")?.call::<()>(sprite.clone())?;
                        sprite.get::<Function>("diffuse")?.call::<()>((sprite.clone(), 0.0, 0.0, 0.0, 1.0))?;
                        sprite.get::<Function>("sleep")?.call::<()>((sprite.clone(), 1.4))?;
                        sprite.get::<Function>("accelerate")?.call::<()>((sprite.clone(), 0.6))?;
                    } else {
                        let asset = if spooky { "_VisualStyles/Spooky/ExtraSpooky/Bats".to_string() }
                            else { format!("_VisualStyles/{style}/GameplayIn {}", if index == 3 { "minisplode" } else { "splode" }) };
                        set_actor_texture_from_path(&sprite, &load_native_gameplay_in_bitmap(&asset)?)?;
                        sprite.get::<Function>("diffuse")?.call::<()>((sprite.clone(), color.clone()))?;
                        sprite.get::<Function>("rotationz")?.call::<()>((sprite.clone(), if index == 2 { -10.0 } else { 10.0 }))?;
                        if index == 2 { sprite.get::<Function>("rotationy")?.call::<()>((sprite.clone(), 180.0))?; }
                        sprite.get::<Function>("zoom")?.call::<()>((sprite.clone(), 0.0))?;
                        if index != 3 { sprite.get::<Function>("diffusealpha")?.call::<()>((sprite.clone(), if index == 1 { 0.9 } else { 0.8 }))?; }
                        sprite.get::<Function>("sleep")?.call::<()>((sprite.clone(), 0.4))?;
                        sprite.get::<Function>(if index == 1 { "linear" } else { "decelerate" })?
                            .call::<()>((sprite.clone(), if index == 3 { 0.8 } else { 0.6 }))?;
                        sprite.get::<Function>("rotationz")?.call::<()>((sprite.clone(), 0.0))?;
                        sprite.get::<Function>("zoom")?.call::<()>((sprite.clone(), [0.0, 1.1, 1.3, 0.9][index]))?;
                    }
                    sprite.get::<Function>("diffusealpha")?.call::<()>((sprite.clone(), 0.0))?;
                    push_sequence_child_once(&visuals, sprite)?;
                }
                // ScreenGameplay in/default.lua labels the current stage even
                // after the lead-in is hidden. Song scripts can still read it.
                let stage_text = lua.load(r#"
if GAMESTATE:IsCourseMode() then
    local entries = GAMESTATE:GetCurrentCourse():GetCourseEntries()
    return ("%s 1 / %d"):format(THEME:GetString("Stage", "Stage"), #entries)
elseif GAMESTATE:IsEventMode() then
    return THEME:GetString("Stage", "Event")
else
    return THEME:GetString("Stage", "Stage") .. " " .. tostring(GAMESTATE:GetCurrentStageIndex() + 1)
end
"#).set_name("@theme:/BGAnimations/ScreenGameplay in/default.lua").eval::<String>()?;
                let text = create_named_text_actor(lua, "BitmapText", "", stage_text, create_dummy_actor)?;
                text.raw_set("__songlua_parent", child.clone())?;
                text.get::<Function>("accelerate")?.call::<()>((text.clone(), 0.5))?;
                text.get::<Function>("diffusealpha")?.call::<()>((text.clone(), 1.0))?;
                text.get::<Function>("sleep")?.call::<()>((text.clone(), 0.66))?;
                text.get::<Function>("accelerate")?.call::<()>((text.clone(), 0.33))?;
                push_sequence_child_once(&child, text)?;
            } else {
                visuals.get::<Function>("playcommand")?.call::<()>((visuals, "Hide"))?;
            }
        } else if name.eq_ignore_ascii_case("Out") {
            // Transition::Load adopts the returned Quad directly. Retain the
            // native Init/On/Off handlers rather than an extra frame owner.
            let quad = create_named_actor(lua, "Quad", "", create_dummy_actor)?;
            quad.raw_set("__songlua_parent", actor.clone())?;
            let commands = lua.load(r#"
return {
	InitCommand=function(self) self:FullScreen():diffuse(0,0,0,0) end,
	OnCommand=function(self) self:sleep(0.5):linear(1):diffusealpha(1) end,
	OffCommand=function(self)
		if SL.Global.GameMode == "ITG" then
			local song = GAMESTATE:GetCurrentSong()
			local totalWhites = 0
			for player in ivalues( GAMESTATE:GetHumanPlayers() ) do
				local pn = ToEnumShortString(player)
				local pss = STATSMAN:GetCurStageStats():GetPlayerStageStats(player)
				local number = pss:GetTapNoteScores("TapNoteScore_W1")
				local faPlus = SL[pn].Stages.Stats[SL.Global.Stages.PlayedThisGame + 1].ex_counts.W0_total
				-- Subtract FA+ count from the overall fantastic window count.
				local whites = number - faPlus
				totalWhites = totalWhites + whites
				-- This will save the white count to Stats.xml, so we can later recover
				-- it when we deprecate FA+ mode and introduce W0.
				--
				-- The Score field is completely unused in Simply Love, and the ability
				-- to set the field is exposed to lua so we can hijack it for our own\
				-- purposes.
				local bestWhites = whites
				if PROFILEMAN:IsPersistentProfile(pn) then
					local steps = GAMESTATE:GetCurrentSteps(pn)
					local scores = PROFILEMAN:GetProfile(pn):GetHighScoreList(song, steps):GetHighScores()
					for hs in ivalues(scores) do
						-- If the player previously quadded the song, retain the better white count.
						-- Technically this is a workaround because the date would be wrong, but
						-- it's still worth to keep the score around
						if (pss:GetPercentDancePoints() == hs:GetPercentDP() and hs:GetPercentDP() == 1.0) then
							bestWhites = math.min(bestWhites, hs:GetScore())
						end
					end
				end
				pss:SetScore(bestWhites)
			end
			if IsRoutine() then
				local routineStats = STATSMAN:GetCurStageStats():GetRoutineStageStats()
				local bestWhites = totalWhites
				if PROFILEMAN:IsPersistentProfile(GAMESTATE:GetMasterPlayerNumber()) then
					local steps = GAMESTATE:GetCurrentSteps(GAMESTATE:GetMasterPlayerNumber())
					local scores = PROFILEMAN:GetProfile(GAMESTATE:GetMasterPlayerNumber()):GetHighScoreList(song, steps):GetHighScores()
					for hs in ivalues(scores) do
						-- If the player previously quadded the song, retain the better white count.
						-- Technically this is a workaround because the date would be wrong, but
						-- it's still worth to keep the score around
						if (routineStats:GetPercentDancePoints() == hs:GetPercentDP() and hs:GetPercentDP() == 1.0) then
							bestWhites = math.min(bestWhites, hs:GetScore())
						end
					end
				end
				routineStats:SetScore(bestWhites)
			end
		end
	end
}"#).set_name("@theme:/BGAnimations/ScreenGameplay out.lua").eval::<Table>()?;
            for name in ["InitCommand", "OnCommand", "OffCommand"] {
                quad.raw_set(name, commands.raw_get::<Function>(name)?)?;
            }
            // Initialize before authored observation, as for the other native
            // actors. Keep the Lua handlers for subsequent command dispatch.
            quad.get::<Function>("FullScreen")?.call::<()>(quad.clone())?;
            quad.get::<Function>("diffuse")?.call::<()>((quad.clone(), 0.0, 0.0, 0.0, 0.0))?;
            quad.get::<Function>("sleep")?.call::<()>((quad.clone(), 0.5))?;
            quad.get::<Function>("linear")?.call::<()>((quad.clone(), 1.0))?;
            quad.get::<Function>("diffusealpha")?.call::<()>((quad.clone(), 1.0))?;
            actor.raw_set("__songlua_transition_child", quad.clone())?;
            push_sequence_child_once(&actor, quad)?;
        } else {
            // Transition::Load keeps the loaded actor as its child. Simply
            // Love's gameplay-cancel redirects to _fade out normal.lua.
            let quad = create_named_actor(lua, "Quad", "", create_dummy_actor)?;
            quad.raw_set("__songlua_parent", actor.clone())?;
            quad.get::<Function>("FullScreen")?.call::<()>(quad.clone())?;
            quad.get::<Function>("diffuse")?.call::<()>((quad.clone(), 0.0, 0.0, 0.0, 0.0))?;
            quad.raw_set("StartTransitioningCommand", lua.create_function(|_, quad: Table| {
                quad.get::<Function>("sleep")?.call::<()>((quad.clone(), 0.1))?;
                quad.get::<Function>("linear")?.call::<()>((quad.clone(), 0.4))?;
                quad.get::<Function>("diffusealpha")?.call::<()>((quad.clone(), 1.0))
            })?)?;
            actor.raw_set("__songlua_transition_child", quad.clone())?;
            push_sequence_child_once(&actor, quad)?;
        }
        return Ok(Some(actor));
    }
    if name.eq_ignore_ascii_case("Debug") {
        // ScreenGameplay::Init owns this BitmapText. Simply Love inherits
        // these Debug metrics from _fallback, including OnCommand's zoom.
        let actor = create_named_text_actor(lua, "BitmapText", name, String::new(), create_dummy_actor)?;
        for (method, value) in [
            ("x", lua.globals().get::<f32>("SCREEN_CENTER_X")?),
            ("y", lua.globals().get::<f32>("SCREEN_BOTTOM")? - 116.0),
            ("zoom", 0.75),
        ] {
            actor.get::<Function>(method)?.call::<()>((actor.clone(), value))?;
        }
        return Ok(Some(actor));
    }
    if name.eq_ignore_ascii_case("Underlay")
        || name.eq_ignore_ascii_case("Overlay")
        || name.eq_ignore_ascii_case("In")
        || name.eq_ignore_ascii_case("Out")
        || name.eq_ignore_ascii_case("Cancel")
        || name.eq_ignore_ascii_case("Header")
        || name.eq_ignore_ascii_case("Footer")
    {
        let actor = create_named_actor(lua, "ActorFrame", name, create_dummy_actor)?;
        if name.eq_ignore_ascii_case("Underlay") {
            copy_dummy_actor_tags(parent, &actor)?;
            actor.set("__songlua_top_screen_child_name", "Underlay")?;
            install_underlay_theme_children(lua, &actor, create_dummy_actor, create_named_child_actor)?;
        }
        return Ok(Some(actor));
    }
    if top_screen_steps_display_index(name).is_some() {
        return create_native_steps_display(lua, name, create_dummy_actor).map(Some);
    }
    if name.starts_with("StepsDescription") || name.starts_with("PlayerOptions")
        || name.starts_with("ActiveAttackList") || name == "SongOptions"
    {
        let actor = create_named_text_actor(lua, "BitmapText", name, String::new(), create_dummy_actor)?;
        // These engine displays are hidden by the inherited gameplay metrics.
        // StepsDescription additionally hibernates in Simply Love's On.
        actor.get::<Function>("visible")?.call::<()>((actor.clone(), false))?;
        return Ok(Some(actor));
    }
    if name.starts_with("OniGameOver") {
        // ScreenGameplay loads the inherited oni-gameover bitmap resource.
        let actor = create_named_actor(lua, "Sprite", name, create_dummy_actor)?;
        set_actor_texture_from_path(&actor, load_native_blank_texture()?)?;
        return Ok(Some(actor));
    }
    if name.eq_ignore_ascii_case("SongBackground") {
        return create_native_song_background(lua, create_dummy_actor).map(Some);
    }
    if name.eq_ignore_ascii_case("SongForeground") {
        return Ok(Some(create_named_actor(
            lua,
            "ActorFrame",
            name,
            create_dummy_actor,
        )?));
    }
    Ok(None)
}

fn create_native_song_background(
    lua: &Lua, create_dummy_actor: fn(&Lua, &'static str) -> mlua::Result<Table>,
) -> mlua::Result<Table> {
    let background = create_named_actor(lua, "ActorFrame", "SongBackground", create_dummy_actor)?;
    let implementation = create_named_actor(lua, "ActorFrame", "", create_dummy_actor)?;
    implementation.raw_set("__songlua_parent", background.clone())?;
    push_sequence_child_once(&background, implementation.clone())?;
    let width = lua.globals().get::<f32>("SCREEN_WIDTH")?;
    let height = lua.globals().get::<f32>("SCREEN_HEIGHT")?;
    // BackgroundImpl::Init owns four border Quads and BrightnessOverlay.
    // _fallback's Background metrics use the full logical screen rectangle.
    for (left, top, right, bottom) in [(0.0, 0.0, 0.0, height), (0.0, 0.0, width, 0.0),
        (width, 0.0, width, height), (0.0, height, width, height)] {
        let quad = create_named_actor(lua, "Quad", "", create_dummy_actor)?;
        quad.raw_set("__songlua_parent", implementation.clone())?;
        quad.get::<Function>("stretchto")?.call::<()>((quad.clone(), left, top, right, bottom))?;
        quad.get::<Function>("diffuse")?.call::<()>((quad.clone(), 0.0, 0.0, 0.0, 1.0))?;
        push_sequence_child_once(&implementation, quad)?;
    }
    let brightness = create_named_actor(lua, "ActorFrame", "", create_dummy_actor)?;
    brightness.raw_set("__songlua_parent", implementation.clone())?;
    push_sequence_child_once(&implementation, brightness.clone())?;
    // BrightnessOverlay in Background.cpp uses a 40-pixel center band.
    let side = width / 2.0 - 20.0;
    let prefs = lua.globals().raw_get::<Table>("PREFSMAN")?;
    let level = prefs.get::<Function>("GetPreference")?.call::<f32>((prefs, "BGBrightness"))?;
    for (left, right) in [(0.0, side), (width - side, width), (side, width - side)] {
        let quad = create_named_actor(lua, "Quad", "BrightnessOverlay", create_dummy_actor)?;
        quad.raw_set("__songlua_parent", brightness.clone())?;
        quad.get::<Function>("stretchto")?.call::<()>((quad.clone(), left, 0.0, right, height))?;
        quad.get::<Function>("diffuse")?.call::<()>((quad.clone(), 0.0, 0.0, 0.0, 1.0 - level))?;
        push_sequence_child_once(&brightness, quad)?;
    }
    native_step_statistics_lookup(lua, &background)?;
    Ok(background)
}

pub fn create_underlay_theme_actor(
    lua: &Lua,
    parent: &Table,
    name: &str,
    create_dummy_actor: fn(&Lua, &'static str) -> mlua::Result<Table>,
) -> mlua::Result<Option<Table>> {
    if let Some(player_index) = underlay_score_index(name) {
        return Ok(Some(create_named_text_actor(
            lua,
            "BitmapText",
            underlay_score_name(player_index),
            "0.00%".to_string(),
            create_dummy_actor,
        )?));
    }
    if name.eq_ignore_ascii_case("Header") {
        let actor = create_named_actor(lua, "Quad", "Header", create_dummy_actor)?;
        actor.raw_set("__songlua_parent", parent.clone())?;
        let commands = lua.load(r#"return {
	Name="Header",
	InitCommand=function(self)
		self:diffuse(0,0,0,0.85):valign(0):xy( _screen.cx, 0 )
		self:zoomtowidth(_screen.w):zoomtoheight(80)
	end
}"#)
            .set_name("@theme:/BGAnimations/ScreenGameplay underlay/Shared/Header.lua")
            .eval::<Table>()?;
        let init = commands.raw_get::<Function>("InitCommand")?;
        actor.raw_set("InitCommand", init.clone())?;
        init.call::<()>(actor.clone())?;
        return Ok(Some(actor));
    }
    Ok(None)
}

pub fn create_screen_timer_actor(
    lua: &Lua,
    create_dummy_actor: fn(&Lua, &'static str) -> mlua::Result<Table>,
) -> mlua::Result<Table> {
    let actor = create_dummy_actor(lua, "Timer")?;
    actor.set(
        "GetSeconds",
        lua.create_function(|_, _args: IgnoredArgs| Ok(0.0_f32))?,
    )?;
    Ok(actor)
}

pub fn create_top_screen_score_actor(
    lua: &Lua,
    player_index: usize,
    create_dummy_actor: fn(&Lua, &'static str) -> mlua::Result<Table>,
) -> mlua::Result<Table> {
    let actor = create_dummy_actor(lua, "ActorFrame")?;
    actor.set("Name", top_screen_score_name(player_index))?;
    // ScoreDisplayPercentage loads its unnamed frame before the percentage child.
    let frame = create_dummy_actor(lua, "Sprite")?;
    set_actor_texture_from_path(&frame, load_native_blank_texture()?)?;
    frame.set("__songlua_parent", actor.clone())?;
    actor_children(lua, &actor)?.set("", frame)?;
    let percent = create_score_display_percent_actor(lua, player_index, create_dummy_actor)?;
    percent.set("__songlua_parent", actor.clone())?;
    actor_children(lua, &actor)?.set("ScoreDisplayPercentage Percent", percent)?;
    Ok(actor)
}

pub fn create_score_display_percent_actor(
    lua: &Lua,
    player_index: usize,
    create_dummy_actor: fn(&Lua, &'static str) -> mlua::Result<Table>,
) -> mlua::Result<Table> {
    let actor = create_dummy_actor(lua, "PercentageDisplay")?;
    actor.set("Name", "ScoreDisplayPercentage Percent")?;
    let text = create_score_percent_text_actor(lua, player_index, create_dummy_actor)?;
    text.set("__songlua_parent", actor.clone())?;
    actor_children(lua, &actor)?.set(top_screen_score_percent_name(player_index), text)?;
    Ok(actor)
}

pub fn create_score_percent_text_actor(
    lua: &Lua,
    player_index: usize,
    create_dummy_actor: fn(&Lua, &'static str) -> mlua::Result<Table>,
) -> mlua::Result<Table> {
    let actor = create_dummy_actor(lua, "BitmapText")?;
    actor.set("Name", top_screen_score_percent_name(player_index))?;
    actor.set("Text", "0.00%")?;
    Ok(actor)
}

pub fn create_option_row_table(
    lua: &Lua,
    name: &str,
    create_dummy_actor: fn(&Lua, &'static str) -> mlua::Result<Table>,
) -> mlua::Result<Table> {
    let row = create_dummy_actor(lua, "OptionRow")?;
    row.set("Name", name)?;
    set_string_method(lua, &row, "GetName", name)?;
    let frame = create_option_row_frame(lua, name, create_dummy_actor)?;
    frame.set("__songlua_parent", row.clone())?;
    actor_children(lua, &row)?.set("", frame)?;
    row.set(
        "GetChoiceInRowWithFocus",
        lua.create_function(|_, _args: IgnoredArgs| Ok(1_i64))?,
    )?;
    Ok(row)
}

fn create_option_row_frame(
    lua: &Lua,
    row_name: &str,
    create_dummy_actor: fn(&Lua, &'static str) -> mlua::Result<Table>,
) -> mlua::Result<Table> {
    let frame = create_dummy_actor(lua, "OptionRowFrame")?;
    let title = create_option_row_text_actor(lua, "Title", row_name, create_dummy_actor)?;
    title.set("__songlua_parent", frame.clone())?;
    actor_children(lua, &frame)?.set("Title", title)?;

    let items = create_actor_child_group(lua)?;
    let item_text = option_row_default_text(row_name);
    for index in 1..=LUA_PLAYERS {
        let item = create_option_row_text_actor(lua, "Item", &item_text, create_dummy_actor)?;
        item.set("__songlua_parent", frame.clone())?;
        items.raw_set(index, item)?;
    }
    actor_children(lua, &frame)?.set("Item", items)?;
    Ok(frame)
}

fn create_option_row_text_actor(
    lua: &Lua,
    name: &str,
    text: &str,
    create_dummy_actor: fn(&Lua, &'static str) -> mlua::Result<Table>,
) -> mlua::Result<Table> {
    let actor = create_dummy_actor(lua, "BitmapText")?;
    actor.set("Name", name)?;
    actor.set("Text", text)?;
    Ok(actor)
}

pub fn create_top_screen_player_actor(
    lua: &Lua,
    player: SongLuaPlayerContext,
    player_index: usize,
    style_name: &str,
    create_dummy_actor: fn(&Lua, &'static str) -> mlua::Result<Table>,
) -> mlua::Result<Table> {
    let actor = create_dummy_actor(lua, "PlayerActor")?;
    actor.set("Name", top_screen_player_name(player_index))?;
    actor.set("__songlua_player_index", player_index as i64)?;
    actor.set("__songlua_style_name", style_name)?;
    actor.set("__songlua_visible", true)?;
    actor.set("get_oitg_zoom_mode", lua.create_function(|_, actor: Table| {
        Ok(actor.raw_get::<Option<bool>>("__songlua_oitg_zoom_mode")?.unwrap_or(false))
    })?)?;
    actor.set("set_oitg_zoom_mode", lua.create_function(|_, (actor, value): (Table, Value)| {
        actor.raw_set("__songlua_oitg_zoom_mode", truthy(&value))?;
        Ok(actor)
    })?)?;
    // Player constructs this unnamed ActorFrame before Init adds its visible
    // parts. AttackDisplay has no child Sprite in the host's regular play mode.
    let attack = create_named_actor(lua, "ActorFrame", "", create_dummy_actor)?;
    attack.raw_set("__songlua_parent", actor.clone())?;
    // The inherited one-side and both-side X offsets are all zero; Player::Load
    // subtracts40. Native leaves Y at its constructor default.
    capture_block_set_f32(lua, &attack, "x", -40.0)?;
    push_sequence_child_once(&actor, attack.clone())?;
    actor_children(lua, &actor)?.set("", attack.clone())?;
    actor.raw_set("__songlua_attack_display", attack)?;
    // The dummy has already registered its live tween pose. Native SetX and
    // PlayerInitCommand initialize that current pose as well as the destination.
    capture_block_set_f32(lua, &actor, "x", player.screen_x)?;
    capture_block_set_f32(lua, &actor, "y", player.screen_y)?;
    if let Some(path) = &player.judgment_texture {
        actor.set("__songlua_judgment_texture", file_path_string(path))?;
    }
    actor.set("SetLife", lua.create_function(move |lua, (actor, life): (Table, f32)| {
        if actor.raw_get::<Option<bool>>("__songlua_inside_set_life")?.unwrap_or(false) {
            return Err(mlua::Error::runtime("Do not call SetLife from inside LifeChangedMessageCommand because SetLife causes a LifeChangedMessageCommand."));
        }
        let life = life.clamp(0.0, 1.0);
        actor.raw_set("__songlua_inside_set_life", true)?;
        let result = (|| {
            crate::live::set_player_life(lua, player_index, life)?;
            let meter: Table = actor.raw_get("__songlua_life_meter")?;
            meter.raw_set("__songlua_life", life)?;
            let params = lua.create_table()?;
            params.raw_set("Player", format!("PlayerNumber_P{}", player_index + 1))?;
            params.raw_set("LifeMeter", meter)?;
            broadcast_song_lua_message(lua, "LifeChanged", Some(Value::Table(params)))
        })();
        actor.raw_set("__songlua_inside_set_life", false)?;
        result?;
        note_song_lua_side_effect(lua)?;
        Ok(actor)
    })?)?;
    Ok(actor)
}

pub fn install_top_screen_theme_children(
    lua: &Lua,
    top_screen: &Table,
    players: &[SongLuaPlayerContext; LUA_PLAYERS],
    create_named_child_actor: fn(&Lua, &Table, &str) -> mlua::Result<Table>,
) -> mlua::Result<()> {
    let children = actor_children(lua, top_screen)?;
    for &name in TOP_SCREEN_THEME_CHILD_NAMES {
        // ScreenGameplay::Init creates score and difficulty displays only
        // for enabled players; Simply Love hibernates those engine actors.
        let player = top_screen_score_index(name).or_else(|| top_screen_steps_display_index(name))
            .or_else(|| ["StepsDescription", "PlayerOptions", "OniGameOver"].iter()
                .find_map(|prefix| name.strip_prefix(prefix)
                    .and_then(|side| match side { "P1" => Some(0), "P2" => Some(1), _ => None })));
        if player.is_some_and(|index| !players[index].enabled) {
            continue;
        }
        let child = create_named_child_actor(lua, top_screen, name)?;
        if top_screen_score_index(name).is_some() || top_screen_steps_display_index(name).is_some()
            || name.starts_with("StepsDescription") || matches!(name, "Header" | "Footer") {
            child.raw_set("__songlua_theme_hibernating", true)?;
        }
        if let Some(index) = top_screen_steps_display_index(name) {
            child.raw_set("__songlua_preferred_difficulty", players[index].difficulty.sm_name())?;
        }
        children.set(name, child)?;
    }
    // ActorFrame traverses its ordered actor tree. Named lookup aliases must
    // not let Lua hash order place foreground Off before the theme overlay.
    for name in ["SongBackground", "Underlay", "PlayerP1", "PlayerP2", "LifeP1", "LifeP2",
        "ScoreP1", "ScoreP2", "StepsDisplayP1", "StepsDisplayP2",
        "Overlay", "SongForeground", "In", "Debug"] {
        if let Some(child) = children.raw_get::<Option<Table>>(name)? {
            push_sequence_child_once(top_screen, child)?;
        }
    }
    for &name in TOP_SCREEN_THEME_CHILD_NAMES {
        if let Some(child) = children.raw_get::<Option<Table>>(name)? {
            push_sequence_child_once(top_screen, child)?;
        }
    }
    Ok(())
}

fn create_underlay_background_filter(
    lua: &Lua, underlay: &Table, player: usize,
    create_dummy_actor: fn(&Lua, &'static str) -> mlua::Result<Table>,
) -> mlua::Result<Option<Table>> {
    let commands = lua.load(r#"
local player = ...
local pn = ToEnumShortString(player)
local mods = SL[pn].ActiveModifiers

-- if no BackgroundFilter is necessary, it's safe to bail now
if mods.BackgroundFilter == "Off" then return end
if IsRoutine() and player ~= GAMESTATE:GetMasterPlayerNumber() then return end

local FilterAlpha = BackgroundFilterValues()
-- Native named choices are unchanged; the production profile also supports percentages.
if type(mods.BackgroundFilter) == "number" then FilterAlpha[mods.BackgroundFilter] = mods.BackgroundFilter end
return {
	InitCommand=function(self)
		self:xy(GetNotefieldX(player), _screen.cy )
			:diffuse(Color.Black)
			:diffusealpha( FilterAlpha[mods.BackgroundFilter]/100 or 0 )
			:zoomto( GetNotefieldWidth(), _screen.h )
	end,
	OffCommand=function(self) self:queuecommand("ComboFlash") end,
	ComboFlashCommand=function(self)
		local pss = STATSMAN:GetCurStageStats():GetPlayerStageStats(player)
		local FlashColor = nil
		local WorstAcceptableFC = SL.Preferences[SL.Global.GameMode].MinTNSToHideNotes:gsub("TapNoteScore_W", "")

		for i=1, tonumber(WorstAcceptableFC) do
			if pss:FullComboOfScore("TapNoteScore_W"..i) then
				FlashColor = SL.JudgmentColors[SL.Global.GameMode][i]
				break
			end
		end

		if (FlashColor ~= nil) then
			self:accelerate(0.25):diffuse( FlashColor )
				:accelerate(0.5):faderight(1):fadeleft(1)
				:accelerate(0.15):diffusealpha(0)
		end
	end
}
"#).set_name("@theme:/BGAnimations/ScreenGameplay underlay/PerPlayer/BackgroundFilter.lua")
        .call::<Option<Table>>(player_number_name(player))?;
    let Some(commands) = commands else { return Ok(None); };
    let sl = lua.globals().get::<Table>("SL")?;
    let mods = sl.get::<Table>(format!("P{}", player + 1))?.get::<Table>("ActiveModifiers")?;
    let filter = mods.get::<Value>("BackgroundFilter")?;
    let alpha = match filter {
        Value::Integer(value) => value as f32 / 100.0,
        Value::Number(value) => value as f32 / 100.0,
        Value::String(name) => {
            let values = lua.globals().get::<Function>("BackgroundFilterValues")?.call::<Table>(())?;
            values.get::<f32>(name)? / 100.0
        }
        _ => return Err(mlua::Error::runtime("invalid initial BackgroundFilter")),
    };
    let actor = create_named_actor(lua, "Quad", "", create_dummy_actor)?;
    actor.raw_set("__songlua_parent", underlay.clone())?;
    for command in ["InitCommand", "OffCommand", "ComboFlashCommand"] {
        actor.raw_set(command, commands.raw_get::<Function>(command)?)?;
    }
    // Constructor initialization occurs before authored observation.
    let x = lua.globals().get::<Function>("GetNotefieldX")?.call::<f32>(player_number_name(player))?;
    let width = lua.globals().get::<Function>("GetNotefieldWidth")?.call::<f32>(())?;
    let height = lua.globals().get::<f32>("SCREEN_HEIGHT")?;
    actor.get::<Function>("xy")?.call::<()>((actor.clone(), x, height * 0.5))?;
    actor.get::<Function>("diffuse")?.call::<()>((actor.clone(), 0.0, 0.0, 0.0, alpha))?;
    actor.get::<Function>("zoomto")?.call::<()>((actor.clone(), width, height))?;
    Ok(Some(actor))
}

fn create_underlay_difficulty_meter(
    lua: &Lua, underlay: &Table, player: usize,
    create_dummy_actor: fn(&Lua, &'static str) -> mlua::Result<Table>,
) -> mlua::Result<Table> {
    let commands = lua.load(r#"
local player = ...
local pn = ToEnumShortString(player)

local _x = _screen.cx + (player==PLAYER_1 and -1 or 1) * SL_WideScale(292.5, 342.5)

return {
	InitCommand=function(self)
		local adjusted_offset_x = SL[pn].ActiveModifiers.NoteFieldOffsetX * (player == PLAYER_1 and -1 or 1)
		self:xy(_x + adjusted_offset_x, 56)
	end,


	-- colored background for player's chart's difficulty meter
	{
		InitCommand=function(self)
			self:zoomto(30, 30)
		end,
		CurrentSongChangedMessageCommand=function(self) self:queuecommand("Begin") end,
		BeginCommand=function(self)
			local currentSteps = GAMESTATE:GetCurrentSteps(player)
			if currentSteps then
				local currentDifficulty = currentSteps:GetDifficulty()
				self:diffuse(DifficultyColor(currentDifficulty))
			end
		end
	},

	-- player's chart's difficulty meter
	{
		InitCommand=function(self)
			self:diffuse( Color.Black )
			self:zoom( 0.4 )
		end,
		CurrentSongChangedMessageCommand=function(self) self:queuecommand("Begin") end,
		BeginCommand=function(self)
			local steps = GAMESTATE:GetCurrentSteps(player)
			local meter = steps:GetMeter()

			if meter then
				self:settext(meter)
			end
		end
	}
}"#).set_name("@theme:/BGAnimations/ScreenGameplay underlay/PerPlayer/DifficultyMeter.lua")
        .call::<Table>(player_number_name(player))?;
    let frame = create_named_actor(lua, "ActorFrame", "", create_dummy_actor)?;
    frame.raw_set("__songlua_parent", underlay.clone())?;
    frame.raw_set("InitCommand", commands.raw_get::<Function>("InitCommand")?)?;
    // Native LoadActor runs Init before Screen::BeginScreen propagates Begin.
    commands.raw_get::<Function>("InitCommand")?.call::<()>(frame.clone())?;
    let quad = create_named_actor(lua, "Quad", "", create_dummy_actor)?;
    let text = create_named_text_actor(lua, "BitmapText", "", String::new(), create_dummy_actor)?;
    text.raw_set("Font", "Common Bold")?;
    let unnamed = create_actor_child_group(lua)?;
    for (index, actor) in [(1, quad.clone()), (2, text.clone())] {
        let part = commands.raw_get::<Table>(index)?;
        for command in ["InitCommand", "BeginCommand", "CurrentSongChangedMessageCommand"] {
            actor.raw_set(command, part.raw_get::<Function>(command)?)?;
        }
        actor.raw_set("__songlua_parent", frame.clone())?;
        part.raw_get::<Function>("InitCommand")?.call::<()>(actor.clone())?;
        push_sequence_child_once(&frame, actor.clone())?;
        unnamed.raw_set(index, actor)?;
    }
    actor_children(lua, &frame)?.raw_set("", unnamed)?;
    // Initialize from actual selected Steps, before authored query observation.
    // Calling the public command dispatcher here would record extra song events.
    let game = lua.globals().get::<Table>("GAMESTATE")?;
    if let Some(steps) = game.get::<Function>("GetCurrentSteps")?
        .call::<Option<Table>>((game, player_number_name(player)))?
    {
        let difficulty = steps.get::<Function>("GetDifficulty")?.call::<String>(steps.clone())?;
        let color = lua.globals().get::<Function>("DifficultyColor")?.call::<Table>(difficulty)?;
        quad.get::<Function>("diffuse")?.call::<()>((quad.clone(), color))?;
        let meter = steps.get::<Function>("GetMeter")?.call::<u32>(steps)?;
        text.get::<Function>("settext")?.call::<()>((text.clone(), meter))?;
    }
    Ok(frame)
}

fn load_native_standard_life_swoosh() -> mlua::Result<String> {
    static PREPARED: OnceLock<Result<String, String>> = OnceLock::new();
    PREPARED.get_or_init(|| {
        let bytes = include_bytes!("../assets/simply-love/BGAnimations/ScreenGameplay underlay/PerPlayer/LifeMeter/swoosh.png");
        let decoded = image::load_from_memory_with_format(bytes, image::ImageFormat::Png)
            .map_err(|error| error.to_string())?;
        let mut image = decoded.into_rgba8();
        let source_size = [image.width(), image.height()];
        let hints = deadlib_assets::parse_texture_hints("swoosh.png");
        deadlib_assets::apply_texture_hints(&mut image, &hints);
        deadlib_assets::fix_hidden_alpha(&mut image);
        let path = format!("{}B/ScreenGameplay underlay/PerPlayer/LifeMeter/swoosh.png", crate::SONG_LUA_THEME_PATH_PREFIX);
        deadlib_assets::register_prepared_generated_texture(&path, image, source_size, hints.sampler_desc());
        Ok(path)
    }).as_ref().cloned().map_err(|error| mlua::Error::external(error.clone()))
}

fn create_underlay_standard_life_meter(
    lua: &Lua, underlay: &Table, player: usize,
    create_dummy_actor: fn(&Lua, &'static str) -> mlua::Result<Table>,
) -> mlua::Result<Option<Table>> {
    // LifeMeter/default.lua selects the actual per-player variant. Other
    // variants retain their missing implementation; never stand in for them.
    let sl = lua.globals().get::<Table>("SL")?;
    let mods = sl.get::<Table>(format!("P{}", player + 1))?.get::<Table>("ActiveModifiers")?;
    if mods.get::<bool>("HideLifebar")?
        || sl.get::<Table>("Global")?.get::<String>("GameMode")? != "ITG"
        || mods.get::<String>("LifeMeterType")? != "Standard" { return Ok(None); }
    let game = lua.globals().get::<Table>("GAMESTATE")?;
    let state = game.get::<Function>("GetPlayerState")?.call::<Table>((game, player_number_name(player)))?;
    let position = state.get::<Function>("GetSongPosition")?.call::<Table>(state)?;
    let commands = lua.load(r#"
local player = ...

local w = 136
local h = 18
local _x = _screen.cx + (player==PLAYER_1 and -1 or 1) * SL_WideScale(238, 288)

-- get SongPosition specific to this player so that
-- split BPMs are handled if there are any
local songposition = select(2, ...)
local swoosh, velocity

local Update = function(self)
	if not swoosh then return end
	velocity = -(songposition:GetCurBPS() * 0.5)
	if songposition:GetFreeze() or songposition:GetDelay() then velocity = 0 end
	swoosh:texcoordvelocity(velocity,0)
end

local meter = {

	InitCommand=function(self) self:y(20):SetUpdateFunction(Update):visible(false) end,
	OnCommand=function(self) self:visible(true) end,

	-- frame
	{ InitCommand=function(self) self:x(_x):zoomto(w+4, h+4) end },
	{ InitCommand=function(self) self:x(_x):zoomto(w, h):diffuse(0,0,0,1) end },

	-- the Quad that changes width/color depending on current Life
	{
		Name="MeterFill",
		InitCommand=function(self) self:zoomto(0,h):diffuse(PlayerColor(player,true)):horizalign(left) end,
		OnCommand=function(self) self:x( _x - w/2 ) end,

		-- check whether the player's LifeMeter is "Hot"
		-- in LifeMeterBar.cpp, the engine says a LifeMeter is Hot if the current
		-- LifePercentage is greater than or equal to the HOT_VALUE, which is
		-- defined in Metrics.ini under [LifeMeterBar] like HotValue=1.0
		HealthStateChangedMessageCommand=function(self,params)
			if params.PlayerNumber == player then
				if params.HealthState == 'HealthState_Hot' then
					self:diffuse(1,1,1,1)
				else
					-- ~~man's~~ lifebar's not hot
					self:diffuse( PlayerColor(player,true) )
				end
			end
		end,

		-- when the engine broadcasts that the player's LifeMeter value has changed
		-- change the width of this MeterFill Quad to accommodate
		LifeChangedMessageCommand=function(self,params)
			if params.Player == player then
				local life = params.LifeMeter:GetLife() * w
				self:finishtweening()
				self:bouncebegin(0.1):zoomx( life )
			end
		end,
	},

	-- a simple scrolling gradient texture applied on top of MeterFill
	{
		Name="MeterSwoosh",
		InitCommand=function(self)
			swoosh = self

			self:zoomto(w,h)
				 :diffusealpha(0.2)
				 :horizalign( left )
		end,
		OnCommand=function(self)
			self:x(_x - w/2)
			self:customtexturerect(0,0,1,1)
			--texcoordvelocity is handled by the Update function below
		end,
		HealthStateChangedMessageCommand=function(self,params)
			if(params.PlayerNumber == player) then
				if(params.HealthState == 'HealthState_Hot') then
					self:diffusealpha(1)
				else
					self:diffusealpha(0.2)
				end
			end
		end,

		-- life-changing
		-- adjective
		--  /ˈlaɪfˌtʃeɪn.dʒɪŋ/
		-- having an effect that is strong enough to change someone's life
		-- synonyms: compelling, life-altering, puissant, blazing
		LifeChangedMessageCommand=function(self,params)
			if params.Player == player then
				local life = params.LifeMeter:GetLife() * w
				self:finishtweening()
				self:bouncebegin(0.1):zoomto( life, h )
			end
		end
	}
}

return meter

-- copyright 2008-2012 AJ Kelly/freem.
-- do not use this code in your own themes without my permission."#).set_name("@theme:/BGAnimations/ScreenGameplay underlay/PerPlayer/LifeMeter/Standard.lua")
        .call::<Table>((player_number_name(player), position))?;
    let meter = create_named_actor(lua, "ActorFrame", "", create_dummy_actor)?;
    meter.raw_set("__songlua_parent", underlay.clone())?;
    for command in ["InitCommand", "OnCommand"] {
        meter.raw_set(command, commands.raw_get::<Function>(command)?)?;
    }
    commands.raw_get::<Function>("InitCommand")?.call::<()>(meter.clone())?;
    let unnamed = create_actor_child_group(lua)?;
    for (index, kind, name) in [(1, "Quad", ""), (2, "Quad", ""),
        (3, "Quad", "MeterFill"), (4, "Sprite", "MeterSwoosh")] {
        let actor = create_named_actor(lua, kind, name, create_dummy_actor)?;
        actor.raw_set("__songlua_parent", meter.clone())?;
        if kind == "Sprite" { set_actor_texture_from_path(&actor, &load_native_standard_life_swoosh()?)?; }
        let definition = commands.raw_get::<Table>(index)?;
        for command in ["InitCommand", "OnCommand", "LifeChangedMessageCommand", "HealthStateChangedMessageCommand"] {
            if let Some(callback) = definition.raw_get::<Option<Function>>(command)? {
                actor.raw_set(command, callback)?;
            }
        }
        definition.raw_get::<Function>("InitCommand")?.call::<()>(actor.clone())?;
        if let Some(on) = definition.raw_get::<Option<Function>>("OnCommand")? { on.call::<()>(actor.clone())?; }
        push_sequence_child_once(&meter, actor.clone())?;
        if name.is_empty() { unnamed.raw_set(unnamed.raw_len() + 1, actor)?; }
        else { actor_children(lua, &meter)?.raw_set(name, actor)?; }
    }
    actor_children(lua, &meter)?.raw_set("", unnamed)?;
    meter.raw_set("__songlua_owned_child_lookup", true)?;
    commands.raw_get::<Function>("OnCommand")?.call::<()>(meter.clone())?;
    Ok(Some(meter))
}

fn create_shared_bpm_node(
    lua: &Lua, parent: &Table, definition: Table,
    create_dummy_actor: fn(&Lua, &'static str) -> mlua::Result<Table>,
) -> mlua::Result<Table> {
    let kind = definition.raw_get::<String>("__songlua_builtin_kind")?;
    let name = definition.raw_get::<Option<String>>("Name")?.unwrap_or_default();
    let actor = if kind == "BitmapText" {
        let text = definition.raw_get::<Option<String>>("Text")?.unwrap_or_default();
        let actor = create_named_text_actor(lua, "BitmapText", &name, text, create_dummy_actor)?;
        actor.raw_set("Font", definition.raw_get::<String>("Font")?)?;
        actor
    } else { create_named_actor(lua, "ActorFrame", &name, create_dummy_actor)? };
    actor.raw_set("__songlua_parent", parent.clone())?;
    definition.for_each::<Value, Value>(|key, value| {
        if let (Value::String(key), Value::Function(value)) = (key, value)
            && key.to_str()?.ends_with("Command") { actor.raw_set(key, value)?; }
        Ok(())
    })?;
    if let Some(init) = definition.raw_get::<Option<Function>>("InitCommand")? {
        init.call::<()>(actor.clone())?;
    }
    let unnamed = create_actor_child_group(lua)?;
    for child in definition.sequence_values::<Table>() {
        let child = create_shared_bpm_node(lua, &actor, child?, create_dummy_actor)?;
        let name = actor_name(&child)?;
        push_sequence_child_once(&actor, child.clone())?;
        if name.is_empty() { unnamed.raw_set(unnamed.raw_len() + 1, child)?; }
        else { actor_children(lua, &actor)?.set(name, child)?; }
    }
    match unnamed.raw_len() {
        0 => (),
        1 => actor_children(lua, &actor)?.raw_set("", unnamed.raw_get::<Table>(1)?)?,
        _ => actor_children(lua, &actor)?.raw_set("", unnamed)?,
    }
    if kind == "ActorFrame" { actor.raw_set("__songlua_owned_child_lookup", true)?; }
    Ok(actor)
}

fn create_underlay_shared_bpm(
    lua: &Lua, underlay: &Table,
    create_dummy_actor: fn(&Lua, &'static str) -> mlua::Result<Table>,
) -> mlua::Result<Table> {
    // Keep the native Lua selectors, Init handlers and update closures intact.
    // Node/Text replace only Def/LoadFont construction before Def is installed.
    let definition = lua.load(r#"local function Node(t) t.__songlua_builtin_kind='ActorFrame';return t end;local function Text(t) t.__songlua_builtin_kind='BitmapText';t.Font='Common Normal';return t end;local Players = GAMESTATE:GetHumanPlayers()
local MusicRate = SL.Global.ActiveModifiers.MusicRate

local MasterPlayerState = GAMESTATE:GetPlayerState(GAMESTATE:GetMasterPlayerNumber())
local so = GAMESTATE:GetSongOptionsObject("ModsLevel_Song")

local bpmDisplay, SongPosition

-- -----------------------------------------------------------------------

local xRateText = THEME:GetString("OptionTitles", "MusicRateMultiplier")

-- the update function when a single BPM Display is in use
local UpdateSingleBPM = function(af)
	-- BPM stuff first
	SongPosition = MasterPlayerState:GetSongPosition()

	-- then, MusicRate stuff
	MusicRate = so:MusicRate()

	-- BPM Display
	bpmDisplay:settext( round(SongPosition:GetCurBPS() * 60 * MusicRate) )

	-- MusicRate Display
	MusicRate = string.format("%.2f", MusicRate )
	MusicRateDisplay:settext( MusicRate ~= "1.00" and MusicRate..xRateText or "" )
end

-- the update function when two BPM Displays are needed for divergent TimingData (split BPMs)
local Update2PBPM = function(self)
	MusicRate = so:MusicRate()

	-- need current bpm for p1 and p2
	for player in ivalues(Players) do
		bpmDisplay = (player == PLAYER_1) and dispP1 or dispP2
		SongPosition = GAMESTATE:GetPlayerState(player):GetSongPosition()
		bpmDisplay:settext( round( SongPosition:GetCurBPS() * 60 * MusicRate ) )
	end

	MusicRate = string.format("%.2f", MusicRate )
	MusicRateDisplay:settext( MusicRate ~= "1.00" and MusicRate..xRateText or "" )
end



local SingleBPMDisplay = function()
	return Node{
		InitCommand=function(self) self:SetUpdateFunction(UpdateSingleBPM) end,

		Text{
			Name="BPMDisplay",
			InitCommand=function(self)
				self:zoom(1)
				bpmDisplay = self
			end
		}
	}
end

local DualBPMDisplay = function()
	return Node{
		InitCommand=function(self) self:SetUpdateFunction(Update2PBPM) end,

		Text{
			Name="DisplayP1",
			InitCommand=function(self)
				self:x(-18):zoom(1):shadowlength(1)
				dispP1 = self
			end
		},
		Text{
			Name="DisplayP2",
			InitCommand=function(self)
				self:x(18):zoom(1):shadowlength(1)
				dispP2 = self
			end
		}
	}
end

-- -----------------------------------------------------------------------

local t = Node{
	InitCommand=function(self)
		self:xy(_screen.cx, 52):valign(1):zoom(1.33)

		local styletype = ToEnumShortString(GAMESTATE:GetCurrentStyle():GetStyleType())
		local mpn = GAMESTATE:GetMasterPlayerNumber()

		-- if only one player is joined, and their notefield is centered, and they are using UpperNPSGraph
		-- push this BPM bmt off to the side; the UpperNPSGraph will take priority in the center
		if #GAMESTATE:GetHumanPlayers() == 1
		and GetNotefieldX(mpn) == _screen.cx
		and SL[ToEnumShortString(mpn)].ActiveModifiers.NPSGraphAtTop
		then
			self:x(_screen.cx + _screen.w * (mpn==PLAYER_1 and 0.3 or -0.3))
		end
	end,

	Text{
		Name="RatemodDisplay",
		Text=MusicRate ~= 1 and MusicRate.."x rate" or "",
		InitCommand=function(self)
			self:zoom(0.5):y(12)
			MusicRateDisplay = self
		end
	}
}


if #Players == 1 then
	t[#t+1] = SingleBPMDisplay()
end


if #Players == 2 then

	if not GAMESTATE:IsCourseMode() then
		-- check if both players are playing the same steps
		local StepsP1 = GAMESTATE:GetCurrentSteps(PLAYER_1)
		local StepsP2 = GAMESTATE:GetCurrentSteps(PLAYER_2)

		-- get timing data...
		local TimingDataP1 = StepsP1:GetTimingData()
		local TimingDataP2 = StepsP2:GetTimingData()

		local dispP1, dispP2

		if TimingDataP1 == TimingDataP2 then
			-- both players have the same TimingData; only need one BPM Display.
			t[#t+1] = SingleBPMDisplay()
		else
			t[#t+1] = DualBPMDisplay()
		end

	-- if we ARE in CourseMode
	else
		local TrailP1 = GAMESTATE:GetCurrentTrail(PLAYER_1)
		local TrailP2 = GAMESTATE:GetCurrentTrail(PLAYER_2)

		if TrailP1 == TrailP2 then
			-- both players have the same trail; only need one BPM Display.
			t[#t+1] = SingleBPMDisplay()
		else
			-- Two different Trails may effectively share the the same TimingData for each of their TrailEntries,
			-- but this is not guaranteed.  A single song within the course may feature split BPMs, for example.
			-- So, loop through the TrailEntries of both and compare the TimingData of each.
			-- If there is even one discrepancy, break from the loop and use a DualBPMDisplay.
			local TrailEntriesP1 = TrailP1:GetTrailEntries()
			local TrailEntriesP2 = TrailP2:GetTrailEntries()
			local DivergentTimingData = false

			for i=1, #TrailEntriesP1 do
				if TrailEntriesP1[i]:GetSteps():GetTimingData() ~= TrailEntriesP2[i]:GetSteps():GetTimingData() then
					DivergentTimingData = true
					break
				end
			end

			t[#t+1] = DivergentTimingData and DualBPMDisplay() or SingleBPMDisplay()
		end
	end
end

return t
"#)
        .set_name("@theme:/BGAnimations/ScreenGameplay underlay/Shared/BPMDisplay.lua")
        .eval::<Table>()?;
    create_shared_bpm_node(lua, underlay, definition, create_dummy_actor)
}

pub fn install_underlay_theme_children(
    lua: &Lua,
    underlay: &Table,
    create_dummy_actor: fn(&Lua, &'static str) -> mlua::Result<Table>,
    create_named_child_actor: fn(&Lua, &Table, &str) -> mlua::Result<Table>,
) -> mlua::Result<()> {
    let children = actor_children(lua, underlay)?;
    let unnamed = create_actor_child_group(lua)?;
    for &name in UNDERLAY_THEME_CHILD_NAMES {
        // Simply Love builds its PerPlayer underlay actors by iterating
        // GAMESTATE:GetHumanPlayers; an inactive side has no score or pane.
        let player = underlay_score_index(name)
            .or_else(|| top_screen_step_stats_pane_index(name))
            .or_else(|| top_screen_danger_index(name))
            .or_else(|| name.strip_prefix("NoteFieldContainerP")
                .and_then(|number| number.parse::<usize>().ok()).map(|number| number - 1));
        if let Some(player) = player {
            let key = ["__songlua_player1_enabled", "__songlua_player2_enabled"][player];
            if underlay.raw_get::<Option<bool>>(key)? == Some(false) {
                continue;
            }
        }
        if let Some(player) = top_screen_step_stats_pane_index(name) {
            if let Some(filter) = create_underlay_background_filter(lua, underlay, player, create_dummy_actor)? {
                filter.raw_set("__songlua_underlay_role", format!("filter{player}"))?;
                push_sequence_child_once(underlay, filter.clone())?;
                unnamed.raw_set(unnamed.raw_len() + 1, filter)?;
            }
            continue; // Native StepStatistics is loaded after its host services exist.
        }
        // Danger.lua needs GAMESTATE and Def, installed later with StepStatistics.
        if top_screen_danger_index(name).is_some() { continue; }
        if name.eq_ignore_ascii_case("SongMeter") { continue; } // Load the original shared actor after host services exist.
        if name.starts_with("NoteFieldContainerP") { continue; } // Original per-player source loads after host services exist.
        let child = create_named_child_actor(lua, underlay, name)?;
        if name.starts_with("NoteFieldContainerP") {
            // Named lookup and screen Update share the same pane owner.
            push_sequence_child_once(underlay, child.clone())?;
        }
        children.set(name, child)?;
        if let Some(player) = underlay_score_index(name) {
            let meter = create_underlay_difficulty_meter(lua, underlay, player, create_dummy_actor)?;
            meter.raw_set("__songlua_underlay_role", format!("difficulty{player}"))?;
            push_sequence_child_once(underlay, meter.clone())?;
            unnamed.raw_set(unnamed.raw_len() + 1, meter)?;
            if let Some(life) = create_underlay_standard_life_meter(lua, underlay, player, create_dummy_actor)? {
                life.raw_set("__songlua_underlay_role", format!("life{player}"))?;
                push_sequence_child_once(underlay, life.clone())?;
                unnamed.raw_set(unnamed.raw_len() + 1, life)?;
            }
        }
    }
    let bpm = create_underlay_shared_bpm(lua, underlay, create_dummy_actor)?;
    bpm.raw_set("__songlua_underlay_role", "bpm")?;
    push_sequence_child_once(underlay, bpm.clone())?;
    unnamed.raw_set(unnamed.raw_len() + 1, bpm)?;
    match unnamed.raw_len() {
        0 => (),
        1 => children.raw_set("", unnamed.raw_get::<Table>(1)?)?,
        _ => children.raw_set("", unnamed)?,
    }
    Ok(())
}

fn install_named_theme_children(
    lua: &Lua,
    parent: &Table,
    names: &[&str],
    create_named_child_actor: fn(&Lua, &Table, &str) -> mlua::Result<Table>,
) -> mlua::Result<()> {
    let children = actor_children(lua, parent)?;
    for &name in names {
        let child = if let Some(child) = children.get::<Option<Table>>(name)? {
            child
        } else {
            let child = create_named_child_actor(lua, parent, name)?;
            children.set(name, child.clone())?;
            child
        };
        push_sequence_child_once(parent, child)?;
    }
    Ok(())
}

fn install_hold_judgments(
    lua: &Lua, player: &Table, player_index: usize, style_name: &str,
    create_dummy_actor: fn(&Lua, &'static str) -> mlua::Result<Table>,
) -> mlua::Result<()> {
    // Player::Init adds an unnamed HoldJudgment ActorFrame for each column.
    // Its named Sprite is shared by sequence traversal and GetChildren.
    let frames = create_actor_child_group(lua)?;
    let unnamed = create_actor_child_group(lua)?;
    if let Some(attack) = player.raw_get::<Option<Table>>("__songlua_attack_display")? {
        unnamed.raw_set(1, attack)?;
    }
    let sl: Table = lua.globals().raw_get("SL")?;
    let mods: Table = sl.raw_get::<Table>(if player_index == 0 { "P1" } else { "P2" })?.raw_get("ActiveModifiers")?;
    let label = mods.raw_get::<Option<String>>("HoldJudgment")?.unwrap_or_else(|| "None 1x2.png".into());
    let theme: Table = lua.globals().raw_get("THEME")?;
    let texture = theme.get::<Function>("GetPathG")?.call::<String>(
        (theme.clone(), "", format!("_HoldJudgments/{label}")))?;
    for column in 0..song_lua_style_info(style_name).columns {
        let frame = create_named_actor(lua, "HoldJudgment", "", create_dummy_actor)?;
        frame.raw_set("__songlua_parent", player.clone())?;
        let sprite = create_named_actor(lua, "Sprite", "HoldJudgment", create_dummy_actor)?;
        sprite.raw_set("__songlua_parent", frame.clone())?;
        sprite.raw_set("__songlua_sprite_animate", false)?;
        set_actor_texture_from_path(&sprite, &texture)?;
        sprite.get::<Function>("diffusealpha")?.call::<()>((sprite.clone(), 0.0))?;
        push_sequence_child_once(&frame, sprite.clone())?;
        actor_children(lua, &frame)?.set("HoldJudgment", sprite)?;
        frames.raw_set(column + 1, frame.clone())?;
        unnamed.raw_set(unnamed.raw_len() + 1, frame.clone())?;
        push_sequence_child_once(player, frame)?;
    }
    actor_children(lua, player)?.set("", unnamed)?;
    player.raw_set("__songlua_hold_judgments", frames)?;
    Ok(())
}

pub fn create_life_meter_table(
    lua: &Lua,
    name: &'static str,
    create_dummy_actor: fn(&Lua, &'static str) -> mlua::Result<Table>,
) -> mlua::Result<Table> {
    let actor = create_dummy_actor(lua, "LifeMeter")?;
    actor.set("Name", name)?;
    actor.set("__songlua_life", SONG_LUA_INITIAL_LIFE)?;
    let player_index = top_screen_life_meter_index(name);
    actor.set(
        "GetLife",
        lua.create_function({
            let actor = actor.clone();
            move |lua, _args: IgnoredArgs| {
                if let Some(life) = player_index.and_then(|player| crate::live::player_life(lua, player)) {
                    return Ok(life);
                }
                Ok(actor
                    .get::<Option<f32>>("__songlua_life")?
                    .unwrap_or(SONG_LUA_INITIAL_LIFE))
            }
        })?,
    )?;
    actor.set(
        "IsFailing",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: IgnoredArgs| {
                Ok(actor
                    .get::<Option<f32>>("__songlua_life")?
                    .unwrap_or(SONG_LUA_INITIAL_LIFE)
                    <= 0.0)
            }
        })?,
    )?;
    actor.set(
        "IsHot",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: IgnoredArgs| {
                Ok(actor
                    .get::<Option<f32>>("__songlua_life")?
                    .unwrap_or(SONG_LUA_INITIAL_LIFE)
                    >= 1.0)
            }
        })?,
    )?;
    actor.set(
        "IsInDanger",
        lua.create_function({
            let actor = actor.clone();
            move |_, _args: IgnoredArgs| {
                Ok(actor
                    .get::<Option<f32>>("__songlua_life")?
                    .unwrap_or(SONG_LUA_INITIAL_LIFE)
                    < SONG_LUA_DANGER_LIFE)
            }
        })?,
    )?;
    Ok(actor)
}

pub fn create_note_field_actor(
    lua: &Lua,
    player_index: usize,
    style_name: &str,
    create_dummy_actor: fn(&Lua, &'static str) -> mlua::Result<Table>,
) -> mlua::Result<Table> {
    let actor = create_dummy_actor(lua, "NoteField")?;
    // NoteField constructs Board from the inherited blank bitmap. A bitmap
    // has no On handler; its constructor dispatch leaves the Sprite unchanged.
    let board = create_named_actor(lua, "Sprite", "Board", create_dummy_actor)?;
    set_actor_texture_from_path(&board, load_native_blank_texture()?)?;
    board.raw_set("__songlua_parent", actor.clone())?;
    push_sequence_child_once(&actor, board.clone())?;
    actor_children(lua, &actor)?.set("Board", board)?;
    actor.set("__songlua_player_index", player_index as i64)?;
    actor.set("__songlua_style_name", style_name)?;
    actor.set("__songlua_player_child_name", "NoteField")?;
    actor.set("__songlua_beat_bars", false)?;
    actor.set("__songlua_beat_bars_alpha", lua.create_table()?)?;
    actor.set(
        "SetBeatBars",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                if let Some(value) = method_arg(&args, 0) {
                    actor.set("__songlua_beat_bars", truthy(value))?;
                    note_song_lua_side_effect(lua)?;
                }
                Ok(actor.clone())
            }
        })?,
    )?;
    actor.set(
        "SetBeatBarsAlpha",
        lua.create_function({
            let actor = actor.clone();
            move |lua, args: MultiValue| {
                let alpha = lua.create_table()?;
                for index in 0..4 {
                    let value = method_arg(&args, index)
                        .cloned()
                        .and_then(read_f32)
                        .unwrap_or(0.0);
                    alpha.raw_set(index + 1, value)?;
                }
                actor.set("__songlua_beat_bars_alpha", alpha)?;
                note_song_lua_side_effect(lua)?;
                Ok(actor.clone())
            }
        })?,
    )?;
    capture_block_set_f32(lua, &actor, "x", 0.0_f32)?;
    capture_block_set_f32(
        lua, &actor, "y",
        f32::midpoint(THEME_RECEPTOR_Y_STD, THEME_RECEPTOR_Y_REV),
    )?;
    capture_block_set_f32(lua, &actor, "z", 0.0_f32)?;
    // Native NoteField::InitColumnRenderers owns the complete style vector
    // before gameplay updates, including columns Lua has never looked up.
    note_field_column_actors(lua, &actor, create_dummy_actor)?;
    Ok(actor)
}

pub fn note_field_column_actors(
    lua: &Lua,
    note_field: &Table,
    create_dummy_actor: fn(&Lua, &'static str) -> mlua::Result<Table>,
) -> mlua::Result<Table> {
    if let Some(columns) = note_field.get::<Option<Table>>("__songlua_note_columns")? {
        return Ok(columns);
    }
    let columns = lua.create_table()?;
    let player_index = note_field
        .get::<Option<i64>>("__songlua_player_index")?
        .unwrap_or(0) as usize;
    let style_name = note_field
        .get::<Option<String>>("__songlua_style_name")?
        .unwrap_or_else(|| current_song_lua_style_name(lua));
    for column_index in 0..song_lua_style_info(&style_name).columns {
        let column = create_note_column_actor(
            lua,
            note_field,
            player_index,
            column_index,
            &style_name,
            create_dummy_actor,
        )?;
        columns.raw_set(column_index + 1, column)?;
    }
    note_field.set("__songlua_note_columns", columns.clone())?;
    invalidate_compile_update_plan(lua);
    Ok(columns)
}

pub fn create_top_screen_table(
    lua: &Lua,
    context: &SongLuaCompileContext,
    current_sort_order: Table,
    current_song: Table,
    create_dummy_actor: fn(&Lua, &'static str) -> mlua::Result<Table>,
    create_named_child_actor: fn(&Lua, &Table, &str) -> mlua::Result<Table>,
) -> mlua::Result<TopScreenLuaTables> {
    let players = context.players.clone();
    let top_screen = create_dummy_actor(lua, "TopScreen")?;
    top_screen.raw_set("__songlua_player1_enabled", players[0].enabled)?;
    top_screen.raw_set("__songlua_player2_enabled", players[1].enabled)?;
    top_screen.set("Name", "ScreenGameplay")?;
    // ScreenGameplay::GetScreenType returns gameplay; LunaScreen pushes its Lua enum.
    top_screen.set(
        "GetScreenType",
        lua.create_function(|_, _args: IgnoredArgs| Ok("ScreenType_Gameplay"))?,
    )?;
    top_screen.set("__songlua_main_title", context.main_title.as_str())?;
    top_screen.set(
        "__songlua_bpm_text",
        display_bpms_text(context.song_display_bpms, song_music_rate(context)),
    )?;
    let top_screen_for_get_child = top_screen.clone();
    let life_meters = [
        create_life_meter_table(lua, "LifeP1", create_dummy_actor)?,
        create_life_meter_table(lua, "LifeP2", create_dummy_actor)?,
    ];
    let player_actors = [
        create_top_screen_player_actor(
            lua,
            players[0].clone(),
            0,
            &context.style_name,
            create_dummy_actor,
        )?,
        create_top_screen_player_actor(
            lua,
            players[1].clone(),
            1,
            &context.style_name,
            create_dummy_actor,
        )?,
    ];
    for (player_index, player_actor) in player_actors.iter().enumerate() {
        player_actor.set("__songlua_parent", top_screen.clone())?;
        // Player retains its life meter while its screen dispatches End.
        player_actor.raw_set("__songlua_life_meter", life_meters[player_index].clone())?;
        install_named_theme_children(lua, player_actor, &["Combo", "Judgment"],
            create_named_child_actor)?;
        install_hold_judgments(lua, player_actor, player_index, &context.style_name, create_dummy_actor)?;
        install_named_theme_children(lua, player_actor, &["NoteField"], create_named_child_actor)?;
    }
    for (player_index, life_meter) in life_meters.iter().enumerate() {
        life_meter.set("__songlua_parent", top_screen.clone())?;
        // Simply Love replaces these engine meters in its Underlay and
        // hibernates them indefinitely; GetVisible still returns true.
        life_meter.raw_set("__songlua_theme_hibernating", true)?;
        life_meter.set(
            "__songlua_top_screen_child_name",
            top_screen_life_meter_name(player_index),
        )?;
    }
    let children = actor_children(lua, &top_screen)?;
    // ScreenGameplay names each Player before adding it. An unnamed alias
    // would expose the same players twice to GetChildren fade/hide loops.
    for (player_index, player_actor) in player_actors.iter().enumerate() {
        if players[player_index].enabled {
            children.set(top_screen_player_name(player_index), player_actor.clone())?;
        }
    }
    for (player_index, life_meter) in life_meters.iter().enumerate() {
        if players[player_index].enabled {
            children.set(top_screen_life_meter_name(player_index), life_meter.clone())?;
        }
    }
    install_top_screen_theme_children(lua, &top_screen, &players, create_named_child_actor)?;
    // ScreenWithMenuElements::Init loads the Underlay before ScreenGameplay
    // loads engine LifeMeters. LifeMeterBar::Load calls AfterLifeChanged before
    // any song Foreground exists, so only already-loaded theme listeners hear it.
    // These messages precede song construction, so dispatch them to the loaded
    // theme without adding pre-song events to the song's startup output.
    let song_output = lua.remove_app_data::<LiveBroadcasts>();
    // These theme-only construction callbacks precede authored song contexts.
    // Keep normal listener dispatch while excluding both song output streams.
    let source_output = lua.remove_app_data::<LiveSourceBroadcasts>();
    let loaded_life = (|| -> mlua::Result<()> {
        for (player, life_meter) in life_meters.iter().enumerate() {
            if !players[player].enabled { continue; }
            let params = lua.create_table()?;
            params.raw_set("Player", player_number_name(player))?;
            params.raw_set("LifeMeter", life_meter.clone())?;
            broadcast_native_song_lua_message(lua, "LifeChanged", Some(Value::Table(params)))?;
        }
        Ok(())
    })();
    if let Some(song_output) = song_output { lua.set_app_data(song_output); }
    if let Some(source_output) = source_output { lua.set_app_data(source_output); }
    loaded_life?;
    // ScreenGameplay.h declares all five unnamed owners as Transition.
    // Keep the duplicate-name group as lookup aliases of the same actor tree.
    let unnamed = create_actor_child_group(lua)?;
    for role in ["Toasty", "NextSong", "Ready", "Go", "Failed"] {
        let actor = create_named_actor(lua, "ActorFrame", "", create_dummy_actor)?;
        actor.raw_set("__songlua_transition_state", 0_u8)?;
        actor.raw_set("__songlua_parent", top_screen.clone())?;
        actor.raw_set("__songlua_screen_actor_role", role)?;
        if role == "Toasty" {
            actor.get::<Function>("visible")?.call::<()>((actor.clone(), false))?;
        }
        unnamed.raw_set(unnamed.raw_len() + 1, actor.clone())?;
        push_sequence_child_once(&top_screen, actor)?;
    }
    children.set("", unnamed)?;
    top_screen.raw_set("__songlua_owned_child_lookup", true)?;
    let mut ordered = top_screen.sequence_values::<Table>()
        .collect::<mlua::Result<Vec<_>>>()?;
    // ScreenWithMenuElements adopts Header/Footer before gameplay creates its
    // player displays. Stable draw sorting preserves this construction order
    // among the default-order actors (ActorFrame.cpp). ScreenGameplay first
    // adopts every Player, then adopts their OniGameOver actors in a second loop.
    let construction_names = ["Underlay", "Overlay", "Header", "Footer", "In", "Out", "Cancel",
        "SongBackground", "SongForeground", "Toasty", "PlayerP1", "PlayerP2", "OniGameOverP1", "OniGameOverP2",
        "NextSong", "LifeP1", "LifeP2", "ScoreP1", "ScoreP2", "StepsDescriptionP1", "PlayerOptionsP1",
        "StepsDisplayP1", "StepsDescriptionP2", "PlayerOptionsP2", "StepsDisplayP2", "SongOptions",
        "ActiveAttackListP1", "ActiveAttackListP2", "LyricDisplay", "Ready", "Go", "Failed", "Debug"];
    ordered.sort_by_key(|actor| {
        let name = actor.raw_get::<Option<String>>("__songlua_screen_actor_role")
            .ok().flatten().or_else(|| actor_name(actor).ok());
        name.and_then(|name| construction_names.iter().position(|known| *known == name))
            .unwrap_or(construction_names.len())
    });
    for actor in &ordered {
        let order = match actor_name(actor)?.as_str() {
            "SongBackground" => -200,
            "Underlay" => -100,
            "Overlay" => 100,
            "SongForeground" => 101,
            "Debug" => 199,
            "In" | "Out" | "Cancel" => 200,
            _ => match actor.raw_get::<Option<String>>("__songlua_screen_actor_role")?.as_deref() {
                Some("NextSong" | "Failed") => 199,
                _ => 0,
            },
        };
        actor.raw_set("__songlua_state_draw_order", order)?;
    }
    ordered.sort_by_key(|actor| actor.raw_get::<i32>("__songlua_state_draw_order").unwrap_or(0));
    for (index, actor) in ordered.into_iter().enumerate() {
        top_screen.raw_set(index + 1, actor)?;
    }
    top_screen.set(
        "SetMinSecondsToMusic",
        lua.create_function(|_, (screen, seconds): (Table, f32)| {
            if !seconds.is_finite() || !(0.0..=60.0).contains(&seconds) {
                return Err(mlua::Error::runtime(
                    "music lead-in must be between 0 and 60 seconds",
                ));
            }
            screen.raw_set("__songlua_min_seconds_to_music", seconds)?;
            Ok(screen)
        })?,
    )?;
    let player_actors_for_get_child = player_actors.clone();
    let life_meters_for_get_child = life_meters.clone();
    let players_for_get_child = players.clone();
    top_screen.set(
        "GetChild",
        lua.create_function(move |lua, args: MultiValue| {
            let Some(name) = method_arg(&args, 0).cloned().and_then(read_string) else {
                return Ok(Value::Nil);
            };
            if let Some(player_index) = top_screen_player_index(&name) {
                return if players_for_get_child[player_index].enabled {
                    Ok(Value::Table(
                        player_actors_for_get_child[player_index].clone(),
                    ))
                } else {
                    Ok(Value::Nil)
                };
            }
            if let Some(player_index) = top_screen_life_meter_index(&name) {
                return if players_for_get_child[player_index].enabled {
                    Ok(Value::Table(
                        life_meters_for_get_child[player_index].clone(),
                    ))
                } else {
                    Ok(Value::Nil)
                };
            }
            let children = actor_children(lua, &top_screen_for_get_child)?;
            if let Some(child) = children.get::<Option<Table>>(name.as_str())? {
                return Ok(Value::Table(child));
            }
            // ActorFrame::PushChildTable returns nil when no child matches.
            // Looking up a name never creates an actor or changes the tree.
            Ok(Value::Nil)
        })?,
    )?;
    let life_meters_for_get_life_meter = life_meters;
    let players_for_get_life_meter = players;
    top_screen.set(
        "GetLifeMeter",
        lua.create_function(move |_, args: MultiValue| {
            let Some(player_index) = method_arg(&args, 0).and_then(player_index_from_value) else {
                return Ok(Value::Nil);
            };
            if !players_for_get_life_meter[player_index].enabled {
                return Ok(Value::Nil);
            }
            Ok(Value::Table(
                life_meters_for_get_life_meter[player_index].clone(),
            ))
        })?,
    )?;
    top_screen.set(
        "StartTransitioningScreen",
        lua.create_function({
            let top_screen = top_screen.clone();
            move |lua, args: MultiValue| {
                note_song_lua_side_effect(lua)?;
                if crate::live::active(lua) {
                    let message = method_arg(&args, 0).cloned().and_then(read_string)
                        .ok_or_else(|| mlua::Error::runtime("expected a screen transition message"))?;
                    crate::live::capture_source_request(lua, crate::live::SongLuaLiveRequest::Transition(message.clone()))?;
                    run_named_command_on_children_recursively(lua, &top_screen, "OffCommand", None)?;
                    if let Some(transition) = actor_children(lua, &top_screen)?.raw_get::<Option<Table>>("Out")? {
                        start_actor_transition(lua, &transition)?;
                    }
                    lua.set_app_data(crate::live::TransitionRequest(message));
                }
                Ok(top_screen.clone())
            }
        })?,
    )?;
    top_screen.set(
        "SetNextScreenName",
        lua.create_function({
            let top_screen = top_screen.clone();
            move |lua, args: MultiValue| {
                let next_screen = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_string)
                    .unwrap_or_default();
                top_screen.set("__songlua_next_screen_name", next_screen)?;
                note_song_lua_side_effect(lua)?;
                Ok(top_screen.clone())
            }
        })?,
    )?;
    top_screen.set(
        "SetPrevScreenName",
        lua.create_function({
            let top_screen = top_screen.clone();
            move |lua, args: MultiValue| {
                let prev_screen = method_arg(&args, 0)
                    .cloned()
                    .and_then(read_string)
                    .unwrap_or_default();
                top_screen.set("__songlua_prev_screen_name", prev_screen)?;
                note_song_lua_side_effect(lua)?;
                Ok(top_screen.clone())
            }
        })?,
    )?;
    top_screen.set(
        "GetNextScreenName",
        lua.create_function({
            let top_screen = top_screen.clone();
            move |lua, _args: IgnoredArgs| {
                Ok(Value::String(
                    lua.create_string(
                        top_screen
                            .get::<Option<String>>("__songlua_next_screen_name")?
                            .unwrap_or_default(),
                    )?,
                ))
            }
        })?,
    )?;
    top_screen.set(
        "GetPrevScreenName",
        lua.create_function({
            let top_screen = top_screen.clone();
            move |lua, _args: IgnoredArgs| {
                Ok(Value::String(
                    lua.create_string(
                        top_screen
                            .get::<Option<String>>("__songlua_prev_screen_name")?
                            .unwrap_or_default(),
                    )?,
                ))
            }
        })?,
    )?;
    top_screen.set(
        "GetGoToOptions",
        lua.create_function(|_, _args: IgnoredArgs| Ok(false))?,
    )?;
    top_screen.set(
        "GetEditState",
        lua.create_function(|lua, _args: IgnoredArgs| {
            Ok(Value::String(lua.create_string("EditState_Playing")?))
        })?,
    )?;
    top_screen.set(
        "GetCurrentRowIndex",
        lua.create_function({
            let top_screen = top_screen.clone();
            move |_, _args: IgnoredArgs| {
                Ok(top_screen
                    .get::<Option<i64>>("__songlua_current_row_index")?
                    .unwrap_or(0))
            }
        })?,
    )?;
    top_screen.set(
        "GetNumRows",
        lua.create_function(|_, _args: IgnoredArgs| {
            Ok(SONG_LUA_TOP_SCREEN_OPTION_ROWS.len() as i64)
        })?,
    )?;
    top_screen.set(
        "GetOptionRow",
        lua.create_function(move |lua, args: MultiValue| {
            let row_name = top_screen_option_row_name(method_arg(&args, 0).cloned());
            create_option_row_table(lua, &row_name, create_dummy_actor)
        })?,
    )?;
    top_screen.set(
        "SetOptionRowIndex",
        lua.create_function({
            let top_screen = top_screen.clone();
            move |lua, args: MultiValue| {
                if let Some(index) = method_arg(&args, 1)
                    .or_else(|| method_arg(&args, 0))
                    .cloned()
                    .and_then(read_i32_value)
                {
                    top_screen.set("__songlua_current_row_index", i64::from(index.max(0)))?;
                }
                note_song_lua_side_effect(lua)?;
                Ok(top_screen.clone())
            }
        })?,
    )?;
    top_screen.set(
        "RedrawOptions",
        lua.create_function({
            let top_screen = top_screen.clone();
            move |lua, _args: IgnoredArgs| {
                note_song_lua_side_effect(lua)?;
                Ok(top_screen.clone())
            }
        })?,
    )?;
    top_screen.set(
        "IsPaused",
        lua.create_function({
            let top_screen = top_screen.clone();
            move |_, _args: IgnoredArgs| {
                Ok(top_screen
                    .get::<Option<bool>>("__songlua_paused")?
                    .unwrap_or(false))
            }
        })?,
    )?;
    top_screen.set(
        "PauseGame",
        lua.create_function({
            let top_screen = top_screen.clone();
            move |lua, args: MultiValue| {
                top_screen.set("__songlua_paused", method_arg(&args, 0).is_some_and(truthy))?;
                note_song_lua_side_effect(lua)?;
                Ok(top_screen.clone())
            }
        })?,
    )?;
    top_screen.set(
        "AllAreOnLastRow",
        lua.create_function(|_, _args: IgnoredArgs| Ok(false))?,
    )?;
    let music_wheel = create_music_wheel_table(lua, current_sort_order)?;
    top_screen.set(
        "GetMusicWheel",
        lua.create_function(move |_, _args: IgnoredArgs| Ok(music_wheel.clone()))?,
    )?;
    top_screen.set(
        "GetNextCourseSong",
        lua.create_function(move |_, _args: IgnoredArgs| {
            Ok(current_song
                .raw_get::<Option<Table>>(1)?
                .map_or(Value::Nil, Value::Table))
        })?,
    )?;
    crate::input::install(lua, &top_screen)?;
    for name in [
        "PostScreenMessage",
        "SetProfileIndex",
        "Cancel",
        "Continue",
        "Finish",
        "Load",
        "begin_backing_out",
    ] {
        top_screen.set(
            name,
            lua.create_function({
                let top_screen = top_screen.clone();
                move |lua, _args: IgnoredArgs| {
                    note_song_lua_side_effect(lua)?;
                    Ok(top_screen.clone())
                }
            })?,
        )?;
    }
    Ok(TopScreenLuaTables {
        top_screen,
        players: player_actors,
    })
}

pub fn create_note_column_actor(
    lua: &Lua,
    note_field: &Table,
    player_index: usize,
    column_index: usize,
    style_name: &str,
    create_dummy_actor: fn(&Lua, &'static str) -> mlua::Result<Table>,
) -> mlua::Result<Table> {
    let actor = create_dummy_actor(lua, "NoteColumnRenderer")?;
    actor.set("__songlua_parent", note_field.clone())?;
    actor.set("__songlua_player_index", player_index as i64)?;
    actor.set("__songlua_column_index", column_index as i64)?;
    actor.set(
        "__songlua_state_x",
        song_lua_style_column_x(style_name, column_index),
    )?;
    actor.set("__songlua_state_y", 0.0_f32)?;
    actor.set("__songlua_state_z", 0.0_f32)?;

    let pos_handler = create_note_column_spline_handler(lua)?;
    let rot_handler = create_note_column_spline_handler(lua)?;
    let zoom_handler = create_note_column_spline_handler(lua)?;
    actor.set("__songlua_pos_handler", pos_handler.clone())?;
    actor.set("__songlua_rot_handler", rot_handler.clone())?;
    actor.set("__songlua_zoom_handler", zoom_handler.clone())?;
    actor.set(
        "GetPosHandler",
        lua.create_function(move |_, _args: IgnoredArgs| Ok(pos_handler.clone()))?,
    )?;
    actor.set(
        "GetRotHandler",
        lua.create_function(move |_, _args: IgnoredArgs| Ok(rot_handler.clone()))?,
    )?;
    actor.set(
        "GetZoomHandler",
        lua.create_function(move |_, _args: IgnoredArgs| Ok(zoom_handler.clone()))?,
    )?;
    for (alias, name) in [
        ("get_pos_handler", "GetPosHandler"),
        ("get_rot_handler", "GetRotHandler"),
        ("get_zoom_handler", "GetZoomHandler"),
    ] {
        actor.set(alias, actor.get::<Function>(name)?)?;
    }
    Ok(actor)
}

pub fn create_note_column_spline_handler(lua: &Lua) -> mlua::Result<Table> {
    let handler = lua.create_table()?;
    let spline = create_cubic_spline_table(lua)?;
    handler.set("__songlua_spline", spline.clone())?;
    handler.set("__songlua_spline_mode", "NoteColumnSplineMode_Disabled")?;
    handler.set("__songlua_subtract_song_beat", true)?;
    handler.set("__songlua_receptor_t", 0.0_f32)?;
    handler.set("__songlua_beats_per_t", 1.0_f32)?;
    handler.set(
        "GetSpline",
        lua.create_function(move |_, _args: IgnoredArgs| Ok(spline.clone()))?,
    )?;
    handler.set(
        "SetSplineMode",
        lua.create_function({
            let handler = handler.clone();
            move |_, args: MultiValue| {
                if let Some(mode) = args.get(1).cloned().and_then(read_string) {
                    handler.set("__songlua_spline_mode", mode)?;
                }
                Ok(handler.clone())
            }
        })?,
    )?;
    handler.set(
        "SetSubtractSongBeat",
        lua.create_function({
            let handler = handler.clone();
            move |_, args: MultiValue| {
                handler.set(
                    "__songlua_subtract_song_beat",
                    args.get(1).is_some_and(truthy),
                )?;
                Ok(handler.clone())
            }
        })?,
    )?;
    handler.set(
        "SetReceptorT",
        lua.create_function({
            let handler = handler.clone();
            move |_, args: MultiValue| {
                if let Some(value) = args.get(1).cloned().and_then(read_f32) {
                    handler.set("__songlua_receptor_t", value)?;
                }
                Ok(handler.clone())
            }
        })?,
    )?;
    handler.set(
        "SetBeatsPerT",
        lua.create_function({
            let handler = handler.clone();
            move |_, args: MultiValue| {
                if let Some(value) = args.get(1).cloned().and_then(read_f32) {
                    handler.set("__songlua_beats_per_t", value)?;
                }
                Ok(handler.clone())
            }
        })?,
    )?;
    for (alias, name) in [
        ("get_spline", "GetSpline"),
        ("set_spline_mode", "SetSplineMode"),
        ("set_subtract_song_beat", "SetSubtractSongBeat"),
        ("set_receptor_t", "SetReceptorT"),
        ("set_beats_per_t", "SetBeatsPerT"),
    ] {
        handler.set(alias, handler.get::<Function>(name)?)?;
    }
    Ok(handler)
}

fn create_cubic_spline_table(lua: &Lua) -> mlua::Result<Table> {
    let spline = lua.create_table()?;
    spline.set("__songlua_spline_size", 0_i64)?;
    spline.set("__songlua_spline_loop", false)?;
    spline.set("__songlua_spline_points", lua.create_table()?)?;
    spline.set("__songlua_spline_coefficients", lua.create_table()?)?;
    spline.set(
        "evaluate",
        lua.create_function({
            let spline = spline.clone();
            move |lua, args: MultiValue| {
                let t = lua.coerce_number(args.get(1).cloned().unwrap_or(Value::Nil))?
                    .ok_or_else(|| mlua::Error::runtime("Spline evaluation requires a number"))?;
                eval_spline_table(lua, &spline, t as f32)
            }
        })?,
    )?;
    spline.set(
        "SetSize",
        lua.create_function({
            let spline = spline.clone();
            move |lua, args: MultiValue| {
                let size = lua
                    .coerce_number(args.get(1).cloned().unwrap_or(Value::Nil))?
                    .filter(|value| value.is_finite())
                    .ok_or_else(|| mlua::Error::runtime("Spline size must be a number"))?
                    .trunc();
                if size < 0.0 {
                    return Err(mlua::Error::runtime(
                        "A spline cannot have less than 0 points.",
                    ));
                }
                // This is the same bounded geometry limit used by the spline reader.
                if size > 65536.0 {
                    return Err(mlua::Error::runtime("Spline exceeds 65536 points"));
                }
                let size = size as i64;
                let prior = spline.get::<i64>("__songlua_spline_size")?;
                let points = spline.get::<Table>("__songlua_spline_points")?;
                let coefficients = spline.get::<Table>("__songlua_spline_coefficients")?;
                for index in size + 1..=prior {
                    points.raw_set(index, Value::Nil)?;
                    coefficients.raw_set(index, Value::Nil)?;
                }
                // Unwritten knots are implicit native zeros. Avoid thousands of
                // Lua tables in startup snapshots; SetPoint owns stored vectors.
                spline.set("__songlua_spline_size", size)?;
                Ok(spline.clone())
            }
        })?,
    )?;
    spline.set(
        "GetSize",
        lua.create_function({
            let spline = spline.clone();
            move |_, _args: IgnoredArgs| spline.get::<i64>("__songlua_spline_size")
        })?,
    )?;
    spline.set(
        "SetPoint",
        lua.create_function({
            let spline = spline.clone();
            move |lua, args: MultiValue| {
                let index = lua
                    .coerce_number(args.get(1).cloned().unwrap_or(Value::Nil))?
                    .filter(|value| value.is_finite())
                    .ok_or_else(|| mlua::Error::runtime("Spline point index must be a number"))?
                    .trunc();
                let size = spline.get::<i64>("__songlua_spline_size")?;
                if index < 1.0 || index > size as f64 {
                    return Err(mlua::Error::runtime("Spline point index out of range."));
                }
                let Some(Value::Table(source)) = args.get(2) else {
                    return Err(mlua::Error::runtime("Spline point must be a table."));
                };
                let point = lua.create_table()?;
                for axis in 1..=source.raw_len().max(3) {
                    // LunaCubicSplineN copies and pads/truncates the authored vector.
                    let value = if axis <= source.raw_len() {
                        lua.coerce_number(source.raw_get::<Value>(axis)?)?
                            .ok_or_else(|| {
                                mlua::Error::runtime("Spline coordinate must be a number")
                            })? as f32
                    } else {
                        0.0
                    };
                    if axis <= 3 {
                        point.raw_set(axis, value)?;
                    }
                }
                spline
                    .get::<Table>("__songlua_spline_points")?
                    .raw_set(index as i64, point)?;
                Ok(spline.clone())
            }
        })?,
    )?;
    spline.set(
        "Solve",
        lua.create_function({
            let spline = spline.clone();
            move |lua, _args: IgnoredArgs| {
                solve_spline_table(lua, &spline)?;
                Ok(spline.clone())
            }
        })?,
    )?;
    spline.set(
        "SetPolygonal",
        lua.create_function({
            let spline = spline.clone();
            move |_, _args: IgnoredArgs| Ok(spline.clone())
        })?,
    )?;
    spline.set(
        "SetLoop",
        lua.create_function({
            let spline = spline.clone();
            move |_, args: MultiValue| {
                spline.set("__songlua_spline_loop", args.get(1).is_some_and(truthy))?;
                Ok(spline.clone())
            }
        })?,
    )?;
    spline.set(
        "get_loop",
        lua.create_function({
            let spline = spline.clone();
            move |_, _args: IgnoredArgs| spline.get::<bool>("__songlua_spline_loop")
        })?,
    )?;
    for (alias, name) in [
        ("Evaluate", "evaluate"),
        ("set_size", "SetSize"),
        ("get_size", "GetSize"),
        ("set_point", "SetPoint"),
        ("solve", "Solve"),
        ("set_polygonal", "SetPolygonal"),
        ("set_loop", "SetLoop"),
    ] {
        spline.set(alias, spline.get::<Function>(name)?)?;
    }
    Ok(spline)
}

fn solve_spline_table(lua: &Lua, spline: &Table) -> mlua::Result<()> {
    let size = spline.get::<usize>("__songlua_spline_size")?;
    let table = spline.get::<Table>("__songlua_spline_points")?;
    let mut points = Vec::with_capacity(size);
    for index in 1..=size {
        points.push(read_spline_point(&table, index)?);
    }
    let looping = spline.get::<bool>("__songlua_spline_loop")?;
    let mut solver = deadsync_gameplay::SongLuaSplineSolver::default();
    let coefficients = solver.solve_with_loop(&points, looping);
    let table = lua.create_table()?;
    for (index, point) in coefficients.iter().enumerate() {
        let axes = lua.create_table()?;
        for (axis, values) in point.iter().enumerate() {
            axes.raw_set(axis + 1, lua.create_sequence_from(values[1..].iter().copied())?)?;
        }
        table.raw_set(index + 1, axes)?;
    }
    spline.set("__songlua_spline_coefficients", table)
}

fn eval_spline_table(lua: &Lua, spline: &Table, t: f32) -> mlua::Result<Table> {
    let size = spline.get::<usize>("__songlua_spline_size")?;
    let mut values = [0.0_f32; 3];
    if size > 0 {
        // CubicSpline::p_and_tfrac_from_t truncates, including negative
        // fractions, and clamps at the final knot for non-looping splines.
        let index = t as i32;
        let (index, fraction) = if spline.get::<bool>("__songlua_spline_loop")? {
            let mut wrapped = t % size as f32;
            if wrapped < 0.0 { wrapped += size as f32; }
            (wrapped as usize, wrapped - wrapped.trunc())
        } else if index < 0 {
            (0, 0.0)
        } else if index as usize >= size - 1 {
            (size - 1, 0.0)
        } else {
            (index as usize, t - index as f32)
        };
        let square = fraction * fraction;
        let cube = square * fraction;
        values = read_spline_point(&spline.get::<Table>("__songlua_spline_points")?, index + 1)?;
        let coefficients = spline.get::<Table>("__songlua_spline_coefficients")?;
        let point = coefficients.raw_get::<Option<Table>>(index + 1)?;
        for (axis, value) in values.iter_mut().enumerate() {
            if let Some(point) = &point {
                let [b, c, d] = read_spline_point(point, axis + 1)?;
                *value = *value + b * fraction + c * square + d * cube;
            }
        }
    }
    lua.create_sequence_from(values)
}

fn read_spline_point(points: &Table, index: usize) -> mlua::Result<[f32; 3]> {
    let Some(point) = points.raw_get::<Option<Table>>(index)? else {
        return Ok([0.0; 3]);
    };
    let mut values = [0.0; 3];
    for (axis, value) in values.iter_mut().enumerate() {
        *value = point.raw_get::<Option<f32>>(axis + 1)?.unwrap_or(0.0);
    }
    Ok(values)
}

pub fn note_zoom_point_hides(point: &Table) -> bool {
    let zoom_x = point.raw_get::<Value>(1).ok().and_then(read_f32);
    let zoom_y = point.raw_get::<Value>(2).ok().and_then(read_f32);
    let zoom_z = point.raw_get::<Value>(3).ok().and_then(read_f32);
    match (zoom_x, zoom_y, zoom_z) {
        (Some(x), Some(y), Some(z)) => x <= -0.99 && y <= -0.99 && z <= -0.99,
        _ => false,
    }
}

pub fn push_note_hide_window(
    out: &mut Vec<SongLuaNoteHideWindow>,
    player: usize,
    column: usize,
    beats_per_t: f32,
    start_index: usize,
    end_index: usize,
) {
    if let Some(window) =
        crate::note_hide_window_from_indices(player, column, beats_per_t, start_index, end_index)
    {
        out.push(window);
    }
}

pub fn read_note_column_zoom_hides_for_actor(
    player: usize,
    column: usize,
    actor: &Table,
    out: &mut Vec<SongLuaNoteHideWindow>,
) -> Result<(), String> {
    let Some(handler) = actor
        .get::<Option<Table>>("__songlua_zoom_handler")
        .map_err(|err| err.to_string())?
    else {
        return Ok(());
    };
    let mode = handler
        .get::<Option<String>>("__songlua_spline_mode")
        .map_err(|err| err.to_string())?
        .unwrap_or_default();
    let subtract_song_beat = handler
        .get::<Option<bool>>("__songlua_subtract_song_beat")
        .map_err(|err| err.to_string())?
        .unwrap_or(false);
    let beats_per_t = handler
        .get::<Option<f32>>("__songlua_beats_per_t")
        .map_err(|err| err.to_string())?
        .unwrap_or(1.0);
    let Some(beats_per_t) =
        crate::note_column_zoom_hide_beats_per_t(&mode, subtract_song_beat, beats_per_t)
    else {
        return Ok(());
    };
    let Some(spline) = handler
        .get::<Option<Table>>("__songlua_spline")
        .map_err(|err| err.to_string())?
    else {
        return Ok(());
    };
    let size = spline
        .get::<Option<i64>>("__songlua_spline_size")
        .map_err(|err| err.to_string())?
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(0);
    let points = spline
        .get::<Table>("__songlua_spline_points")
        .map_err(|err| err.to_string())?;
    let mut hidden = Vec::with_capacity(size);
    for index in 1..=size {
        hidden.push(
            points
                .raw_get::<Option<Table>>(index)
                .map_err(|err| err.to_string())?
                .is_some_and(|point| note_zoom_point_hides(&point)),
        );
    }
    out.extend(crate::note_hide_windows_from_flags(
        player,
        column,
        beats_per_t,
        &hidden,
    ));
    Ok(())
}

pub fn read_note_column_zoom_hides(lua: &Lua) -> Result<Vec<SongLuaNoteHideWindow>, String> {
    let globals = lua.globals();
    let mut out = Vec::new();
    for player in 0..LUA_PLAYERS {
        let key = if player == 0 {
            "__songlua_top_screen_player_1"
        } else {
            "__songlua_top_screen_player_2"
        };
        let Some(player_actor) = globals
            .get::<Option<Table>>(key)
            .map_err(|err| err.to_string())?
        else {
            continue;
        };
        let Some(note_field) =
            note_field_table(lua, &player_actor).map_err(|err| err.to_string())?
        else {
            continue;
        };
        let Some(columns) = note_field
            .get::<Option<Table>>("__songlua_note_columns")
            .map_err(|err| err.to_string())?
        else {
            continue;
        };
        for column in columns.sequence_values::<Table>() {
            let column = column.map_err(|err| err.to_string())?;
            let Some(local_col) = column
                .get::<Option<i64>>("__songlua_column_index")
                .map_err(|err| err.to_string())?
                .and_then(|value| usize::try_from(value).ok())
            else {
                continue;
            };
            read_note_column_zoom_hides_for_actor(player, local_col, &column, &mut out)?;
        }
    }
    crate::sort_note_hide_windows(&mut out);
    Ok(out)
}

pub fn note_column_pos_offset_y(actor: &Table) -> Result<Option<f32>, String> {
    let Some(handler) = actor
        .get::<Option<Table>>("__songlua_pos_handler")
        .map_err(|err| err.to_string())?
    else {
        return Ok(None);
    };
    let mode = handler
        .get::<Option<String>>("__songlua_spline_mode")
        .map_err(|err| err.to_string())?
        .unwrap_or_default();
    if mode.eq_ignore_ascii_case("NoteColumnSplineMode_Disabled") {
        return Ok(crate::note_column_pos_offset_y_from_points(&mode, &[]));
    }
    if !mode.eq_ignore_ascii_case("NoteColumnSplineMode_Offset") {
        return Ok(None);
    }
    let Some(spline) = handler
        .get::<Option<Table>>("__songlua_spline")
        .map_err(|err| err.to_string())?
    else {
        return Ok(None);
    };
    let size = spline
        .get::<Option<i64>>("__songlua_spline_size")
        .map_err(|err| err.to_string())?
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(0);
    if size == 0 {
        return Ok(crate::note_column_pos_offset_y_from_points(&mode, &[]));
    }
    let points = spline
        .get::<Table>("__songlua_spline_points")
        .map_err(|err| err.to_string())?;
    let mut first_y = None::<f32>;
    let mut valid = true;
    for index in 1..=size {
        let [x, point_y, _] = read_spline_point(&points, index)
            .map_err(|err| err.to_string())?;
        // Keep reading after a geometric mismatch: a later malformed table
        // must still produce the same lookup error as the collecting path.
        // NCSplineHandler's Offset adds each axis independently. A nonzero
        // horizontal offset does not invalidate a uniform vertical offset.
        valid &= x.is_finite() && point_y.is_finite();
        if let Some(first_y) = first_y {
            valid &= (point_y - first_y).abs() <= 0.001;
        } else {
            first_y = Some(point_y);
        }
    }
    Ok(if valid { first_y } else { None })
}

// A stored registry without sequence children needs no merge. Raw lookup
// matches copying registry entries into a fresh table without its metatable.
// sequence_values also uses raw access, so actor metatables do not affect this.
// Scripted registry getters/setters can add sequence children; retain the full
// merge path when the registry is not already stored on the actor.
fn note_field_table(lua: &Lua, player_actor: &Table) -> mlua::Result<Option<Table>> {
    if player_actor.raw_len() == 0
        && let Some(children) = player_actor.raw_get::<Option<Table>>("__songlua_children")?
    {
        return children.raw_get("NoteField");
    }
    actor_named_children(lua, player_actor)?.get("NoteField")
}

fn for_each_note_field(
    lua: &Lua,
    mut visit: impl FnMut(Table) -> mlua::Result<()>,
) -> mlua::Result<()> {
    let globals = lua.globals();
    for player in 0..LUA_PLAYERS {
        let key = if player == 0 {
            "__songlua_top_screen_player_1"
        } else {
            "__songlua_top_screen_player_2"
        };
        let Some(player_actor) = globals.get::<Option<Table>>(key)? else {
            continue;
        };
        let Some(note_field) = note_field_table(lua, &player_actor)? else {
            continue;
        };
        visit(note_field)?;
    }
    Ok(())
}

pub fn note_field_tables(lua: &Lua) -> mlua::Result<Vec<Table>> {
    let mut out = Vec::new();
    for_each_note_field(lua, |field| {
        out.push(field);
        Ok(())
    })?;
    Ok(out)
}

pub fn read_note_column_pos_samples(lua: &Lua) -> Result<Vec<SongLuaColumnOffsetSample>, String> {
    read_note_column_pos_samples_for_fields(note_field_tables(lua).map_err(|err| err.to_string())?)
}

pub fn read_note_column_transform_samples(
    lua: &Lua,
) -> Result<Vec<SongLuaColumnTransformSample>, String> {
    read_note_column_transform_samples_for_fields(
        note_field_tables(lua).map_err(|err| err.to_string())?,
    )
}

/// Compile-lifetime buffers and one recent geometry (at most 65536 points).
/// Coordinates are read and validated before reuse, including in-place edits.
/// No table identity or metadata is used to skip Lua reads. All storage is
/// released after capture; the existing logical track-byte limit is unchanged.
#[derive(Default)]
pub(crate) struct ColumnSplineReadScratch {
    points: Vec<[f32; 3]>,
    solver: deadsync_gameplay::SongLuaSplineSolver,
    recent: Option<std::sync::Arc<[[[f32; 4]; 3]]>>,
    recent_looping: bool,
}

fn spline_points_match(points: &[[f32; 3]], coefficients: &[[[f32; 4]; 3]]) -> bool {
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

fn read_position_spline(
    column: &Table,
    key: &str,
    previous: Option<&deadsync_gameplay::SongLuaSplineData>,
    scratch: &mut ColumnSplineReadScratch,
) -> Result<Option<deadsync_gameplay::SongLuaSplineData>, String> {
    let Some(handler) = column
        .get::<Option<Table>>(key)
        .map_err(|err| err.to_string())?
    else {
        return Ok(None);
    };
    let mode = handler
        .get::<LuaFieldText<32>>("__songlua_spline_mode")
        .map_err(|err| err.to_string())?;
    if !mode
        .as_str()
        .eq_ignore_ascii_case("NoteColumnSplineMode_Position")
    {
        return Ok(None);
    }
    let spline = handler
        .get::<Table>("__songlua_spline")
        .map_err(|err| err.to_string())?;
    let size = spline
        .get::<usize>("__songlua_spline_size")
        .map_err(|err| err.to_string())?;
    if size == 0 {
        return Ok(None);
    }
    if size > 65536 {
        return Err("Position spline exceeds 65536 points".into());
    }
    let beats_per_t = handler
        .get::<f32>("__songlua_beats_per_t")
        .map_err(|err| err.to_string())?;
    let receptor_t = handler
        .get::<f32>("__songlua_receptor_t")
        .map_err(|err| err.to_string())?;
    if !beats_per_t.is_finite() || beats_per_t == 0.0 || !receptor_t.is_finite() {
        return Err("Position spline has an invalid beat conversion".into());
    }
    let table = spline
        .get::<Table>("__songlua_spline_points")
        .map_err(|err| err.to_string())?;
    let points = &mut scratch.points;
    points.clear();
    points.reserve_exact(size);
    let looping = spline.get::<bool>("__songlua_spline_loop").map_err(|err| err.to_string())?;
    let mut unchanged = previous.is_some_and(|spline|
        spline.coefficients.len() == size && spline.looping == looping);
    for index in 1..=size {
        let values = read_spline_point(&table, index).map_err(|err| err.to_string())?;
        if values.iter().any(|value| !value.is_finite()) {
            return Err("Position spline contains a nonfinite point".into());
        }
        // Compare authored coordinates, not table identity: Lua can mutate a
        // point in place. Bits preserve signed zero even when metadata changes.
        unchanged = unchanged
            && previous
                .and_then(|spline| spline.coefficients.get(index - 1))
                .is_some_and(|coefficients| {
                    values
                        .iter()
                        .zip(coefficients)
                        .all(|(value, axis)| value.to_bits() == axis[0].to_bits())
                });
        points.push(values);
    }
    let constant = points.iter().all(|point| *point == points[0]);
    let result = deadsync_gameplay::SongLuaSplineData {
        coefficients: if unchanged {
            std::sync::Arc::clone(&previous.expect("matching spline exists").coefficients)
        } else if let Some(recent) = &scratch.recent
            && scratch.recent_looping == looping
            && spline_points_match(points, recent)
        {
            std::sync::Arc::clone(recent)
        } else {
            std::sync::Arc::from(scratch.solver.solve_with_loop(points, looping))
        },
        constant,
        looping,
        beats_per_t,
        receptor_t,
        subtract_song_beat: handler
            .get::<bool>("__songlua_subtract_song_beat")
            .map_err(|err| err.to_string())?,
    };
    // A preceding-frame hit needs no change to the recent-geometry slot.
    // Avoid cloning/dropping that reference for every steady lane read.
    if !unchanged
        && !scratch
            .recent
            .as_ref()
            .is_some_and(|recent| std::sync::Arc::ptr_eq(recent, &result.coefficients))
    {
        scratch.recent = Some(std::sync::Arc::clone(&result.coefficients));
        scratch.recent_looping = looping;
    }
    Ok(Some(result))
}

/// Reuse capture scratch and the preceding frame's immutable coefficients.
/// Every Lua field and coordinate is still read and validated on every sample.
pub(crate) fn read_column_position_splines<'a>(
    lua: &Lua,
    mut previous: impl FnMut(usize, usize) -> Option<&'a deadsync_gameplay::SongLuaColumnSplineFrame>,
    scratch: &mut ColumnSplineReadScratch,
    out: &mut Vec<(usize, usize, deadsync_gameplay::SongLuaColumnSplineFrame)>,
) -> Result<(), String> {
    out.clear();
    let mut fields = smallvec::SmallVec::<[Table; LUA_PLAYERS]>::new();
    for_each_note_field(lua, |field| {
        fields.push(field);
        Ok(())
    })
    .map_err(|err| err.to_string())?;
    for field in fields {
        let Some(columns) = field
            .get::<Option<Table>>("__songlua_note_columns")
            .map_err(|err| err.to_string())?
        else {
            continue;
        };
        for column in columns.sequence_values::<Table>() {
            let column = column.map_err(|err| err.to_string())?;
            let player = column
                .get::<usize>("__songlua_player_index")
                .map_err(|err| err.to_string())?;
            let local_col = column
                .get::<usize>("__songlua_column_index")
                .map_err(|err| err.to_string())?;
            let last = previous(player, local_col);
            out.push((
                player,
                local_col,
                deadsync_gameplay::SongLuaColumnSplineFrame {
                    second: 0.0,
                    position: read_position_spline(
                        &column,
                        "__songlua_pos_handler",
                        last.and_then(|frame| frame.position.as_ref()),
                        scratch,
                    )?,
                    zoom: read_position_spline(
                        &column,
                        "__songlua_zoom_handler",
                        last.and_then(|frame| frame.zoom.as_ref()),
                        scratch,
                    )?,
                },
            ));
        }
    }
    Ok(())
}

pub fn compile_note_column_pos_function_ease(
    lua: &Lua,
    function: &Function,
    unit: SongLuaTimeUnit,
    start: f32,
    limit: f32,
    span_mode: SongLuaSpanMode,
    from: f32,
    to: f32,
    easing: Option<String>,
    sustain: Option<f32>,
    opt1: Option<f32>,
    opt2: Option<f32>,
) -> Result<Vec<SongLuaColumnOffsetWindow>, String> {
    let start_beat = start;
    let end_beat = song_lua_span_end(start, limit, span_mode).max(start_beat);
    let from_samples = capture_note_column_transform_samples(lua, function, from, start_beat)?;
    let to_samples = capture_note_column_transform_samples(lua, function, to, end_beat)?;
    if from_samples.is_empty() && to_samples.is_empty() {
        return Ok(Vec::new());
    }
    Ok(column_transform_windows_from_samples(
        &from_samples,
        &to_samples,
        SongLuaColumnOffsetBuildParams {
            unit,
            start,
            limit,
            span_mode,
            easing,
            sustain,
            opt1,
            opt2,
        },
    ))
}

fn capture_note_column_transform_samples(
    lua: &Lua,
    function: &Function,
    value: f32,
    song_beat: f32,
) -> Result<Vec<SongLuaColumnTransformSample>, String> {
    let previous = compile_song_runtime_values(lua).map_err(|err| err.to_string())?;
    set_compile_song_runtime_beat(lua, song_beat).map_err(|err| err.to_string())?;
    let snapshot = snapshot_note_field_columns(lua).map_err(|err| err.to_string())?;
    let result = function.call::<Value>(value);
    let samples = read_note_column_transform_samples(lua);
    restore_note_field_columns(lua, snapshot).map_err(|err| err.to_string())?;
    set_compile_song_runtime_values(lua, previous.0, previous.1).map_err(|err| err.to_string())?;
    result.map_err(|err| err.to_string())?;
    samples
}

fn note_column_handler_uniform_component(
    column: &Table,
    handler_key: &str,
    component: i64,
    baseline: f32,
) -> Result<Option<f32>, String> {
    const EPS: f32 = 0.001;
    let Some(handler) = column
        .get::<Option<Table>>(handler_key)
        .map_err(|err| err.to_string())?
    else {
        return Ok(None);
    };
    let mode = handler
        .get::<Option<String>>("__songlua_spline_mode")
        .map_err(|err| err.to_string())?
        .unwrap_or_default();
    if mode.eq_ignore_ascii_case("NoteColumnSplineMode_Disabled") {
        return Ok(Some(baseline));
    }
    let offset_mode = mode.eq_ignore_ascii_case("NoteColumnSplineMode_Offset");
    if !offset_mode && !mode.eq_ignore_ascii_case("NoteColumnSplineMode_Position") {
        return Ok(None);
    }
    let Some(spline) = handler
        .get::<Option<Table>>("__songlua_spline")
        .map_err(|err| err.to_string())?
    else {
        return Ok(None);
    };
    let size = spline
        .get::<Option<i64>>("__songlua_spline_size")
        .map_err(|err| err.to_string())?
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(0);
    if size == 0 {
        return Ok(Some(baseline));
    }
    let points = spline
        .get::<Table>("__songlua_spline_points")
        .map_err(|err| err.to_string())?;
    if !offset_mode && handler_key == "__songlua_zoom_handler" {
        return Ok(Some(baseline));
    }
    if !offset_mode {
        return Ok(Some(read_spline_point(&points, 1).map_err(|err| err.to_string())?
            [component as usize - 1]));
    }
    // Compile-session, single-thread hint: one index per XYZ component, warmed
    // by the first scan and destroyed with the Lua spline. Re-read both points
    // on every use, so in-place edits remain visible. A valid witness proves
    // nonuniformity with two reads; a miss scans at most the declared size.
    let witness_key = match component {
        1 => "__songlua_nonuniform_x",
        2 => "__songlua_nonuniform_y",
        _ => "__songlua_nonuniform_z",
    };
    if let Some(index) = spline
        .get::<Option<usize>>(witness_key)
        .map_err(|err| err.to_string())?
        && index <= size
    {
        let value_at = |index| -> Option<f32> {
            Some(read_spline_point(&points, index).ok()?[component as usize - 1])
        };
        if let Some((first, other)) = value_at(1).zip(value_at(index))
            && (other - first).abs() > EPS
        {
            return Ok(None);
        }
    }
    let mut uniform = None::<f32>;
    for index in 1..=size {
        let value = read_spline_point(&points, index).map_err(|err| err.to_string())?
            [component as usize - 1];
        if let Some(prior) = uniform {
            if (value - prior).abs() > EPS {
                spline
                    .set(witness_key, index)
                    .map_err(|err| err.to_string())?;
                return Ok(None);
            }
        } else {
            uniform = Some(value);
        }
    }
    let value = uniform.unwrap_or_default();
    Ok(Some(if offset_mode { baseline + value } else { value }))
}

fn note_column_position_x_offset(column: &Table) -> Result<Option<f32>, String> {
    let Some(handler) = column
        .get::<Option<Table>>("__songlua_pos_handler")
        .map_err(|err| err.to_string())?
    else {
        return Ok(None);
    };
    let mode = handler
        .get::<String>("__songlua_spline_mode")
        .map_err(|err| err.to_string())?;
    if mode.eq_ignore_ascii_case("NoteColumnSplineMode_Offset") {
        return note_column_handler_uniform_component(column, "__songlua_pos_handler", 1, 0.0);
    }
    Ok((mode.eq_ignore_ascii_case("NoteColumnSplineMode_Disabled")
        || mode.eq_ignore_ascii_case("NoteColumnSplineMode_Position"))
    .then_some(0.0))
}

// Two-point splines are linear even when Solve uses cubic interpolation.
// Keep point and handler validation together to avoid partial captures.
fn note_column_pos_spline(
    actor: &Table,
) -> Result<Option<[(SongLuaColumnTransformTarget, f32); 10]>, String> {
    use SongLuaColumnTransformTarget::*;
    let Some(handler) = actor
        .get::<Option<Table>>("__songlua_pos_handler")
        .map_err(|err| err.to_string())?
    else {
        return Ok(None);
    };
    let mode = handler
        .get::<String>("__songlua_spline_mode")
        .map_err(|err| err.to_string())?;
    if !mode.eq_ignore_ascii_case("NoteColumnSplineMode_Offset") {
        return Ok(None);
    }
    let spline = handler
        .get::<Table>("__songlua_spline")
        .map_err(|err| err.to_string())?;
    if spline
        .get::<i64>("__songlua_spline_size")
        .map_err(|err| err.to_string())?
        != 2
    {
        return Ok(None);
    }
    let points = spline
        .get::<Table>("__songlua_spline_points")
        .map_err(|err| err.to_string())?;
    let mut out = [(PositionEnabled, 0.0); 10];
    for point in 0..2u8 {
        let values = read_spline_point(&points, usize::from(point) + 1)
            .map_err(|err| err.to_string())?;
        for axis in 0..3u8 {
            let value = values[usize::from(axis)];
            if !value.is_finite() {
                return Ok(None);
            }
            out[usize::from(point) * 3 + usize::from(axis)] =
                (PositionPoint { point, axis }, value);
        }
    }
    let beats = handler
        .get::<f32>("__songlua_beats_per_t")
        .map_err(|err| err.to_string())?;
    let receptor = handler
        .get::<f32>("__songlua_receptor_t")
        .map_err(|err| err.to_string())?;
    if !beats.is_finite() || beats == 0.0 || !receptor.is_finite() {
        return Ok(None);
    }
    let subtract = handler
        .get::<bool>("__songlua_subtract_song_beat")
        .map_err(|err| err.to_string())?;
    out[6..].copy_from_slice(&[
        (PositionEnabled, 1.0),
        (PositionBeatsPerT, beats),
        (PositionReceptorT, receptor),
        (PositionSubtractBeat, if subtract { 1.0 } else { 0.0 }),
    ]);
    Ok(Some(out))
}

pub fn read_note_column_transform_samples_for_fields(
    note_fields: Vec<Table>,
) -> Result<Vec<SongLuaColumnTransformSample>, String> {
    let mut out = Vec::new();
    for note_field in note_fields {
        let Some(columns) = note_field
            .get::<Option<Table>>("__songlua_note_columns")
            .map_err(|err| err.to_string())?
        else {
            continue;
        };
        for column in columns.sequence_values::<Table>() {
            let column = column.map_err(|err| err.to_string())?;
            let Some(player) = column
                .get::<Option<i64>>("__songlua_player_index")
                .map_err(|err| err.to_string())?
                .and_then(|value| usize::try_from(value).ok())
            else {
                continue;
            };
            let Some(local_col) = column
                .get::<Option<i64>>("__songlua_column_index")
                .map_err(|err| err.to_string())?
                .and_then(|value| usize::try_from(value).ok())
            else {
                continue;
            };
            let mut push = |target, value: Option<f32>| {
                if let Some(value) = value.filter(|value| value.is_finite()) {
                    out.push(SongLuaColumnTransformSample {
                        player,
                        column: local_col,
                        target,
                        value,
                    });
                }
            };
            let position_spline = note_column_pos_spline(&column)?;
            let absolute = column
                .get::<Option<Table>>("__songlua_pos_handler")
                .map_err(|err| err.to_string())?
                .is_some_and(|handler| {
                    handler
                        .get::<String>("__songlua_spline_mode")
                        .is_ok_and(|mode| {
                            mode.eq_ignore_ascii_case("NoteColumnSplineMode_Position")
                        })
                });
            if position_spline.is_some() || absolute {
                // Clear the uniform translation when switching to a beat spline.
                push(SongLuaColumnTransformTarget::OffsetX, Some(0.0));
                push(SongLuaColumnTransformTarget::OffsetY, Some(0.0));
            } else {
                push(
                    SongLuaColumnTransformTarget::OffsetX,
                    note_column_position_x_offset(&column)?,
                );
                push(
                    SongLuaColumnTransformTarget::OffsetY,
                    note_column_pos_offset_y(&column)?,
                );
            }
            push(
                SongLuaColumnTransformTarget::Zoom,
                note_column_handler_uniform_component(&column, "__songlua_zoom_handler", 1, 1.0)?,
            );
            push(
                SongLuaColumnTransformTarget::RotationZ,
                note_column_handler_uniform_component(&column, "__songlua_rot_handler", 3, 0.0)?,
            );
            // Keep target order so frame capture uses the linear merge path.
            if let Some(spline) = position_spline {
                for (target, value) in spline {
                    push(target, Some(value));
                }
            } else {
                push(SongLuaColumnTransformTarget::PositionEnabled, Some(0.0));
            }
        }
    }
    Ok(out)
}

pub fn read_note_column_pos_samples_for_fields(
    note_fields: Vec<Table>,
) -> Result<Vec<SongLuaColumnOffsetSample>, String> {
    let mut out = Vec::new();
    for note_field in note_fields {
        let Some(columns) = note_field
            .get::<Option<Table>>("__songlua_note_columns")
            .map_err(|err| err.to_string())?
        else {
            continue;
        };
        for column in columns.sequence_values::<Table>() {
            let column = column.map_err(|err| err.to_string())?;
            let Some(player) = column
                .get::<Option<i64>>("__songlua_player_index")
                .map_err(|err| err.to_string())?
                .and_then(|value| usize::try_from(value).ok())
            else {
                continue;
            };
            let Some(local_col) = column
                .get::<Option<i64>>("__songlua_column_index")
                .map_err(|err| err.to_string())?
                .and_then(|value| usize::try_from(value).ok())
            else {
                continue;
            };
            let Some(y) = note_column_pos_offset_y(&column)? else {
                continue;
            };
            out.push(SongLuaColumnOffsetSample {
                player,
                column: local_col,
                y,
            });
        }
    }
    Ok(out)
}

pub struct SongLuaNoteColumnHandlerSnapshot {
    handler: Table,
    spline: Table,
    mode: Value,
    subtract_song_beat: Value,
    receptor_t: Value,
    beats_per_t: Value,
    spline_size: Value,
    spline_loop: Value,
    spline_points: Value,
}

pub struct SongLuaNoteFieldColumnSnapshot {
    player_actor: Table,
    note_field: Option<Table>,
    columns: Option<Table>,
    handlers: Vec<SongLuaNoteColumnHandlerSnapshot>,
}

pub fn snapshot_note_column_handlers(
    lua: &Lua,
    columns: &Table,
) -> mlua::Result<Vec<SongLuaNoteColumnHandlerSnapshot>> {
    let mut out = Vec::new();
    for column in columns.sequence_values::<Table>() {
        let column = column?;
        for key in [
            "__songlua_pos_handler",
            "__songlua_rot_handler",
            "__songlua_zoom_handler",
        ] {
            let Some(handler) = column.get::<Option<Table>>(key)? else {
                continue;
            };
            let Some(spline) = handler.get::<Option<Table>>("__songlua_spline")? else {
                continue;
            };
            out.push(SongLuaNoteColumnHandlerSnapshot {
                mode: clone_lua_value(lua, handler.get::<Value>("__songlua_spline_mode")?)?,
                subtract_song_beat: clone_lua_value(
                    lua,
                    handler.get::<Value>("__songlua_subtract_song_beat")?,
                )?,
                receptor_t: clone_lua_value(lua, handler.get::<Value>("__songlua_receptor_t")?)?,
                beats_per_t: clone_lua_value(lua, handler.get::<Value>("__songlua_beats_per_t")?)?,
                spline_size: clone_lua_value(lua, spline.get::<Value>("__songlua_spline_size")?)?,
                spline_loop: spline.get::<Value>("__songlua_spline_loop")?,
                spline_points: clone_lua_value(
                    lua,
                    spline.get::<Value>("__songlua_spline_points")?,
                )?,
                handler,
                spline,
            });
        }
    }
    Ok(out)
}

pub fn snapshot_note_field_columns(lua: &Lua) -> mlua::Result<Vec<SongLuaNoteFieldColumnSnapshot>> {
    let mut out = Vec::new();
    let globals = lua.globals();
    for player in 0..LUA_PLAYERS {
        let key = if player == 0 {
            "__songlua_top_screen_player_1"
        } else {
            "__songlua_top_screen_player_2"
        };
        let Some(player_actor) = globals.get::<Option<Table>>(key)? else {
            continue;
        };
        let note_field = note_field_table(lua, &player_actor)?;
        let columns = match &note_field {
            Some(note_field) => note_field.get::<Option<Table>>("__songlua_note_columns")?,
            None => None,
        };
        let handlers = match &columns {
            Some(columns) => snapshot_note_column_handlers(lua, columns)?,
            None => Vec::new(),
        };
        out.push(SongLuaNoteFieldColumnSnapshot {
            player_actor,
            note_field,
            columns,
            handlers,
        });
    }
    Ok(out)
}

pub fn restore_note_column_handlers(
    snapshots: Vec<SongLuaNoteColumnHandlerSnapshot>,
) -> mlua::Result<()> {
    for snapshot in snapshots {
        snapshot
            .handler
            .set("__songlua_spline_mode", snapshot.mode)?;
        snapshot
            .handler
            .set("__songlua_subtract_song_beat", snapshot.subtract_song_beat)?;
        snapshot
            .handler
            .set("__songlua_receptor_t", snapshot.receptor_t)?;
        snapshot
            .handler
            .set("__songlua_beats_per_t", snapshot.beats_per_t)?;
        snapshot
            .spline
            .set("__songlua_spline_size", snapshot.spline_size)?;
        snapshot
            .spline
            .set("__songlua_spline_loop", snapshot.spline_loop)?;
        snapshot
            .spline
            .set("__songlua_spline_points", snapshot.spline_points)?;
    }
    Ok(())
}

pub fn restore_note_field_columns(
    lua: &Lua,
    snapshots: Vec<SongLuaNoteFieldColumnSnapshot>,
) -> mlua::Result<()> {
    for snapshot in snapshots {
        match snapshot.note_field {
            Some(note_field) => {
                actor_children(lua, &snapshot.player_actor)?
                    .set("NoteField", note_field.clone())?;
                match snapshot.columns {
                    Some(columns) => note_field.set("__songlua_note_columns", columns.clone())?,
                    None => note_field.set("__songlua_note_columns", Value::Nil)?,
                }
            }
            None => {
                actor_children(lua, &snapshot.player_actor)?.set("NoteField", Value::Nil)?;
            }
        }
        restore_note_column_handlers(snapshot.handlers)?;
    }
    Ok(())
}

pub fn probe_target_kind(actor: &Table) -> mlua::Result<&'static str> {
    let Some(_player_index) = actor.get::<Option<i64>>("__songlua_player_index")? else {
        return Ok("overlay");
    };
    Ok(
        match actor
            .get::<Option<String>>("__songlua_player_child_name")?
            .as_deref()
        {
            None => "player",
            Some(name) if name.eq_ignore_ascii_case("NoteField") => "notefield",
            Some(name) if name.eq_ignore_ascii_case("Judgment") => "judgment",
            Some(name) if name.eq_ignore_ascii_case("Combo") => "combo",
            _ => "player-child",
        },
    )
}

pub fn record_probe_method_call(lua: &Lua, actor: &Table, method_name: &str) -> mlua::Result<()> {
    if lua.app_data_ref::<SongLuaProbeCaptureActive>().is_none() {
        return Ok(());
    }
    record_probe_actor_call(lua, actor)?;
    let globals = lua.globals();
    let Some(calls) = globals.get::<Option<Table>>(SONG_LUA_PROBE_METHODS_KEY)? else {
        return Ok(());
    };
    calls.raw_set(
        calls.raw_len() + 1,
        format!("{}.{}", probe_target_kind(actor)?, method_name),
    )?;
    Ok(())
}

fn record_probe_actor_call(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    if lua.app_data_ref::<SongLuaProbeCaptureActive>().is_none() {
        return Ok(());
    }
    let globals = lua.globals();
    let Some(actors) = globals.get::<Option<Table>>(SONG_LUA_PROBE_ACTORS_KEY)? else {
        return Ok(());
    };
    let Some(seen) = globals.get::<Option<Table>>(SONG_LUA_PROBE_ACTOR_SET_KEY)? else {
        return Ok(());
    };
    if seen
        .raw_get::<Option<bool>>(actor.clone())?
        .unwrap_or(false)
    {
        return Ok(());
    }
    seen.raw_set(actor.clone(), true)?;
    actors.raw_set(actors.raw_len() + 1, actor.clone())?;
    Ok(())
}

pub fn prepare_capture_scope_actor(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    if lua.app_data_ref::<SongLuaActionCaptureActive>().is_none() {
        return Ok(());
    }
    let globals = lua.globals();
    let Some(actors) = globals.get::<Option<Table>>(SONG_LUA_CAPTURE_ACTORS_KEY)? else {
        return Ok(());
    };
    let Some(seen) = globals.get::<Option<Table>>(SONG_LUA_CAPTURE_ACTOR_SET_KEY)? else {
        return Ok(());
    };
    let Some(snapshots) = globals.get::<Option<Table>>(SONG_LUA_CAPTURE_SNAPSHOTS_KEY)? else {
        return Ok(());
    };
    if seen
        .raw_get::<Option<bool>>(actor.clone())?
        .unwrap_or(false)
    {
        return Ok(());
    }
    seen.raw_set(actor.clone(), true)?;
    actors.raw_set(actors.raw_len() + 1, actor.clone())?;

    let snapshot = lua.create_table()?;
    snapshot.raw_set(1, actor.clone())?;
    snapshot.raw_set(2, snapshot_actor_semantic_state_table(lua, actor)?)?;
    snapshots.raw_set(snapshots.raw_len() + 1, snapshot)?;

    reset_actor_capture(lua, actor)?;
    Ok(())
}

pub fn classify_function_ease_probe(calls: &Table) -> mlua::Result<Option<SongLuaEaseTarget>> {
    let mut saw_x = false;
    let mut saw_y = false;
    let mut saw_z = false;
    let mut saw_rotation_x = false;
    let mut saw_rotation_z = false;
    let mut saw_rotation_y = false;
    let mut saw_skew_x = false;
    let mut saw_skew_y = false;
    let mut saw_zoom = false;
    let mut saw_zoom_x = false;
    let mut saw_zoom_y = false;
    let mut saw_zoom_z = false;
    for value in calls.sequence_values::<String>() {
        let value = value?;
        let (target_kind, method_name) =
            value.split_once('.').unwrap_or(("player", value.as_str()));
        if !matches!(target_kind, "player" | "overlay") {
            return Ok(None);
        }
        match method_name {
            "x" if target_kind == "player" => saw_x = true,
            "y" if target_kind == "player" => saw_y = true,
            "z" if target_kind != "overlay" => saw_z = true,
            "rotationx" if target_kind != "overlay" => saw_rotation_x = true,
            "rotationz" if target_kind != "overlay" => saw_rotation_z = true,
            "rotationy" if target_kind != "overlay" => saw_rotation_y = true,
            "skewx" if target_kind == "player" => saw_skew_x = true,
            "skewy" if target_kind == "player" => saw_skew_y = true,
            "zoom" if target_kind != "overlay" => saw_zoom = true,
            "zoomx" if target_kind != "overlay" => saw_zoom_x = true,
            "zoomy" if target_kind != "overlay" => saw_zoom_y = true,
            "zoomz" if target_kind != "overlay" => saw_zoom_z = true,
            _ => return Ok(None),
        }
    }
    Ok(
        match (
            saw_x,
            saw_y,
            saw_z,
            saw_rotation_x,
            saw_rotation_z,
            saw_rotation_y,
            saw_skew_x,
            saw_skew_y,
            saw_zoom,
            saw_zoom_x,
            saw_zoom_y,
            saw_zoom_z,
        ) {
            (true, false, false, false, false, false, false, false, false, false, false, false) => {
                Some(SongLuaEaseTarget::PlayerX)
            }
            (false, true, false, false, false, false, false, false, false, false, false, false) => {
                Some(SongLuaEaseTarget::PlayerY)
            }
            (false, false, true, false, false, false, false, false, false, false, false, false) => {
                Some(SongLuaEaseTarget::PlayerZ)
            }
            (false, false, false, true, false, false, false, false, false, false, false, false) => {
                Some(SongLuaEaseTarget::PlayerRotationX)
            }
            (false, false, false, false, true, false, false, false, false, false, false, false) => {
                Some(SongLuaEaseTarget::PlayerRotationZ)
            }
            (false, false, false, false, false, true, false, false, false, false, false, false) => {
                Some(SongLuaEaseTarget::PlayerRotationY)
            }
            (false, false, false, false, false, false, true, false, false, false, false, false) => {
                Some(SongLuaEaseTarget::PlayerSkewX)
            }
            (false, false, false, false, false, false, false, true, false, false, false, false) => {
                Some(SongLuaEaseTarget::PlayerSkewY)
            }
            (false, false, false, false, false, false, false, false, true, false, false, false) => {
                Some(SongLuaEaseTarget::PlayerZoom)
            }
            (false, false, false, false, false, false, false, false, false, true, false, false) => {
                Some(SongLuaEaseTarget::PlayerZoomX)
            }
            (false, false, false, false, false, false, false, false, false, false, true, false) => {
                Some(SongLuaEaseTarget::PlayerZoomY)
            }
            (false, false, false, false, false, false, false, false, false, false, false, true) => {
                Some(SongLuaEaseTarget::PlayerZoomZ)
            }
            _ => None,
        },
    )
}

pub fn probe_function_ease_target(
    lua: &Lua,
    function: &Function,
) -> mlua::Result<(Option<SongLuaEaseTarget>, Vec<String>, Vec<usize>)> {
    let globals = lua.globals();
    let previous_time = compile_song_runtime_values(lua)?;
    set_compile_song_runtime_beat(lua, 0.0)?;
    let previous_methods = globals.get::<Value>(SONG_LUA_PROBE_METHODS_KEY)?;
    let previous_actors = globals.get::<Value>(SONG_LUA_PROBE_ACTORS_KEY)?;
    let previous_actor_set = globals.get::<Value>(SONG_LUA_PROBE_ACTOR_SET_KEY)?;
    let calls = lua.create_table()?;
    let actors = lua.create_table()?;
    globals.set(SONG_LUA_PROBE_METHODS_KEY, calls.clone())?;
    globals.set(SONG_LUA_PROBE_ACTORS_KEY, actors.clone())?;
    globals.set(SONG_LUA_PROBE_ACTOR_SET_KEY, lua.create_table()?)?;
    let random = crate::compat::preserve_random(lua);
    lua.set_app_data(SongLuaProbeCaptureActive);
    let result = function.call::<Value>(1.0_f32);
    drop(random);
    lua.remove_app_data::<SongLuaProbeCaptureActive>();
    let methods = probe_call_names(&calls)?;
    let actor_ptrs = probe_actor_pointers(&actors)?;
    let classify = classify_function_ease_probe(&calls);
    globals.set(SONG_LUA_PROBE_METHODS_KEY, previous_methods)?;
    globals.set(SONG_LUA_PROBE_ACTORS_KEY, previous_actors)?;
    globals.set(SONG_LUA_PROBE_ACTOR_SET_KEY, previous_actor_set)?;
    set_compile_song_runtime_values(lua, previous_time.0, previous_time.1)?;
    match result {
        Ok(_) => Ok((classify?, methods, actor_ptrs)),
        Err(_) => Ok((None, methods, actor_ptrs)),
    }
}

pub fn probe_call_names(calls: &Table) -> mlua::Result<Vec<String>> {
    let mut out = Vec::new();
    for value in calls.sequence_values::<String>() {
        out.push(value?);
    }
    Ok(out)
}

pub fn probe_actor_pointers(actors: &Table) -> mlua::Result<Vec<usize>> {
    let mut out = Vec::new();
    for value in actors.sequence_values::<Table>() {
        out.push(value?.to_pointer() as usize);
    }
    Ok(out)
}

pub fn capture_scope_actor_pointers(actors: &Table) -> mlua::Result<HashSet<usize>> {
    let mut out = HashSet::with_capacity(actors.raw_len());
    for actor in actors.sequence_values::<Table>() {
        out.insert(actor?.to_pointer() as usize);
    }
    Ok(out)
}

pub fn capture_scope_actor_tables(actors: &Table) -> mlua::Result<Vec<Table>> {
    let mut out = Vec::with_capacity(actors.raw_len());
    for actor in actors.sequence_values::<Table>() {
        out.push(actor?);
    }
    Ok(out)
}

pub struct SongLuaActionCaptureScope {
    pub actors: Table,
    pub snapshots: Table,
    pub previous_actors: Value,
    pub previous_actor_set: Value,
    pub previous_snapshots: Value,
    previous_capture: Option<SongLuaActionCaptureActive>,
    random: Option<crate::compat::RandomSnapshot>,
}

pub fn begin_action_capture_scope(lua: &Lua) -> mlua::Result<SongLuaActionCaptureScope> {
    let globals = lua.globals();
    let previous_actors = globals.get::<Value>(SONG_LUA_CAPTURE_ACTORS_KEY)?;
    let previous_actor_set = globals.get::<Value>(SONG_LUA_CAPTURE_ACTOR_SET_KEY)?;
    let previous_snapshots = globals.get::<Value>(SONG_LUA_CAPTURE_SNAPSHOTS_KEY)?;
    let actors = lua.create_table()?;
    let snapshots = lua.create_table()?;
    globals.set(SONG_LUA_CAPTURE_ACTORS_KEY, actors.clone())?;
    globals.set(SONG_LUA_CAPTURE_ACTOR_SET_KEY, lua.create_table()?)?;
    globals.set(SONG_LUA_CAPTURE_SNAPSHOTS_KEY, snapshots.clone())?;
    let previous_capture = lua.remove_app_data::<SongLuaActionCaptureActive>();
    lua.set_app_data(SongLuaActionCaptureActive {
        functions: HashSet::new(),
        locals: Vec::new(),
    });
    Ok(SongLuaActionCaptureScope {
        actors,
        snapshots,
        previous_actors,
        previous_actor_set,
        previous_snapshots,
        previous_capture,
        random: crate::compat::preserve_random(lua),
    })
}

pub fn restore_action_capture_scope(
    lua: &Lua,
    scope: SongLuaActionCaptureScope,
) -> mlua::Result<()> {
    let capture = lua.remove_app_data::<SongLuaActionCaptureActive>();
    if let Some(mut previous) = scope.previous_capture {
        // Stability probes intentionally run twice with changing locals. A
        // nested scope restores actor captures but defers locals to its owner.
        if let Some(capture) = capture {
            previous.functions.extend(capture.functions);
            previous.locals.extend(capture.locals);
        }
        lua.set_app_data(previous);
    } else if let Some(capture) = capture {
        // Shared cells may be encountered by several commands. Undo the last
        // snapshot first so the earliest value wins, retaining table identity.
        for locals in capture.locals.into_iter().rev() {
            restore_function_action_tables(lua, locals)?;
        }
    }
    let globals = lua.globals();
    globals.set(SONG_LUA_CAPTURE_ACTORS_KEY, scope.previous_actors)?;
    globals.set(SONG_LUA_CAPTURE_ACTOR_SET_KEY, scope.previous_actor_set)?;
    globals.set(SONG_LUA_CAPTURE_SNAPSHOTS_KEY, scope.previous_snapshots)?;
    drop(scope.random);
    Ok(())
}

pub fn capture_scope_snapshots(
    snapshots: &Table,
) -> mlua::Result<Vec<(Table, Vec<(String, Value)>)>> {
    let mut out = Vec::with_capacity(snapshots.raw_len());
    for snapshot in snapshots.sequence_values::<Table>() {
        let snapshot = snapshot?;
        let actor = snapshot.raw_get(1)?;
        let state = read_actor_semantic_state_table(&snapshot.raw_get(2)?)?;
        out.push((actor, state));
    }
    Ok(out)
}

pub fn reset_actor_capture_tables(lua: &Lua, actors: &[Table]) -> Result<(), String> {
    for actor in actors {
        reset_actor_capture(lua, actor).map_err(|err| err.to_string())?;
    }
    Ok(())
}

pub fn reset_indexed_actor_capture_tables(
    lua: &Lua,
    actors: &[(usize, Table)],
) -> Result<(), String> {
    for (_, actor) in actors {
        reset_actor_capture(lua, actor).map_err(|err| err.to_string())?;
    }
    Ok(())
}

pub fn collect_indexed_actor_capture_blocks(
    actors: &[(usize, Table)],
) -> Result<Vec<(usize, Vec<SongLuaOverlayCommandBlock>)>, String> {
    collect_borrowed_actor_capture_blocks(actors.iter().map(|(index, actor)| (*index, actor)))
}

fn collect_borrowed_actor_capture_blocks<'a>(
    actors: impl IntoIterator<Item = (usize, &'a Table)>,
) -> Result<Vec<(usize, Vec<SongLuaOverlayCommandBlock>)>, String> {
    let mut out = Vec::new();
    for (index, actor) in actors {
        flush_actor_capture(actor).map_err(|err| err.to_string())?;
        let blocks = read_actor_capture_blocks(actor)?;
        if !blocks.is_empty() {
            out.push((index, blocks));
        }
    }
    Ok(out)
}

pub fn capture_indexed_actor_function_blocks(
    lua: &Lua,
    actors: &[(usize, Table)],
    function: &Function,
    arg: Option<f32>,
    song_beat: Option<f32>,
) -> Result<Vec<(usize, Vec<SongLuaOverlayCommandBlock>)>, String> {
    let previous = compile_song_runtime_values(lua).map_err(|err| err.to_string())?;
    if let Some(song_beat) = song_beat {
        set_compile_song_runtime_beat(lua, song_beat).map_err(|err| err.to_string())?;
    }
    let snapshot_tables: Vec<_> = actors.iter().map(|(_, actor)| actor.clone()).collect();
    let state_snapshot =
        snapshot_actors_semantic_state(lua, &snapshot_tables).map_err(|err| err.to_string())?;
    reset_indexed_actor_capture_tables(lua, actors)?;
    let result = match arg {
        Some(value) => function.call::<Value>(value),
        None => function.call::<Value>(()),
    };
    let blocks = collect_indexed_actor_capture_blocks(actors);
    reset_indexed_actor_capture_tables(lua, actors)?;
    restore_actors_semantic_state(state_snapshot).map_err(|err| err.to_string())?;
    set_compile_song_runtime_values(lua, previous.0, previous.1).map_err(|err| err.to_string())?;
    let blocks = blocks?;
    result.map_err(|err| err.to_string())?;
    Ok(blocks)
}

fn capture_touched_blocks(
    lua: &Lua,
    actors: &[(usize, Table)],
    function: &Function,
    arg: Option<f32>,
    song_beat: Option<f32>,
) -> Result<Vec<(usize, Vec<SongLuaOverlayCommandBlock>)>, String> {
    let previous = compile_song_runtime_values(lua).map_err(|err| err.to_string())?;
    if let Some(song_beat) = song_beat {
        set_compile_song_runtime_beat(lua, song_beat).map_err(|err| err.to_string())?;
    }
    let capture_scope = begin_action_capture_scope(lua).map_err(|err| err.to_string())?;
    let result = match arg {
        Some(value) => function.call::<Value>(value),
        None => function.call::<Value>(()),
    };
    let touched_actors =
        capture_scope_actor_tables(&capture_scope.actors).map_err(|err| err.to_string())?;
    let actor_ptrs =
        capture_scope_actor_pointers(&capture_scope.actors).map_err(|err| err.to_string())?;
    let state_snapshot =
        capture_scope_snapshots(&capture_scope.snapshots).map_err(|err| err.to_string())?;
    restore_action_capture_scope(lua, capture_scope).map_err(|err| err.to_string())?;
    let touched = actors
        .iter()
        .filter(|(_, actor)| actor_ptrs.contains(&(actor.to_pointer() as usize)))
        .cloned()
        .collect::<Vec<_>>();
    let blocks = collect_indexed_actor_capture_blocks(&touched);
    reset_actor_capture_tables(lua, &touched_actors)?;
    restore_actors_semantic_state(state_snapshot).map_err(|err| err.to_string())?;
    set_compile_song_runtime_values(lua, previous.0, previous.1).map_err(|err| err.to_string())?;
    let blocks = blocks?;
    result.map_err(|err| err.to_string())?;
    Ok(blocks)
}

pub fn capture_overlay_function_eases(
    lua: &Lua,
    overlays: &[(usize, Table)],
    function: &Function,
    unit: SongLuaTimeUnit,
    start: f32,
    limit: f32,
    span_mode: SongLuaSpanMode,
    from: f32,
    to: f32,
    easing: Option<String>,
    sustain: Option<f32>,
    opt1: Option<f32>,
    opt2: Option<f32>,
    probe_actor_ptrs: &[usize],
) -> Result<Vec<SongLuaOverlayEase>, String> {
    let start_beat = start;
    let end_beat = song_lua_span_end(start, limit, span_mode).max(start_beat);
    let probe_actor_ptrs = probe_actor_ptrs.iter().copied().collect::<HashSet<_>>();
    let overlay_positions = actor_indices_for_pointers(
        overlays.len(),
        |index| overlays[index].1.to_pointer() as usize,
        &probe_actor_ptrs,
    );
    let (from_blocks, to_blocks) = if overlay_positions.is_empty() {
        (
            capture_touched_blocks(lua, overlays, function, Some(from), Some(start_beat))?,
            capture_touched_blocks(lua, overlays, function, Some(to), Some(end_beat))?,
        )
    } else {
        let overlay_tables = overlay_positions
            .into_iter()
            .filter_map(|index| {
                overlays
                    .get(index)
                    .map(|(index, actor)| (*index, actor.clone()))
            })
            .collect::<Vec<_>>();
        (
            capture_indexed_actor_function_blocks(
                lua,
                &overlay_tables,
                function,
                Some(from),
                Some(start_beat),
            )?,
            capture_indexed_actor_function_blocks(
                lua,
                &overlay_tables,
                function,
                Some(to),
                Some(end_beat),
            )?,
        )
    };
    if from_blocks.is_empty() && to_blocks.is_empty() {
        return Ok(Vec::new());
    }
    Ok(overlay_eases_from_captures(
        overlays.len(),
        &from_blocks,
        &to_blocks,
        SongLuaOverlayEaseBuildParams {
            unit,
            start,
            limit,
            span_mode,
            easing,
            sustain,
            opt1,
            opt2,
        },
    ))
}

pub fn capture_overlay_compile_actor_function_eases<Kind>(
    lua: &Lua,
    overlays: &[SongLuaOverlayCompileActor<Kind>],
    function: &Function,
    unit: SongLuaTimeUnit,
    start: f32,
    limit: f32,
    span_mode: SongLuaSpanMode,
    from: f32,
    to: f32,
    easing: Option<String>,
    sustain: Option<f32>,
    opt1: Option<f32>,
    opt2: Option<f32>,
    probe_actor_ptrs: &[usize],
) -> Result<Vec<SongLuaOverlayEase>, String> {
    let indices: Vec<_> = (0..overlays.len()).collect();
    let overlay_tables = overlay_compile_actor_tables_for_indices(overlays, &indices);
    capture_overlay_function_eases(
        lua,
        &overlay_tables,
        function,
        unit,
        start,
        limit,
        span_mode,
        from,
        to,
        easing,
        sustain,
        opt1,
        opt2,
        probe_actor_ptrs,
    )
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SongLuaFunctionActionCapture {
    pub overlay_blocks: Vec<(usize, Vec<SongLuaOverlayCommandBlock>)>,
    pub tracked_blocks: Vec<(usize, Vec<SongLuaOverlayCommandBlock>)>,
    pub broadcasts: Vec<(String, bool)>,
    pub sound_paths: Vec<PathBuf>,
    pub side_effects: i64,
}

struct FunctionActionSnapshot {
    tables: Table,
    cells: Table,
}

fn snapshot_function_action_table(lua: &Lua, table: Table) -> mlua::Result<Table> {
    let entries = lua.create_table()?;
    let mut count = 0;
    // Keep the traversal key on Lua's stack. `pairs` clones it for its Rust
    // cursor, allocating a shared handle for each reference-valued key.
    table.for_each::<Value, Value>(|key, value| {
        count += 2;
        entries.raw_set(count - 1, key)?;
        entries.raw_set(count, value)?;
        Ok(())
    })?;
    lua.create_sequence_from([table, entries])
}

fn snapshot_function_action_tables(
    lua: &Lua,
    functions: &Table,
) -> mlua::Result<FunctionActionSnapshot> {
    let globals = lua.globals();
    // Resolve environments before capturing their shared pre-probe state.
    let targets = lua.create_table()?;
    let mut seen = HashSet::from([globals.to_pointer() as usize]);
    for function in functions.sequence_values::<Function>() {
        let function = function?;
        if let Some(environment) = function.environment() {
            let target = environment
                .raw_get::<Option<Table>>("__songlua_env_target")?
                .unwrap_or(environment);
            if seen.insert(target.to_pointer() as usize) {
                targets.raw_set(targets.raw_len() + 1, target)?;
            }
        }
    }
    let snapshots = lua.create_table()?;
    snapshots.raw_set(1, snapshot_function_action_table(lua, globals)?)?;
    for target in targets.sequence_values::<Table>() {
        snapshots.raw_set(snapshots.raw_len() + 1, snapshot_function_action_table(lua, target?)?)?;
    }
    snapshot_function_locals(lua, functions, snapshots)
}

fn snapshot_function_locals(
    lua: &Lua,
    functions: &Table,
    snapshots: Table,
) -> mlua::Result<FunctionActionSnapshot> {
    // Command probes may edit shared local tables or replace upvalue cells.
    // Preserve their identities, including cycles and aliases. Actor state
    // belongs to the existing action capture scope; C closures own host data.
    let cells = lua.create_table()?;
    let mut cell_count = 0;
    let mut seen = HashSet::new();
    // Global bindings are shallow snapshots. Preserve referenced table contents
    // alongside locals without following the entire host environment through _G.
    seen.insert(lua.globals().to_pointer() as usize);
    for snapshot in snapshots.sequence_values::<Table>() {
        seen.insert(snapshot?.raw_get::<Table>(1)?.to_pointer() as usize);
    }
    // Lua owns both entries and snapshot rows. Even two Rust Table handles per
    // saved table exhaust mlua's 8000-slot Lua 5.1 auxiliary stack for large
    // scripts such as Bad Trails. Keep only the current row in Rust.
    let pending = functions.clone();
    let mut pending_count = pending.raw_len();
    while pending_count > 0 {
        let value = pending.raw_get::<Value>(pending_count)?;
        pending.raw_set(pending_count, Value::Nil)?;
        pending_count -= 1;
        match value {
            Value::Function(function) => {
                let Some(environment) = function.environment() else { continue; };
                if !seen.insert(function.to_pointer() as usize) { continue; }
                let chunk = function.dump(true);
                let globals = lua.globals();
                let environment = environment
                    .raw_get::<Option<Table>>("__songlua_env_target")?
                    .unwrap_or(environment);
                let environment = (environment.to_pointer() != globals.to_pointer())
                    .then_some(environment);
                for namespace in [Some(globals), environment].into_iter().flatten() {
                    namespace.for_each::<Value, Value>(|key, value| {
                        if let Value::String(name) = key
                            && matches!(value, Value::Table(_) | Value::Function(_))
                        {
                            let name = name.as_bytes();
                            if !name.is_empty()
                                && chunk
                                    .windows(name.len())
                                    .any(|bytes| bytes == name.as_ref())
                            {
                                pending_count += 1;
                                pending.raw_set(pending_count, value)?;
                            }
                        }
                        Ok(())
                    })?;
                }
                for index in 1..=function_upvalue_count(lua, &function)? {
                    let (name, value) = read_function_upvalue(lua, function.clone(), index.into())?;
                    if matches!(&name, Value::String(name) if name.to_str()?.as_ref() == "_ENV") {
                        continue;
                    }
                    pending_count += 1;
                    pending.raw_set(pending_count, value.clone())?;
                    cell_count += 1;
                    cells.raw_set(
                        cell_count,
                        lua.create_sequence_from([
                            Value::Function(function.clone()),
                            Value::Integer(index.into()),
                            value,
                        ])?,
                    )?;
                }
            }
            Value::Table(table)
                if seen.insert(table.to_pointer() as usize)
                    && table.raw_get::<Value>("__songlua_actor_type")?.is_nil() =>
            {
                let snapshot = snapshot_function_action_table(lua, table)?;
                let entries = snapshot.raw_get::<Table>(2)?;
                for index in (2..=entries.raw_len()).step_by(2) {
                    pending_count += 1;
                    pending.raw_set(pending_count, entries.raw_get::<Value>(index)?)?;
                }
                snapshots.raw_set(snapshots.raw_len() + 1, snapshot)?;
            }
            _ => {}
        }
    }
    Ok(FunctionActionSnapshot {
        tables: snapshots,
        cells,
    })
}

fn function_upvalue_count(lua: &Lua, function: &Function) -> mlua::Result<u8> {
    // SAFETY: >u consumes a rooted function and fills only debug metadata.
    // It does not execute the function or allocate/throw in Lua 5.1.
    unsafe {
        crate::raw_vm::with_read_stack(lua, 1, |raw, _| {
            raw.push(function)?;
            let mut info: ffi::lua_Debug = std::mem::zeroed();
            assert_ne!(ffi::lua_getinfo(raw.state(), c">u".as_ptr(), &mut info), 0);
            Ok(info.nups as u8)
        })
    }
}

fn read_function_upvalue(
    lua: &Lua,
    function: Function,
    index: i64,
) -> mlua::Result<(Value, Value)> {
    // SAFETY: exec_raw owns this frame. Lua's debug API reads a pushed
    // function/index pair and returns a copied name/value pair. No pointers
    // or stack references survive the conversion to mlua Values.
    unsafe {
        lua.exec_raw((function, index), |state| {
            let name = ffi::lua_getupvalue(state, 1, ffi::lua_tointeger(state, 2) as c_int);
            ffi::lua_remove(state, 2);
            ffi::lua_remove(state, 1);
            if name.is_null() {
                ffi::lua_pushnil(state);
                ffi::lua_pushnil(state);
                return;
            }
            ffi::lua_pushstring(state, name);
            ffi::lua_insert(state, -2);
        })
    }
}

fn restore_function_action_tables(lua: &Lua, snapshot: FunctionActionSnapshot) -> mlua::Result<()> {
    for cell in snapshot.cells.sequence_values::<Table>() {
        let cell = cell?;
        let function = cell.raw_get::<Function>(1)?;
        let index = cell.raw_get::<i64>(2)?;
        let value = cell.raw_get::<Value>(3)?;
        // SAFETY: exec_raw owns the function, index and saved value on its
        // stack. lua_setupvalue consumes only the saved value; no Lua stack
        // references escape this call.
        unsafe {
            lua.exec_raw::<()>((function, index, value), |state| {
                ffi::lua_setupvalue(state, 1, ffi::lua_tointeger(state, 2) as c_int);
                ffi::lua_settop(state, 0);
            })?;
        }
    }
    for row in snapshot.tables.sequence_values::<Table>() {
        let row = row?;
        let table = row.raw_get::<Table>(1)?;
        table.clear()?;
        let entries = row.raw_get::<Table>(2)?;
        for index in (2..=entries.raw_len()).step_by(2) {
            table.raw_set(entries.raw_get::<Value>(index - 1)?, entries.raw_get::<Value>(index)?)?;
        }
    }
    Ok(())
}

pub fn capture_function_action_blocks(
    lua: &Lua,
    overlays: &[(usize, Table)],
    tracked_actors: &[SongLuaTrackedActor],
    function: &Function,
    beat: f32,
) -> Result<SongLuaFunctionActionCapture, String> {
    capture_function_action_blocks_inner(lua, overlays, tracked_actors, function, beat, true)
}

fn capture_function_action_blocks_inner(
    lua: &Lua,
    overlays: &[(usize, Table)],
    tracked_actors: &[SongLuaTrackedActor],
    function: &Function,
    beat: f32,
    restore_function_tables: bool,
) -> Result<SongLuaFunctionActionCapture, String> {
    let table_snapshots = restore_function_tables
        .then(|| {
            let functions = lua.create_sequence_from([function.clone()])?;
            snapshot_function_action_tables(lua, &functions)
        })
        .transpose()
        .map_err(|err| err.to_string())?;
    let previous = compile_song_runtime_values(lua).map_err(|err| err.to_string())?;
    let side_effect_before = song_lua_side_effect_count(lua).map_err(|err| err.to_string())?;
    let globals = lua.globals();
    let column_snapshot = snapshot_note_field_columns(lua).map_err(|err| err.to_string())?;
    let previous_broadcasts = globals
        .get::<Value>(SONG_LUA_BROADCASTS_KEY)
        .map_err(|err| err.to_string())?;
    let broadcast_table = lua.create_table().map_err(|err| err.to_string())?;
    globals
        .set(SONG_LUA_BROADCASTS_KEY, broadcast_table.clone())
        .map_err(|err| err.to_string())?;
    let previous_sound_calls = globals
        .get::<Value>(SONG_LUA_SOUND_CALLS_KEY)
        .map_err(|err| err.to_string())?;
    let sound_calls = lua.create_table().map_err(|err| err.to_string())?;
    globals
        .set(SONG_LUA_SOUND_CALLS_KEY, sound_calls.clone())
        .map_err(|err| err.to_string())?;
    let previous_suppress_broadcasts = globals
        .get::<Value>(SONG_LUA_SUPPRESS_BROADCAST_KEY)
        .map_err(|err| err.to_string())?;
    globals
        .set(SONG_LUA_SUPPRESS_BROADCAST_KEY, true)
        .map_err(|err| err.to_string())?;
    set_compile_song_runtime_beat(lua, beat).map_err(|err| err.to_string())?;
    let capture_scope = begin_action_capture_scope(lua).map_err(|err| err.to_string())?;
    let result = function.call::<Value>(());
    let touched_actors =
        capture_scope_actor_tables(&capture_scope.actors).map_err(|err| err.to_string())?;
    let actor_ptrs =
        capture_scope_actor_pointers(&capture_scope.actors).map_err(|err| err.to_string())?;
    let state_snapshot =
        capture_scope_snapshots(&capture_scope.snapshots).map_err(|err| err.to_string())?;
    restore_action_capture_scope(lua, capture_scope).map_err(|err| err.to_string())?;
    let overlay_tables: Vec<_> = overlays
        .iter()
        .filter(|(_, actor)| actor_ptrs.contains(&(actor.to_pointer() as usize)))
        .map(|(index, actor)| (*index, actor.clone()))
        .collect();
    let tracked_indices = tracked_indices_for_actor_pointers(tracked_actors, &actor_ptrs);
    let overlay_blocks = collect_indexed_actor_capture_blocks(&overlay_tables);
    let tracked_blocks =
        collect_tracked_capture_blocks_for_indices(tracked_actors, &tracked_indices);
    let broadcasts = read_song_lua_broadcasts(&broadcast_table).map_err(|err| err.to_string());
    let sound_paths = read_path_table(&sound_calls);
    restore_note_field_columns(lua, column_snapshot).map_err(|err| err.to_string())?;
    let side_effects = song_lua_side_effect_count(lua)
        .map_err(|err| err.to_string())?
        .saturating_sub(side_effect_before);
    reset_actor_capture_tables(lua, &touched_actors)?;
    restore_actors_semantic_state(state_snapshot).map_err(|err| err.to_string())?;
    globals
        .set(SONG_LUA_BROADCASTS_KEY, previous_broadcasts)
        .map_err(|err| err.to_string())?;
    globals
        .set(SONG_LUA_SOUND_CALLS_KEY, previous_sound_calls)
        .map_err(|err| err.to_string())?;
    globals
        .set(
            SONG_LUA_SUPPRESS_BROADCAST_KEY,
            previous_suppress_broadcasts,
        )
        .map_err(|err| err.to_string())?;
    set_compile_song_runtime_values(lua, previous.0, previous.1).map_err(|err| err.to_string())?;
    if let Some(table_snapshots) = table_snapshots {
        restore_function_action_tables(lua, table_snapshots).map_err(|err| err.to_string())?;
    }
    let overlay_blocks = overlay_blocks?;
    let tracked_blocks = tracked_blocks?;
    let broadcasts = broadcasts?;
    let sound_paths = sound_paths?;
    result.map_err(|err| err.to_string())?;
    Ok(SongLuaFunctionActionCapture {
        overlay_blocks,
        tracked_blocks,
        broadcasts,
        sound_paths,
        side_effects,
    })
}

fn actor_capture_effects(
    capture_blocks: &[(usize, Vec<SongLuaOverlayCommandBlock>)],
    source_index: usize,
) -> Vec<(usize, Vec<SongLuaOverlayCommandBlock>)> {
    let blocks = capture_blocks
        .iter()
        .filter(|(index, _)| *index != source_index);
    if capture_blocks.windows(2).all(|pair| pair[0].0 < pair[1].0) {
        blocks.cloned().collect()
    } else {
        // Preserve last-write-wins for duplicate actor indices.
        blocks
            .cloned()
            .collect::<std::collections::BTreeMap<_, _>>()
            .into_iter()
            .collect()
    }
}

fn message_capture_runner(
    lua: &Lua,
    source: &Table,
    command_name: &str,
    command: &Function,
) -> mlua::Result<Function> {
    let params = default_message_command_params(lua, command_name)?;
    let source = source.clone();
    let command = command.clone();
    let command_name = command_name.to_owned();
    lua.create_function(move |lua, ()| {
        run_guarded_actor_command(lua, &source, &command_name, &command, true, params.clone())?;
        // Dispatch already drains the receiver and actors it queues. Probing
        // a message must not execute unrelated work left on other actors.
        Ok(())
    })
}

pub(crate) fn capture_deferred_messages<Kind>(
    lua: &Lua,
    overlays: &mut [SongLuaOverlayCompileActor<Kind>],
    tracked_actors: &mut [SongLuaTrackedActor],
    runtime_captures: &[crate::SongLuaStatefulMessageCapture],
    skipped: &mut Vec<String>,
) -> Result<Vec<(usize, String, PathBuf)>, String> {
    let Some(deferred) = lua.remove_app_data::<SongLuaDeferredMessages>() else {
        return Ok(Vec::new());
    };
    let overlay_tables = overlays
        .iter()
        .enumerate()
        .map(|(index, actor)| (index, actor.table.clone()))
        .collect::<Vec<_>>();
    let beat = compile_song_runtime_values(lua)
        .map_err(|err| err.to_string())?
        .0;
    let mut sounds = Vec::new();
    // This pass owns command validation, source effects and cross-actor effects
    // together. A successful empty source block can still target both players.
    for deferred in deferred.0 {
        let Some(source_index) = overlays
            .iter()
            .position(|actor| actor.table.to_pointer() == deferred.actor.to_pointer())
        else {
            continue;
        };
        let message = deferred
            .command
            .strip_suffix("MessageCommand")
            .expect("message suffix");
        if deferred.observed
            || runtime_captures.iter().any(|capture| {
                capture.message == message
                    && capture
                        .overlay_targets
                        .iter()
                        .any(|(index, _)| *index == source_index)
            })
        {
            // The actual broadcast has already captured this receiver. Retrying
            // at song end would freeze callback upvalues at the wrong beat.
            skipped.retain(|detail| detail != &deferred.detail);
            continue;
        }
        let command = deferred
            .actor
            .get::<Function>(deferred.command.as_str())
            .map_err(|err| err.to_string())?;
        let snapshots = lua
            .create_sequence_from([command.clone()])
            .and_then(|functions| snapshot_function_action_tables(lua, &functions))
            .map_err(|err| err.to_string())?;
        let runner = message_capture_runner(lua, &deferred.actor, &deferred.command, &command)
            .map_err(|err| err.to_string())?;
        let local_scope = begin_action_capture_scope(lua).map_err(|err| err.to_string())?;
        let first = capture_function_action_blocks_inner(
            lua,
            &overlay_tables,
            tracked_actors,
            &runner,
            beat,
            false,
        );
        let second = first.as_ref().ok().map(|_| {
            capture_function_action_blocks_inner(
                lua,
                &overlay_tables,
                tracked_actors,
                &runner,
                beat,
                false,
            )
        });
        restore_action_capture_scope(lua, local_scope).map_err(|err| err.to_string())?;
        restore_function_action_tables(lua, snapshots).map_err(|err| err.to_string())?;
        let (Ok(first), Some(Ok(second))) = (first, second) else {
            continue;
        };
        // Every captured PlayOnce increments the side-effect counter once.
        // Preserve repeated calls, while rejecting music changes, missing sound
        // resources and other operations that cannot be replayed by these blocks.
        if first != second
            || first.side_effects > first.sound_paths.len() as i64
            || !first.broadcasts.is_empty()
        {
            // Keep the original diagnostic when a command cannot be represented
            // by stable blocks. Runtime writes remain captured at each broadcast.
            continue;
        }
        let message = deferred
            .command
            .strip_suffix("MessageCommand")
            .expect("message suffix");
        for (index, blocks) in actor_capture_effects(&first.overlay_blocks, usize::MAX) {
            overlays[index]
                .actor
                .message_commands
                .push(SongLuaOverlayMessageCommand {
                    frame_advance: 0.0,
                    message: message.to_owned(),
                    blocks,
                });
        }
        for (index, blocks) in actor_capture_effects(&first.tracked_blocks, usize::MAX) {
            tracked_actors[index]
                .actor
                .message_commands
                .push(SongLuaOverlayMessageCommand {
                    frame_advance: 0.0,
                    message: message.to_owned(),
                    blocks,
                });
        }
        sounds.extend(
            first
                .sound_paths
                .into_iter()
                .map(|path| (source_index, message.to_owned(), path)),
        );
        skipped.retain(|detail| detail != &deferred.detail);
    }
    Ok(sounds)
}

pub fn capture_stable_cross_actor_message_commands<Kind>(
    lua: &Lua,
    overlays: &mut [SongLuaOverlayCompileActor<Kind>],
    mut on_dynamic: impl FnMut(String),
) -> Result<(), String> {
    let overlay_tables = overlays
        .iter()
        .enumerate()
        .map(|(index, overlay)| (index, overlay.table.clone()))
        .collect::<Vec<_>>();
    let mut commands = Vec::new();
    for (source_index, source) in overlays.iter().enumerate() {
        for_each_actor_message_command(&source.table, |name, function| {
            let message = name
                .strip_suffix("MessageCommand")
                .expect("filtered suffix")
                .to_owned();
            commands.push((
                source_index,
                source.table.clone(),
                name,
                message,
                function,
            ));
        })
        .map_err(|err| err.to_string())?;
    }

    let mut additions = Vec::new();
    for (source_index, source, command_name, message, command) in commands {
        let table_snapshots = lua
            .create_sequence_from([command.clone()])
            .and_then(|functions| snapshot_function_action_tables(lua, &functions))
            .map_err(|err| err.to_string())?;
        let runner = message_capture_runner(lua, &source, &command_name, &command)
            .map_err(|err| err.to_string())?;
        let local_scope = begin_action_capture_scope(lua).map_err(|err| err.to_string())?;
        let first =
            capture_function_action_blocks_inner(lua, &overlay_tables, &[], &runner, 0.0, false);
        let second = first.as_ref().ok().map(|_| {
            capture_function_action_blocks_inner(lua, &overlay_tables, &[], &runner, 0.0, false)
        });
        restore_action_capture_scope(lua, local_scope).map_err(|err| err.to_string())?;
        restore_function_action_tables(lua, table_snapshots).map_err(|err| err.to_string())?;
        let Ok(first) = first else {
            // The ordinary per-actor capture already records this command as
            // skipped. Cross-actor probing must not turn that into a hard error.
            continue;
        };
        let second = match second.expect("second capture exists after a successful first capture") {
            Ok(second) => second,
            Err(err) => {
                let detail = defer_actor_message(
                    lua,
                    &source,
                    &command_name,
                    format!(
                        "{}.{} cannot be replayed for stable cross-actor capture: {err}",
                        actor_debug_label(&source),
                        command_name
                    ),
                );
                on_dynamic(detail);
                continue;
            }
        };
        let first = actor_capture_effects(&first.overlay_blocks, source_index);
        let second = actor_capture_effects(&second.overlay_blocks, source_index);
        if first.is_empty() {
            continue;
        }
        if first != second {
            let detail = defer_actor_message(
                lua,
                &source,
                &command_name,
                format!(
                    "{}.{} changes cross-actor targets or effects between runs",
                    actor_debug_label(&source),
                    command_name
                ),
            );
            on_dynamic(detail);
            continue;
        }
        additions.extend(first.into_iter().map(|(target, blocks)| {
            (
                target,
                SongLuaOverlayMessageCommand {
                    frame_advance: 0.0,
                    message: message.clone(),
                    blocks,
                },
            )
        }));
    }
    for (target, command) in additions {
        overlays[target].actor.message_commands.push(command);
    }
    Ok(())
}

fn read_path_table(table: &Table) -> Result<Vec<PathBuf>, String> {
    table
        .sequence_values::<String>()
        .map(|path| path.map(PathBuf::from).map_err(|err| err.to_string()))
        .collect()
}

pub fn capture_overlay_compile_actor_function_action_blocks<Kind>(
    lua: &Lua,
    overlays: &[SongLuaOverlayCompileActor<Kind>],
    tracked_actors: &[SongLuaTrackedActor],
    function: &Function,
    beat: f32,
) -> Result<SongLuaFunctionActionCapture, String> {
    let indices: Vec<_> = (0..overlays.len()).collect();
    let overlay_tables = overlay_compile_actor_tables_for_indices(overlays, &indices);
    capture_function_action_blocks(lua, &overlay_tables, tracked_actors, function, beat)
}

pub fn compile_overlay_compile_actor_function_action<Kind>(
    lua: &Lua,
    overlays: &mut [SongLuaOverlayCompileActor<Kind>],
    tracked_actors: &mut [SongLuaTrackedActor],
    function: &Function,
    beat: f32,
    persists: bool,
    counter: &mut usize,
    messages: &mut Vec<SongLuaMessageEvent>,
    sound_events: &mut Vec<SongLuaSoundEvent>,
) -> Result<bool, String> {
    let capture = capture_overlay_compile_actor_function_action_blocks(
        lua,
        overlays,
        tracked_actors,
        function,
        beat,
    )?;
    sound_events.extend(
        capture
            .sound_paths
            .iter()
            .cloned()
            .map(|path| SongLuaSoundEvent {
                beat,
                path,
                source_message: None,
                queued_startup: false,
            }),
    );
    let mut handled_broadcast = false;
    if !capture.broadcasts.is_empty()
        && capture
            .broadcasts
            .iter()
            .all(|(_, has_params)| !*has_params)
    {
        for (message, _) in &capture.broadcasts {
            if overlay_compile_actors_have_message_listener(overlays, tracked_actors, &message) {
                messages.push(SongLuaMessageEvent {
                    beat,
                    message: message.clone(),
                    persists,
                });
                handled_broadcast = true;
            }
        }
    }

    let has_direct_effects =
        !capture.overlay_blocks.is_empty() || !capture.tracked_blocks.is_empty();
    if handled_broadcast && !has_direct_effects {
        return Ok(true);
    }
    if !has_direct_effects {
        return Ok(capture.side_effects > 0);
    }

    let message = format!("__songlua_overlay_fn_action_{counter}");
    *counter += 1;
    for (overlay_index, blocks) in capture.overlay_blocks {
        overlays[overlay_index]
            .actor
            .message_commands
            .push(SongLuaOverlayMessageCommand {
                frame_advance: 0.0,
                message: message.clone(),
                blocks,
            });
    }
    for (tracked_index, blocks) in capture.tracked_blocks {
        tracked_actors[tracked_index]
            .actor
            .message_commands
            .push(SongLuaOverlayMessageCommand {
                frame_advance: 0.0,
                message: message.clone(),
                blocks,
            });
    }
    messages.push(SongLuaMessageEvent {
        beat,
        message,
        persists,
    });
    Ok(true)
}

pub fn read_overlay_compile_actor_actions<Kind>(
    lua: &Lua,
    table: Option<Table>,
    overlays: &mut [SongLuaOverlayCompileActor<Kind>],
    tracked_actors: &mut [SongLuaTrackedActor],
    messages: &mut Vec<SongLuaMessageEvent>,
    sound_events: &mut Vec<SongLuaSoundEvent>,
    counter: &mut usize,
    info: &mut SongLuaCompileInfo,
) -> Result<(), String> {
    read_actions_with_function_capture(table, messages, |input, messages| {
        if !matches!(
            compile_overlay_compile_actor_function_action(
                lua,
                overlays,
                tracked_actors,
                &input.function,
                input.beat,
                input.persists,
                counter,
                messages,
                sound_events,
            ),
            Ok(true)
        ) {
            let detail =
                record_unsupported_function_action_capture(info, input.beat, input.persists);
            log::debug!("Unsupported song lua function action capture: {detail}");
        }
        Ok(())
    })
}

fn overlay_compile_actors_have_message_listener<Kind>(
    overlays: &[SongLuaOverlayCompileActor<Kind>],
    tracked_actors: &[SongLuaTrackedActor],
    message: &str,
) -> bool {
    message_command_lists_have_listener(
        overlays
            .iter()
            .map(|overlay| overlay.actor.message_commands.as_slice())
            .chain(
                tracked_actors
                    .iter()
                    .map(|actor| actor.actor.message_commands.as_slice()),
            ),
        message,
    )
}

pub fn reset_tracked_capture_tables(
    lua: &Lua,
    tracked_actors: &[SongLuaTrackedActor],
) -> Result<(), String> {
    for actor in tracked_actors {
        reset_actor_capture(lua, &actor.table).map_err(|err| err.to_string())?;
    }
    Ok(())
}

pub fn collect_tracked_capture_blocks_for_indices(
    tracked_actors: &[SongLuaTrackedActor],
    indices: &[usize],
) -> Result<Vec<(usize, Vec<SongLuaOverlayCommandBlock>)>, String> {
    let actors = indices
        .iter()
        .filter_map(|&index| tracked_actors.get(index).map(|actor| (index, &actor.table)));
    collect_borrowed_actor_capture_blocks(actors)
}

pub fn tracked_song_lua_actor(
    table: Table,
    target: SongLuaTrackedActorTarget,
) -> Result<SongLuaTrackedActor, String> {
    // Whole-Player draws now replay their individual draw records. The ordinary
    // Player source must retain the actual state after the callback, including
    // hiding/resetting it; replacing it with the last Draw resurrects originals.
    let draw_state = if matches!(target, SongLuaTrackedActorTarget::Player(_)) {
        None
    } else {
        table
            .get::<Option<Table>>(MANUAL_DRAW_STATE_KEY)
            .map_err(|err| err.to_string())?
    };
    let initial_state = match draw_state {
        Some(draw_state) => actor_overlay_initial_state(&draw_state)?,
        None => actor_overlay_initial_state(&table)?,
    };
    let manual_hud_draw = if matches!(target, SongLuaTrackedActorTarget::Player(_)) {
        manually_draws_player_hud(&table)?
    } else {
        false
    };
    Ok(SongLuaTrackedActor {
        actor: SongLuaCapturedActor {
            initial_state,
            message_commands: Vec::new(),
            manual_hud_draw,
            ..SongLuaCapturedActor::default()
        },
        table,
        target,
    })
}

fn manually_draws_player_hud(player: &Table) -> Result<bool, String> {
    let Some(children) = player
        .get::<Option<Table>>("__songlua_children")
        .map_err(|err| err.to_string())?
    else {
        return Ok(false);
    };
    for name in ["Judgment", "Combo"] {
        let Some(child) = children
            .get::<Option<Table>>(name)
            .map_err(|err| err.to_string())?
        else {
            continue;
        };
        if child
            .get::<Option<Table>>(MANUAL_DRAW_STATE_KEY)
            .map_err(|err| err.to_string())?
            .is_some()
        {
            return Ok(true);
        }
    }
    Ok(false)
}

pub fn tracked_indices_for_actor_pointers(
    tracked_actors: &[SongLuaTrackedActor],
    actor_ptrs: &HashSet<usize>,
) -> Vec<usize> {
    actor_indices_for_pointers(
        tracked_actors.len(),
        |index| tracked_actors[index].table.to_pointer() as usize,
        actor_ptrs,
    )
}

pub fn actor_indices_for_pointers(
    len: usize,
    mut pointer_at: impl FnMut(usize) -> usize,
    actor_ptrs: &HashSet<usize>,
) -> Vec<usize> {
    (0..len)
        .filter(|&index| actor_ptrs.contains(&pointer_at(index)))
        .collect()
}

pub fn actor_pointers_touch_actor(
    len: usize,
    mut pointer_at: impl FnMut(usize) -> usize,
    actor_ptrs: &[usize],
) -> bool {
    const LINEAR_MAX: usize = 8;
    if actor_ptrs.is_empty() {
        return false;
    }
    if actor_ptrs.len() <= LINEAR_MAX {
        return (0..len).any(|index| actor_ptrs.contains(&pointer_at(index)));
    }
    // Dense mod charts can probe hundreds of actor tables at once. Keep the
    // compilation scratch inline through that range; pathological charts spill
    // once here, never on a gameplay frame.
    let mut sorted = smallvec::SmallVec::<[usize; 512]>::from_slice(actor_ptrs);
    sorted.sort_unstable();
    sorted.dedup();
    (0..len).any(|index| sorted.binary_search(&pointer_at(index)).is_ok())
}

pub fn read_color_args(args: &MultiValue) -> Option<[f32; 4]> {
    read_color_values(args.iter().skip(method_arg_offset(args)))
}

struct CaptureResetKeys(Table);

fn capture_reset_keys(lua: &Lua) -> mlua::Result<Table> {
    if let Some(keys) = lua.app_data_ref::<CaptureResetKeys>() {
        return Ok(keys.0.clone());
    }
    let keys = lua.create_sequence_from([
        "__songlua_capture_cursor", "__songlua_capture_duration",
        "__songlua_capture_tween_time_left", "__songlua_startup_queue_seconds",
        "__songlua_capture_easing", "__songlua_capture_opt1", "__songlua_capture_opt2",
        "__songlua_capture_block", "__songlua_capture_immediate_block",
    ])?;
    lua.set_app_data(CaptureResetKeys(keys.clone()));
    Ok(keys)
}

pub fn reset_actor_capture(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    if live_actor_playback(lua) { return Ok(()); }
    if crate::live::active(lua) {
        let keys = capture_reset_keys(lua)?;
        // These are private audit fields, not actor commands. Reset them under
        // one VM lock, using keys interned once for this VM. The compiler must
        // still replace the blocks table: saved captures can retain the old one.
        // SAFETY: exec_raw roots actor/keys at indices 1/2. Two reserved slots
        // hold an interned key and primitive value; rawset consumes both.
        unsafe {
            lua.exec_raw::<()>((actor, keys), |state| {
                ffi::luaL_checkstack(state, 2, c"capture reset".as_ptr());
                for index in 1..=3 {
                    ffi::lua_rawgeti(state, 2, index);
                    ffi::lua_pushnumber(state, 0.0);
                    ffi::lua_rawset(state, 1);
                }
                for index in 4..=9 {
                    ffi::lua_rawgeti(state, 2, index);
                    ffi::lua_pushnil(state);
                    ffi::lua_rawset(state, 1);
                }
                ffi::lua_settop(state, 0);
            })?;
        }
        if let Some(blocks) = actor.raw_get::<Option<Table>>("__songlua_capture_blocks")? {
            blocks.clear()?;
        } else {
            actor.raw_set("__songlua_capture_blocks", lua.create_table()?)?;
        }
        return Ok(());
    }
    actor.raw_set("__songlua_capture_cursor", 0.0_f32)?;
    actor.raw_set("__songlua_startup_queue_seconds", Value::Nil)?;
    actor.raw_set("__songlua_capture_duration", 0.0_f32)?;
    actor.raw_set("__songlua_capture_tween_time_left", 0.0_f32)?;
    actor.raw_set("__songlua_capture_easing", Value::Nil)?;
    actor.raw_set("__songlua_capture_opt1", Value::Nil)?;
    actor.raw_set("__songlua_capture_opt2", Value::Nil)?;
    actor.raw_set("__songlua_capture_blocks", lua.create_table()?)?;
    actor.raw_set("__songlua_capture_block", Value::Nil)?;
    actor.raw_set("__songlua_capture_immediate_block", Value::Nil)?;
    Ok(())
}

pub fn flush_actor_capture(actor: &Table) -> mlua::Result<()> {
    let Some(block) = actor.get::<Option<Table>>("__songlua_capture_block")? else {
        return Ok(());
    };
    let duration = block
        .get::<Option<f32>>("duration")?
        .unwrap_or(0.0)
        .max(0.0);
    let cursor = block.get::<Option<f32>>("start")?.unwrap_or(0.0);
    if block
        .get::<Option<bool>>("__songlua_has_changes")?
        .unwrap_or(false)
    {
        let blocks: Table = actor.get("__songlua_capture_blocks")?;
        let index = blocks.raw_len() + 1;
        blocks.raw_set(index, block)?;
    }
    actor.set("__songlua_capture_cursor", cursor + duration)?;
    actor.set("__songlua_capture_duration", 0.0_f32)?;
    actor.set("__songlua_capture_easing", Value::Nil)?;
    actor.set("__songlua_capture_opt1", Value::Nil)?;
    actor.set("__songlua_capture_opt2", Value::Nil)?;
    actor.set("__songlua_capture_block", Value::Nil)?;
    Ok(())
}

pub fn actor_tween_time_left(lua: &Lua, actor: &Table) -> mlua::Result<f32> {
    if let Some(state) = actor.raw_get::<Option<u8>>("__songlua_transition_state")? {
        return if state == 1 {
            actor.raw_get::<Option<Table>>("__songlua_transition_child")?
                .map_or(Ok(0.0), |child| actor_tween_time_left(lua, &child))
        } else { Ok(0.0) };
    }
    let hibernate = if actor
        .raw_get::<Option<bool>>("__songlua_theme_hibernating")?
        .unwrap_or(false)
    {
        f32::INFINITY
    } else {
        actor
            .raw_get::<Option<f32>>("__songlua_hibernate_seconds")?
            .unwrap_or(0.0)
    };
    let native = lua.app_data_ref::<ActorTweenReplays>().and_then(|replays| {
        replays.0.get(&(actor.to_pointer() as usize)).map(|replay| {
            replay
                .queue
                .iter()
                .fold(hibernate, |left, step| left + step.left)
        })
    });
    let mut left = match native {
        Some(left) => left,
        None => {
            hibernate
                + actor
                    .get::<Option<f32>>("__songlua_capture_tween_time_left")?
                    .unwrap_or(0.0)
                    .max(0.0)
        }
    };
    for child in actor.sequence_values::<Value>() {
        if let Value::Table(child) = child? {
            let child = hibernate + actor_tween_time_left(lua, &child)?;
            if left < child {
                left = child;
            }
        }
    }
    Ok(left)
}

pub(crate) fn start_actor_transition(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    if actor.raw_get::<Option<u8>>("__songlua_transition_state")? != Some(0) {
        return Ok(());
    }
    if let Some(child) = actor.raw_get::<Option<Table>>("__songlua_transition_child")? {
        run_actor_named_command(lua, &child, "StartTransitioningCommand")?;
        run_named_command_on_children_recursively(lua, &child, "StartTransitioningCommand", None)?;
        actor.raw_set("__songlua_transition_state", 1_u8)?;
    }
    Ok(())
}

fn scale_actor_capture_f32(actor: &Table, key: &str, scale: f32) -> mlua::Result<()> {
    let value = actor.get::<Option<f32>>(key)?.unwrap_or(0.0);
    actor.set(key, value * scale)
}

pub fn hurry_actor_tweening(lua: &Lua, actor: &Table, factor: f32) -> mlua::Result<()> {
    flush_actor_capture(actor)?;
    // Actor::HurryTweening multiplies duration and remaining time together,
    // preserving the current interpolation fraction. Hibernation is separate.
    let scale = factor;
    if let Some(mut replays) = lua.app_data_mut::<ActorTweenReplays>()
        && let Some(replay) = replays.0.get_mut(&(actor.to_pointer() as usize)) {
        for step in &mut replay.queue {
            step.duration *= scale;
            step.left *= scale;
        }
    }
    if let Some(mut frames) = lua.app_data_mut::<SongLuaCompileFrames>()
        && let Some(clock) = frames.clocks.get_mut(&(actor.to_pointer() as usize)) {
        for seconds in clock.steps.iter_mut().flatten() {
            *seconds = f64::from(*seconds as f32 * scale);
        }
        clock.progress = None;
    }
    if let Some(pose) = actor.raw_get::<Option<Table>>("__songlua_state_spin")? {
        let queue: Table = pose.get("queue")?;
        for index in pose.get::<usize>("head")?..=pose.get::<usize>("tail")? {
            if let Some(tween) = queue.raw_get::<Option<Table>>(index)? {
                scale_actor_capture_f32(&tween, "duration", scale)?;
                scale_actor_capture_f32(&tween, "left", scale)?;
            }
        }
    }
    if let Some(steps) = actor.get::<Option<Table>>("__songlua_command_queue_steps")? {
        for index in 1..=steps.raw_len() {
            if let Some(duration) = read_f32(steps.raw_get::<Value>(index)?) {
                steps.raw_set(index, duration * scale)?;
            }
        }
    }
    if let Some(seconds) = actor.get::<Option<f64>>("__songlua_startup_queue_seconds")? {
        actor.set(
            "__songlua_startup_queue_seconds",
            seconds * f64::from(scale),
        )?;
    }
    if let Some(blocks) = actor.get::<Option<Table>>("__songlua_capture_blocks")? {
        for block in blocks.sequence_values::<Table>() {
            let block = block?;
            let start = block.get::<Option<f32>>("start")?.unwrap_or(0.0);
            let duration = block.get::<Option<f32>>("duration")?.unwrap_or(0.0);
            block.set("start", start * scale)?;
            block.set("duration", duration * scale)?;
        }
    }
    scale_actor_capture_f32(actor, "__songlua_capture_cursor", scale)?;
    scale_actor_capture_f32(actor, "__songlua_capture_duration", scale)?;
    scale_actor_capture_f32(actor, "__songlua_capture_tween_time_left", scale)?;
    for key in ["__songlua_recurring_update_exact_interval", "__songlua_recurring_update_time_left"] {
        if let Some(value) = actor.raw_get::<Option<f64>>(key)? {
            actor.raw_set(key, value * f64::from(scale))?;
        }
    }
    // ActorFrame propagates HurryTweening to its owned children.
    for child in actor.sequence_values::<Table>() {
        hurry_actor_tweening(lua, &child?, factor)?;
    }
    Ok(())
}

fn is_capture_block_meta_key(key: &str) -> bool {
    matches!(
        key,
        "start"
            | "duration"
            | "easing"
            | "opt1"
            | "opt2"
            | "__songlua_has_changes"
            | "__songlua_queued_block"
    )
}

pub fn finish_actor_tweening(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    flush_actor_capture(actor)?;
    finish_pending_tweens(lua, actor)?;
    if live_actor_playback(lua) { return Ok(()); }
    let final_block = lua.create_table()?;
    let mut has_changes = false;
    if let Some(blocks) = actor.get::<Option<Table>>("__songlua_capture_blocks")? {
        for block in blocks.sequence_values::<Table>() {
            for pair in block?.pairs::<Value, Value>() {
                let (key, value) = pair?;
                if matches!(
                    &key,
                    Value::String(text)
                        if text
                            .to_str()
                            .ok()
                            .is_some_and(|name| is_capture_block_meta_key(&name))
                ) {
                    continue;
                }
                final_block.set(clone_lua_value(lua, key)?, clone_lua_value(lua, value)?)?;
                has_changes = true;
            }
        }
    }

    let blocks = lua.create_table()?;
    if has_changes {
        final_block.set("start", 0.0_f32)?;
        final_block.set("duration", 0.0_f32)?;
        final_block.set("easing", Value::Nil)?;
        final_block.set("opt1", Value::Nil)?;
        final_block.set("opt2", Value::Nil)?;
        final_block.set("__songlua_has_changes", true)?;
        blocks.raw_set(1, final_block)?;
    }
    actor.set("__songlua_capture_blocks", blocks)?;
    actor.set("__songlua_capture_block", Value::Nil)?;
    actor.set("__songlua_capture_cursor", 0.0_f32)?;
    actor.set("__songlua_capture_duration", 0.0_f32)?;
    actor.set("__songlua_capture_tween_time_left", 0.0_f32)?;
    actor.set("__songlua_capture_easing", Value::Nil)?;
    actor.set("__songlua_capture_opt1", Value::Nil)?;
    actor.set("__songlua_capture_opt2", Value::Nil)?;
    Ok(())
}

pub fn read_actor_capture_blocks(actor: &Table) -> Result<Vec<SongLuaOverlayCommandBlock>, String> {
    let Some(blocks) = actor
        .get::<Option<Table>>("__songlua_capture_blocks")
        .map_err(|err| err.to_string())?
    else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for value in blocks.sequence_values::<Value>() {
        let Value::Table(block) = value.map_err(|err| err.to_string())? else {
            continue;
        };
        let start = block
            .get::<Option<f32>>("start")
            .map_err(|err| err.to_string())?
            .unwrap_or(0.0);
        let duration = block
            .get::<Option<f32>>("duration")
            .map_err(|err| err.to_string())?
            .unwrap_or(0.0)
            .max(0.0);
        let easing = block
            .get::<Option<String>>("easing")
            .map_err(|err| err.to_string())?;
        out.push(SongLuaOverlayCommandBlock {
            progress: None,
            queued: block
                .get::<Option<bool>>("__songlua_queued_block")
                .map_err(|err| err.to_string())?
                .unwrap_or(false),
            start,
            duration,
            easing,
            opt1: block
                .get::<Option<f32>>("opt1")
                .map_err(|err| err.to_string())?,
            opt2: block
                .get::<Option<f32>>("opt2")
                .map_err(|err| err.to_string())?,
            delta: SongLuaOverlayStateDelta {
                aux: block
                    .get::<Option<f32>>("aux")
                    .map_err(|err| err.to_string())?,
                x: block
                    .get::<Option<f32>>("x")
                    .map_err(|err| err.to_string())?,
                y: block
                    .get::<Option<f32>>("y")
                    .map_err(|err| err.to_string())?,
                z: block
                    .get::<Option<f32>>("z")
                    .map_err(|err| err.to_string())?,
                z_bias: block
                    .get::<Option<f32>>("z_bias")
                    .map_err(|err| err.to_string())?,
                draw_order: block
                    .get::<Option<i32>>("draw_order")
                    .map_err(|err| err.to_string())?,
                draw_by_z_position: block
                    .get::<Option<bool>>("draw_by_z_position")
                    .map_err(|err| err.to_string())?,
                aft_preserve: block
                    .get::<Option<bool>>("aft_preserve")
                    .map_err(|err| err.to_string())?,
                aft_created: block
                    .get::<Option<bool>>("aft_created")
                    .map_err(|err| err.to_string())?,
                halign: block
                    .get::<Option<f32>>("halign")
                    .map_err(|err| err.to_string())?,
                valign: block
                    .get::<Option<f32>>("valign")
                    .map_err(|err| err.to_string())?,
                text_align: block
                    .get::<Option<String>>("text_align")
                    .map_err(|err| err.to_string())?
                    .as_deref()
                    .and_then(parse_overlay_text_align),
                uppercase: block
                    .get::<Option<bool>>("uppercase")
                    .map_err(|err| err.to_string())?,
                shadow_len: block
                    .get::<Option<Table>>("shadow_len")
                    .map_err(|err| err.to_string())?
                    .and_then(|value| table_vec2(&value)),
                shadow_color: block
                    .get::<Option<Table>>("shadow_color")
                    .map_err(|err| err.to_string())?
                    .and_then(|value| table_vec4(&value)),
                glow: block
                    .get::<Option<Table>>("glow")
                    .map_err(|err| err.to_string())?
                    .and_then(|value| table_vec4(&value)),
                fov: block
                    .get::<Option<f32>>("fov")
                    .map_err(|err| err.to_string())?,
                vanishpoint: block
                    .get::<Option<Table>>("vanishpoint")
                    .map_err(|err| err.to_string())?
                    .and_then(|value| table_vec2(&value)),
                diffuse: block
                    .get::<Option<Table>>("diffuse")
                    .map_err(|err| err.to_string())?
                    .and_then(|value| table_vec4(&value)),
                vertex_colors: block
                    .get::<Option<Table>>("vertex_colors")
                    .map_err(|err| err.to_string())?
                    .and_then(|value| table_vertex_colors(&value)),
                visible: block
                    .get::<Option<bool>>("visible")
                    .map_err(|err| err.to_string())?,
                hibernating: block
                    .get::<Option<bool>>("hibernating")
                    .map_err(|err| err.to_string())?,
                cropleft: block
                    .get::<Option<f32>>("cropleft")
                    .map_err(|err| err.to_string())?,
                cropright: block
                    .get::<Option<f32>>("cropright")
                    .map_err(|err| err.to_string())?,
                croptop: block
                    .get::<Option<f32>>("croptop")
                    .map_err(|err| err.to_string())?,
                cropbottom: block
                    .get::<Option<f32>>("cropbottom")
                    .map_err(|err| err.to_string())?,
                fadeleft: block
                    .get::<Option<f32>>("fadeleft")
                    .map_err(|err| err.to_string())?,
                faderight: block
                    .get::<Option<f32>>("faderight")
                    .map_err(|err| err.to_string())?,
                fadetop: block
                    .get::<Option<f32>>("fadetop")
                    .map_err(|err| err.to_string())?,
                fadebottom: block
                    .get::<Option<f32>>("fadebottom")
                    .map_err(|err| err.to_string())?,
                mask_source: block
                    .get::<Option<bool>>("mask_source")
                    .map_err(|err| err.to_string())?,
                mask_dest: block
                    .get::<Option<bool>>("mask_dest")
                    .map_err(|err| err.to_string())?,
                cull_mode: block
                    .get::<Option<u8>>("cull_mode")
                    .map_err(|err| err.to_string())?
                    .map(|mode| match mode {
                        1 => deadlib_render_core::CullMode::Back,
                        2 => deadlib_render_core::CullMode::Front,
                        _ => deadlib_render_core::CullMode::None,
                    }),
                depth_test: block
                    .get::<Option<bool>>("depth_test")
                    .map_err(|err| err.to_string())?,
                depth_write: block
                    .get::<Option<bool>>("depth_write")
                    .map_err(|err| err.to_string())?,
                depth_test_on_fail: block
                    .get::<Option<bool>>("depth_test_on_fail")
                    .map_err(|err| err.to_string())?,
                zoom: block
                    .get::<Option<f32>>("zoom")
                    .map_err(|err| err.to_string())?,
                zoom_x: block
                    .get::<Option<f32>>("zoom_x")
                    .map_err(|err| err.to_string())?,
                zoom_y: block
                    .get::<Option<f32>>("zoom_y")
                    .map_err(|err| err.to_string())?,
                zoom_z: block
                    .get::<Option<f32>>("zoom_z")
                    .map_err(|err| err.to_string())?,
                basezoom: block
                    .get::<Option<f32>>("basezoom")
                    .map_err(|err| err.to_string())?,
                basezoom_x: block
                    .get::<Option<f32>>("basezoom_x")
                    .map_err(|err| err.to_string())?,
                basezoom_y: block
                    .get::<Option<f32>>("basezoom_y")
                    .map_err(|err| err.to_string())?,
                basezoom_z: block
                    .get::<Option<f32>>("basezoom_z")
                    .map_err(|err| err.to_string())?,
                rot_x_deg: block
                    .get::<Option<f32>>("rot_x_deg")
                    .map_err(|err| err.to_string())?,
                rot_y_deg: block
                    .get::<Option<f32>>("rot_y_deg")
                    .map_err(|err| err.to_string())?,
                rot_z_deg: block
                    .get::<Option<f32>>("rot_z_deg")
                    .map_err(|err| err.to_string())?,
                base_rotation: block
                    .get::<Option<Table>>("base_rotation")
                    .map_err(|err| err.to_string())?
                    .and_then(|value| table_vec3(&value)),
                skew_x: block
                    .get::<Option<f32>>("skew_x")
                    .map_err(|err| err.to_string())?,
                skew_y: block
                    .get::<Option<f32>>("skew_y")
                    .map_err(|err| err.to_string())?,
                blend: block
                    .get::<Option<String>>("blend")
                    .map_err(|err| err.to_string())?
                    .as_deref()
                    .and_then(parse_overlay_blend_mode),
                vibrate: block
                    .get::<Option<bool>>("vibrate")
                    .map_err(|err| err.to_string())?,
                effect_magnitude: block
                    .get::<Option<Table>>("effect_magnitude")
                    .map_err(|err| err.to_string())?
                    .and_then(|value| table_vec3(&value)),
                effect_clock: block
                    .get::<Option<String>>("effect_clock")
                    .map_err(|err| err.to_string())?
                    .as_deref()
                    .and_then(parse_overlay_effect_clock),
                effect_timer: block
                    .get::<Option<bool>>("effect_timer")
                    .map_err(|err| err.to_string())?,
                effect_mode: block
                    .get::<Option<String>>("effect_mode")
                    .map_err(|err| err.to_string())?
                    .as_deref()
                    .and_then(parse_overlay_effect_mode),
                effect_color1: block
                    .get::<Option<Table>>("effect_color1")
                    .map_err(|err| err.to_string())?
                    .and_then(|value| table_vec4(&value)),
                effect_color2: block
                    .get::<Option<Table>>("effect_color2")
                    .map_err(|err| err.to_string())?
                    .and_then(|value| table_vec4(&value)),
                effect_period: block
                    .get::<Option<f32>>("effect_period")
                    .map_err(|err| err.to_string())?,
                effect_offset: block
                    .get::<Option<f32>>("effect_offset")
                    .map_err(|err| err.to_string())?,
                effect_timing: block
                    .get::<Option<Table>>("effect_timing")
                    .map_err(|err| err.to_string())?
                    .and_then(|value| table_vec5(&value)),
                rainbow: block
                    .get::<Option<bool>>("rainbow")
                    .map_err(|err| err.to_string())?,
                rainbow_scroll: block
                    .get::<Option<bool>>("rainbow_scroll")
                    .map_err(|err| err.to_string())?,
                text_jitter: block
                    .get::<Option<bool>>("text_jitter")
                    .map_err(|err| err.to_string())?,
                text_distortion: block
                    .get::<Option<f32>>("text_distortion")
                    .map_err(|err| err.to_string())?,
                text_glow_mode: block
                    .get::<Option<String>>("text_glow_mode")
                    .map_err(|err| err.to_string())?
                    .as_deref()
                    .and_then(parse_overlay_text_glow_mode),
                mult_attrs_with_diffuse: block
                    .get::<Option<bool>>("mult_attrs_with_diffuse")
                    .map_err(|err| err.to_string())?,
                sprite_texture: block
                    .get::<Option<bool>>("sprite_texture")
                    .map_err(|err| err.to_string())?,
                sprite_animate: block
                    .get::<Option<bool>>("sprite_animate")
                    .map_err(|err| err.to_string())?,
                sprite_loop: block
                    .get::<Option<bool>>("sprite_loop")
                    .map_err(|err| err.to_string())?,
                sprite_playback_rate: block
                    .get::<Option<f32>>("sprite_playback_rate")
                    .map_err(|err| err.to_string())?,
                sprite_state_delay: block
                    .get::<Option<f32>>("sprite_state_delay")
                    .map_err(|err| err.to_string())?,
                sprite_state_index: block
                    .get::<Option<u32>>("sprite_state_index")
                    .map_err(|err| err.to_string())?,
                vert_spacing: block
                    .get::<Option<i32>>("vert_spacing")
                    .map_err(|err| err.to_string())?,
                wrap_width_pixels: block
                    .get::<Option<i32>>("wrap_width_pixels")
                    .map_err(|err| err.to_string())?,
                max_width: block
                    .get::<Option<f32>>("max_width")
                    .map_err(|err| err.to_string())?,
                max_height: block
                    .get::<Option<f32>>("max_height")
                    .map_err(|err| err.to_string())?,
                max_w_pre_zoom: block
                    .get::<Option<bool>>("max_w_pre_zoom")
                    .map_err(|err| err.to_string())?,
                max_h_pre_zoom: block
                    .get::<Option<bool>>("max_h_pre_zoom")
                    .map_err(|err| err.to_string())?,
                max_dimension_uses_zoom: block
                    .get::<Option<bool>>("max_dimension_uses_zoom")
                    .map_err(|err| err.to_string())?,
                texture_filtering: block
                    .get::<Option<bool>>("texture_filtering")
                    .map_err(|err| err.to_string())?,
                texture_wrapping: block
                    .get::<Option<bool>>("texture_wrapping")
                    .map_err(|err| err.to_string())?,
                texcoord_offset: block
                    .get::<Option<Table>>("texcoord_offset")
                    .map_err(|err| err.to_string())?
                    .and_then(|value| table_vec2(&value)),
                custom_texture_rect: block
                    .get::<Option<Table>>("custom_texture_rect")
                    .map_err(|err| err.to_string())?
                    .and_then(|value| table_vec4(&value)),
                texcoord_velocity: block
                    .get::<Option<Table>>("texcoord_velocity")
                    .map_err(|err| err.to_string())?
                    .and_then(|value| table_vec2(&value)),
                size: block
                    .get::<Option<Table>>("size")
                    .map_err(|err| err.to_string())?
                    .and_then(|value| table_vec2(&value)),
                stretch_rect: block
                    .get::<Option<Table>>("stretch_rect")
                    .map_err(|err| err.to_string())?
                    .and_then(|value| table_vec4(&value)),
                sound_play: block
                    .get::<Option<bool>>("sound_play")
                    .map_err(|err| err.to_string())?,
            },
        });
    }
    Ok(out)
}

// Fixed scalar layouts are copied in one protected VM frame. The shared
// decoder below keeps the compiler and retained scene's defaults identical.
macro_rules! actor_scalar_fields {
    ($name:ident { $($field:ident => $key:literal),* $(,)? }) => {
        #[derive(Clone, Copy)]
        enum $name { $($field),* }
        impl $name {
            const KEYS: &[&str] = &[$($key),*];
            fn key(self) -> &'static str { Self::KEYS[self as usize] }
        }
    };
}

actor_scalar_fields!(ActorNumberField {
    HibernateSeconds => "__songlua_hibernate_seconds",
    Aux => "__songlua_state_aux",
    X => "__songlua_state_x",
    Y => "__songlua_state_y",
    Z => "__songlua_state_z",
    ZBias => "__songlua_state_z_bias",
    Halign => "__songlua_state_halign",
    Valign => "__songlua_state_valign",
    Fov => "__songlua_state_fov",
    DeclarationFov => "FOV",
    Cropleft => "__songlua_state_cropleft",
    Cropright => "__songlua_state_cropright",
    Croptop => "__songlua_state_croptop",
    Cropbottom => "__songlua_state_cropbottom",
    Fadeleft => "__songlua_state_fadeleft",
    Faderight => "__songlua_state_faderight",
    Fadetop => "__songlua_state_fadetop",
    Fadebottom => "__songlua_state_fadebottom",
    Zoom => "__songlua_state_zoom",
    ZoomX => "__songlua_state_zoom_x",
    ZoomY => "__songlua_state_zoom_y",
    ZoomZ => "__songlua_state_zoom_z",
    Basezoom => "__songlua_state_basezoom",
    BasezoomX => "__songlua_state_basezoom_x",
    BasezoomY => "__songlua_state_basezoom_y",
    BasezoomZ => "__songlua_state_basezoom_z",
    RotXDeg => "__songlua_state_rot_x_deg",
    RotYDeg => "__songlua_state_rot_y_deg",
    RotZDeg => "__songlua_state_rot_z_deg",
    SkewX => "__songlua_state_skew_x",
    SkewY => "__songlua_state_skew_y",
    EffectPeriod => "__songlua_state_effect_period",
    EffectOffset => "__songlua_state_effect_offset",
    TextDistortion => "__songlua_state_text_distortion",
    SpritePlaybackRate => "__songlua_state_sprite_playback_rate",
    SpriteStateDelay => "__songlua_state_sprite_state_delay",
    MaxWidth => "__songlua_state_max_width",
    MaxHeight => "__songlua_state_max_height",
});

actor_scalar_fields!(ActorFlagField {
    ThemeHibernating => "__songlua_theme_hibernating",
    SpriteTexture => "__songlua_state_sprite_texture",
    Visible => "__songlua_visible",
    DrawByZPosition => "__songlua_state_draw_by_z_position",
    AftPreserve => "__songlua_state_aft_preserve",
    AftCreated => "__songlua_state_aft_created",
    Uppercase => "__songlua_state_uppercase",
    MaskSource => "__songlua_state_mask_source",
    MaskDest => "__songlua_state_mask_dest",
    DepthTest => "__songlua_state_depth_test",
    DepthWrite => "__songlua_state_depth_write",
    DepthTestOnFail => "__songlua_state_depth_test_on_fail",
    Vibrate => "__songlua_state_vibrate",
    EffectTimer => "__songlua_state_effect_timer",
    Rainbow => "__songlua_state_rainbow",
    RainbowScroll => "__songlua_state_rainbow_scroll",
    TextJitter => "__songlua_state_text_jitter",
    MultAttrsWithDiffuse => "__songlua_state_mult_attrs_with_diffuse",
    SpriteAnimate => "__songlua_state_sprite_animate",
    SpriteLoop => "__songlua_state_sprite_loop",
    MaxWPreZoom => "__songlua_state_max_w_pre_zoom",
    MaxHPreZoom => "__songlua_state_max_h_pre_zoom",
    MaxDimensionUsesZoom => "__songlua_state_max_dimension_uses_zoom",
    DecodeMovie => "__songlua_state_decode_movie",
    TextureFiltering => "__songlua_state_texture_filtering",
    TextureWrapping => "__songlua_state_texture_wrapping",
});

#[derive(Clone, Copy)]
enum ActorNumberValue {
    Value(Option<f32>),
    Fallback,
}

pub fn actor_overlay_initial_state(actor: &Table) -> Result<SongLuaOverlayState, String> {
    read_actor_overlay_state(
        actor,
        None,
        None,
        |field| actor.raw_get::<Option<f32>>(field.key()),
        |field| actor.raw_get::<Option<bool>>(field.key()),
    )
}

struct ActorScalarKeys(Table);

#[cfg(test)]
pub(crate) mod draw_state_baseline {
    include!("../tests/perf/draw_capture/state_baseline.rs");
}

fn actor_scalar_keys(lua: &Lua) -> mlua::Result<Table> {
    if let Some(keys) = lua.app_data_ref::<ActorScalarKeys>() {
        return Ok(keys.0.clone());
    }
    let keys = lua.create_sequence_from(ActorNumberField::KEYS.iter()
        .chain(ActorFlagField::KEYS).copied())?;
    lua.set_app_data(ActorScalarKeys(keys.clone()));
    Ok(keys)
}

pub(crate) fn actor_overlay_state_batched(
    lua: &Lua,
    actor: &Table,
) -> Result<SongLuaOverlayState, String> {
    actor_overlay_state_with_pose(lua, actor, None, None, None)
}

pub(crate) fn live_actor_overlay_state(lua: &Lua, actor: &Table) -> Result<SongLuaOverlayState, String> {
    if !crate::live::active(lua) { return actor_overlay_state_batched(lua, actor); }
    let state = lua.app_data_ref::<ActorTweenReplays>().and_then(|replays|
        replays.0.get(&(actor.to_pointer() as usize))
            .map(|replay| (replay.current, replay.render_flags, replay.base_transform.as_deref().copied())));
    actor_overlay_state_with_pose(lua, actor, state.map(|state| state.0),
        state.and_then(|state| state.1), state.and_then(|state| state.2))
}

fn actor_overlay_state_with_pose(
    lua: &Lua,
    actor: &Table,
    pose: Option<TweenPose>,
    render_flags: Option<RenderFlags>,
    base_transform: Option<BaseTransform>,
) -> Result<SongLuaOverlayState, String> {
    let mut numbers = [ActorNumberValue::Fallback; ActorNumberField::KEYS.len()];
    if let Some(pose) = &pose {
        // The native tween queue owns these properties. Do not deserialize
        // destination fields from Lua just to overwrite them with current.
        macro_rules! native {
            ($($field:ident => $member:ident),* $(,)?) => {
                $(numbers[ActorNumberField::$field as usize] = ActorNumberValue::Value(Some(pose.$member));)*
            };
        }
        native! {
            Aux => aux, X => x, Y => y, Z => z,
            Zoom => zoom, ZoomX => zoom_x, ZoomY => zoom_y, ZoomZ => zoom_z,
            RotXDeg => rot_x_deg, RotYDeg => rot_y_deg, RotZDeg => rot_z_deg,
            SkewX => skew_x, SkewY => skew_y,
            Cropleft => cropleft, Cropright => cropright, Croptop => croptop, Cropbottom => cropbottom,
            Fadeleft => fadeleft, Faderight => faderight, Fadetop => fadetop, Fadebottom => fadebottom,
        }
    }
    if let Some(base) = base_transform {
        for (field, native) in [
            (ActorNumberField::Basezoom, BaseField::Zoom),
            (ActorNumberField::BasezoomX, BaseField::ZoomX),
            (ActorNumberField::BasezoomY, BaseField::ZoomY),
            (ActorNumberField::BasezoomZ, BaseField::ZoomZ),
            (ActorNumberField::Halign, BaseField::HAlign),
            (ActorNumberField::Valign, BaseField::VAlign),
        ] {
            numbers[field as usize] = ActorNumberValue::Value(base.get(native));
        }
    }
    let mut flags = [None; ActorFlagField::KEYS.len()];
    if let Some(native) = render_flags {
        for (field, flag) in [
            (ActorFlagField::DrawByZPosition, RenderFlag::DrawByZPosition),
            (ActorFlagField::MaskSource, RenderFlag::MaskSource),
            (ActorFlagField::MaskDest, RenderFlag::MaskDest),
            (ActorFlagField::DepthTest, RenderFlag::DepthTest),
            (ActorFlagField::DepthWrite, RenderFlag::DepthWrite),
            (ActorFlagField::DepthTestOnFail, RenderFlag::DepthTestOnFail),
            (ActorFlagField::TextureFiltering, RenderFlag::TextureFiltering),
            (ActorFlagField::TextureWrapping, RenderFlag::TextureWrapping),
        ] {
            flags[field as usize] = Some(native.get(flag));
        }
    }
    let keys = actor_scalar_keys(lua).map_err(|error| error.to_string())?;
    // SAFETY: actor and interned keys are rooted in four reserved slots.
    // Only raw reads and primitive conversions run under this lock. They
    // neither allocate Lua objects nor call authored code or metamethods.
    unsafe {
        crate::raw_vm::with_read_stack(lua, 4, |raw, base| {
                let state = raw.state();
                raw.push(actor)?;
                raw.push(keys)?;
                for index in 0..ActorNumberField::KEYS.len() {
                    if matches!(numbers[index], ActorNumberValue::Value(_)) { continue; }
                    ffi::lua_rawgeti(state, base + 2, (index + 1) as ffi::lua_Integer);
                    ffi::lua_rawget(state, base + 1);
                    numbers[index] = if ffi::lua_type(state, -1) == ffi::LUA_TNIL {
                        ActorNumberValue::Value(None)
                    } else if ffi::lua_isnumber(state, -1) != 0 {
                        ActorNumberValue::Value(Some(ffi::lua_tonumber(state, -1) as f32))
                    } else {
                        // Preserve the standard reader's exact type error, and
                        // only raise it if the decoder actually reads this key.
                        ActorNumberValue::Fallback
                    };
                    ffi::lua_pop(state, 1);
                }
                for index in 0..ActorFlagField::KEYS.len() {
                    if flags[index].is_some() { continue; }
                    ffi::lua_rawgeti(state, base + 2, (ActorNumberField::KEYS.len() + index + 1) as ffi::lua_Integer);
                    ffi::lua_rawget(state, base + 1);
                    if ffi::lua_type(state, -1) != ffi::LUA_TNIL {
                        flags[index] = Some(ffi::lua_toboolean(state, -1) != 0);
                    }
                    ffi::lua_pop(state, 1);
                }
                Ok(())
        })
        .map_err(|error| error.to_string())?;
    }
    read_actor_overlay_state(
        actor,
        Some(lua),
        pose.as_ref(),
        |field| match numbers[field as usize] {
            ActorNumberValue::Value(value) => Ok(value),
            ActorNumberValue::Fallback => actor.raw_get::<Option<f32>>(field.key()),
        },
        |field| Ok(flags[field as usize]),
    )
}

#[cfg(test)]
mod actor_state_read_tests {
    use super::*;

    include!("../tests/overlay_state_reads.rs");

    #[test]
    fn native_render_flag_promotion_avoids_authored_reads_and_retains_model_defaults() {
        let lua = Lua::new();
        begin_live_runtime(&lua).unwrap();
        let actor = lua.load(r#"
TEXTURE_READS = 0
return setmetatable({__songlua_actor_type='Sprite'}, {
    __index=function(_, key)
        if key == 'Texture' then TEXTURE_READS=TEXTURE_READS+1 end
    end,
})
"#).eval::<Table>().unwrap();
        ensure_tween_replay(&lua, &actor).unwrap();
        let reads: u32 = lua.globals().get("TEXTURE_READS").unwrap();
        assert!(reads > 0);
        begin_live_actor_playback(&lua, &[actor]).unwrap();
        assert_eq!(lua.globals().get::<u32>("TEXTURE_READS").unwrap(), reads);

        let model = lua.load("{__songlua_actor_type='Model'}").eval::<Table>().unwrap();
        ensure_tween_replay(&lua, &model).unwrap();
        let state = live_actor_overlay_state(&lua, &model).unwrap();
        assert!(state.depth_test && state.depth_write && state.texture_wrapping && state.texture_filtering);
        capture_block_set_bool(&lua, &model, "depth_test", false).unwrap();
        capture_block_set_bool(&lua, &model, "texture_wrapping", false).unwrap();
        assert!(!live_actor_overlay_state(&lua, &model).unwrap().depth_test);
        init_actor_native_state(&lua, &model).unwrap();
        let state = live_actor_overlay_state(&lua, &model).unwrap();
        assert!(state.depth_test && state.depth_write && state.texture_wrapping && state.texture_filtering);
    }

    fn vector_bits<const N: usize>(value: Option<[f32; N]>) -> Option<[u32; N]> {
        value.map(|items| items.map(f32::to_bits))
    }

    fn compare_vectors(table: &Table) {
        assert_eq!(vector_bits(table_vec2(table)), vector_bits(draw_state_baseline::table_vec2(table)));
        assert_eq!(vector_bits(table_vec3(table)), vector_bits(draw_state_baseline::table_vec3(table)));
        assert_eq!(vector_bits(table_vec4(table)), vector_bits(draw_state_baseline::table_vec4(table)));
        assert_eq!(vector_bits(table_vec5(table)), vector_bits(draw_state_baseline::table_vec5(table)));
    }

    #[test]
    fn vector_batches_preserve_bits_coercions_holes_and_raw_lookup() {
        let lua = Lua::new();
        let table = lua.create_sequence_from([1.0, 2.0, 3.0, 4.0, 5.0]).unwrap();
        for value in [Value::Nil, Value::Boolean(false), Value::Table(lua.create_table().unwrap()),
            Value::String(lua.create_string(" -0.0 ").unwrap()),
            Value::String(lua.create_string("0x10").unwrap()),
            Value::String(lua.create_string("not a number").unwrap()),
            Value::String(lua.create_string([b'1',0,b'x']).unwrap()),
            Value::String(lua.create_string([0xff]).unwrap()),
            Value::Number(f64::INFINITY), Value::Number(f64::NEG_INFINITY),
            Value::Number(f64::from_bits(0xfff8_2468_0000_1234)), Value::Number(-0.0)] {
            for position in 1..=6 {
                table.raw_set(position, value.clone()).unwrap();
                compare_vectors(&table);
                table.raw_set(position, position as f32).unwrap();
            }
        }
        let missing = lua.load(r#"lookups=0; return setmetatable({}, {__index=function()
            lookups=lookups+1; error('raw reads must not call __index') end})"#).eval::<Table>().unwrap();
        compare_vectors(&missing);
        assert_eq!(lua.globals().get::<i32>("lookups").unwrap(), 0);
        let bad = lua.create_sequence_from([false]).unwrap();
        lua.exec_raw_lua(|raw| unsafe {
            assert_ne!(ffi::lua_checkstack(raw.state(), 8), 0);
            raw.push(77).unwrap();
            let top = ffi::lua_gettop(raw.state());
            compare_vectors(&table);
            compare_vectors(&bad);
            assert_eq!(ffi::lua_gettop(raw.state()), top);
            assert_eq!(raw.pop::<i32>().unwrap(), 77);
        });
        lua.globals().set("read_vector", lua.create_function(|_, table: Table| Ok(table_vec4(&table))).unwrap()).unwrap();
        let (ok, result): (bool, Table) = lua.load("return coroutine.resume(coroutine.create(function() return read_vector{1,2,3,4} end))").eval().unwrap();
        assert!(ok);
        assert_eq!(table_vec4(&result), Some([1.0,2.0,3.0,4.0]));
    }

    #[test]
    fn color_batches_preserve_nested_failures_aliases_and_stack() {
        let lua = Lua::new();
        let row = lua.create_sequence_from([1.0, 0.5, 0.25, 0.75, 99.0]).unwrap();
        let colors = lua.create_sequence_from([row.clone(),row.clone(),row.clone(),row.clone(),row.clone()]).unwrap();
        let compare = || {
            let bits = |v: Option<[[f32;4];4]>| v.map(|rows| rows.map(|items| items.map(f32::to_bits)));
            assert_eq!(bits(table_vertex_colors(&colors)), bits(draw_state_baseline::table_vertex_colors(&colors)));
        };
        compare();
        for position in 1..=5 {
            for value in [Value::Nil, Value::Boolean(false), Value::Number(2.0),
                Value::Table(lua.create_table().unwrap())] {
                colors.raw_set(position, value).unwrap();
                compare();
            }
            colors.raw_set(position, row.clone()).unwrap();
        }
        for position in 1..=5 {
            for value in [Value::Nil, Value::Boolean(true),
                Value::String(lua.create_string(" 0.25 ").unwrap()),
                Value::Number(f64::NAN), Value::Number(f64::INFINITY), Value::Number(-0.0)] {
                row.raw_set(position, value).unwrap();
                compare();
            }
            row.raw_set(position, 0.5).unwrap();
        }
        colors.raw_set(3, false).unwrap();
        lua.exec_raw_lua(|raw| unsafe {
            assert_ne!(ffi::lua_checkstack(raw.state(), 8), 0);
            raw.push(42).unwrap();
            let top = ffi::lua_gettop(raw.state());
            compare();
            assert_eq!(ffi::lua_gettop(raw.state()), top);
            assert_eq!(raw.pop::<i32>().unwrap(),42);
        });
        lua.globals().set("color_lookups",0).unwrap();
        let missing=lua.load(r#"return setmetatable({}, {__index=function()
            color_lookups=color_lookups+1; error('unexpected color lookup') end})"#).eval::<Table>().unwrap();
        assert_eq!(table_vertex_colors(&missing),draw_state_baseline::table_vertex_colors(&missing));
        colors.raw_set(3,missing).unwrap();
        compare();
        assert_eq!(lua.globals().get::<i32>("color_lookups").unwrap(),0);
        assert_eq!(lua.load("return 6*7").eval::<i32>().unwrap(),42);
    }

    #[test]
    fn state_class_and_texture_reads_match_frozen_decoder() {
        let lua = Lua::new();
        for field in ["__songlua_actor_type", "Texture"] {
            for value in [Value::Nil, Value::String(lua.create_string("Model").unwrap()),
                Value::String(lua.create_string("Sprite").unwrap()),
                Value::String(lua.create_string("x".repeat(200)).unwrap()),
                Value::String(lua.create_string([0xff, 0xfe]).unwrap()),
                Value::Integer(42), Value::Number(1.5), Value::Boolean(false),
                Value::Table(lua.create_table().unwrap())] {
                let actor = lua.create_table().unwrap();
                actor.raw_set("__songlua_actor_type", "Sprite").unwrap();
                actor.raw_set(field, value).unwrap();
                assert_eq!(actor_overlay_initial_state(&actor), draw_state_baseline::initial_state(&actor));
            }
        }
    }

    #[test]
    fn compile_update_slots_preserve_alias_visits_and_plan_rebuilds() {
        let lua = Lua::new();
        let root = lua.load(r#"
            events = {}
            local function update(_, delta) events[#events+1] = delta end
            local shared = { __songlua_update_function = update }
            local root = {
                { shared, __songlua_update_rate = 2 },
                { shared, __songlua_update_rate = 3 },
            }
            return root
        "#).eval::<Table>().unwrap();
        run_actor_compile_update_functions_with_delta(&lua, &Value::Table(root.clone()), 1.0).unwrap();
        let events: Vec<f64> = lua.globals().get::<Table>("events").unwrap()
            .sequence_values().collect::<mlua::Result<_>>().unwrap();
        assert_eq!(events, [2.0, 3.0], "shared actors retain each parent's chronological delta");
        let new_actor = lua.load(r#"return { __songlua_update_function = function(_, delta)
            events[#events+1] = delta
        end, __songlua_update_rate = 5 }"#).eval::<Table>().unwrap();
        root.raw_set(3, new_actor).unwrap();
        invalidate_compile_update_plan(&lua);
        run_actor_compile_update_functions_with_delta(&lua, &Value::Table(root), 0.5).unwrap();
        let events: Vec<f64> = lua.globals().get::<Table>("events").unwrap()
            .sequence_values().collect::<mlua::Result<_>>().unwrap();
        assert_eq!(events, [2.0, 3.0, 1.0, 1.5, 2.5], "rebuilds resize scratch without retaining old deltas");
    }

    #[test]
    fn short_class_fields_keep_string_coercions_spills_and_errors() {
        let lua = Lua::new();
        let actor = lua.create_table().unwrap();
        for value in [Value::Nil, Value::String(lua.create_string("ActorFrame").unwrap()),
            Value::String(lua.create_string("x".repeat(200)).unwrap()),
            Value::String(lua.create_string([0xff, 0xfe]).unwrap()),
            Value::Integer(42), Value::Number(1.5), Value::Boolean(false),
            Value::Table(lua.create_table().unwrap())] {
            actor.raw_set("__songlua_actor_type", value).unwrap();
            let expected = actor.raw_get::<Option<String>>("__songlua_actor_type")
                .map_err(|err| err.to_string());
            let actual = actor.raw_get::<Option<LuaFieldText<64>>>("__songlua_actor_type")
                .map(|value| value.map(|value| value.as_str().to_owned()))
                .map_err(|err| err.to_string());
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn capture_reset_preserves_compiler_snapshots_and_reuses_live_blocks() {
        let lua = Lua::new();
        let actor = lua.create_table().unwrap();
        let blocks = lua.create_sequence_from([42]).unwrap();
        actor.raw_set("__songlua_capture_blocks", blocks.clone()).unwrap();
        actor.raw_set("__songlua_capture_cursor", 3.0).unwrap();
        actor.raw_set("__songlua_capture_block", blocks.clone()).unwrap();
        reset_actor_capture(&lua, &actor).unwrap();
        assert_eq!(blocks.raw_get::<i32>(1).unwrap(), 42, "saved compiler captures retain their data");
        let fresh = actor.raw_get::<Table>("__songlua_capture_blocks").unwrap();
        assert_ne!(fresh, blocks);
        assert_eq!(actor.raw_get::<f32>("__songlua_capture_cursor").unwrap(), 0.0);
        assert!(actor.raw_get::<Value>("__songlua_capture_block").unwrap().is_nil());
        fresh.raw_set(1, 17).unwrap();
        lua.set_app_data(crate::live::LiveMode(1));
        reset_actor_capture(&lua, &actor).unwrap();
        assert_eq!(actor.raw_get::<Table>("__songlua_capture_blocks").unwrap(), fresh);
        assert_eq!(fresh.raw_len(), 0);
        actor.raw_set("__songlua_capture_blocks", false).unwrap();
        assert!(reset_actor_capture(&lua, &actor).is_err(), "live reset retains type validation");
    }

    #[test]
    fn native_pose_definition_clones_keep_destination_and_independence() {
        let lua = Lua::new();
        begin_live_runtime(&lua).unwrap();
        let actor = lua.load("{__songlua_actor_type='Quad', authored={score=17}}")
            .eval::<Table>().unwrap();
        ensure_tween_replay(&lua, &actor).unwrap();
        begin_live_actor_playback(&lua, &[actor.clone()]).unwrap();
        queue_replay_tween(&lua, &actor, 1.0, Some("linear"));
        capture_block_set_color(&lua, &actor, [0.25, 0.5, 0.75, 1.0]).unwrap();
        capture_block_set_vec4(&lua, &actor, "glow", [1.0, 0.75, 0.5, 0.25]).unwrap();
        capture_block_set_vertex_colors(&lua, &actor, [[0.5; 4]; 4]).unwrap();
        capture_block_set_bool(&lua, &actor, "depth_test", true).unwrap();
        capture_block_set_bool(&lua, &actor, "texture_filtering", false).unwrap();
        capture_block_set_f32(&lua, &actor, "basezoom", 2.0).unwrap();
        capture_block_set_f32(&lua, &actor, "basezoom_x", 1.0).unwrap();
        capture_block_set_f32(&lua, &actor, "halign", 0.25).unwrap();
        for (key, value) in [("x",12.0), ("zoom_y",3.0), ("skew_x",0.25), ("fadeleft",0.5)] {
            capture_block_set_f32(&lua, &actor, key, value).unwrap();
        }
        let Value::Table(generic) = clone_lua_value(&lua, Value::Table(actor.clone())).unwrap()
            else { panic!("table clone"); };
        let definition = clone_actor_def(&lua, &actor, |_, _| Ok(()), false).unwrap();
        capture_block_set_color(&lua, &actor, [0.0; 4]).unwrap();
        capture_block_set_f32(&lua, &actor, "x", 24.0).unwrap();
        capture_block_set_bool(&lua, &actor, "depth_test", false).unwrap();
        for copy in [generic, definition] {
            assert_eq!(actor_overlay_initial_state(&copy).unwrap().diffuse, [0.25, 0.5, 0.75, 1.0]);
            assert_eq!(actor_glow(&lua, &copy).unwrap(), [1.0, 0.75, 0.5, 0.25]);
            assert_eq!(actor_vertex_colors(&lua, &copy).unwrap(), [[0.5; 4]; 4]);
            let pose = actor_overlay_initial_state(&copy).unwrap();
            assert!(pose.depth_test && !pose.texture_filtering);
            assert_eq!((pose.basezoom,pose.basezoom_x,pose.halign), (2.0,1.0,0.25));
            assert_eq!((pose.x,pose.zoom_y,pose.skew_x,pose.fadeleft), (12.0,3.0,0.25,0.5));
            assert_eq!(copy.get::<Table>("authored").unwrap().get::<u32>("score").unwrap(), 17);
            ensure_tween_replay(&lua, &copy).unwrap();
            assert!(!live_actor_overlay_state(&lua, &actor).unwrap().depth_test);
            assert!(live_actor_overlay_state(&lua, &copy).unwrap().depth_test);
            assert!(copy.raw_get::<Value>("__songlua_state_depth_test").unwrap().is_nil());
            assert!(copy.raw_get::<Value>("__songlua_state_basezoom_x").unwrap().is_nil());
            assert_eq!(actor_base_zoom_axis(&lua, &copy, "__songlua_state_basezoom_x").unwrap(), 1.0);
            assert_eq!(actor_base_zoom_axis(&lua, &copy, "__songlua_state_basezoom_y").unwrap(), 2.0);
            capture_block_set_f32(&lua, &copy, "halign", 0.75).unwrap();
            assert_eq!(actor_halign(&lua, &actor).unwrap(), 0.25);
            set_actor_overlay_update_getter_value(&lua, &copy,
                SongLuaOverlayUpdateTarget::BaseZoomX, &SongLuaOverlayUpdateValue::F32(5.0)).unwrap();
            assert_eq!(actor_base_zoom_axis(&lua, &copy, "__songlua_state_basezoom_x").unwrap(), 5.0);
            assert!(copy.raw_get::<Value>("__songlua_state_basezoom_x").unwrap().is_nil());
            capture_block_set_color(&lua, &copy, [0.75; 4]).unwrap();
            capture_block_set_f32(&lua, &copy, "x", 36.0).unwrap();
            assert_eq!(actor_diffuse(&lua, &actor).unwrap(), [0.0; 4]);
            assert_eq!(actor_destination_position(&lua, &actor, 0).unwrap(), 24.0);
            assert_eq!(actor_current_position(&lua, &copy, 0).unwrap(), 36.0);
        }
    }

    #[test]
    #[cfg(feature = "test-support")]
    fn live_vertex_color_observer_preserves_owned_samples_and_queue_rules() {
        let run = |live| {
            let lua = Lua::new();
            begin_live_runtime(&lua).unwrap();
            let actor = lua.load("{__songlua_actor_type='Quad'}").eval::<Table>().unwrap();
            ensure_tween_replay(&lua, &actor).unwrap();
            if live { begin_live_actor_playback(&lua, &[actor.clone()]).unwrap(); }
            begin_overlay_update_capture_from_indices(&lua, [(actor.to_pointer() as usize, 0)]);
            let sample = || {
                let capture = lua.app_data_ref::<SongLuaOverlayUpdateCapture>().unwrap();
                let samples = &capture.writes[&(0, SongLuaOverlayUpdateTarget::VertexColors)];
                assert_eq!(samples.len(), 1);
                let SongLuaOverlayUpdateValue::VertexColors(colors) = &samples[0].value
                    else { panic!("owned vertex color observation"); };
                colors.clone()
            };
            capture_block_set_vertex_colors(&lua, &actor, [[0.25; 4]; 4]).unwrap();
            let first = sample();
            queue_replay_tween(&lua, &actor, 1.0, Some("linear"));
            capture_block_set_vertex_colors(&lua, &actor, [[0.5; 4]; 4]).unwrap();
            assert_eq!(*sample(), *first, "same-beat queued writes retain the earlier observation");
            clear_replay_tweens(&lua, &actor, false).unwrap();
            let args = MultiValue::from_vec(vec![Value::Table(actor.clone()), Value::Number(1.0),
                Value::Number(0.75), Value::Number(0.5), Value::Number(0.25)]);
            capture_actor_vertex_diffuse(&lua, &actor, &args, 3).unwrap();
            let second = sample();
            assert_eq!(second[0], [1.0, 0.75, 0.5, 0.25]);
            assert_eq!(*first, [[0.25; 4]; 4], "later writes cannot mutate an owned sample");
            lua.app_data_mut::<SongLuaOverlayUpdateCapture>().unwrap().queued_depth = 1;
            capture_block_set_vertex_colors(&lua, &actor, [[0.75; 4]; 4]).unwrap();
            assert_eq!(*sample(), *second, "ordinary queued commands are not update samples");
            lua.app_data_mut::<SongLuaOverlayUpdateCapture>().unwrap().update_command = true;
            capture_block_set_vertex_colors(&lua, &actor, [[1.0; 4]; 4]).unwrap();
            let third = sample();
            assert_eq!(*third, [[1.0; 4]; 4]);
            lua.app_data_mut::<SongLuaOverlayUpdateCapture>().unwrap().active_broadcast = Some("Tint".into());
            capture_block_set_vertex_colors(&lua, &actor, [[0.0; 4]; 4]).unwrap();
            assert_eq!(*sample(), *third, "broadcast writes are not update samples");
            [*first, *second, *third]
        };
        assert_eq!(run(true), run(false), "live mutation retains capture-backed observation rules");
    }

    #[test]
    fn live_scalar_read_preserves_defaults_coercions_and_errors() {
        let lua = Lua::new();
        let actor = lua.create_table().unwrap();
        assert_eq!(
            actor_overlay_state_batched(&lua, &actor).unwrap(),
            actor_overlay_initial_state(&actor).unwrap()
        );
        for numeric in [
            Value::Number(0.75),
            Value::String(lua.create_string(" 1.25 ").unwrap()),
        ] {
            for key in ActorNumberField::KEYS {
                actor.raw_set(*key, numeric.clone()).unwrap();
            }
            for truthy in [
                Value::Boolean(false),
                Value::Boolean(true),
                Value::Number(0.0),
                Value::String(lua.create_string("").unwrap()),
            ] {
                for key in ActorFlagField::KEYS {
                    actor.raw_set(*key, truthy.clone()).unwrap();
                }
                let expected = actor_overlay_initial_state(&actor).unwrap();
                let actual = actor_overlay_state_batched(&lua, &actor).unwrap();
                assert_eq!(actual, expected);
                if matches!(truthy, Value::Number(_)) {
                    assert!(actual.visible, "Lua zero is truthy");
                }
                assert_eq!(lua.load("return 6*7").eval::<u32>().unwrap(), 42);
            }
        }
        for key in ActorNumberField::KEYS {
            actor.raw_set(*key, Value::Nil).unwrap();
        }
        actor.raw_set("__songlua_state_x", f32::NAN).unwrap();
        actor.raw_set("__songlua_state_z", f32::INFINITY).unwrap();
        let mut expected = actor_overlay_initial_state(&actor).unwrap();
        let mut actual = actor_overlay_state_batched(&lua, &actor).unwrap();
        assert_eq!(actual.x.to_bits(), expected.x.to_bits());
        actual.x = 0.0;
        expected.x = 0.0;
        assert_eq!(actual, expected);
        actor
            .raw_set("__songlua_state_x", lua.create_table().unwrap())
            .unwrap();
        assert_eq!(
            actor_overlay_state_batched(&lua, &actor).unwrap_err(),
            actor_overlay_initial_state(&actor).unwrap_err()
        );
        assert_eq!(lua.load("return 6*7").eval::<u32>().unwrap(), 42);
    }
}

fn read_actor_overlay_state(
    actor: &Table,
    lua: Option<&Lua>,
    pose: Option<&TweenPose>,
    number: impl Fn(ActorNumberField) -> mlua::Result<Option<f32>>,
    flag: impl Fn(ActorFlagField) -> mlua::Result<Option<bool>>,
) -> Result<SongLuaOverlayState, String> {
    // These fields belong to this instance. Missing render state must not
    // invoke Lua class lookup for every default property on every frame.
    let mut state = SongLuaOverlayState::default();
    // Model::Model enables the Z buffer and texture wrapping before Lua overrides.
    state.depth_test = actor
        .raw_get::<Option<LuaFieldText<32>>>("__songlua_actor_type")
        .map_err(|err| err.to_string())?
        .is_some_and(|kind| kind.as_str().eq_ignore_ascii_case("Model"));
    state.depth_write = state.depth_test;
    state.texture_wrapping = state.depth_test;
    state.cull_mode = if state.depth_test {
        deadlib_render_core::CullMode::Back
    } else {
        deadlib_render_core::CullMode::None
    };
    if let Some(mode) = actor
        .raw_get::<Option<u8>>("__songlua_state_cull_mode")
        .map_err(|err| err.to_string())?
    {
        state.cull_mode = match mode {
            1 => deadlib_render_core::CullMode::Back,
            2 => deadlib_render_core::CullMode::Front,
            _ => deadlib_render_core::CullMode::None,
        };
    }
    state.hibernating = number(ActorNumberField::HibernateSeconds)
        .map_err(|err| err.to_string())?
        .is_some_and(|left| left > 0.0)
        || flag(ActorFlagField::ThemeHibernating)
            .map_err(|err| err.to_string())?
            .unwrap_or(false);
    state.aux = number(ActorNumberField::Aux)
        .map_err(|err| err.to_string())?
        .unwrap_or(0.0);
    state.sprite_texture = match flag(ActorFlagField::SpriteTexture)
        .map_err(|err| err.to_string())? {
        Some(value) => value,
        None => {
            !actor
                .raw_get::<Option<LuaFieldText<32>>>("__songlua_actor_type")
                .map_err(|err| err.to_string())?
                .is_some_and(|kind| kind.as_str().eq_ignore_ascii_case("Sprite"))
                || actor
                    .get::<Option<LuaFieldText<64>>>("Texture")
                    .map_err(|err| err.to_string())?
                    .is_some()
                || actor_aft_capture_name(actor)
                    .map_err(|err| err.to_string())?
                    .is_some()
        }
    };
    if let Some(visible) = flag(ActorFlagField::Visible).map_err(|err| err.to_string())? {
        state.visible = visible;
    }
    if let Some(pose) = pose {
        state.diffuse = pose.diffuse;
        state.vertex_colors = pose.vertex_colors;
    } else {
        if let Some(diffuse) = actor
            .raw_get::<Option<Table>>("__songlua_state_diffuse")
            .map_err(|err| err.to_string())?
            .and_then(|value| table_vec4(&value))
        {
            state.diffuse = diffuse;
        }
        if let Some(colors) = actor
            .raw_get::<Option<Table>>("__songlua_state_vertex_colors")
            .map_err(|err| err.to_string())?
            .and_then(|value| table_vertex_colors(&value))
        {
            state.vertex_colors = Some(colors);
        }
    }
    if let Some(value) = number(ActorNumberField::X).map_err(|err| err.to_string())? {
        state.x = value;
    }
    if let Some(value) = number(ActorNumberField::Y).map_err(|err| err.to_string())? {
        state.y = value;
    }
    if let Some(value) = number(ActorNumberField::Z).map_err(|err| err.to_string())? {
        state.z = value;
    }
    if let Some(value) = number(ActorNumberField::ZBias).map_err(|err| err.to_string())? {
        state.z_bias = value;
    }
    if let Some(value) = actor
        .raw_get::<Option<i32>>("__songlua_state_draw_order")
        .map_err(|err| err.to_string())?
    {
        state.draw_order = value;
    }
    if let Some(value) = flag(ActorFlagField::DrawByZPosition).map_err(|err| err.to_string())? {
        state.draw_by_z_position = value;
    }
    if let Some(value) = flag(ActorFlagField::AftPreserve).map_err(|err| err.to_string())? {
        state.aft_preserve = value;
    }
    if let Some(value) = flag(ActorFlagField::AftCreated).map_err(|err| err.to_string())? {
        state.aft_created = value;
    }
    if let Some(value) = number(ActorNumberField::Halign).map_err(|err| err.to_string())? {
        state.halign = value;
    }
    if let Some(value) = number(ActorNumberField::Valign).map_err(|err| err.to_string())? {
        state.valign = value;
    }
    if let Some(value) = actor
        .raw_get::<Option<LuaFieldText<32>>>("__songlua_state_text_align")
        .map_err(|err| err.to_string())?
        .as_ref()
        .map(LuaFieldText::as_str)
        .and_then(parse_overlay_text_align)
    {
        state.text_align = value;
    }
    if let Some(value) = flag(ActorFlagField::Uppercase).map_err(|err| err.to_string())? {
        state.uppercase = value;
    }
    if let Some(value) = actor
        .raw_get::<Option<ActorVectorValue<2>>>("__songlua_state_shadow_len")
        .map_err(|err| err.to_string())?
        .and_then(|value| value.0)
    {
        state.shadow_len = value;
    }
    if let Some(value) = actor
        .raw_get::<Option<ActorVectorValue<4>>>("__songlua_state_shadow_color")
        .map_err(|err| err.to_string())?
        .and_then(|value| value.0)
    {
        state.shadow_color = value;
    }
    if let Some(pose) = pose {
        state.glow = pose.glow;
    } else if let Some(value) = actor
        .raw_get::<Option<Table>>("__songlua_state_glow")
        .map_err(|err| err.to_string())?
        .and_then(|value| table_vec4(&value))
    {
        state.glow = value;
    }
    // ActorFrame::LoadFromNode reads FOV before commands run. A setter wins
    // over the declaration; -1 disables this frame's perspective camera.
    state.fov = number(ActorNumberField::Fov)
        .map_err(|err| err.to_string())?
        .or(number(ActorNumberField::DeclarationFov).map_err(|err| err.to_string())?)
        .filter(|value| *value != -1.0);
    if let Some(value) = actor
        .raw_get::<Option<Table>>("__songlua_state_vanishpoint")
        .map_err(|err| err.to_string())?
        .and_then(|value| table_vec2(&value))
    {
        state.vanishpoint = Some(value);
    }
    if let Some(value) = number(ActorNumberField::Cropleft).map_err(|err| err.to_string())? {
        state.cropleft = value;
    }
    if let Some(value) = number(ActorNumberField::Cropright).map_err(|err| err.to_string())? {
        state.cropright = value;
    }
    if let Some(value) = number(ActorNumberField::Croptop).map_err(|err| err.to_string())? {
        state.croptop = value;
    }
    if let Some(value) = number(ActorNumberField::Cropbottom).map_err(|err| err.to_string())? {
        state.cropbottom = value;
    }
    if let Some(value) = number(ActorNumberField::Fadeleft).map_err(|err| err.to_string())? {
        state.fadeleft = value;
    }
    if let Some(value) = number(ActorNumberField::Faderight).map_err(|err| err.to_string())? {
        state.faderight = value;
    }
    if let Some(value) = number(ActorNumberField::Fadetop).map_err(|err| err.to_string())? {
        state.fadetop = value;
    }
    if let Some(value) = number(ActorNumberField::Fadebottom).map_err(|err| err.to_string())? {
        state.fadebottom = value;
    }
    if let Some(value) = flag(ActorFlagField::MaskSource).map_err(|err| err.to_string())? {
        state.mask_source = value;
    }
    if let Some(value) = flag(ActorFlagField::MaskDest).map_err(|err| err.to_string())? {
        state.mask_dest = value;
    }
    if let Some(value) = flag(ActorFlagField::DepthTest).map_err(|err| err.to_string())? {
        state.depth_test = value;
    }
    if let Some(value) = flag(ActorFlagField::DepthWrite).map_err(|err| err.to_string())? {
        state.depth_write = value;
    }
    if let Some(value) = flag(ActorFlagField::DepthTestOnFail).map_err(|err| err.to_string())? {
        state.depth_test_on_fail = value;
    }
    if let Some(value) = number(ActorNumberField::Zoom).map_err(|err| err.to_string())? {
        state.zoom = value;
    }
    if let Some(value) = number(ActorNumberField::ZoomX).map_err(|err| err.to_string())? {
        state.zoom_x = value;
    }
    if let Some(value) = number(ActorNumberField::ZoomY).map_err(|err| err.to_string())? {
        state.zoom_y = value;
    }
    if let Some(value) = number(ActorNumberField::ZoomZ).map_err(|err| err.to_string())? {
        state.zoom_z = value;
    }
    if let Some(value) = number(ActorNumberField::Basezoom).map_err(|err| err.to_string())? {
        state.basezoom = value;
    }
    if let Some(value) = number(ActorNumberField::BasezoomX).map_err(|err| err.to_string())? {
        state.basezoom_x = value;
    }
    if let Some(value) = number(ActorNumberField::BasezoomY).map_err(|err| err.to_string())? {
        state.basezoom_y = value;
    }
    if let Some(value) = number(ActorNumberField::BasezoomZ).map_err(|err| err.to_string())? {
        state.basezoom_z = value;
    }
    if let Some(value) = number(ActorNumberField::RotXDeg).map_err(|err| err.to_string())? {
        state.rot_x_deg = value;
    }
    if let Some(value) = number(ActorNumberField::RotYDeg).map_err(|err| err.to_string())? {
        state.rot_y_deg = value;
    }
    if let Some(value) = number(ActorNumberField::RotZDeg).map_err(|err| err.to_string())? {
        state.rot_z_deg = value;
    }
    if let Some(value) = actor
        .raw_get::<Option<ActorVectorValue<3>>>("__songlua_state_base_rotation")
        .map_err(|err| err.to_string())?
        .and_then(|value| value.0)
    {
        state.base_rotation = value;
    }
    if let Some(value) = number(ActorNumberField::SkewX).map_err(|err| err.to_string())? {
        state.skew_x = value;
    }
    if let Some(value) = number(ActorNumberField::SkewY).map_err(|err| err.to_string())? {
        state.skew_y = value;
    }
    if let Some(value) = flag(ActorFlagField::Vibrate).map_err(|err| err.to_string())? {
        state.vibrate = value;
    }
    if let Some(value) = actor
        .raw_get::<Option<ActorVectorValue<3>>>("__songlua_state_effect_magnitude")
        .map_err(|err| err.to_string())?
        .and_then(|value| value.0)
    {
        state.effect_magnitude = value;
    }
    if let Some(value) = actor
        .raw_get::<Option<LuaFieldText<32>>>("__songlua_state_effect_clock")
        .map_err(|err| err.to_string())?
        .as_ref()
        .map(LuaFieldText::as_str)
        .and_then(parse_overlay_effect_clock)
    {
        state.effect_clock = value;
    }
    state.effect_timer = flag(ActorFlagField::EffectTimer)
        .map_err(|err| err.to_string())?
        .unwrap_or(true);
    if let Some(value) = actor
        .raw_get::<Option<ActorVectorValue<4>>>("__songlua_state_effect_color1")
        .map_err(|err| err.to_string())?
        .and_then(|value| value.0)
    {
        state.effect_color1 = value;
    }
    if let Some(value) = actor
        .raw_get::<Option<ActorVectorValue<4>>>("__songlua_state_effect_color2")
        .map_err(|err| err.to_string())?
        .and_then(|value| value.0)
    {
        state.effect_color2 = value;
    }
    if let Some(value) = number(ActorNumberField::EffectPeriod).map_err(|err| err.to_string())? {
        state.effect_period = value;
    }
    if let Some(value) = number(ActorNumberField::EffectOffset).map_err(|err| err.to_string())? {
        state.effect_offset = value;
    }
    if let Some(value) = actor
        .raw_get::<Option<ActorVectorValue<5>>>("__songlua_state_effect_timing")
        .map_err(|err| err.to_string())?
        .and_then(|value| value.0)
    {
        state.effect_timing = Some(value);
    }
    if let Some(value) = flag(ActorFlagField::Rainbow).map_err(|err| err.to_string())? {
        state.rainbow = value;
    }
    if let Some(value) = flag(ActorFlagField::RainbowScroll).map_err(|err| err.to_string())? {
        state.rainbow_scroll = value;
    }
    if let Some(value) = flag(ActorFlagField::TextJitter).map_err(|err| err.to_string())? {
        state.text_jitter = value;
    }
    if let Some(value) = number(ActorNumberField::TextDistortion).map_err(|err| err.to_string())? {
        state.text_distortion = value;
    }
    if let Some(value) = actor
        .raw_get::<Option<LuaFieldText<32>>>("__songlua_state_text_glow_mode")
        .map_err(|err| err.to_string())?
        .as_ref()
        .map(LuaFieldText::as_str)
        .and_then(parse_overlay_text_glow_mode)
    {
        state.text_glow_mode = value;
    }
    if let Some(value) =
        flag(ActorFlagField::MultAttrsWithDiffuse).map_err(|err| err.to_string())?
    {
        state.mult_attrs_with_diffuse = value;
    }
    if let Some(value) = flag(ActorFlagField::SpriteAnimate).map_err(|err| err.to_string())? {
        state.sprite_animate = value;
    }
    if let Some(value) = flag(ActorFlagField::SpriteLoop).map_err(|err| err.to_string())? {
        state.sprite_loop = value;
    }
    if let Some(value) =
        number(ActorNumberField::SpritePlaybackRate).map_err(|err| err.to_string())?
    {
        state.sprite_playback_rate = value;
    }
    if let Some(value) =
        number(ActorNumberField::SpriteStateDelay).map_err(|err| err.to_string())?
    {
        state.sprite_state_delay = value;
    }
    if let Some(value) = actor
        .raw_get::<Option<i32>>("__songlua_state_vert_spacing")
        .map_err(|err| err.to_string())?
    {
        state.vert_spacing = Some(value);
    }
    if let Some(value) = actor
        .raw_get::<Option<i32>>("__songlua_state_wrap_width_pixels")
        .map_err(|err| err.to_string())?
    {
        state.wrap_width_pixels = Some(value);
    }
    if let Some(value) = number(ActorNumberField::MaxWidth).map_err(|err| err.to_string())? {
        state.max_width = Some(value);
    }
    if let Some(value) = number(ActorNumberField::MaxHeight).map_err(|err| err.to_string())? {
        state.max_height = Some(value);
    }
    if let Some(value) = flag(ActorFlagField::MaxWPreZoom).map_err(|err| err.to_string())? {
        state.max_w_pre_zoom = value;
    }
    if let Some(value) = flag(ActorFlagField::MaxHPreZoom).map_err(|err| err.to_string())? {
        state.max_h_pre_zoom = value;
    }
    if let Some(value) =
        flag(ActorFlagField::MaxDimensionUsesZoom).map_err(|err| err.to_string())?
    {
        state.max_dimension_uses_zoom = value;
    }
    if let Some(value) = actor
        .raw_get::<Option<u32>>("__songlua_state_sprite_state_index")
        .map_err(|err| err.to_string())?
    {
        state.sprite_state_index = Some(value);
    }
    if let Some(value) = flag(ActorFlagField::DecodeMovie).map_err(|err| err.to_string())? {
        state.decode_movie = value;
    }
    if let Some(value) = flag(ActorFlagField::TextureFiltering).map_err(|err| err.to_string())? {
        state.texture_filtering = value;
    }
    if let Some(value) = flag(ActorFlagField::TextureWrapping).map_err(|err| err.to_string())? {
        state.texture_wrapping = value;
    }
    if let Some(value) = actor
        .raw_get::<Option<Table>>("__songlua_state_texcoord_offset")
        .map_err(|err| err.to_string())?
        .and_then(|value| table_vec2(&value))
    {
        state.texcoord_offset = Some(value);
    }
    if let Some(value) = actor
        .raw_get::<Option<Table>>("__songlua_state_custom_texture_rect")
        .map_err(|err| err.to_string())?
        .and_then(|value| table_vec4(&value))
    {
        state.custom_texture_rect = Some(value);
    }
    if let Some(value) = actor
        .raw_get::<Option<Table>>("__songlua_state_texcoord_velocity")
        .map_err(|err| err.to_string())?
        .and_then(|value| table_vec2(&value))
    {
        state.texcoord_velocity = Some(value);
    }
    if let Some(value) = actor
        .raw_get::<Option<ActorVectorValue<2>>>("__songlua_state_size")
        .map_err(|err| err.to_string())?
        .and_then(|value| value.0)
    {
        state.size = Some(value);
    }
    if let Some(value) = actor
        .raw_get::<Option<Table>>("__songlua_state_stretch_rect")
        .map_err(|err| err.to_string())?
        .and_then(|value| table_vec4(&value))
    {
        state.stretch_rect = Some(value);
        if pose.is_some() {
            // Preserve replay_tween_pose's translation from the destination
            // rectangle to current XY, including its float operation order.
            let (x, y) = (state.x, state.y);
            if let Some([dest_x, dest_y]) = lua.and_then(|lua|
                live_destination_value(lua, actor, |dest| [dest.x, dest.y])) {
                state.x = dest_x;
                state.y = dest_y;
            } else {
                state.x = actor.raw_get::<Option<f32>>("__songlua_state_x")
                    .map_err(|err| err.to_string())?.unwrap_or(0.0);
                state.y = actor.raw_get::<Option<f32>>("__songlua_state_y")
                    .map_err(|err| err.to_string())?.unwrap_or(0.0);
            }
            crate::move_overlay(&mut state, x, y);
        }
    }
    if let Some(raw) = actor
        .raw_get::<Option<LuaFieldText<32>>>("__songlua_state_blend")
        .map_err(|err| err.to_string())?
        .as_ref()
        .map(LuaFieldText::as_str)
        .and_then(parse_overlay_blend_mode)
    {
        state.blend = raw;
    }
    if let Some(raw) = actor
        .raw_get::<Option<LuaFieldText<32>>>("__songlua_state_effect_mode")
        .map_err(|err| err.to_string())?
        .as_ref()
        .map(LuaFieldText::as_str)
        .and_then(parse_overlay_effect_mode)
    {
        state.effect_mode = raw;
    }
    Ok(state)
}

pub fn set_actor_overlay_getter_state(
    lua: &Lua,
    actor: &Table,
    state: SongLuaOverlayState,
) -> Result<(), String> {
    macro_rules! set {
        ($key:literal, $value:expr) => {
            actor.set($key, $value).map_err(|err| err.to_string())?
        };
    }
    set!("__songlua_visible", state.visible);
    set!("__songlua_state_aux", state.aux);
    set!("__songlua_aux", state.aux);
    set!("__songlua_state_sprite_texture", state.sprite_texture);
    set!("__songlua_state_aft_preserve", state.aft_preserve);
    set!("__songlua_state_aft_created", state.aft_created);
    set!("__songlua_state_x", state.x);
    set!("__songlua_state_y", state.y);
    set!("__songlua_state_z", state.z);
    set!("__songlua_current_x", state.x);
    set!("__songlua_current_y", state.y);
    set!("__songlua_current_z", state.z);
    set!("__songlua_state_zoom", state.zoom);
    set!("__songlua_state_zoom_x", state.zoom_x);
    set!("__songlua_state_zoom_y", state.zoom_y);
    set!("__songlua_state_zoom_z", state.zoom_z);
    if !set_live_base_field(lua, actor, "basezoom", state.basezoom) {
        set!("__songlua_state_basezoom", state.basezoom);
    }
    if !set_live_base_field(lua, actor, "basezoom_x", state.basezoom_x) {
        set!("__songlua_state_basezoom_x", state.basezoom_x);
    }
    if !set_live_base_field(lua, actor, "basezoom_y", state.basezoom_y) {
        set!("__songlua_state_basezoom_y", state.basezoom_y);
    }
    if !set_live_base_field(lua, actor, "basezoom_z", state.basezoom_z) {
        set!("__songlua_state_basezoom_z", state.basezoom_z);
    }
    set!("__songlua_state_rot_x_deg", state.rot_x_deg);
    set!("__songlua_state_rot_y_deg", state.rot_y_deg);
    set!("__songlua_state_rot_z_deg", state.rot_z_deg);
    set!(
        "__songlua_state_base_rotation",
        lua.create_sequence_from(state.base_rotation)
            .map_err(|err| err.to_string())?
    );
    if !set_live_base_field(lua, actor, "halign", state.halign) {
        set!("__songlua_state_halign", state.halign);
    }
    if !set_live_base_field(lua, actor, "valign", state.valign) {
        set!("__songlua_state_valign", state.valign);
    }
    set!(
        "__songlua_state_effect_mode",
        crate::effect_mode_label(state.effect_mode)
    );
    set!(
        "__songlua_state_effect_clock",
        effect_clock_label(state.effect_clock)
    );
    set!("__songlua_state_effect_timer", state.effect_timer);
    set!("__songlua_state_effect_period", state.effect_period);
    let diffuse = make_color_table(lua, state.diffuse).map_err(|err| err.to_string())?;
    set!("__songlua_state_diffuse", diffuse.clone());
    set!("__songlua_diffuse", diffuse);
    set!(
        "__songlua_state_glow",
        make_color_table(lua, state.glow).map_err(|err| err.to_string())?
    );
    set!(
        "__songlua_state_effect_magnitude",
        lua.create_sequence_from(state.effect_magnitude)
            .map_err(|err| err.to_string())?
    );
    if let Some(size) = state.size {
        set!(
            "__songlua_state_size",
            lua.create_sequence_from(size)
                .map_err(|err| err.to_string())?
        );
    }
    if let Some(rect) = state.stretch_rect {
        set!(
            "__songlua_state_stretch_rect",
            lua.create_sequence_from(rect)
                .map_err(|err| err.to_string())?
        );
    }
    sync_tween_getters(lua, actor).map_err(|err| err.to_string())
}

pub fn set_actor_overlay_update_getter_value(
    lua: &Lua,
    actor: &Table,
    target: SongLuaOverlayUpdateTarget,
    value: &SongLuaOverlayUpdateValue,
) -> Result<(), String> {
    use SongLuaOverlayUpdateTarget as Target;
    use SongLuaOverlayUpdateValue as UpdateValue;

    // Actor.h exposes destination scale/rotation/skew to Lua. Current player
    // transforms are sampled separately; positional getters use current XYZ.
    if matches!(
        target,
        Target::Zoom
            | Target::ZoomX
            | Target::ZoomY
            | Target::ZoomZ
            | Target::RotationX
            | Target::RotationY
            | Target::RotationZ
            | Target::SkewX
            | Target::SkewY
    ) && actor
        .raw_get::<Option<i64>>("__songlua_player_index")
        .map_err(|err| err.to_string())?
        .is_some()
        && actor
            .raw_get::<Option<String>>("__songlua_player_child_name")
            .map_err(|err| err.to_string())?
            .is_none()
    {
        return Ok(());
    }

    let key = match target {
        Target::Aux => "aux",
        Target::X => "x",
        Target::Y => "y",
        Target::Z => "z",
        Target::Zoom => "zoom",
        Target::ZoomX => "zoom_x",
        Target::ZoomY => "zoom_y",
        Target::ZoomZ => "zoom_z",
        Target::BaseZoom => "basezoom",
        Target::BaseZoomX => "basezoom_x",
        Target::BaseZoomY => "basezoom_y",
        Target::BaseZoomZ => "basezoom_z",
        Target::RotationX => "rot_x_deg",
        Target::RotationY => "rot_y_deg",
        Target::RotationZ => "rot_z_deg",
        Target::BaseRotation => "base_rotation",
        Target::SkewX => "skew_x",
        Target::SkewY => "skew_y",
        Target::CropLeft => "cropleft",
        Target::CropRight => "cropright",
        Target::CropTop => "croptop",
        Target::CropBottom => "cropbottom",
        Target::FadeLeft => "fadeleft",
        Target::FadeRight => "faderight",
        Target::FadeTop => "fadetop",
        Target::FadeBottom => "fadebottom",
        Target::EffectPeriod => "effect_period",
        Target::EffectOffset => "effect_offset",
        _ => "",
    };
    match value {
        UpdateValue::F32(value) if !key.is_empty() => {
            if set_live_base_field(lua, actor, key, *value) { return Ok(()); }
            let current_key = match target {
                Target::Aux => Some("__songlua_aux"),
                Target::X => Some("__songlua_current_x"),
                Target::Y => Some("__songlua_current_y"),
                Target::Z => Some("__songlua_current_z"),
                _ => None,
            };
            if let Some(current_key) = current_key {
                actor
                    .set(current_key, *value)
                    .map_err(|err| err.to_string())
            } else {
                set_actor_capture_state(actor, key, *value).map_err(|err| err.to_string())
            }
        }
        UpdateValue::Bool(value) if target == Target::Hibernating => {
            set_actor_capture_state(actor, "hibernating", *value).map_err(|err| err.to_string())
        }
        UpdateValue::Bool(value) if target == Target::Visible => {
            actor
                .set("__songlua_visible", *value)
                .map_err(|err| err.to_string())?;
            set_actor_capture_state(actor, "visible", *value).map_err(|err| err.to_string())
        }
        UpdateValue::Vec4(value) if target == Target::Diffuse => {
            let diffuse = make_color_table(lua, *value).map_err(|err| err.to_string())?;
            actor
                .set("__songlua_state_diffuse", diffuse.clone())
                .map_err(|err| err.to_string())?;
            actor
                .set("__songlua_diffuse", diffuse)
                .map_err(|err| err.to_string())
        }
        UpdateValue::Vec3(value) if target == Target::EffectMagnitude => {
            let value = lua
                .create_sequence_from(*value)
                .map_err(|err| err.to_string())?;
            actor
                .set("__songlua_state_effect_magnitude", value)
                .map_err(|err| err.to_string())
        }
        _ => Ok(()),
    }
}

pub fn read_graph_display_line_state(
    lua: &Lua,
    actor: &Table,
) -> Result<SongLuaOverlayState, String> {
    let Some(line) = actor_named_children(lua, actor)
        .map_err(|err| err.to_string())?
        .get::<Option<Table>>("Line")
        .map_err(|err| err.to_string())?
    else {
        return Ok(SongLuaOverlayState::default());
    };
    actor_overlay_initial_state(&line)
}

pub fn read_graph_display_body_state(
    lua: &Lua,
    actor: &Table,
) -> Result<SongLuaOverlayState, String> {
    let Some(body_group) = actor_named_children(lua, actor)
        .map_err(|err| err.to_string())?
        .get::<Option<Table>>("")
        .map_err(|err| err.to_string())?
    else {
        return Ok(SongLuaOverlayState::default());
    };
    let Some(body) = body_group
        .raw_get::<Option<Table>>(2)
        .map_err(|err| err.to_string())?
        .or_else(|| body_group.raw_get::<Option<Table>>(1).ok().flatten())
    else {
        return Ok(SongLuaOverlayState::default());
    };
    actor_overlay_initial_state(&body)
}

pub fn read_song_meter_display_state(
    lua: &Lua,
    actor: &Table,
) -> Result<Option<(f32, SongLuaOverlayState)>, String> {
    let stream_width = actor
        .get::<Option<f32>>("__songlua_stream_width")
        .map_err(|err| err.to_string())?
        .or_else(|| {
            actor
                .get::<Option<Value>>("StreamWidth")
                .ok()
                .flatten()
                .and_then(read_f32)
        })
        .unwrap_or(0.0)
        .max(0.0);
    if stream_width <= f32::EPSILON {
        return Ok(None);
    }
    let Some(stream) = actor_named_children(lua, actor)
        .map_err(|err| err.to_string())?
        .get::<Option<Table>>("Stream")
        .map_err(|err| err.to_string())?
    else {
        return Ok(None);
    };
    let stream_state = actor_overlay_initial_state(&stream)?;
    Ok(Some((stream_width, stream_state)))
}

pub fn install_song_meter_display_children(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    let width = actor
        .get::<Option<Value>>("StreamWidth")?
        .and_then(read_f32)
        .unwrap_or(0.0);
    actor.set("__songlua_stream_width", width)?;
    if let Some(stream) = actor.get::<Option<Table>>("Stream")? {
        // MeterDisplay::LoadFromNode loads the Stream declaration as an actor
        // before its Init commands and then assigns its engine-owned name.
        let host = lua.app_data_ref::<ActorDefinitionHost>().map(|host| *host);
        let stream = if let Some(host) = host {
            let Value::Table(stream) = own_actor_root(lua, Value::Table(stream), host.install_methods)? else {
                return Err(mlua::Error::runtime("SongMeterDisplay Stream must load an actor"));
            };
            stream
        } else { stream };
        stream.set("Name", "Stream")?;
        stream.set("__songlua_parent", actor.clone())?;
        actor.raw_set("Stream", stream.clone())?;
        actor_children(lua, actor)?.set("Stream", stream)?;
    }
    Ok(())
}

#[must_use]
pub fn read_graph_display_size(
    state: SongLuaOverlayState,
    context: &SongLuaCompileContext,
) -> [f32; 2] {
    let fallback = graph_display_body_size(song_lua_human_player_count(context));
    let size = state.size.unwrap_or(fallback);
    [size[0].abs().max(1.0), size[1].abs().max(1.0)]
}

pub fn capture_graph_display_values(
    lua: &Lua,
    actor: &Table,
    stage_stats: Option<Value>,
    player_stats: Option<Value>,
) -> mlua::Result<()> {
    let total_seconds = stage_stats
        .and_then(|value| match value {
            Value::Table(table) => table
                .get::<Option<f32>>("__songlua_total_steps_seconds")
                .ok()
                .flatten(),
            _ => None,
        })
        .unwrap_or(0.0);
    let values = player_stats
        .and_then(|value| match value {
            Value::Table(table) => {
                let method = table
                    .get::<Option<Function>>("GetLifeRecord")
                    .ok()
                    .flatten()?;
                method
                    .call::<Value>((table, total_seconds, GRAPH_DISPLAY_VALUE_RESOLUTION as i64))
                    .ok()
            }
            _ => None,
        })
        .and_then(|value| match value {
            Value::Table(table) => Some(table),
            _ => None,
        })
        .unwrap_or(create_life_record_table(
            lua,
            GRAPH_DISPLAY_VALUE_RESOLUTION,
            SONG_LUA_INITIAL_LIFE,
        )?);
    actor.set("__songlua_graph_display_values", values)?;
    Ok(())
}

pub fn read_graph_display_values(actor: &Table) -> Result<Arc<[f32]>, String> {
    let values = actor
        .get::<Option<Table>>("__songlua_graph_display_values")
        .map_err(|err| err.to_string())?;
    let Some(values) = values else {
        return Ok(default_graph_display_values());
    };
    let mut out = Vec::with_capacity(GRAPH_DISPLAY_VALUE_RESOLUTION);
    for index in 1..=GRAPH_DISPLAY_VALUE_RESOLUTION {
        let value = values
            .raw_get::<Option<Value>>(index)
            .map_err(|err| err.to_string())?
            .and_then(read_f32)
            .unwrap_or(SONG_LUA_INITIAL_LIFE)
            .clamp(0.0, 1.0);
        out.push(value);
    }
    Ok(Arc::from(out.into_boxed_slice()))
}

fn default_graph_display_values() -> Arc<[f32]> {
    Arc::from([SONG_LUA_INITIAL_LIFE; GRAPH_DISPLAY_VALUE_RESOLUTION])
}

pub fn read_actor_multi_vertex_texture_path(
    actor: &Table,
    aft_capture_names: &HashSet<String>,
) -> Result<Option<PathBuf>, String> {
    if actor
        .get::<Option<String>>("__songlua_aft_capture_name")
        .map_err(|err| err.to_string())?
        .is_some()
    {
        return Ok(None);
    }
    let Some(texture) = actor
        .get::<Option<String>>("Texture")
        .map_err(|err| err.to_string())?
    else {
        return Ok(None);
    };
    if aft_capture_names.contains(&texture) {
        return Ok(None);
    }
    Ok(resolve_actor_asset_path(actor, &texture).ok())
}

pub fn read_actor_multi_vertex_mesh(
    actor: &Table,
) -> Result<Option<Arc<[SongLuaOverlayMeshVertex]>>, String> {
    let source: Arc<[SongLuaActorMultiVertexPoint]> = match actor
        .get::<Value>("__songlua_vertices")
        .map_err(|err| err.to_string())?
    {
        Value::Nil => return Ok(None),
        Value::UserData(points) => {
            let points = points
                .borrow::<Arc<[SongLuaActorMultiVertexPoint]>>()
                .map_err(|err| err.to_string())?;
            Arc::clone(&points)
        }
        // Retain compatibility with compiler fixtures that supply raw tables.
        Value::Table(vertices) => read_actor_multi_vertex_points(&vertices)?.into(),
        _ => return Err("invalid ActorMultiVertex vertex buffer".into()),
    };
    let Some((start, end)) = read_actor_multi_vertex_range(actor, source.len())? else {
        return Ok(None);
    };
    let draw_mode = actor
        .get::<Option<LuaFieldText<32>>>("__songlua_draw_state_mode")
        .map_err(|err| err.to_string())?;
    let draw_mode = draw_mode
        .as_ref()
        .map_or("DrawMode_Triangles", LuaFieldText::as_str);
    let line_width = actor
        .get::<Option<f32>>("__songlua_line_width")
        .map_err(|err| err.to_string())?
        .unwrap_or(1.0)
        .max(0.0);
    Ok(actor_multi_vertex_mesh_arc(
        &source[start..end],
        draw_mode,
        line_width,
    ))
}

pub(crate) fn live_multi_vertex_texture_path(actor: &Table) -> mlua::Result<Option<PathBuf>> {
    if actor.raw_get::<bool>("__songlua_default_texture")? {
        // RageTexture_Default is an opaque white texel, unlike the transparent
        // fallback theme's _blank.png. AMV always has this texture initially.
        const KEY: &str = "__songlua_native_default_texture.png";
        static PREPARED: OnceLock<()> = OnceLock::new();
        PREPARED.get_or_init(|| {
            deadlib_assets::register_prepared_generated_texture(KEY,
                image::RgbaImage::from_pixel(1, 1, image::Rgba([255; 4])),
                [1, 1], Default::default());
        });
        Ok(Some(PathBuf::from(KEY)))
    } else { actor_texture_path(actor) }
}

pub fn read_model_paths(actor: &Table) -> Result<Option<[PathBuf; 3]>, String> {
    let mut paths = [None, None, None];
    for (index, key) in ["Meshes", "Materials", "Bones"].into_iter().enumerate() {
        let Some(raw) = actor
            .get::<Option<String>>(key)
            .map_err(|err| err.to_string())?
            .filter(|path| !path.trim().is_empty())
        else {
            continue;
        };
        paths[index] = Some(resolve_actor_asset_path(actor, &raw).map_err(|err| err.to_string())?);
    }
    let Some(fallback) = paths.iter().flatten().next().cloned() else {
        return Ok(None);
    };
    // A single-file model contains all sections; explicit pieces keep their own paths.
    Ok(Some(
        paths.map(|path| path.unwrap_or_else(|| fallback.clone())),
    ))
}

pub fn read_actor_model_layers<Slot, Vertex, ReadSlots, ReadLayer>(
    actor: &Table,
    read_slots: ReadSlots,
    read_layer: ReadLayer,
) -> Result<Option<Arc<[SongLuaOverlayModelLayer<Vertex>]>>, String>
where
    ReadSlots: Fn(&Path, &Path, &Path) -> Result<Arc<[Slot]>, String>,
    ReadLayer: Fn(&Slot) -> Option<SongLuaOverlayModelLayer<Vertex>>,
{
    let Some(paths) = read_model_paths(actor)? else {
        return Ok(None);
    };
    let slots = read_slots(&paths[0], &paths[1], &paths[2])?;
    Ok(overlay_model_layers_from_slots(slots.as_ref(), read_layer))
}

pub fn overlay_model_layers_from_slots<Slot, Vertex, ReadLayer>(
    slots: &[Slot],
    read_layer: ReadLayer,
) -> Option<Arc<[SongLuaOverlayModelLayer<Vertex>]>>
where
    ReadLayer: Fn(&Slot) -> Option<SongLuaOverlayModelLayer<Vertex>>,
{
    let mut layers = Vec::with_capacity(slots.len());
    for slot in slots.iter() {
        let Some(layer) = read_layer(slot) else {
            continue;
        };
        layers.push(layer);
    }
    if layers.is_empty() {
        None
    } else {
        Some(Arc::from(layers.into_boxed_slice()))
    }
}

pub struct SongLuaNoteskinTapActorModel {
    pub skin: String,
    pub element: String,
    pub model_paths: [PathBuf; 3],
}

pub fn read_noteskin_tap_actor_model(
    actor: &Table,
) -> Result<Option<SongLuaNoteskinTapActorModel>, String> {
    let Some(skin) = actor
        .get::<Option<String>>("__songlua_noteskin_name")
        .map_err(|err| err.to_string())?
        .filter(|value| !value.trim().is_empty())
    else {
        return Ok(None);
    };
    let Some(element) = actor
        .get::<Option<String>>("__songlua_noteskin_element")
        .map_err(|err| err.to_string())?
        .filter(|value| value.eq_ignore_ascii_case("Tap Note"))
    else {
        return Ok(None);
    };
    let Some(model_paths) = read_model_paths(actor)? else {
        return Ok(None);
    };
    Ok(Some(SongLuaNoteskinTapActorModel {
        skin,
        element,
        model_paths,
    }))
}

pub fn read_noteskin_tap_actor_slots<Slot, ReadSlots>(
    actor: &Table,
    read_slots: ReadSlots,
) -> Result<Option<Arc<[Slot]>>, String>
where
    ReadSlots: Fn(&Path, &Path, &Path) -> Result<Arc<[Slot]>, String>,
{
    let Some(model) = read_noteskin_tap_actor_model(actor)? else {
        return Ok(None);
    };
    let paths = &model.model_paths;
    read_slots(&paths[0], &paths[1], &paths[2])
        .map(Some)
        .map_err(|err| {
            format!(
                "failed to load noteskin actor '{} {}': {err}",
                model.skin, model.element
            )
        })
}

pub fn collect_aft_capture_names(actor: &Table, out: &mut HashSet<String>) -> Result<(), String> {
    if actor
        .get::<Option<String>>("__songlua_actor_type")
        .map_err(|err| err.to_string())?
        .as_deref()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("ActorFrameTexture"))
        && actor
            .raw_get::<Option<Table>>("__songlua_aft_allocation")
            .map_err(|err| err.to_string())?
            .is_some()
        && let Some(capture_name) = actor_aft_capture_name(actor).map_err(|err| err.to_string())?
    {
        out.insert(capture_name);
    }
    for child in actor.sequence_values::<Value>() {
        let Value::Table(child) = child.map_err(|err| err.to_string())? else {
            continue;
        };
        collect_aft_capture_names(&child, out)?;
    }
    Ok(())
}

fn global_actor_references(lua: &Lua) -> Result<HashSet<usize>, String> {
    let registry = song_lua_actor_registry(lua).map_err(|err| err.to_string())?;
    let actor_pointers = registry
        .sequence_values::<Table>()
        .map(|actor| actor.map(|actor| actor.to_pointer() as usize))
        .collect::<mlua::Result<HashSet<_>>>()
        .map_err(|err| err.to_string())?;
    let globals = lua.globals();
    let globals_pointer = globals.to_pointer() as usize;
    let registry_pointer = registry.to_pointer() as usize;
    let mut referenced = HashSet::new();
    // Inspect only the same two table levels as before. Keys are irrelevant
    // to reachability, so avoid the pair iterator's cloned cursor handles.
    globals
        .for_each::<Value, Value>(|_, value| {
            let Value::Table(table) = value else {
                return Ok(());
            };
            let pointer = table.to_pointer() as usize;
            if actor_pointers.contains(&pointer) {
                referenced.insert(pointer);
                return Ok(());
            }
            if pointer == globals_pointer || pointer == registry_pointer {
                return Ok(());
            }
            table.for_each::<Value, Value>(|_, value| {
                let Value::Table(value) = value else {
                    return Ok(());
                };
                let pointer = value.to_pointer() as usize;
                if actor_pointers.contains(&pointer) {
                    referenced.insert(pointer);
                }
                Ok(())
            })
        })
        .map_err(|err| err.to_string())?;
    Ok(referenced)
}

pub fn actor_aft_capture_name(actor: &Table) -> mlua::Result<Option<String>> {
    if let Some(allocation) = actor.raw_get::<Option<Table>>("__songlua_aft_allocation")? {
        return allocation.get("name");
    }
    actor.get("__songlua_aft_capture_name")
}

/// Successful Create owns these settings; later setters only change requests.
pub fn read_aft_kind<N, M, T>(actor: &Table) -> Result<SongLuaOverlayKind<N, M, T>, String> {
    let allocation = actor
        .raw_get::<Option<Table>>("__songlua_aft_allocation")
        .map_err(|err| err.to_string())?;
    let flags = allocation.as_ref().unwrap_or(actor);
    Ok(SongLuaOverlayKind::ActorFrameTexture {
        capture_name: actor_aft_capture_name(actor)
            .map_err(|err| err.to_string())?
            .ok_or_else(|| "ActorFrameTexture has no texture name".to_string())?,
        capture_size: allocation
            .as_ref()
            .map(|allocation| {
                allocation
                    .get::<Table>("size")
                    .map(|size| table_vec2(&size))
            })
            .transpose()
            .map_err(|err| err.to_string())?
            .flatten(),
        alpha_buffer: flags
            .get::<Option<bool>>("__songlua_aft_alpha_buffer")
            .map_err(|err| err.to_string())?
            .unwrap_or(false),
        depth_buffer: flags
            .get::<Option<bool>>("__songlua_aft_depth_buffer")
            .map_err(|err| err.to_string())?
            .unwrap_or(false),
        float_buffer: flags
            .get::<Option<bool>>("__songlua_aft_float_buffer")
            .map_err(|err| err.to_string())?
            .unwrap_or(false),
    })
}

pub fn report_script_error(lua: &Lua, message: &str) -> mlua::Result<()> {
    const GUARD: &str = "__songlua_reporting_script_error";
    let globals = lua.globals();
    if globals.raw_get::<Option<bool>>(GUARD)?.unwrap_or(false) {
        return Ok(());
    }
    globals.raw_set(GUARD, true)?;
    let result = (|| {
        crate::note_song_lua_side_effect(lua)?;
        let params = lua.create_table()?;
        params.set("message", message)?;
        broadcast_song_lua_message(lua, "ScriptError", Some(Value::Table(params)))?;
        log::warn!("{message}");
        Ok(())
    })();
    globals.raw_set(GUARD, false)?;
    result
}

// RageTextureID preserves a leading ./ and a final ..; only slash-terminated
// components are collapsed (RageUtil.cpp:1855).
fn collapse_texture_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for part in name.split_inclusive('/') {
        if (part == "/" || part == "./") && !out.is_empty() {
            continue;
        }
        if part == "../" && !out.is_empty() && out != "/" {
            let start = out[..out.len() - 1].rfind('/').map_or(0, |index| index + 1);
            if &out[start..] != "../" {
                out.truncate(start);
                continue;
            }
        }
        out.push_str(part);
    }
    out
}

fn create_aft(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    if actor
        .raw_get::<Option<Table>>("__songlua_aft_allocation")?
        .is_some()
    {
        return report_script_error(lua, "Can't Create an already created ActorFrameTexture");
    }
    let name = collapse_texture_name(&actor.get::<String>("__songlua_aft_capture_name")?);
    for other in song_lua_actor_registry(lua)?.sequence_values::<Table>() {
        let other = other?;
        if let Some(allocation) = other.raw_get::<Option<Table>>("__songlua_aft_allocation")?
            && allocation.get::<String>("name")? == name
        {
            return report_script_error(lua, "ActorFrameTexture: Texture Name already in use.");
        }
        if let Some(path) = actor_texture_path(&other)?
            && collapse_texture_name(&file_path_string(&path)) == name
        {
            return report_script_error(lua, "ActorFrameTexture: Texture Name already in use.");
        }
    }
    let size = actor
        .raw_get::<Option<ActorVectorValue<2>>>("__songlua_state_size")?
        .and_then(|size| size.0)
        .unwrap_or([1.0; 2])
        .map(f32::trunc);
    if size.iter().any(|value| !value.is_finite() || *value < 1.0) {
        return report_script_error(
            lua,
            "ActorFrameTexture: Cannot have width or height less than 1",
        );
    }
    let allocation = lua.create_table()?;
    allocation.set("name", name)?;
    allocation.set("size", lua.create_sequence_from(size)?)?;
    for key in [
        "__songlua_aft_alpha_buffer",
        "__songlua_aft_depth_buffer",
        "__songlua_aft_float_buffer",
    ] {
        allocation.set(key, actor.get::<Option<bool>>(key)?.unwrap_or(false))?;
    }
    actor.raw_set("__songlua_aft_allocation", allocation)?;
    crate::note_song_lua_side_effect(lua)?;
    capture_immediate_bool(lua, actor, "aft_created", true)
}

pub fn read_overlay_compile_actors<NoteskinSlot, ModelVertex, ReadModel, ReadNoteskin, OnSkipped>(
    lua: &Lua,
    root: &Value,
    context: &SongLuaCompileContext,
    read_model_layers: ReadModel,
    read_noteskin_tap_actor_slots: ReadNoteskin,
    mut on_skipped_message_capture: OnSkipped,
) -> Result<
    Vec<SongLuaOverlayCompileActor<SongLuaOverlayKind<NoteskinSlot, ModelVertex, TextAttribute>>>,
    String,
>
where
    ReadModel: Fn(&Table) -> Result<Option<Arc<[SongLuaOverlayModelLayer<ModelVertex>]>>, String>,
    ReadNoteskin: Fn(&Table, &SongLuaCompileContext) -> Result<Option<Arc<[NoteskinSlot]>>, String>,
    OnSkipped: FnMut(String),
{
    let Value::Table(root) = root else {
        return Ok(Vec::new());
    };
    let mut aft_capture_names = HashSet::new();
    collect_aft_capture_names(root, &mut aft_capture_names)?;
    let referenced_actors = global_actor_references(lua)?;
    let mut out = Vec::new();
    read_overlay_compile_actors_from_table(
        lua,
        root,
        None,
        &aft_capture_names,
        &referenced_actors,
        &mut out,
        context,
        &read_model_layers,
        &read_noteskin_tap_actor_slots,
        &mut on_skipped_message_capture,
    )?;
    if !out.iter().any(|overlay| matches!(overlay.actor.kind,
        SongLuaOverlayKind::ActorProxy { target: SongLuaProxyTarget::Actor { .. } }))
    {
        return Ok(out);
    }
    let actor_indices = out
        .iter()
        .enumerate()
        .map(|(index, overlay)| (overlay.table.to_pointer() as usize, index))
        .collect::<HashMap<_, _>>();
    for overlay in &mut out {
        let SongLuaOverlayKind::ActorProxy {
            target: SongLuaProxyTarget::Actor { overlay_index },
        } = &mut overlay.actor.kind
        else {
            continue;
        };
        *overlay_index = actor_indices
            .get(overlay_index)
            .copied()
            .unwrap_or(usize::MAX);
    }
    Ok(out)
}

fn read_overlay_compile_actors_from_table<
    NoteskinSlot,
    ModelVertex,
    ReadModel,
    ReadNoteskin,
    OnSkipped,
>(
    lua: &Lua,
    actor: &Table,
    parent_index: Option<usize>,
    aft_capture_names: &HashSet<String>,
    referenced_actors: &HashSet<usize>,
    out: &mut Vec<
        SongLuaOverlayCompileActor<SongLuaOverlayKind<NoteskinSlot, ModelVertex, TextAttribute>>,
    >,
    context: &SongLuaCompileContext,
    read_model_layers: &ReadModel,
    read_noteskin_tap_actor_slots: &ReadNoteskin,
    on_skipped_message_capture: &mut OnSkipped,
) -> Result<(), String>
where
    ReadModel: Fn(&Table) -> Result<Option<Arc<[SongLuaOverlayModelLayer<ModelVertex>]>>, String>,
    ReadNoteskin: Fn(&Table, &SongLuaCompileContext) -> Result<Option<Arc<[NoteskinSlot]>>, String>,
    OnSkipped: FnMut(String),
{
    // Highest-index wrappers are outermost, exactly as Actor::Draw.
    let mut parent_index = parent_index;
    if let Some(wrappers) = actor
        .get::<Option<Table>>("__songlua_wrappers")
        .map_err(|err| err.to_string())?
    {
        for index in (1..=wrappers.raw_len()).rev() {
            let wrapper = wrappers
                .raw_get::<Table>(index)
                .map_err(|err| err.to_string())?;
            read_overlay_compile_actors_from_table(
                lua,
                &wrapper,
                parent_index,
                aft_capture_names,
                referenced_actors,
                out,
                context,
                read_model_layers,
                read_noteskin_tap_actor_slots,
                on_skipped_message_capture,
            )?;
            parent_index = Some(out.len() - 1);
        }
    }
    let next_parent_index = if let Some(overlay) = read_overlay_compile_actor(
        lua,
        actor,
        parent_index,
        aft_capture_names,
        referenced_actors,
        context,
        read_model_layers,
        read_noteskin_tap_actor_slots,
        on_skipped_message_capture,
    )? {
        let index = out.len();
        out.push(overlay);
        Some(index)
    } else {
        parent_index
    };
    // Actor::Draw only calls BeginDraw/EndDraw on wrapper ActorFrames; their
    // own ActorFrame children are not drawn.
    if actor_type_is(actor, "WrapperState").map_err(|err| err.to_string())? {
        return Ok(());
    }
    for child in actor.sequence_values::<Value>() {
        let Value::Table(child) = child.map_err(|err| err.to_string())? else {
            continue;
        };
        read_overlay_compile_actors_from_table(
            lua,
            &child,
            next_parent_index,
            aft_capture_names,
            referenced_actors,
            out,
            context,
            read_model_layers,
            read_noteskin_tap_actor_slots,
            on_skipped_message_capture,
        )?;
    }
    Ok(())
}

fn read_overlay_compile_actor<NoteskinSlot, ModelVertex, ReadModel, ReadNoteskin, OnSkipped>(
    lua: &Lua,
    actor: &Table,
    parent_index: Option<usize>,
    aft_capture_names: &HashSet<String>,
    referenced_actors: &HashSet<usize>,
    context: &SongLuaCompileContext,
    read_model_layers: &ReadModel,
    read_noteskin_tap_actor_slots: &ReadNoteskin,
    on_skipped_message_capture: &mut OnSkipped,
) -> Result<
    Option<
        SongLuaOverlayCompileActor<SongLuaOverlayKind<NoteskinSlot, ModelVertex, TextAttribute>>,
    >,
    String,
>
where
    ReadModel: Fn(&Table) -> Result<Option<Arc<[SongLuaOverlayModelLayer<ModelVertex>]>>, String>,
    ReadNoteskin: Fn(&Table, &SongLuaCompileContext) -> Result<Option<Arc<[NoteskinSlot]>>, String>,
    OnSkipped: FnMut(String),
{
    let Some(actor_type) = actor
        .get::<Option<String>>("__songlua_actor_type")
        .map_err(|err| err.to_string())?
    else {
        return Ok(None);
    };
    let initial_state = overlay_state_after_blocks(actor_overlay_initial_state(actor)?, &[], 0.0);
    let captured_commands = capture_actor_message_commands(lua, actor)?;
    for skipped in captured_commands.skipped {
        log::debug!("Skipping song lua overlay message capture for {skipped}");
        on_skipped_message_capture(skipped);
    }
    let message_commands = captured_commands.commands;
    let message_sounds = captured_commands.sounds;
    let name = actor
        .get::<Option<String>>("Name")
        .map_err(|err| err.to_string())?;

    let kind = if actor_type.eq_ignore_ascii_case("Actor") {
        // A referenced plain Actor can drive other actors through tween getters.
        if !context.live && name.is_none()
            && initial_state == SongLuaOverlayState::default()
            && message_commands.is_empty()
            && message_sounds.is_empty()
            && !has_deferred_message(lua, actor, None)
            && !referenced_actors.contains(&(actor.to_pointer() as usize))
        {
            return Ok(None);
        }
        SongLuaOverlayKind::Actor
    } else if actor_type.eq_ignore_ascii_case("WrapperState") {
        SongLuaOverlayKind::WrapperState
    } else if actor_type.eq_ignore_ascii_case("ActorFrame") {
        let has_draw_function = actor
            .get::<Option<Function>>("__songlua_draw_function")
            .map_err(|err| err.to_string())?
            .is_some();
        if !context.live && parent_index.is_none()
            && name.is_none()
            && initial_state == SongLuaOverlayState::default()
            && message_commands.is_empty()
            && message_sounds.is_empty()
            && !has_deferred_message(lua, actor, None)
            && !has_draw_function
            && !referenced_actors.contains(&(actor.to_pointer() as usize))
        {
            return Ok(None);
        }
        SongLuaOverlayKind::ActorFrame
    } else if actor_type.eq_ignore_ascii_case("ActorFrameTexture") {
        read_aft_kind(actor)?
    } else if actor_type.eq_ignore_ascii_case("ActorProxy") {
        // Some foregrounds bind proxies from an UpdateFunction after startup.
        // Keep their draw-tree position so the compiler can resolve the target
        // after it replays update functions.
        let target = read_proxy_target_kind(actor)?.unwrap_or(SongLuaProxyTarget::Actor {
            overlay_index: usize::MAX,
        });
        SongLuaOverlayKind::ActorProxy { target }
    } else if actor_type.eq_ignore_ascii_case("Sprite") || actor_type.eq_ignore_ascii_case("Banner") {
        if let Some(capture_name) = actor
            .get::<Option<String>>("__songlua_aft_capture_name")
            .map_err(|err| err.to_string())?
        {
            SongLuaOverlayKind::AftSprite { capture_name }
        } else {
            let binding = actor
                .raw_get::<Option<Table>>("__songlua_startup_sprite")
                .map_err(|err| err.to_string())?
                .unwrap_or_else(|| actor.clone());
            let texture = binding
                .get::<Option<String>>("Texture")
                .map_err(|err| err.to_string())?;
            if let Some(texture) = texture {
                let capture_name = collapse_texture_name(&texture);
                if aft_capture_names.contains(&capture_name) {
                    SongLuaOverlayKind::AftSprite { capture_name }
                } else {
                    let Some(texture_path) = resolve_actor_asset_path(actor, &texture).ok() else {
                        return Ok(None);
                    };
                    let texture_key = Arc::<str>::from(texture_path.to_string_lossy().into_owned());
                    SongLuaOverlayKind::Sprite {
                        texture_path,
                        texture_key,
                        states: read_sprite_states(&binding)?.into(),
                        textures: [].into(),
                    }
                }
            } else {
                // A message or recurring command can bind a texture later.
                // Keep the actor's slot so its updates and proxy targets survive.
                SongLuaOverlayKind::Actor
            }
        }
    } else if actor_type.eq_ignore_ascii_case("Sound") {
        let Some(file) = actor
            .get::<Option<String>>("File")
            .map_err(|err| err.to_string())?
        else {
            return Ok(None);
        };
        let Ok(sound_path) = resolve_actor_asset_path(actor, &file) else {
            return Ok(None);
        };
        SongLuaOverlayKind::Sound { sound_path }
    } else if actor_type.eq_ignore_ascii_case("BitmapText")
        || actor_type.eq_ignore_ascii_case("RollingNumbers")
    {
        let Some((font_name, font_path)) = read_bitmap_font(actor)? else {
            return Ok(None);
        };
        SongLuaOverlayKind::BitmapText {
            font_name,
            font_path,
            text: Arc::<str>::from(
                actor
                    .raw_get::<Option<String>>("__songlua_startup_text")
                    .map_err(|err| err.to_string())?
                    .or(actor
                        .get::<Option<String>>("Text")
                        .map_err(|err| err.to_string())?)
                    .unwrap_or_default(),
            ),
            stroke_color: read_actor_color_field(actor, "__songlua_stroke_color")?
                .or_else(|| read_actor_color_field(actor, "StrokeColor").ok().flatten()),
            attributes: read_bitmap_text_attributes(actor)?,
            text_changes: Arc::from([]),
            text_changes_in_seconds: false,
        }
    } else if actor_type.eq_ignore_ascii_case("DeviceList")
        || actor_type.eq_ignore_ascii_case("InputList")
    {
        let Some((font_name, font_path)) = read_bitmap_font(actor)? else {
            return Ok(None);
        };
        SongLuaOverlayKind::BitmapText {
            font_name,
            font_path,
            text: Arc::<str>::from(input_status_actor_text(&actor_type).unwrap_or_default()),
            text_changes: Arc::from([]),
            text_changes_in_seconds: false,
            stroke_color: None,
            attributes: Arc::<[TextAttribute]>::from([]),
        }
    } else if actor_type.eq_ignore_ascii_case("ActorMultiVertex") {
        let vertices = match read_actor_multi_vertex_mesh(actor)? {
            Some(vertices) => vertices,
            None if context.live => Arc::from([]),
            None => return Ok(None),
        };
        let texture_path = if context.live {
            live_multi_vertex_texture_path(actor).map_err(|error| error.to_string())?
        } else { read_actor_multi_vertex_texture_path(actor, aft_capture_names)? };
        let texture_key = texture_path
            .as_ref()
            .map(|path| Arc::<str>::from(path.to_string_lossy().into_owned()));
        SongLuaOverlayKind::ActorMultiVertex {
            vertices,
            texture_path,
            texture_key,
            capture_name: actor_aft_capture_name(actor)
                .map_err(|err| err.to_string())?
                .or(actor
                    .get::<Option<String>>("Texture")
                    .map_err(|err| err.to_string())?
                    .filter(|name| aft_capture_names.contains(name))),
        }
    } else if actor_type.eq_ignore_ascii_case("Model") {
        if let Some(slots) = read_noteskin_tap_actor_slots(actor, context)? {
            SongLuaOverlayKind::NoteskinActor { slots, texture_samples: Arc::from([]) }
        } else {
            let Some(layers) = read_model_layers(actor)? else {
                return Ok(None);
            };
            SongLuaOverlayKind::Model { layers }
        }
    } else if actor_type.eq_ignore_ascii_case("SongMeterDisplay") {
        let Some((stream_width, stream_state)) = read_song_meter_display_state(lua, actor)? else {
            return Ok(None);
        };
        SongLuaOverlayKind::SongMeterDisplay {
            stream_width,
            stream_state,
            music_length_seconds: context.music_length_seconds.max(0.0),
        }
    } else if actor_type.eq_ignore_ascii_case("CourseContentsList") {
        SongLuaOverlayKind::ActorFrame
    } else if actor_type.eq_ignore_ascii_case("GraphDisplay") {
        SongLuaOverlayKind::GraphDisplay {
            size: read_graph_display_size(initial_state, context),
            body_values: read_graph_display_values(actor)?,
            body_state: read_graph_display_body_state(lua, actor)?,
            line_state: Box::new(read_graph_display_line_state(lua, actor)?),
        }
    } else if actor_type.eq_ignore_ascii_case("Quad") {
        SongLuaOverlayKind::Quad
    } else {
        return Ok(None);
    };
    Ok(Some(SongLuaOverlayCompileActor {
        table: actor.clone(),
        message_sounds,
        actor: crate::SongLuaOverlayActor {
            kind,
            name,
            parent_index,
            initial_state,
            message_commands,
        },
    }))
}

pub(crate) fn read_sprite_states(actor: &Table) -> Result<Vec<crate::SongLuaSpriteState>, String> {
    let mut states = Vec::new();
    visit_sprite_states(actor, |state| states.push(state))?;
    Ok(states)
}

/// Retain the published native array until authored frame values change.
/// Inspect values in one pass: Frames can be edited in place, and legacy
/// frame fields can invoke __index. Identity checks or a second decoding pass
/// would miss mutations or repeat authored getter side effects.
pub(crate) fn sync_sprite_states(
    actor: &Table,
    states: &mut Arc<[crate::SongLuaSpriteState]>,
) -> Result<(), String> {
    let mut replacement: Option<Vec<crate::SongLuaSpriteState>> = None;
    let mut index = 0;
    visit_sprite_states(actor, |state| {
        if let Some(values) = &mut replacement {
            values.push(state);
        } else if states.get(index).is_none_or(|old|
            old.frame != state.frame || old.delay.to_bits() != state.delay.to_bits())
        {
            let mut values = Vec::with_capacity(states.len().max(index + 1));
            values.extend_from_slice(&states[..index]);
            values.push(state);
            replacement = Some(values);
        }
        index += 1;
    })?;
    // Publish only after a successful read; old draw snapshots keep their Arc.
    if let Some(values) = replacement { *states = values.into(); }
    else if index != states.len() { *states = Arc::from(&states[..index]); }
    Ok(())
}

fn visit_sprite_states(
    actor: &Table,
    mut visit: impl FnMut(crate::SongLuaSpriteState),
) -> Result<(), String> {
    if actor
        .raw_get::<Option<bool>>("__songlua_state_sprite_default_states")
        .map_err(|err| err.to_string())?
        .unwrap_or(false)
    {
        return Ok(());
    }
    if let Some(frames) = actor
        .raw_get::<Option<Table>>("Frames")
        .map_err(|err| err.to_string())?
    {
        let mut frame = 0;
        for entry in frames.sequence_values::<Table>() {
            let entry = entry.map_err(|err| err.to_string())?;
            frame = entry
                .get::<Option<u32>>("Frame")
                .map_err(|err| err.to_string())?
                .unwrap_or(frame);
            let delay = entry
                .get::<Option<f32>>("Delay")
                .map_err(|err| err.to_string())?
                .unwrap_or(0.1);
            visit(crate::SongLuaSpriteState { frame, delay });
        }
        return Ok(());
    }
    let mut key = [0u8; 16];
    for index in 0i32.. {
        let len = write_sprite_frame_key(index, &mut key);
        let frame_key = std::str::from_utf8(&key[..len]).expect("sprite keys are ASCII");
        let Some(frame) = actor
            .get::<Option<u32>>(frame_key)
            .map_err(|err| err.to_string())?
        else {
            break;
        };
        key[..5].copy_from_slice(b"Delay");
        let delay_key = std::str::from_utf8(&key[..len]).expect("sprite keys are ASCII");
        let delay = actor
            .get::<Option<f32>>(delay_key)
            .map_err(|err| err.to_string())?
            .unwrap_or(0.1);
        visit(crate::SongLuaSpriteState { frame, delay });
    }
    Ok(())
}

fn write_sprite_frame_key(index: i32, key: &mut [u8; 16]) -> usize {
    use std::io::Write;

    let mut output = &mut key[..];
    // Five prefix bytes plus the longest signed i32 fit in sixteen bytes.
    write!(&mut output, "Frame{index:04}").expect("sprite key buffer fits every i32");
    16 - output.len()
}

#[cfg(test)]
mod sprite_state_publication_tests {
    use super::*;

    fn assert_fresh(actor: &Table, states: &mut Arc<[crate::SongLuaSpriteState]>) {
        let expected = read_sprite_states(actor).unwrap();
        sync_sprite_states(actor, states).unwrap();
        assert_eq!(states.as_ref(), expected);
    }

    #[test]
    fn native_sprite_array_reuses_unchanged_values_and_preserves_old_snapshots() {
        let lua = Lua::new();
        let actor: Table = lua.load("return {Frames={{Frame=2,Delay='0.25'},{Delay=0.5}}}").eval().unwrap();
        let mut states: Arc<[_]> = Arc::from([]);
        assert_fresh(&actor, &mut states);
        let original = states.clone();
        assert_fresh(&actor, &mut states);
        assert!(Arc::ptr_eq(&original, &states));
        let frames = actor.raw_get::<Table>("Frames").unwrap();
        frames.raw_get::<Table>(1).unwrap().set("Frame", 3).unwrap();
        assert_fresh(&actor, &mut states);
        assert_eq!(states.iter().map(|state| state.frame).collect::<Vec<_>>(), [3, 3]);
        assert_eq!(original.iter().map(|state| state.frame).collect::<Vec<_>>(), [2, 2]);
        assert!(!Arc::ptr_eq(&original, &states));
        frames.raw_set(3, lua.load("return {Frame=7}").eval::<Table>().unwrap()).unwrap();
        assert_fresh(&actor, &mut states);
        assert_eq!(states.len(), 3);
        frames.raw_set(2, Value::Nil).unwrap();
        assert_fresh(&actor, &mut states);
        assert_eq!(states.len(), 1, "sequence traversal stops at the first hole");
        actor.raw_set("__songlua_state_sprite_default_states", true).unwrap();
        assert_fresh(&actor, &mut states);
        assert!(states.is_empty());
        let empty = states.clone();
        assert_fresh(&actor, &mut states);
        assert!(Arc::ptr_eq(&empty, &states));
        actor.raw_set("__songlua_state_sprite_default_states", false).unwrap();
        assert_fresh(&actor, &mut states);
        assert_eq!(states.len(), 1);
        actor.raw_set("Frames", Value::Nil).unwrap();
        actor.set("Frame0000", "9").unwrap();
        actor.set("Delay0000", 0.75).unwrap();
        assert_fresh(&actor, &mut states);
        assert_eq!(states[0].frame, 9);
        let legacy = states.clone();
        assert_fresh(&actor, &mut states);
        assert!(Arc::ptr_eq(&legacy, &states));
    }

    #[test]
    fn native_sprite_publication_visits_getters_once_and_keeps_last_good_array_on_error() {
        let lua = Lua::new();
        let actor: Table = lua.load(r#"
reads, frame, bad = 0, 4, false
return setmetatable({}, {__index=function(_, key)
    reads=reads+1
    if key=='Frame0000' then return frame end
    if key=='Delay0000' then return '0.5' end
    if key=='Frame0001' and bad then return {} end
end})
"#).eval().unwrap();
        let mut states: Arc<[_]> = Arc::from([]);
        for frame in [4, 4, 8] {
            lua.globals().set("reads", 0).unwrap();
            lua.globals().set("frame", frame).unwrap();
            let previous = states.clone();
            sync_sprite_states(&actor, &mut states).unwrap();
            assert_eq!(lua.globals().get::<u32>("reads").unwrap(), 3);
            assert_eq!(states[0].frame, frame);
            if previous.first().is_some_and(|state| state.frame == frame) {
                assert!(Arc::ptr_eq(&previous, &states));
            }
        }
        let previous = states.clone();
        lua.load("reads=0; frame=20; bad=true").exec().unwrap();
        let error = sync_sprite_states(&actor, &mut states).unwrap_err();
        assert_eq!(lua.globals().get::<u32>("reads").unwrap(), 3);
        assert_eq!(error, read_sprite_states(&actor).unwrap_err());
        assert!(Arc::ptr_eq(&previous, &states), "a partial read must not publish");
    }

    #[test]
    fn native_sprite_publication_compares_delay_bits() {
        let lua = Lua::new();
        let actor: Table = lua.load("return {Frames={{Frame=0,Delay=0}}}").eval().unwrap();
        let entry = actor.raw_get::<Table>("Frames").unwrap().raw_get::<Table>(1).unwrap();
        let mut states: Arc<[_]> = Arc::from([]);
        sync_sprite_states(&actor, &mut states).unwrap();
        let positive_zero = states.clone();
        entry.set("Delay", -0.0_f32).unwrap();
        sync_sprite_states(&actor, &mut states).unwrap();
        assert_eq!(states[0].delay.to_bits(), (-0.0_f32).to_bits());
        assert!(!Arc::ptr_eq(&positive_zero, &states));
        entry.set("Delay", f64::NAN).unwrap();
        sync_sprite_states(&actor, &mut states).unwrap();
        assert!(states[0].delay.is_nan());
        let nan = states.clone();
        sync_sprite_states(&actor, &mut states).unwrap();
        assert!(Arc::ptr_eq(&nan, &states), "unchanged NaNs should not force allocations");
    }
}

fn read_actor_multi_vertex_range(
    actor: &Table,
    vertex_count: usize,
) -> Result<Option<(usize, usize)>, String> {
    if vertex_count == 0 {
        return Ok(None);
    }
    let state = actor
        .get::<Option<Table>>("__songlua_draw_state")
        .map_err(|err| err.to_string())?;
    let first = state
        .as_ref()
        .and_then(|state| state.get::<Option<Value>>("First").ok().flatten())
        .and_then(read_i32_value)
        .unwrap_or(1)
        .max(1) as usize;
    let start = first.saturating_sub(1).min(vertex_count);
    let num = state
        .as_ref()
        .and_then(|state| state.get::<Option<Value>>("Num").ok().flatten())
        .and_then(read_i32_value)
        .unwrap_or(-1);
    let end = if num < 0 {
        vertex_count
    } else {
        start.saturating_add(num as usize).min(vertex_count)
    };
    Ok((end > start).then_some((start, end)))
}

// Copy the common numeric vertex layout with one VM lock and stack frame.
// Noncanonical inputs retain the existing conversions and defaults below.
fn read_actor_multi_vertex_upload(
    lua: &Lua,
    vertices: &Table,
) -> mlua::Result<Arc<[SongLuaActorMultiVertexPoint]>> {
    let count = vertices.raw_len();
    let mut points = Vec::with_capacity(count);
    let mut complete = false;
    // SAFETY: exec_raw locks the VM and owns a stack frame containing `vertices`
    // at index 1. Every raw access checks its table type first. The helper pops
    // each scalar, we reserve stack space, and no stack reference escapes.
    unsafe {
        lua.exec_raw::<()>(vertices, |state| {
            if count <= c_int::MAX as usize && ffi::lua_checkstack(state, 4) != 0 {
                for index in 1..=count {
                    ffi::lua_settop(state, 1);
                    ffi::lua_rawgeti(state, 1, index as _);
                    if ffi::lua_type(state, 2) != ffi::LUA_TTABLE {
                        break;
                    }
                    ffi::lua_rawgeti(state, 2, 1);
                    let Some(position) = vertex_numeric_array::<3>(state, 3) else {
                        break;
                    };
                    ffi::lua_pop(state, 1);
                    ffi::lua_rawgeti(state, 2, 2);
                    let Some(color) = vertex_numeric_array::<4>(state, 3) else {
                        break;
                    };
                    ffi::lua_pop(state, 1);
                    ffi::lua_rawgeti(state, 2, 3);
                    let Some(uv) = vertex_numeric_array::<2>(state, 3) else {
                        break;
                    };
                    // The general UV reader validates finite numbers; positions
                    // deliberately retain NaNs used to hide complete primitives.
                    if !uv.iter().all(|value| value.is_finite()) {
                        break;
                    }
                    points.push(SongLuaActorMultiVertexPoint {
                        pos: [position[0], position[1]],
                        z: position[2],
                        color,
                        uv,
                    });
                }
                complete = points.len() == count;
            }
            ffi::lua_settop(state, 0);
        })?;
    }
    if complete {
        return Ok(points.into());
    }
    read_actor_multi_vertex_points(vertices)
        .map(Into::into)
        .map_err(mlua::Error::external)
}

#[cfg(test)]
mod vertex_upload_tests {
    use super::*;

    #[test]
    fn packed_vertex_upload_matches_general_reader_and_restores_stack() {
        let lua = Lua::new();
        for expression in [
            "{}",
            "{{{1,2,3},{0.25,0.5,0.75,1},{0.1,0.2}}}",
            "{{{0/0,2,-3},{1,1,1,1},{0,1}}}",
            "{{{1,2},{1,0,0},{0,1}}}",
            "{{{1,2,3},'#ff0000',{0/0,1}}}",
            "{{{'1','2','3'},{'0.5',1,1,1},{'0.1','0.2'}}}",
            "{false,{{1,2,3},{1,1,1,1},{0,1}}}",
        ] {
            let vertices: Table = lua.load(format!("return {expression}")).eval().unwrap();
            let expected = read_actor_multi_vertex_points(&vertices).unwrap();
            let actual = read_actor_multi_vertex_upload(&lua, &vertices).unwrap();
            assert_eq!(actual.len(), expected.len(), "{expression}");
            for (actual, expected) in actual.iter().zip(&expected) {
                let scalars = |point: &SongLuaActorMultiVertexPoint| {
                    [
                        point.pos[0],
                        point.pos[1],
                        point.z,
                        point.color[0],
                        point.color[1],
                        point.color[2],
                        point.color[3],
                        point.uv[0],
                        point.uv[1],
                    ]
                    .map(f32::to_bits)
                };
                assert_eq!(scalars(actual), scalars(expected), "{expression}");
            }
            assert_eq!(lua.load("return 6*7").eval::<u32>().unwrap(), 42);
        }
    }
}

unsafe fn vertex_numeric_array<const N: usize>(
    state: *mut ffi::lua_State,
    table: c_int,
) -> Option<[f32; N]> {
    // SAFETY: the caller owns this live stack frame and reserves one scalar
    // slot. Only numeric values are converted, and each pushed value is popped.
    unsafe {
        if ffi::lua_type(state, table) != ffi::LUA_TTABLE {
            return None;
        }
        let mut values = [0.0; N];
        for (index, value) in values.iter_mut().enumerate() {
            ffi::lua_rawgeti(state, table, (index + 1) as _);
            let numeric = ffi::lua_type(state, -1) == ffi::LUA_TNUMBER;
            if numeric {
                *value = ffi::lua_tonumber(state, -1) as f32;
            }
            ffi::lua_pop(state, 1);
            if !numeric {
                return None;
            }
        }
        Some(values)
    }
}

fn read_actor_multi_vertex_points(
    vertices: &Table,
) -> Result<Vec<SongLuaActorMultiVertexPoint>, String> {
    let mut out = Vec::with_capacity(vertices.raw_len());
    for value in vertices.sequence_values::<Value>() {
        let Some(vertex) = read_actor_multi_vertex_point(value.map_err(|err| err.to_string())?)?
        else {
            continue;
        };
        out.push(vertex);
    }
    Ok(out)
}

fn read_actor_multi_vertex_point(
    value: Value,
) -> Result<Option<SongLuaActorMultiVertexPoint>, String> {
    let Value::Table(vertex) = value else {
        return Ok(None);
    };
    let Value::Table(position) = vertex.raw_get::<Value>(1).map_err(|err| err.to_string())? else {
        return Ok(None);
    };
    let Some(x) = read_actor_scalar_f32(
        position
            .raw_get::<Value>(1)
            .map_err(|err| err.to_string())?,
    ) else {
        return Ok(None);
    };
    let Some(y) = read_actor_scalar_f32(
        position
            .raw_get::<Value>(2)
            .map_err(|err| err.to_string())?,
    ) else {
        return Ok(None);
    };
    let color = read_color_value(vertex.raw_get::<Value>(2).map_err(|err| err.to_string())?)
        .unwrap_or([1.0; 4]);
    let uv = match vertex.raw_get::<Value>(3).map_err(|err| err.to_string())? {
        Value::Table(table) => [
            table
                .raw_get::<Value>(1)
                .ok()
                .and_then(read_f32)
                .unwrap_or(0.0),
            table
                .raw_get::<Value>(2)
                .ok()
                .and_then(read_f32)
                .unwrap_or(0.0),
        ],
        _ => [0.0, 0.0],
    };
    Ok(Some(SongLuaActorMultiVertexPoint {
        pos: [x, y],
        z: read_actor_scalar_f32(position.raw_get::<Value>(3).map_err(|err| err.to_string())?).unwrap_or(0.0),
        color,
        uv,
    }))
}

fn actor_multi_vertex_mesh_arc(
    vertices: &[SongLuaActorMultiVertexPoint],
    mode: &str,
    width: f32,
) -> Option<Arc<[SongLuaOverlayMeshVertex]>> {
    if mode.eq_ignore_ascii_case("DrawMode_Quads") {
        let groups = vertices.as_chunks::<4>().0;
        if groups.is_empty() {
            return None;
        }
        return Some(
            (0..groups.len() * 6)
                .map(|i| {
                    actor_multi_vertex_mesh_vertex(vertices[i / 6 * 4 + [0, 1, 2, 2, 3, 0][i % 6]])
                })
                .collect(),
        );
    }
    if mode.eq_ignore_ascii_case("DrawMode_QuadStrip") {
        let count = vertices.len().saturating_sub(2) / 2;
        if count == 0 {
            return None;
        }
        return Some(
            (0..count * 6)
                .map(|i| {
                    actor_multi_vertex_mesh_vertex(vertices[i / 6 * 2 + [0, 1, 3, 3, 2, 0][i % 6]])
                })
                .collect(),
        );
    }
    if mode.eq_ignore_ascii_case("DrawMode_LineStrip") {
        let mesh = actor_multi_vertex_line_strip(vertices, width);
        return (!mesh.is_empty()).then(|| mesh.into());
    }
    if mode.eq_ignore_ascii_case("DrawMode_Fan") {
        if vertices.len() < 3 {
            return None;
        }
        return Some(
            (0..(vertices.len() - 2) * 3)
                .map(|i| {
                    actor_multi_vertex_mesh_vertex(match i % 3 {
                        0 => vertices[0],
                        1 => vertices[i / 3 + 1],
                        _ => vertices[i / 3 + 2],
                    })
                })
                .collect(),
        );
    }
    let count = vertices.len() / 3 * 3;
    if count == 0 {
        return None;
    }
    Some(
        vertices[..count]
            .iter()
            .copied()
            .map(actor_multi_vertex_mesh_vertex)
            .collect(),
    )
}

fn actor_multi_vertex_line_strip(
    vertices: &[SongLuaActorMultiVertexPoint],
    line_width: f32,
) -> Vec<SongLuaOverlayMeshVertex> {
    if vertices.len() < 2 || line_width <= f32::EPSILON {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(vertices.len().saturating_sub(1) * 6);
    let half_width = 0.5 * line_width;
    let (mut len, mut normal) = actor_multi_vertex_line_segment(vertices[0], vertices[1]);
    let mut offset = actor_multi_vertex_line_endpoint(len, normal, half_width);
    // Only adjacent segment normals and join offsets are needed. Calculate
    // each segment once and carry the shared endpoint into the next quad.
    for (index, pair) in vertices.windows(2).enumerate() {
        let next = vertices
            .get(index + 2)
            .map(|&next| actor_multi_vertex_line_segment(pair[1], next));
        let next_offset = if let Some((_, next_normal)) = next {
            actor_multi_vertex_line_join_offset(normal, next_normal, half_width)
        } else {
            actor_multi_vertex_line_endpoint(len, normal, half_width)
        };
        let current_offset = offset;
        let skip_segment = len <= f32::EPSILON;
        offset = next_offset;
        if let Some((next_len, next_normal)) = next {
            len = next_len;
            normal = next_normal;
        }
        // Carry the join forward even when a degenerate segment is skipped.
        if skip_segment {
            continue;
        }
        let a0 = actor_multi_vertex_offset_point(pair[0], current_offset, 1.0);
        let a1 = actor_multi_vertex_offset_point(pair[0], current_offset, -1.0);
        let b0 = actor_multi_vertex_offset_point(pair[1], next_offset, 1.0);
        let b1 = actor_multi_vertex_offset_point(pair[1], next_offset, -1.0);
        push_actor_multi_vertex_triangle(&mut out, a0, b0, b1);
        push_actor_multi_vertex_triangle(&mut out, a0, b1, a1);
    }
    out
}

fn actor_multi_vertex_line_segment(
    a: SongLuaActorMultiVertexPoint,
    b: SongLuaActorMultiVertexPoint,
) -> (f32, [f32; 2]) {
    let dx = b.pos[0] - a.pos[0];
    let dy = b.pos[1] - a.pos[1];
    let len = dx.hypot(dy);
    let normal = if len <= f32::EPSILON {
        [0.0, 0.0]
    } else {
        [-dy / len, dx / len]
    };
    (len, normal)
}

fn actor_multi_vertex_line_endpoint(len: f32, normal: [f32; 2], half_width: f32) -> [f32; 2] {
    if len <= f32::EPSILON {
        [0.0, 0.0]
    } else {
        [normal[0] * half_width, normal[1] * half_width]
    }
}

fn actor_multi_vertex_line_join_offset(
    prev_normal: [f32; 2],
    next_normal: [f32; 2],
    half_width: f32,
) -> [f32; 2] {
    let miter = [
        prev_normal[0] + next_normal[0],
        prev_normal[1] + next_normal[1],
    ];
    let miter_len = miter[0].hypot(miter[1]);
    if miter_len <= f32::EPSILON {
        return [prev_normal[0] * half_width, prev_normal[1] * half_width];
    }
    let miter = [miter[0] / miter_len, miter[1] / miter_len];
    let denom = miter[1].mul_add(prev_normal[1], miter[0] * prev_normal[0]);
    if denom.abs() <= 0.1 {
        return [next_normal[0] * half_width, next_normal[1] * half_width];
    }
    let scale = (half_width / denom).clamp(-half_width * 4.0, half_width * 4.0);
    [miter[0] * scale, miter[1] * scale]
}

fn actor_multi_vertex_offset_point(
    vertex: SongLuaActorMultiVertexPoint,
    offset: [f32; 2],
    sign: f32,
) -> SongLuaActorMultiVertexPoint {
    SongLuaActorMultiVertexPoint {
        pos: [
            offset[0].mul_add(sign, vertex.pos[0]),
            offset[1].mul_add(sign, vertex.pos[1]),
        ],
        z: vertex.z,
        color: vertex.color,
        uv: vertex.uv,
    }
}

fn push_actor_multi_vertex_triangle(
    out: &mut Vec<SongLuaOverlayMeshVertex>,
    a: SongLuaActorMultiVertexPoint,
    b: SongLuaActorMultiVertexPoint,
    c: SongLuaActorMultiVertexPoint,
) {
    out.push(actor_multi_vertex_mesh_vertex(a));
    out.push(actor_multi_vertex_mesh_vertex(b));
    out.push(actor_multi_vertex_mesh_vertex(c));
}

#[inline(always)]
const fn actor_multi_vertex_mesh_vertex(
    vertex: SongLuaActorMultiVertexPoint,
) -> SongLuaOverlayMeshVertex {
    SongLuaOverlayMeshVertex {
        pos: vertex.pos,
        z: vertex.z,
        color: vertex.color,
        uv: vertex.uv,
    }
}

fn actor_base_zoom_axis(lua: &Lua, actor: &Table, key: &str) -> mlua::Result<f32> {
    if let Some(field) = key.strip_prefix("__songlua_state_").and_then(BaseField::from_key)
        && let Some(base) = live_base_transform(lua, actor) {
        return Ok(base.zoom_axis(field));
    }
    Ok(actor.get::<Option<f32>>(key)?
        .or(actor.get::<Option<f32>>("__songlua_state_basezoom")?).unwrap_or(1.0))
}

pub fn actor_zoom_axis(lua: &Lua, actor: &Table, zoom_key: &str, basezoom_key: &str) -> mlua::Result<f32> {
    let zoom = match zoom_key.strip_prefix("__songlua_state_")
        .and_then(|key| live_destination_scalar(lua, actor, key)) {
        Some(value) => value,
        None => actor.get::<Option<f32>>(zoom_key)?
            .or(actor.get::<Option<f32>>("__songlua_state_zoom")?).unwrap_or(1.0),
    };
    let basezoom = actor_base_zoom_axis(lua, actor, basezoom_key)?;
    Ok(basezoom * zoom)
}

pub fn actor_texture_path(actor: &Table) -> mlua::Result<Option<PathBuf>> {
    // Quad::LoadFromNode bypasses Sprite's Texture attribute loading.
    if actor.raw_get::<bool>("__songlua_default_texture")? {
        return Ok(None);
    }
    let Some(texture) = actor.get::<Option<String>>("Texture")? else {
        return Ok(None);
    };
    let texture = texture.trim();
    if texture.is_empty() {
        return Ok(None);
    }
    let raw = Path::new(texture);
    if deadlib_assets::generated_texture_shared_key(texture).is_some() {
        return Ok(Some(raw.to_path_buf()));
    }
    if raw.is_absolute() && raw.exists() {
        return Ok(Some(raw.to_path_buf()));
    }
    if raw.is_absolute()
        && let Some(path) = resolve_actor_asset_prefix(Path::new(""), raw)
    {
        return Ok(Some(path));
    }
    if let Some(script_dir) = actor.get::<Option<String>>("__songlua_script_dir")? {
        let candidate = Path::new(&script_dir).join(texture);
        if candidate.exists() {
            return Ok(Some(candidate));
        }
        if let Some(path) = resolve_actor_asset_prefix(Path::new(&script_dir), raw) {
            return Ok(Some(path));
        }
    }
    if let Some(song_dir) = actor.get::<Option<String>>("__songlua_song_dir")? {
        let candidate = Path::new(&song_dir).join(texture);
        if candidate.exists() {
            return Ok(Some(candidate));
        }
        if let Some(path) = resolve_actor_asset_prefix(Path::new(&song_dir), raw) {
            return Ok(Some(path));
        }
    }
    Ok(None)
}

pub fn read_proxy_target_kind(actor: &Table) -> Result<Option<SongLuaProxyTarget>, String> {
    let Some(raw_kind) = actor
        .get::<Option<String>>("__songlua_proxy_target_kind")
        .map_err(|err| err.to_string())?
    else {
        return Ok(None);
    };
    let player_index = actor
        .get::<Option<i64>>("__songlua_proxy_player_index")
        .map_err(|err| err.to_string())?
        .and_then(|value| usize::try_from(value).ok());
    let target_actor = actor
        .get::<Option<Table>>("__songlua_proxy_target_actor")
        .map_err(|err| err.to_string())?;
    let target_hidden = target_actor
        .as_ref()
        .and_then(|target| {
            target
                .get::<Option<bool>>("__songlua_visible")
                .ok()
                .flatten()
        })
        .is_some_and(|visible| !visible);
    Ok(match raw_kind.as_str() {
        "player" => player_index.map(|player_index| SongLuaProxyTarget::Player { player_index }),
        "notefield" => {
            player_index.map(|player_index| SongLuaProxyTarget::NoteField { player_index })
        }
        "judgment" => {
            player_index.map(|player_index| SongLuaProxyTarget::Judgment { player_index })
        }
        "combo" => player_index.map(|player_index| SongLuaProxyTarget::Combo { player_index }),
        "score" => player_index.map(|player_index| SongLuaProxyTarget::Score { player_index }),
        "underlay" => Some(SongLuaProxyTarget::Underlay {
            hidden: target_hidden,
        }),
        "overlay" => Some(SongLuaProxyTarget::Overlay {
            hidden: target_hidden,
        }),
        "actor" => target_actor.map(|target| SongLuaProxyTarget::Actor {
            overlay_index: target.to_pointer() as usize,
        }),
        _ => None,
    })
}

pub fn resolve_actor_asset_path(actor: &Table, raw: &str) -> Result<PathBuf, String> {
    let raw_path = Path::new(raw.trim());
    if deadlib_assets::generated_texture_shared_key(raw.trim()).is_some() {
        return Ok(raw_path.to_path_buf());
    }
    if raw_path.is_absolute() && raw_path.is_file() {
        return Ok(raw_path.to_path_buf());
    }
    if let Some(script_dir) = actor
        .get::<Option<String>>("__songlua_script_dir")
        .map_err(|err| err.to_string())?
        .filter(|dir| !dir.trim().is_empty())
    {
        let candidate = Path::new(&script_dir).join(raw);
        if candidate.is_file() {
            return Ok(candidate);
        }
        if let Some(path) = resolve_actor_asset_prefix(Path::new(&script_dir), raw_path) {
            return Ok(path);
        }
    }
    Err(format!("actor asset '{raw}' could not be resolved"))
}

fn resolve_actor_asset_prefix(script_dir: &Path, raw_path: &Path) -> Option<PathBuf> {
    if raw_path.extension().is_some() {
        return None;
    }
    let prefix = raw_path.file_name()?.to_str()?.trim();
    if prefix.is_empty() {
        return None;
    }
    let dir = raw_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .map(|parent| script_dir.join(parent))
        .unwrap_or_else(|| script_dir.to_path_buf());
    if !dir.is_dir() {
        return None;
    }
    let key = (
        dir.to_string_lossy().to_ascii_lowercase(),
        prefix.to_ascii_lowercase(),
    );
    let cache = ACTOR_ASSET_PREFIX_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(cached) = cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&key)
        .cloned()
    {
        return cached;
    }
    let found = fs::read_dir(&dir)
        .ok()?
        .flatten()
        .filter_map(|entry| {
            let file_name = entry.file_name();
            let name = file_name.to_str()?;
            let path = name
                .get(..prefix.len())
                .is_some_and(|start| start.eq_ignore_ascii_case(prefix))
                .then(|| entry.path())?;
            (path.is_file() && is_song_lua_media_path(&path)).then_some(path)
        })
        .min_by(|left, right| {
            let left = left
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("");
            let right = right
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("");
            left.bytes()
                .map(|byte| byte.to_ascii_lowercase())
                .cmp(right.bytes().map(|byte| byte.to_ascii_lowercase()))
        });
    cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(key, found.clone());
    found
}

pub fn read_bitmap_font(actor: &Table) -> Result<Option<(&'static str, PathBuf)>, String> {
    let Some(font) = actor
        .get::<Option<String>>("Font")
        .map_err(|err| err.to_string())?
    else {
        return Ok(None);
    };
    if let Ok(font_path) = resolve_actor_asset_path(actor, &font) {
        return Ok(Some((song_lua_font_name(font_path.as_path()), font_path)));
    }
    if song_lua_uses_builtin_theme_font(&font) {
        return Ok(Some((
            "miso",
            PathBuf::from("assets/fonts/miso/_miso light.ini"),
        )));
    }
    Ok(None)
}

pub fn read_bitmap_text_attributes(actor: &Table) -> Result<Arc<[TextAttribute]>, String> {
    let Some(attributes) = actor
        .get::<Option<Table>>("__songlua_text_attributes")
        .map_err(|err| err.to_string())?
    else {
        return Ok(Arc::<[TextAttribute]>::from([]));
    };
    let mut out = Vec::with_capacity(attributes.raw_len());
    for entry in attributes.sequence_values::<Table>() {
        let entry = entry.map_err(|err| err.to_string())?;
        if let Some(attribute) = read_bitmap_text_attribute(&entry)? {
            out.push(attribute);
        }
    }
    Ok(Arc::from(out.into_boxed_slice()))
}

pub(crate) fn read_bitmap_text_attribute(entry: &Table) -> Result<Option<TextAttribute>, String> {
    let start_value = entry
        .raw_get::<Value>("start")
        .map_err(|err| err.to_string())?;
    let start = read_i32_value(start_value).unwrap_or(0).max(0) as usize;
    let length_value = entry
        .raw_get::<Value>("length")
        .map_err(|err| err.to_string())?;
    let length_raw = read_i32_value(length_value).unwrap_or(1);
    if length_raw == 0 {
        return Ok(None);
    }
    let length = if length_raw < 0 {
        usize::MAX
    } else {
        length_raw as usize
    };
    // Missing or non-table optional colors retain the same fallback without
    // constructing and discarding a Lua conversion error for each field.
    let color = match entry.raw_get::<Value>("color") {
        Ok(Value::Table(color)) => table_vec4(&color),
        _ => None,
    }.unwrap_or([1.0, 1.0, 1.0, 1.0]);
    let vertex_colors = match entry.raw_get::<Value>("vertex_colors") {
        Ok(Value::Table(colors)) => table_vertex_colors(&colors),
        _ => None,
    };
    let glow = match entry.raw_get::<Value>("glow") {
        Ok(Value::Table(color)) => table_vec4(&color),
        _ => None,
    };
    Ok(Some(TextAttribute {
        start,
        length,
        color,
        vertex_colors,
        glow,
    }))
}

fn song_lua_uses_builtin_theme_font(raw: &str) -> bool {
    let trimmed = raw.trim().trim_start_matches('/');
    if trimmed.is_empty() {
        return false;
    }
    let normalized = trimmed.replace('\\', "/");
    normalized.starts_with(SONG_LUA_THEME_PATH_PREFIX)
        || (!normalized.contains('/') && Path::new(&normalized).extension().is_none())
}

fn song_lua_font_name(font_path: &Path) -> &'static str {
    static SONG_LUA_FONT_NAMES: OnceLock<Mutex<HashMap<String, &'static str>>> = OnceLock::new();
    let canonical = font_path.to_string_lossy().replace('\\', "/");
    let cache = SONG_LUA_FONT_NAMES.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(&name) = guard.get(&canonical) {
        return name;
    }
    let leaked = Box::leak(format!("songlua_font:{canonical}").into_boxed_str());
    guard.insert(canonical, leaked);
    leaked
}

pub fn actor_texture_is_video(actor: &Table) -> mlua::Result<bool> {
    if let Some(path) = actor_texture_path(actor)? {
        return Ok(is_song_lua_video_path(&path));
    }
    Ok(actor
        .get::<Option<String>>("Texture")?
        .is_some_and(|texture| is_song_lua_video_path(Path::new(texture.trim()))))
}

pub fn set_actor_decode_movie_for_texture(actor: &Table) -> mlua::Result<()> {
    actor.set(
        "__songlua_state_decode_movie",
        actor_texture_is_video(actor)?,
    )
}

pub fn actor_decode_movie(actor: &Table) -> mlua::Result<bool> {
    Ok(actor
        .get::<Option<bool>>("__songlua_state_decode_movie")?
        .unwrap_or(actor_texture_is_video(actor)?))
}

pub fn read_actor_color_field(actor: &Table, key: &str) -> Result<Option<[f32; 4]>, String> {
    Ok(actor
        .get::<Option<Table>>(key)
        .map_err(|err| err.to_string())?
        .and_then(|value| table_vec4(&value)))
}

pub fn actor_diffuse(lua: &Lua, actor: &Table) -> mlua::Result<[f32; 4]> {
    if let Some(diffuse) = live_destination_value(lua, actor, |dest| dest.diffuse) { return Ok(diffuse); }
    Ok(actor
        .get::<Option<Table>>("__songlua_diffuse")?
        .and_then(|value| table_vec4(&value))
        .unwrap_or([1.0, 1.0, 1.0, 1.0]))
}

pub fn actor_vertex_colors(lua: &Lua, actor: &Table) -> mlua::Result<[[f32; 4]; 4]> {
    if let Some(colors) = live_destination_value(lua, actor, |dest| dest.vertex_colors) {
        return Ok(colors.unwrap_or([[1.0; 4]; 4]));
    }
    Ok(actor
        .get::<Option<Table>>("__songlua_state_vertex_colors")?
        .and_then(|value| table_vertex_colors(&value))
        .unwrap_or([[1.0, 1.0, 1.0, 1.0]; 4]))
}

pub fn text_attribute_matches(
    attr: &Table,
    start: i32,
    length: i32,
    color: [f32; 4],
    vertex_colors: [[f32; 4]; 4],
    glow: Option<[f32; 4]>,
) -> mlua::Result<bool> {
    if attr.raw_get::<Option<i32>>("start")?.unwrap_or(-1) != start
        || attr.raw_get::<Option<i32>>("length")?.unwrap_or(0) != length
    {
        return Ok(false);
    }
    let existing_color = attr
        .raw_get::<Option<Table>>("color")?
        .and_then(|value| table_vec4(&value))
        .unwrap_or([1.0; 4]);
    let existing_vertex_colors = attr
        .raw_get::<Option<Table>>("vertex_colors")?
        .and_then(|value| table_vertex_colors(&value))
        .unwrap_or([existing_color; 4]);
    let existing_glow = attr
        .raw_get::<Option<Table>>("glow")?
        .and_then(|value| table_vec4(&value));
    Ok(existing_color == color && existing_vertex_colors == vertex_colors && existing_glow == glow)
}

pub fn text_attribute_value(params: &Table, keys: &[&str]) -> mlua::Result<Option<Value>> {
    for key in keys {
        let value = params.get::<Value>(*key)?;
        if !matches!(value, Value::Nil) {
            return Ok(Some(value));
        }
    }
    Ok(None)
}

pub fn actor_glow(lua: &Lua, actor: &Table) -> mlua::Result<[f32; 4]> {
    if let Some(glow) = live_destination_value(lua, actor, |dest| dest.glow) { return Ok(glow); }
    Ok(actor
        .get::<Option<Table>>("__songlua_state_glow")?
        .and_then(|value| table_vec4(&value))
        .unwrap_or([1.0, 1.0, 1.0, 0.0]))
}

pub fn actor_effect_magnitude(actor: &Table) -> mlua::Result<[f32; 3]> {
    Ok(actor
        .get::<Option<ActorVectorValue<3>>>("__songlua_state_effect_magnitude")?
        .and_then(|value| value.0)
        .unwrap_or([0.0, 0.0, 0.0]))
}

#[must_use]
pub fn banner_sort_order_path(sort_order: &str) -> Option<String> {
    let short = sort_order
        .trim()
        .split_once('_')
        .map(|(_, short)| short)
        .unwrap_or_else(|| sort_order.trim());
    let short = short
        .strip_suffix("_P1")
        .or_else(|| short.strip_suffix("_P2"))
        .unwrap_or(short);
    match short {
        "Group" => None,
        "" | "Invalid" => Some(theme_path("G", "Common", "fallback banner")),
        sort => Some(theme_path("G", "Banner", sort)),
    }
}

pub fn song_lua_screen_size(lua: &Lua) -> mlua::Result<(f32, f32)> {
    let globals = lua.globals();
    let width = globals.get::<Option<i32>>("SCREEN_WIDTH")?.unwrap_or(640) as f32;
    let height = globals.get::<Option<i32>>("SCREEN_HEIGHT")?.unwrap_or(480) as f32;
    Ok((width, height))
}

pub fn song_lua_screen_center(lua: &Lua) -> mlua::Result<(f32, f32)> {
    let globals = lua.globals();
    let center_x = globals
        .get::<Option<f32>>("SCREEN_CENTER_X")?
        .unwrap_or(song_lua_screen_size(lua)?.0 * 0.5);
    let center_y = globals
        .get::<Option<f32>>("SCREEN_CENTER_Y")?
        .unwrap_or(song_lua_screen_size(lua)?.1 * 0.5);
    Ok((center_x, center_y))
}

// Reuse the callback's buffer, then restore it so all arguments stay alive
// through the caller's Text setter, including on the formatter's error path.
fn lua_format_text_in_place(lua: &Lua, args: &mut MultiValue) -> mlua::Result<String> {
    let receiver = if method_arg_offset(args) == 1 {
        args.pop_front()
    } else {
        None
    };
    let result = (|| {
        if args.is_empty() {
            return Ok(String::new());
        }
        let string_table = lua.globals().get::<Table>("string")?;
        let format = string_table.get::<Function>("format")?;
        lua_text_value(format.call::<Value>(&*args)?)
    })();
    if let Some(receiver) = receiver {
        args.push_front(receiver);
    }
    result
}

pub fn lua_format_text(lua: &Lua, args: &MultiValue) -> mlua::Result<String> {
    let offset = method_arg_offset(args);
    if args.get(offset).is_none() {
        return Ok(String::new());
    }
    let mut call_args = MultiValue::new();
    for index in offset..args.len() {
        if let Some(value) = args.get(index) {
            call_args.push_back(value.clone());
        }
    }
    let string_table = lua.globals().get::<Table>("string")?;
    let format = string_table.get::<Function>("format")?;
    lua_text_value(format.call::<Value>(call_args)?)
}

pub fn create_string_array(lua: &Lua, values: &[&str]) -> mlua::Result<Table> {
    let table = lua.create_table_with_capacity(values.len(), 0)?;
    for (index, value) in values.iter().enumerate() {
        table.raw_set(index + 1, *value)?;
    }
    Ok(table)
}

pub fn create_owned_string_array(lua: &Lua, values: &[String]) -> mlua::Result<Table> {
    let table = lua.create_table_with_capacity(values.len(), 0)?;
    for (index, value) in values.iter().enumerate() {
        table.raw_set(index + 1, value.as_str())?;
    }
    Ok(table)
}

pub fn function_named_upvalue_tables(
    getupvalue: &Function,
    function: &Function,
    names: &[&str],
    seen_tables: &mut HashSet<usize>,
) -> Result<Vec<Table>, String> {
    let mut out = Vec::new();
    for index in 1..=function.info().num_upvalues {
        let (name, value): (Value, Value) = getupvalue
            .call((function.clone(), i64::from(index)))
            .map_err(|err| err.to_string())?;
        let Value::String(name) = name else {
            continue;
        };
        let name = name.to_str().map_err(|err| err.to_string())?;
        if !names.iter().any(|candidate| name.as_ref() == *candidate) {
            continue;
        }
        let Value::Table(table) = value else {
            continue;
        };
        if seen_tables.insert(table.to_pointer() as usize) {
            out.push(table);
        }
    }
    Ok(out)
}

pub fn nested_function_named_upvalue_tables(
    getupvalue: &Function,
    function: &Function,
    names: &[&str],
    seen_tables: &mut HashSet<usize>,
    seen_functions: &mut HashSet<usize>,
) -> Result<Vec<Table>, String> {
    let mut out = Vec::new();
    collect_nested_function_named_upvalue_tables(
        getupvalue,
        function,
        names,
        seen_tables,
        seen_functions,
        &mut out,
    )?;
    Ok(out)
}

// Append in depth-first discovery order without allocating and copying a
// separate result vector at every recursive level.
fn collect_nested_function_named_upvalue_tables(
    getupvalue: &Function,
    function: &Function,
    names: &[&str],
    seen_tables: &mut HashSet<usize>,
    seen_functions: &mut HashSet<usize>,
    out: &mut Vec<Table>,
) -> Result<(), String> {
    if !seen_functions.insert(function.to_pointer() as usize) {
        return Ok(());
    }
    for index in 1..=function.info().num_upvalues {
        let (name, value): (Value, Value) = getupvalue
            .call((function, i64::from(index)))
            .map_err(|err| err.to_string())?;
        let matches_name = if let Value::String(name) = &name {
            let name = name.to_str().map_err(|err| err.to_string())?;
            names.iter().any(|candidate| name.as_ref() == *candidate)
        } else {
            false
        };
        match value {
            Value::Table(table)
                if matches_name && seen_tables.insert(table.to_pointer() as usize) =>
            {
                out.push(table);
            }
            Value::Function(child) => collect_nested_function_named_upvalue_tables(
                getupvalue,
                &child,
                names,
                seen_tables,
                seen_functions,
                out,
            )?,
            _ => {}
        }
    }
    Ok(())
}

pub fn read_update_function_tables(
    lua: &Lua,
    root: &Value,
    names: &[&str],
) -> Result<Vec<Table>, String> {
    let Value::Table(root) = root else {
        return Ok(Vec::new());
    };
    let globals = lua.globals();
    let Some(debug) = globals
        .get::<Option<Table>>("debug")
        .map_err(|err| err.to_string())?
    else {
        return Ok(Vec::new());
    };
    let Some(getupvalue) = debug
        .get::<Option<Function>>("getupvalue")
        .map_err(|err| err.to_string())?
    else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    let mut seen_tables = HashSet::new();
    read_update_function_tables_for_table(root, &getupvalue, names, &mut seen_tables, &mut out)?;
    Ok(out)
}

pub fn read_update_function_nested_tables(
    lua: &Lua,
    root: &Value,
    names: &[&str],
) -> Result<Vec<Table>, String> {
    let Value::Table(root) = root else {
        return Ok(Vec::new());
    };
    let globals = lua.globals();
    let Some(debug) = globals
        .get::<Option<Table>>("debug")
        .map_err(|err| err.to_string())?
    else {
        return Ok(Vec::new());
    };
    let Some(getupvalue) = debug
        .get::<Option<Function>>("getupvalue")
        .map_err(|err| err.to_string())?
    else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    let mut seen_tables = HashSet::new();
    let mut seen_functions = HashSet::new();
    read_update_function_nested_tables_for_table(
        root,
        &getupvalue,
        names,
        &mut seen_tables,
        &mut seen_functions,
        &mut out,
    )?;
    Ok(out)
}

pub fn read_global_function_nested_tables(
    lua: &Lua,
    table_name: &str,
    function_names: &[&str],
    names: &[&str],
) -> Result<Vec<Table>, String> {
    let globals = lua.globals();
    let Some(source) = globals
        .get::<Option<Table>>(table_name)
        .map_err(|err| err.to_string())?
    else {
        return Ok(Vec::new());
    };
    let Some(debug) = globals
        .get::<Option<Table>>("debug")
        .map_err(|err| err.to_string())?
    else {
        return Ok(Vec::new());
    };
    let Some(getupvalue) = debug
        .get::<Option<Function>>("getupvalue")
        .map_err(|err| err.to_string())?
    else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    let mut seen_tables = HashSet::new();
    let mut seen_functions = HashSet::new();
    for function_name in function_names {
        let Some(function) = source
            .get::<Option<Function>>(*function_name)
            .map_err(|err| err.to_string())?
        else {
            continue;
        };
        out.extend(nested_function_named_upvalue_tables(
            &getupvalue,
            &function,
            names,
            &mut seen_tables,
            &mut seen_functions,
        )?);
    }
    Ok(out)
}

fn read_update_function_tables_for_table(
    actor: &Table,
    getupvalue: &Function,
    names: &[&str],
    seen_tables: &mut HashSet<usize>,
    out: &mut Vec<Table>,
) -> Result<(), String> {
    if let Some(update) = actor
        .get::<Option<Function>>("__songlua_update_function")
        .map_err(|err| err.to_string())?
    {
        out.extend(function_named_upvalue_tables(
            getupvalue,
            &update,
            names,
            seen_tables,
        )?);
    }
    if let Some(command) = actor
        .get::<Option<String>>("__songlua_recurring_update_command")
        .map_err(|err| err.to_string())?
    {
        if let Some(update) = actor
            .get::<Option<Function>>(command)
            .map_err(|err| err.to_string())?
        {
            out.extend(function_named_upvalue_tables(
                getupvalue,
                &update,
                names,
                seen_tables,
            )?);
        }
    }
    for child in actor.sequence_values::<Value>() {
        let Value::Table(child) = child.map_err(|err| err.to_string())? else {
            continue;
        };
        read_update_function_tables_for_table(&child, getupvalue, names, seen_tables, out)?;
    }
    Ok(())
}

fn read_update_function_nested_tables_for_table(
    actor: &Table,
    getupvalue: &Function,
    names: &[&str],
    seen_tables: &mut HashSet<usize>,
    seen_functions: &mut HashSet<usize>,
    out: &mut Vec<Table>,
) -> Result<(), String> {
    if let Some(update) = actor
        .get::<Option<Function>>("__songlua_update_function")
        .map_err(|err| err.to_string())?
    {
        out.extend(nested_function_named_upvalue_tables(
            getupvalue,
            &update,
            names,
            seen_tables,
            seen_functions,
        )?);
    }
    if let Some(command) = actor
        .get::<Option<String>>("__songlua_recurring_update_command")
        .map_err(|err| err.to_string())?
    {
        if let Some(update) = actor
            .get::<Option<Function>>(command)
            .map_err(|err| err.to_string())?
        {
            out.extend(nested_function_named_upvalue_tables(
                getupvalue,
                &update,
                names,
                seen_tables,
                seen_functions,
            )?);
        }
    }
    for child in actor.sequence_values::<Value>() {
        let Value::Table(child) = child.map_err(|err| err.to_string())? else {
            continue;
        };
        read_update_function_nested_tables_for_table(
            &child,
            getupvalue,
            names,
            seen_tables,
            seen_functions,
            out,
        )?;
    }
    Ok(())
}

pub fn create_bool_array(lua: &Lua, values: &[bool]) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    for (index, value) in values.iter().enumerate() {
        table.raw_set(index + 1, *value)?;
    }
    Ok(table)
}

pub fn create_music_wheel_table(lua: &Lua, current_sort_order: Table) -> mlua::Result<Table> {
    let wheel = lua.create_table()?;
    wheel.set(
        "IsLocked",
        lua.create_function(|_, _args: IgnoredArgs| Ok(false))?,
    )?;
    wheel.set(
        "GetSelectedSection",
        lua.create_function(|lua, _args: IgnoredArgs| Ok(Value::String(lua.create_string("")?)))?,
    )?;
    wheel.set(
        "GetSelectedSong",
        lua.create_function(|_, _args: IgnoredArgs| Ok(Value::Nil))?,
    )?;
    wheel.set(
        "GetSelectedType",
        lua.create_function(|lua, _args: IgnoredArgs| {
            Ok(Value::String(lua.create_string("WheelItemDataType_Song")?))
        })?,
    )?;
    wheel.set(
        "GetSortOrder",
        lua.create_function({
            let current_sort_order = current_sort_order.clone();
            move |lua, _args: IgnoredArgs| {
                let sort = current_sort_order
                    .raw_get::<Option<String>>(1)?
                    .unwrap_or_else(|| "SortOrder_Group".to_string());
                Ok(Value::String(lua.create_string(&sort)?))
            }
        })?,
    )?;
    wheel.set(
        "ChangeSort",
        lua.create_function({
            let wheel = wheel.clone();
            let current_sort_order = current_sort_order;
            move |lua, args: MultiValue| {
                if let Some(sort) = method_arg(&args, 0).cloned().and_then(read_string) {
                    current_sort_order.raw_set(1, sort)?;
                }
                note_song_lua_side_effect(lua)?;
                Ok(wheel.clone())
            }
        })?,
    )?;
    wheel.set(
        "SetOpenSection",
        lua.create_function({
            let wheel = wheel.clone();
            move |lua, _args: IgnoredArgs| {
                note_song_lua_side_effect(lua)?;
                Ok(wheel.clone())
            }
        })?,
    )?;
    wheel.set(
        "Move",
        lua.create_function({
            let wheel = wheel.clone();
            move |lua, _args: IgnoredArgs| {
                note_song_lua_side_effect(lua)?;
                Ok(wheel.clone())
            }
        })?,
    )?;
    Ok(wheel)
}

pub fn create_debug_table(lua: &Lua) -> mlua::Result<Table> {
    let debug = lua.create_table()?;
    debug.set(
        "getinfo",
        lua.create_function(|lua, args: MultiValue| {
            let globals = lua.globals();
            let source = globals
                .get::<Option<String>>("__songlua_current_script_path")?
                .map(|path| format!("@{path}"))
                .unwrap_or_else(|| "=[songlua]".to_string());
            let info = lua.create_table()?;
            info.set("source", source)?;
            info.set("short_src", info.get::<String>("source")?)?;
            info.set("what", "Lua")?;
            info.set("currentline", 0)?;
            info.set("linedefined", 0)?;
            info.set("lastlinedefined", 0)?;
            if args.front().is_some() {
                info.set("namewhat", "")?;
            }
            Ok(info)
        })?,
    )?;
    debug.set(
        "getupvalue",
        lua.create_function(|lua, args: MultiValue| {
            let Some(function) = args.front().cloned().and_then(|value| match value {
                Value::Function(function) => Some(function),
                _ => None,
            }) else {
                return Ok((Value::Nil, Value::Nil));
            };
            let Some(index) = args.get(1).cloned().and_then(|value| match value {
                Value::Integer(value) => Some(value),
                Value::Number(value) => Some(value as i64),
                _ => None,
            }) else {
                return Ok((Value::Nil, Value::Nil));
            };
            read_function_upvalue(lua, function, index)
        })?,
    )?;
    debug.set(
        "traceback",
        lua.create_function(|_, args: MultiValue| {
            let message = args
                .front()
                .cloned()
                .and_then(read_string)
                .unwrap_or_default();
            Ok(message)
        })?,
    )?;
    Ok(debug)
}

#[inline(always)]
pub fn make_color_table(lua: &Lua, rgba: [f32; 4]) -> mlua::Result<Table> {
    let table = lua.create_table_with_capacity(4, 0)?;
    table.raw_set(1, rgba[0])?;
    table.raw_set(2, rgba[1])?;
    table.raw_set(3, rgba[2])?;
    table.raw_set(4, rgba[3])?;
    Ok(table)
}

// SAFETY: index must be a positive index of a rooted table in the locked VM,
// with one spare stack slot. Numeric conversion neither allocates Lua objects
// nor invokes metamethods. Each component is popped before returning.
unsafe fn stack_vector<const N: usize>(state: *mut ffi::lua_State, index: c_int) -> Option<[f32; N]> {
    let mut out = [0.0; N];
    for (component, dest) in out.iter_mut().enumerate() {
        unsafe {
            ffi::lua_rawgeti(state, index, (component + 1) as ffi::lua_Integer);
            let mut numeric = 0;
            let value = ffi::lua_tonumberx(state, -1, &mut numeric);
            ffi::lua_pop(state, 1);
            if numeric == 0 { return None; }
            *dest = value as f32;
        }
    }
    Some(out)
}

pub(crate) fn table_vector<const N: usize>(table: &Table) -> Option<[f32; N]> {
    let lua = table.weak_lua().upgrade();
    // SAFETY: the owning table stays rooted. Three slots match raw_get's
    // original stack reservation and cover the table plus each component.
    unsafe {
        crate::raw_vm::with_read_stack(&lua, 3, |raw, base| {
            raw.push(table)?;
            Ok(stack_vector(raw.state(), base + 1))
        }).ok().flatten()
    }
}

pub fn table_vec2(table: &Table) -> Option<[f32; 2]> { table_vector(table) }
pub fn table_vec3(table: &Table) -> Option<[f32; 3]> { table_vector(table) }
pub fn table_vec4(table: &Table) -> Option<[f32; 4]> { table_vector(table) }
pub fn table_vec5(table: &Table) -> Option<[f32; 5]> { table_vector(table) }

pub fn table_vertex_colors(table: &Table) -> Option<[[f32; 4]; 4]> {
    let lua = table.weak_lua().upgrade();
    // SAFETY: three reserved slots hold the root, current row and component.
    // Every row is type-checked before indexing; only raw reads and numeric
    // conversions run. The shared guard restores the stack on every exit.
    unsafe {
        crate::raw_vm::with_read_stack(&lua, 3, |raw, base| {
            raw.push(table)?;
            let state = raw.state();
            let mut out = [[0.0; 4]; 4];
            for (row, dest) in out.iter_mut().enumerate() {
                ffi::lua_rawgeti(state, base + 1, (row + 1) as ffi::lua_Integer);
                if ffi::lua_type(state, -1) != ffi::LUA_TTABLE { return Ok(None); }
                let Some(value) = stack_vector(state, base + 2) else { return Ok(None); };
                *dest = value;
                ffi::lua_pop(state, 1);
            }
            Ok(Some(out))
        }).ok().flatten()
    }
}

pub fn make_vertex_color_table(lua: &Lua, colors: [[f32; 4]; 4]) -> mlua::Result<Table> {
    let out = lua.create_table_with_capacity(4, 0)?;
    for (index, color) in colors.into_iter().enumerate() {
        out.raw_set(index + 1, make_color_table(lua, color)?)?;
    }
    Ok(out)
}

pub fn create_color_constants_table(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
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
        table.set(
            name,
            make_color_table(lua, parse_color_text(text).unwrap_or([1.0, 1.0, 1.0, 1.0]))?,
        )?;
    }
    table.set(
        "Alpha",
        lua.create_function(|lua, (color, alpha): (Value, f32)| {
            let mut color = read_color_value(color).unwrap_or([1.0, 1.0, 1.0, 1.0]);
            color[3] = alpha;
            make_color_table(lua, color)
        })?,
    )?;
    let table_for_call = table.clone();
    let mt = lua.create_table()?;
    mt.set(
        "__call",
        lua.create_function(move |lua, (_self, value): (Table, Value)| {
            let Some(name) = read_string(value) else {
                return Ok(Value::Nil);
            };
            let existing = table_for_call.get::<Value>(name.as_str())?;
            if !matches!(existing, Value::Nil) {
                return Ok(existing);
            }
            Ok(parse_color_text(&name)
                .map(|color| make_color_table(lua, color).map(Value::Table))
                .transpose()?
                .unwrap_or(Value::Nil))
        })?,
    )?;
    let _ = table.set_metatable(Some(mt));
    Ok(table)
}

#[inline(always)]
fn table_color(table: &Table) -> Option<[f32; 4]> {
    Some([
        table.raw_get::<f32>(1).ok()?,
        table.raw_get::<f32>(2).ok()?,
        table.raw_get::<f32>(3).ok()?,
        table.raw_get::<Option<f32>>(4).ok()?.unwrap_or(1.0),
    ])
}

#[inline(always)]
pub fn read_color_value(value: Value) -> Option<[f32; 4]> {
    match value {
        Value::Table(table) => table_color(&table),
        Value::String(text) => Some(parse_color_text(&text.to_str().ok()?).unwrap_or([1.0; 4])),
        _ => None,
    }
}

pub fn read_vertex_colors_value(value: Value) -> Option<[[f32; 4]; 4]> {
    let Value::Table(table) = value else {
        return None;
    };
    let mut saw_color = false;
    let mut colors = [[1.0, 1.0, 1.0, 1.0]; 4];
    for (index, color) in colors.iter_mut().enumerate() {
        if let Some(value) = table
            .raw_get::<Value>(index + 1)
            .ok()
            .and_then(read_color_value)
        {
            *color = value;
            saw_color = true;
        }
    }
    saw_color.then_some(colors)
}

pub fn read_color_call(args: &MultiValue) -> Option<[f32; 4]> {
    read_color_values(args.iter())
}

fn read_color_values<'a>(mut values: impl Iterator<Item = &'a Value>) -> Option<[f32; 4]> {
    let first = values.next()?;
    match first {
        Value::Table(table) => return table_color(table),
        Value::String(text) => {
            return Some(parse_color_text(&text.to_str().ok()?).unwrap_or([1.0; 4]));
        }
        _ => {}
    }
    let r = read_color_component(first)?;
    let g = read_color_component(values.next()?)?;
    let b = read_color_component(values.next()?)?;
    let a = values.next().and_then(read_color_component).unwrap_or(1.0);
    Some([r, g, b, a])
}

fn read_color_component(value: &Value) -> Option<f32> {
    match value {
        Value::Integer(_) | Value::Number(_) => read_f32(value.clone()),
        Value::String(text) => text.to_str().ok()?.trim().parse::<f32>().ok(),
        _ => None,
    }
}

#[inline(always)]
#[must_use]
pub fn method_arg(args: &MultiValue, index: usize) -> Option<&Value> {
    let offset = method_arg_offset(args);
    args.get(offset + index)
}

#[inline(always)]
#[must_use]
pub fn method_arg_offset(args: &MultiValue) -> usize {
    usize::from(matches!(args.front(), Some(Value::Table(_))))
}

#[cfg(test)]
mod snapshot_tests {
    use super::*;

    fn large_live_actor_snapshot(lua: &Lua, message: bool) -> (Table, Vec<Table>) {
        let root = lua.create_table().unwrap();
        let command = lua.load("return function(self) SEEN[#SEEN + 1] = self.ordinal end")
            .eval::<Function>().unwrap();
        lua.globals().set("SEEN", lua.create_table().unwrap()).unwrap();
        for ordinal in 1..=6000 {
            let actor = lua.create_table().unwrap();
            actor.raw_set("Name", format!("Child{ordinal}")).unwrap();
            actor.raw_set("ordinal", ordinal).unwrap();
            if message { actor.raw_set("GoMessageCommand", command.clone()).unwrap(); }
            root.raw_set(ordinal, actor).unwrap();
        }
        actor_named_children(lua, &root).unwrap();
        let actors = root.sequence_values::<Table>().collect::<mlua::Result<Vec<_>>>().unwrap();
        let registry = lua.create_sequence_from(actors.iter().cloned()).unwrap();
        lua.globals().raw_set("__songlua_actor_registry", registry).unwrap();
        prepare_live_actor_handle_cache(lua, actors.iter().cloned()).unwrap();
        (root, actors)
    }

    #[test]
    fn large_live_actor_snapshot_children_share_retained_handles() {
        let lua = Lua::new();
        let (root, actors) = large_live_actor_snapshot(&lua, false);
        let children = actor_direct_children(&lua, &root)
            .expect("copying native child pointers must not double auxiliary Lua references");
        assert_eq!(children.len(), actors.len());
        for (child, actor) in children.iter().zip(&actors) {
            assert_eq!(child.to_pointer(), actor.to_pointer());
        }
    }

    #[test]
    fn large_live_actor_snapshot_broadcast_preserves_identity_order() {
        let lua = Lua::new();
        let (_root, mut actors) = large_live_actor_snapshot(&lua, true);
        broadcast_song_lua_message(&lua, "Go", None)
            .expect("native subscriber snapshots must reuse retained actor handles");
        actors.sort_unstable_by_key(|actor| actor.to_pointer() as usize);
        let seen: Table = lua.globals().get("SEEN").unwrap();
        assert_eq!(seen.raw_len(), actors.len());
        for (index, actor) in actors.iter().enumerate() {
            assert_eq!(seen.raw_get::<usize>(index + 1).unwrap(), actor.raw_get::<usize>("ordinal").unwrap());
        }
    }

    #[test]
    fn recursive_begin_handles_large_actor_tree_in_order() {
        let lua = Lua::new();
        let root = lua.create_table().expect("actor tree");
        let calls = lua.create_table().expect("Begin observations");
        lua.globals().set("BEGIN_CALLS", calls.clone()).unwrap();
        let begin = lua.load("return function(self) BEGIN_CALLS[#BEGIN_CALLS + 1] = self.ordinal end")
            .eval::<Function>().expect("authored Begin command");
        for ordinal in 1..=2800 {
            let child = lua.create_table().expect("child actor");
            child.raw_set("ordinal", ordinal).unwrap();
            child.raw_set("BeginCommand", begin.clone()).unwrap();
            root.raw_set(ordinal, child).unwrap();
        }
        // Scene preparation keeps initial actor states while Begin traverses
        // the same tree. Those independent handles must remain usable.
        let initial_actors = root.sequence_values::<Table>()
            .collect::<mlua::Result<Vec<_>>>().expect("initial actor states");
        // Named-child queries can initialize every actor's child registry
        // before the screen broadcasts Begin, as authored scene scanners do.
        for actor in &initial_actors {
            let children = actor_named_children(&lua, actor).expect("initial child lookup");
            assert_eq!(children.raw_len(), 0);
        }
        run_named_command_on_children_recursively(&lua, &root, "BeginCommand", None)
            .expect("Begin must traverse a large actor tree within the native Lua stack limit");
        assert_eq!(calls.raw_len(), 2800);
        for (index, actor) in initial_actors.iter().enumerate() {
            let ordinal = index + 1;
            assert_eq!(actor.raw_get::<usize>("ordinal").unwrap(), ordinal);
            assert_eq!(calls.raw_get::<usize>(ordinal).unwrap(), ordinal,
                "Begin runs once per actor in child order");
        }
    }

    #[test]
    fn tap_note_results_do_not_consume_auxiliary_references_per_note() {
        let lua = Lua::new();
        lua.gc_stop();
        let notes = lua.create_table().unwrap();
        let params = lua.create_table().unwrap();
        params.raw_set("TapNoteScore", "TapNoteScore_W1").unwrap();
        for index in 1..=10_000 {
            params.raw_set("TapNoteOffset", index as f32 / 1000.0).unwrap();
            notes.raw_set(index, create_tap_note_table(&lua, &params, None).unwrap()).unwrap();
        }
        for index in [1, 5000, 10_000] {
            let note = notes.raw_get::<Table>(index).unwrap();
            let result = note.get::<Function>("GetTapNoteResult").unwrap().call::<Table>(note.clone()).unwrap();
            let hold = note.get::<Function>("GetHoldNoteResult").unwrap().call::<Table>(note.clone()).unwrap();
            assert_eq!(result.to_pointer(), hold.to_pointer());
            assert_eq!(result.get::<Function>("GetTapNoteOffset").unwrap().call::<f32>(result.clone()).unwrap(), index as f32 / 1000.0);
            assert_eq!(result.get::<Function>("GetTapNoteScore").unwrap().call::<String>(result).unwrap(), "TapNoteScore_W1");
        }
        lua.gc_restart();
    }

    #[test]
    fn large_snapshot_restores_aliases_and_cells() {
        let lua = Lua::new();
        let functions = lua.load(r#"
local graph = {}
for i = 1, 10000 do graph[i] = {value = i} end
local original = graph
local shared = graph[9999]
graph.alias = shared
graph.self = graph
local function mutate()
    for i = 1, 10000 do graph[i].value = -i end
    graph.alias = {}
    graph.self = nil
    shared.extra = 'changed'
    graph = {replacement = true}
end
local function verify()
    assert(graph == original and graph.self == graph)
    assert(graph.alias == shared and graph[9999] == shared)
    assert(shared.extra == nil)
    for i = 1, 10000 do assert(graph[i].value == i) end
end
return {mutate, verify}
"#).eval::<Table>().expect("large aliased graph");
        let mutate = functions.raw_get::<Function>(1).expect("mutation");
        let verify = functions.raw_get::<Function>(2).expect("verification");
        let snapshot = snapshot_function_locals(&lua, &functions, lua.create_table().expect("snapshot rows"))
            .expect("large snapshot must fit the unmodified native Lua stack limit");
        mutate.call::<()>(()).expect("mutate saved locals");
        restore_function_action_tables(&lua, snapshot).expect("restore saved locals");
        verify.call::<()>(()).expect("restore identities, values, aliases and replaced upvalue cells");
    }
}


fn load_native_gameplay_overlay(lua: &Lua, environment: &Table, path: &str, args: MultiValue) -> mlua::Result<Value> {
    let name = path.strip_prefix("./").unwrap_or(path);
    let source = match name {
        "default.lua" => include_str!("../assets/simply-love/BGAnimations/ScreenGameplay overlay/default.lua"),
        "WhoIsCurrentlyWinning.lua" => include_str!("../assets/simply-love/BGAnimations/ScreenGameplay overlay/WhoIsCurrentlyWinning.lua"),
        "FailOnHoldStart.lua" => include_str!("../assets/simply-love/BGAnimations/ScreenGameplay overlay/FailOnHoldStart.lua"),
        "TrackTimeSpentInGameplay.lua" => include_str!("../assets/simply-love/BGAnimations/ScreenGameplay overlay/TrackTimeSpentInGameplay.lua"),
        "JudgmentOffsetTracking.lua" => include_str!("../assets/simply-love/BGAnimations/ScreenGameplay overlay/JudgmentOffsetTracking.lua"),
        "TrackExScoreJudgments.lua" => include_str!("../assets/simply-love/BGAnimations/ScreenGameplay overlay/TrackExScoreJudgments.lua"),
        "TrackFailTime.lua" => include_str!("../assets/simply-love/BGAnimations/ScreenGameplay overlay/TrackFailTime.lua"),
        "PerColumnJudgmentTracking.lua" => include_str!("../assets/simply-love/BGAnimations/ScreenGameplay overlay/PerColumnJudgmentTracking.lua"),
        _ => return Err(mlua::Error::runtime(format!("unknown native Overlay resource {path}"))),
    };
    lua.load(source).set_environment(environment.clone())
        .set_name(format!("@theme:/BGAnimations/ScreenGameplay overlay/{name}"))
        .call(args)
}

pub(crate) fn install_native_gameplay_overlay(
    lua: &Lua, install_methods: Option<fn(&Lua, &Table) -> mlua::Result<()>>,
    context: &SongLuaCompileContext,
) -> mlua::Result<()> {
    let install_methods = install_methods.or_else(|| lua.app_data_ref::<ActorDefinitionHost>()
        .map(|host| host.install_methods))
        .ok_or_else(|| mlua::Error::runtime("Overlay requires its actor host"))?;
    let globals = lua.globals();
    let environment = lua.create_table()?;
    let mt = lua.create_table()?;
    mt.raw_set("__index", globals.clone())?;
    mt.raw_set("__newindex", globals.clone())?;
    environment.set_metatable(Some(mt))?;
    globals.raw_set("__songlua_overlay_environment", environment.clone())?;
    environment.raw_set("LoadActor", lua.create_function(|lua, args: MultiValue| {
        let path = args.front().cloned().and_then(read_string)
            .ok_or_else(|| mlua::Error::runtime("native LoadActor requires a path"))?;
        let environment = lua.globals().raw_get::<Table>("__songlua_overlay_environment")?;
        load_native_gameplay_overlay(lua, &environment, &path,
            MultiValue::from_vec(args.into_iter().skip(1).collect()))
    })?)?;
    let definition = load_native_gameplay_overlay(lua, &environment, "default.lua", MultiValue::new())?;
    let Value::Table(theme) = own_actor_root(lua, definition, install_methods)? else {
        return Err(mlua::Error::runtime("native Overlay must load an actor"));
    };
    let screen = globals.raw_get::<Table>("__songlua_top_screen")?;
    let overlay = actor_children(lua, &screen)?.raw_get::<Table>("Overlay")?;
    #[cfg(feature = "test-support")]
    let captured = context.capture_overlay_probe;
    #[cfg(not(feature = "test-support"))]
    let captured = false;
    if captured {
        theme.raw_set("__songlua_parent", overlay.clone())?;
        push_sequence_child_once(&overlay, theme.clone())?;
        actor_children(lua, &overlay)?.raw_set("", theme.clone())?;
    } else {
        // ScreenWithMenuElements names the loaded base actor Overlay directly.
        // Only the capture tool inserts another ActorFrame around that actor.
        theme.raw_set("__songlua_parent", screen.clone())?;
        set_actor_name(&theme, lua.create_string("Overlay")?)?;
        theme.raw_set("Name", "Overlay")?;
        // Replacing the placeholder must retain its engine-layer identity for
        // explicit Draw and proxies, just as naming the native loaded actor.
        theme.raw_set("__songlua_top_screen_child_name", "Overlay")?;
        let order = overlay.raw_get::<Option<i32>>("__songlua_state_draw_order")?.unwrap_or(0);
        theme.raw_set("__songlua_state_draw_order", order)?;
        for index in 1..=screen.raw_len() {
            if screen.raw_get::<Option<Table>>(index)?.is_some_and(|known| known.to_pointer() == overlay.to_pointer()) {
                screen.raw_set(index, theme.clone())?;
            }
        }
        actor_children(lua, &screen)?.raw_set("Overlay", theme.clone())?;
    }
    native_step_statistics_lookup(lua, &theme)?;
    run_actor_init_commands_for_table(lua, &theme)?;
    native_step_statistics_command(lua, &theme, "BeginCommand")?;
    native_step_statistics_command(lua, &theme, "OnCommand")?;
    // The capture tool wraps the base overlay and appends this real ActorFrame.
    // Reproduce that observable input tree only in diagnostic replay profiles.
    #[cfg(feature = "test-support")]
    if context.capture_overlay_probe {
        let host = lua.app_data_ref::<ActorDefinitionHost>()
            .ok_or_else(|| mlua::Error::runtime("Overlay requires its actor host"))?;
        let probe = create_named_actor(lua, "ActorFrame", "ITGmaniaHarnessTraceProbe", host.create_dummy_actor)?;
        drop(host);
        probe.raw_set("__songlua_parent", overlay.clone())?;
        push_sequence_child_once(&overlay, probe.clone())?;
        actor_children(lua, &overlay)?.raw_set("ITGmaniaHarnessTraceProbe", probe)?;
    }
    #[cfg(not(feature = "test-support"))]
    let _ = context;
    Ok(())
}

pub(crate) fn install_native_gameplay_decorations(
    lua: &Lua, install_methods: Option<fn(&Lua, &Table) -> mlua::Result<()>>,
) -> mlua::Result<()> {
    let install_methods = install_methods.or_else(|| lua.app_data_ref::<ActorDefinitionHost>()
        .map(|host| host.install_methods))
        .ok_or_else(|| mlua::Error::runtime("decorations require their actor host"))?;
    let globals = lua.globals();
    let screen = globals.raw_get::<Table>("__songlua_top_screen")?;
    let screen_name = actor_name(&screen)?;
    let environment = lua.create_table()?;
    let mt = lua.create_table()?;
    mt.raw_set("__index", globals.clone())?;
    environment.set_metatable(Some(mt))?;
    environment.raw_set("ScreenString", lua.create_function(move |lua, key: String|
        lua.create_string(crate::theme_string(&screen_name, &key)))?)?;
    for (name, source) in [
        ("Header", include_str!("../assets/simply-love/Graphics/_header.lua")),
        ("Footer", include_str!("../assets/simply-love/Graphics/_footer.lua")),
    ] {
        let definition = lua.load(source).set_environment(environment.clone())
            .set_name(format!("@theme:/Graphics/_{}.lua", name.to_ascii_lowercase())).eval::<Value>()?;
        let Value::Table(actor) = own_actor_root(lua, definition, install_methods)? else {
            return Err(mlua::Error::runtime("native decoration must load an actor"));
        };
        let children = actor_children(lua, &screen)?;
        let previous = children.raw_get::<Table>(name)?;
        actor.raw_set("__songlua_parent", screen.clone())?;
        native_step_statistics_lookup(lua, &actor)?;
        run_actor_init_commands_for_table(lua, &actor)?;
        // StandardDecorationFromFile merges ScreenGameplay's metric command.
        // Both decorations remain present while hibernating, including children.
        let on = lua.load("return function(self) self:hibernate(math.huge) end")
            .set_name("@theme:/metrics/ScreenGameplay.lua").eval::<Function>()?;
        actor.raw_set("OnCommand", on)?;
        for index in 1..=screen.raw_len() {
            if screen.raw_get::<Option<Table>>(index)?.is_some_and(|known| known.to_pointer() == previous.to_pointer()) {
                screen.raw_set(index, actor.clone())?;
            }
        }
        children.raw_set(name, actor.clone())?;
        native_step_statistics_command(lua, &actor, "BeginCommand")?;
        native_step_statistics_command(lua, &actor, "OnCommand")?;
    }
    // Replacements have now run their real Init commands. BeginScreen's
    // native stable sort sees Footer's90 rather than its placeholder's0.
    let mut ordered = screen.sequence_values::<Table>().collect::<mlua::Result<Vec<_>>>()?;
    ordered.sort_by_key(|actor| actor.raw_get::<i32>("__songlua_state_draw_order").unwrap_or(0));
    for (index, actor) in ordered.into_iter().enumerate() {
        screen.raw_set(index + 1, actor)?;
    }
    Ok(())
}

pub(crate) fn install_native_gameplay_transitions(
    lua: &Lua, install_methods: Option<fn(&Lua, &Table) -> mlua::Result<()>>,
) -> mlua::Result<()> {
    let install_methods = install_methods.or_else(|| lua.app_data_ref::<ActorDefinitionHost>()
        .map(|host| host.install_methods))
        .ok_or_else(|| mlua::Error::runtime("transitions require their actor host"))?;
    let screen = lua.globals().raw_get::<Table>("__songlua_top_screen")?;
    for wrapper in actor_direct_children(lua, &screen)? {
        let Some(role) = wrapper.raw_get::<Option<String>>("__songlua_screen_actor_role")? else { continue; };
        let (source, path) = match role.as_str() {
            "Toasty" => (include_str!("../assets/simply-love/BGAnimations/ScreenGameplay toasty.lua"),
                "@theme:/BGAnimations/ScreenGameplay toasty.lua"),
            "NextSong" => (include_str!("../assets/simply-love/BGAnimations/ScreenGameplay next course song/default.lua"),
                "@theme:/BGAnimations/ScreenGameplay next course song/default.lua"),
            "Ready" | "Go" | "Failed" => ("return Def.Actor{};", "@theme:/BGAnimations/_blank.lua"),
            _ => continue,
        };
        let definition = lua.load(source).set_name(path).eval::<Value>()?;
        let Value::Table(child) = own_actor_root(lua, definition, install_methods)? else {
            return Err(mlua::Error::runtime("native transition must load an actor"));
        };
        // Transition::Load retains exactly one AutoActor and adds it as a child.
        child.raw_set("__songlua_parent", wrapper.clone())?;
        wrapper.raw_set("__songlua_transition_child", child.clone())?;
        push_sequence_child_once(&wrapper, child.clone())?;
        actor_children(lua, &wrapper)?.raw_set("", child.clone())?;
        wrapper.get::<Function>("visible")?.call::<()>((wrapper.clone(), true))?;
        native_step_statistics_lookup(lua, &wrapper)?;
        run_actor_init_commands_for_table(lua, &child)?;
    }
    Ok(())
}

fn load_native_step_statistics(lua: &Lua, environment: &Table, path: &str, args: MultiValue) -> mlua::Result<Value> {
    let name = path.strip_prefix("./").unwrap_or(path);
    let source = match name {
        "default.lua" => include_str!("../assets/simply-love/BGAnimations/ScreenGameplay underlay/PerPlayer/StepStatistics/default.lua"),
        "Banner.lua" => include_str!("../assets/simply-love/BGAnimations/ScreenGameplay underlay/PerPlayer/StepStatistics/Banner.lua"),
        "DarkBackground.lua" => include_str!("../assets/simply-love/BGAnimations/ScreenGameplay underlay/PerPlayer/StepStatistics/DarkBackground.lua"),
        "DensityGraph.lua" => include_str!("../assets/simply-love/BGAnimations/ScreenGameplay underlay/PerPlayer/StepStatistics/DensityGraph.lua"),
        "HoldsMinesRolls.lua" => include_str!("../assets/simply-love/BGAnimations/ScreenGameplay underlay/PerPlayer/StepStatistics/HoldsMinesRolls.lua"),
        "Scorebox.lua" => include_str!("../assets/simply-love/BGAnimations/ScreenGameplay underlay/PerPlayer/StepStatistics/Scorebox.lua"),
        "TapNoteJudgments.lua" => include_str!("../assets/simply-love/BGAnimations/ScreenGameplay underlay/PerPlayer/StepStatistics/TapNoteJudgments.lua"),
        "Time.lua" => include_str!("../assets/simply-love/BGAnimations/ScreenGameplay underlay/PerPlayer/StepStatistics/Time.lua"),
        _ => return Err(mlua::Error::runtime(format!("unknown native StepStatistics resource {path}"))),
    };
    lua.load(source).set_environment(environment.clone())
        .set_name(format!("@theme:/BGAnimations/ScreenGameplay underlay/PerPlayer/StepStatistics/{name}"))
        .call(args)
}

fn native_step_statistics_environment(lua: &Lua) -> mlua::Result<Table> {
    let globals = lua.globals();
    if let Some(environment) = globals.raw_get::<Option<Table>>("__songlua_step_statistics_environment")? {
        return Ok(environment);
    }
    let environment = lua.create_table()?;
    let mt = lua.create_table()?;
    mt.raw_set("__index", globals.clone())?;
    mt.raw_set("__newindex", globals.clone())?;
    environment.set_metatable(Some(mt))?;
    globals.raw_set("__songlua_step_statistics_environment", environment.clone())?;
    environment.raw_set("LoadActor", lua.create_function(|lua, args: MultiValue| {
        let path = args.front().cloned().and_then(read_string)
            .ok_or_else(|| mlua::Error::runtime("native LoadActor requires a path"))?;
        let environment = lua.globals().raw_get::<Table>("__songlua_step_statistics_environment")?;
        load_native_step_statistics(lua, &environment, &path,
            MultiValue::from_vec(args.into_iter().skip(1).collect()))
    })?)?;
    lua.load(include_str!("../assets/simply-love/Scripts/SL-Histogram.lua"))
        .set_environment(environment.clone()).set_name("@theme:/Scripts/SL-Histogram.lua").exec()?;
    Ok(environment)
}

fn native_step_statistics_command(lua: &Lua, actor: &Table, command: &str) -> mlua::Result<()> {
    if let Some(callback) = actor.raw_get::<Option<Function>>(command)? { callback.call::<()>(actor.clone())?; }
    for child in actor_direct_children(lua, actor)? {
        native_step_statistics_command(lua, &child, command)?;
    }
    Ok(())
}

fn native_step_statistics_lookup(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    if actor_type_is(actor, "ActorFrame")? { actor.raw_set("__songlua_owned_child_lookup", true)?; }
    for child in actor_direct_children(lua, actor)? { native_step_statistics_lookup(lua, &child)?; }
    Ok(())
}

fn load_native_note_field(lua: &Lua, name: &str, args: MultiValue) -> mlua::Result<Value> {
    let source = match name {
        "ColumnCues.lua" => include_str!("../assets/simply-love/BGAnimations/ScreenGameplay underlay/PerPlayer/NoteField/ColumnCues.lua"),
        "ColumnFlashOnMiss.lua" => include_str!("../assets/simply-love/BGAnimations/ScreenGameplay underlay/PerPlayer/NoteField/ColumnFlashOnMiss.lua"),
        "default.lua" => include_str!("../assets/simply-love/BGAnimations/ScreenGameplay underlay/PerPlayer/NoteField/default.lua"),
        "DisplayMods.lua" => include_str!("../assets/simply-love/BGAnimations/ScreenGameplay underlay/PerPlayer/NoteField/DisplayMods.lua"),
        "ErrorBar/Colorful.lua" => include_str!("../assets/simply-love/BGAnimations/ScreenGameplay underlay/PerPlayer/NoteField/ErrorBar/Colorful.lua"),
        "ErrorBar/default.lua" => include_str!("../assets/simply-love/BGAnimations/ScreenGameplay underlay/PerPlayer/NoteField/ErrorBar/default.lua"),
        "ErrorBar/Monochrome.lua" => include_str!("../assets/simply-love/BGAnimations/ScreenGameplay underlay/PerPlayer/NoteField/ErrorBar/Monochrome.lua"),
        "ErrorBar/Text.lua" => include_str!("../assets/simply-love/BGAnimations/ScreenGameplay underlay/PerPlayer/NoteField/ErrorBar/Text.lua"),
        "MeasureCounter.lua" => include_str!("../assets/simply-love/BGAnimations/ScreenGameplay underlay/PerPlayer/NoteField/MeasureCounter.lua"),
        "SubtractiveScoring.lua" => include_str!("../assets/simply-love/BGAnimations/ScreenGameplay underlay/PerPlayer/NoteField/SubtractiveScoring.lua"),
        _ => return Err(mlua::Error::runtime(format!("unknown native NoteField actor {name}"))),
    };
    let environment = lua.create_table()?;
    let globals = lua.globals();
    let metatable = lua.create_table()?;
    metatable.raw_set("__index", globals.clone())?;
    metatable.raw_set("__newindex", globals)?;
    environment.set_metatable(Some(metatable))?;
    let directory = name.rsplit_once('/').map_or_else(String::new, |(parent,_)| format!("{parent}/"));
    environment.raw_set("LoadActor", lua.create_function(move |lua, args: MultiValue| {
        let child = args.front().cloned().and_then(read_string)
            .ok_or_else(|| mlua::Error::runtime("native NoteField LoadActor requires a path"))?;
        let path = format!("{}{}", directory, child.trim_start_matches("./"));
        load_native_note_field(lua, &path, MultiValue::from_vec(args.into_iter().skip(1).collect()))
    })?)?;
    lua.load(source).set_environment(environment)
        .set_name(format!("@theme:/BGAnimations/ScreenGameplay underlay/PerPlayer/NoteField/{name}"))
        .call(args)
}

pub(crate) fn install_native_gameplay_underlay(
    lua: &Lua, install_methods: Option<fn(&Lua, &Table) -> mlua::Result<()>>,
) -> mlua::Result<()> {
    // The native theme loads before song Foreground and query observation.
    // SCREENMAN, message services, late helpers and Def must already exist.
    let environment = native_step_statistics_environment(lua)?;
    let install_methods = install_methods.or_else(|| lua.app_data_ref::<ActorDefinitionHost>()
        .map(|host| host.install_methods))
        .ok_or_else(|| mlua::Error::runtime("StepStatistics requires its actor host"))?;
    let screen = lua.globals().raw_get::<Table>("__songlua_top_screen")?;
    let underlay = actor_children(lua, &screen)?.raw_get::<Table>("Underlay")?;
    let game = lua.globals().get::<Table>("GAMESTATE")?;
    let players = game.get::<Function>("GetHumanPlayers")?.call::<Table>(game)?;
    // Native QueueCommand appends a tween; theme Init and song-change queues
    // wait for the screen update along with the later authored startup queues.
    if lua.app_data_ref::<SongLuaStartupQueues>().is_none() {
        lua.set_app_data(SongLuaStartupQueues(Vec::new()));
    }
    let song_output = lua.remove_app_data::<LiveBroadcasts>();
    // These theme-only construction callbacks precede authored song contexts.
    // Keep normal listener dispatch while excluding both song output streams.
    let source_output = lua.remove_app_data::<LiveSourceBroadcasts>();
    let result = (|| -> mlua::Result<()> {
        lua.load(include_str!("../assets/simply-love/Scripts/utf8.lua"))
            .set_name("@theme:/Scripts/utf8.lua").exec()?;
        lua.load(include_str!("../assets/simply-love/Scripts/SL-Helpers-Text.lua"))
            .set_name("@theme:/Scripts/SL-Helpers-Text.lua").exec()?;
        initialize_native_steps_displays(lua, &screen)?;
        // ScreenGameplay::LoadNextSong populates these hidden engine displays
        // from current options before authored foreground construction.
        for player in players.clone().sequence_values::<Value>() {
            let player = player?;
            let Some(index) = player_index_from_value(&player) else { continue; };
            let child = actor_children(lua, &screen)?.raw_get::<Table>(format!("PlayerOptionsP{}", index + 1))?;
            let game = lua.globals().get::<Table>("GAMESTATE")?;
            let state = game.get::<Function>("GetPlayerState")?.call::<Table>((game, player))?;
            let text = state.get::<Function>("GetPlayerOptionsString")?
                .call::<String>((state, "ModsLevel_Current"))?;
            child.raw_set("Text", text)?;
        }
        let mut panes = Vec::new();
        for player in players.sequence_values::<Value>() {
            let player = player?;
            let danger = lua.load(include_str!("../assets/simply-love/BGAnimations/ScreenGameplay underlay/PerPlayer/Danger.lua"))
                .set_name("@theme:/BGAnimations/ScreenGameplay underlay/PerPlayer/Danger.lua")
                .call::<Value>(player.clone())?;
            if !danger.is_nil() {
                let Value::Table(danger) = own_actor_root(lua, danger, install_methods)? else {
                    return Err(mlua::Error::runtime("native Danger must load an actor"));
                };
                danger.raw_set("__songlua_parent", underlay.clone())?;
                run_actor_init_commands_for_table(lua, &danger)?;
                actor_children(lua, &underlay)?.raw_set(actor_name(&danger)?, danger.clone())?;
                push_sequence_child_once(&underlay, danger.clone())?;
                panes.push(danger);
            }
            let definition = load_native_step_statistics(lua, &environment, "default.lua",
                MultiValue::from_vec(vec![player.clone()]))?;
            if !definition.is_nil() {
                let pane = own_actor_root(lua, definition, install_methods)?;
                let Value::Table(pane) = pane else { return Err(mlua::Error::runtime("native StepStatistics must load an actor")); };
                pane.raw_set("__songlua_parent", underlay.clone())?;
                run_actor_init_commands_for_table(lua, &pane)?;
                native_step_statistics_lookup(lua, &pane)?;
                let name = actor_name(&pane)?;
                actor_children(lua, &underlay)?.raw_set(name, pane.clone())?;
                push_sequence_child_once(&underlay, pane.clone())?;
                panes.push(pane);
            }
            let definition = load_native_note_field(lua, "default.lua",
                MultiValue::from_vec(vec![player.clone()]))?;
            let Value::Table(container) = own_actor_root(lua, definition, install_methods)? else {
                return Err(mlua::Error::runtime("native NoteField must load its container actor"));
            };
            container.raw_set("__songlua_parent", underlay.clone())?;
            run_actor_init_commands_for_table(lua, &container)?;
            native_step_statistics_lookup(lua, &container)?;
            actor_children(lua, &underlay)?.raw_set(actor_name(&container)?, container.clone())?;
            push_sequence_child_once(&underlay, container.clone())?;
            panes.push(container);
        }
        let definition = lua.load(include_str!("../assets/simply-love/BGAnimations/ScreenGameplay underlay/Shared/SongInfoBar.lua"))
            .set_name("@theme:/BGAnimations/ScreenGameplay underlay/Shared/SongInfoBar.lua")
            .eval::<Value>()?;
        let Value::Table(song_meter) = own_actor_root(lua, definition, install_methods)? else {
            return Err(mlua::Error::runtime("native SongInfoBar must load an actor"));
        };
        song_meter.raw_set("__songlua_parent", underlay.clone())?;
        run_actor_init_commands_for_table(lua, &song_meter)?;
        native_step_statistics_lookup(lua, &song_meter)?;
        actor_children(lua, &underlay)?.raw_set("SongMeter", song_meter.clone())?;
        push_sequence_child_once(&underlay, song_meter.clone())?;
        panes.push(song_meter);
        // Reproduce default.lua's adoption order so duplicate-name groups are
        // fresh tables constructed from the actual native sequence.
        let mut children = actor_direct_children(lua, &underlay)?;
        children.sort_by_key(|actor| {
            let name = actor_name(actor).unwrap_or_default();
            let role = actor.raw_get::<Option<String>>("__songlua_underlay_role").ok().flatten().unwrap_or(name);
            ["DangerP1","StepStatsPaneP1","filter0","DangerP2","StepStatsPaneP2","filter1",
                "Header","SongMeter","P1Score","difficulty0","life0","NoteFieldContainerP1",
                "P2Score","difficulty1","life1","NoteFieldContainerP2","bpm"]
                .iter().position(|&name| name == role).unwrap_or(99)
        });
        for (index, child) in children.into_iter().enumerate() { underlay.raw_set(index + 1, child)?; }
        underlay.raw_set("__songlua_owned_child_lookup", true)?;
        for pane in &panes { native_step_statistics_command(lua, pane, "BeginCommand")?; }
        for pane in &panes { native_step_statistics_command(lua, pane, "OnCommand")?; }
        // LoadNextSong sets m_pCurSong before authored Foreground is loaded.
        // Theme handlers use real Steps/timing and retain their normal messages.
        broadcast_native_song_lua_message(lua, "CurrentSongChanged", None)?;
        Ok(())
    })();
    if let Some(song_output) = song_output { lua.set_app_data(song_output); }
    if let Some(source_output) = source_output { lua.set_app_data(source_output); }
    result
}

#[cfg(test)]
mod mesh_perf {
    include!("../tests/perf/mesh.rs");
}

#[cfg(test)]
#[path = "../tests/perf/startup_capture.rs"]
mod startup_capture_perf;

#[cfg(test)]
#[path = "../tests/perf/vm_reads_registry.rs"]
mod vm_reads_perf;

#[cfg(test)]
#[path = "../tests/perf/actor_state.rs"]
mod actor_state_perf;

#[cfg(test)]
#[path = "../tests/perf/ignored_actor.rs"]
mod ignored_args_perf;

#[cfg(test)]
#[path = "../tests/perf/prefix_args.rs"]
mod prefix_args_perf;

#[cfg(test)]
#[path = "../tests/perf/metatable_storage.rs"]
mod metatable_storage_perf;
