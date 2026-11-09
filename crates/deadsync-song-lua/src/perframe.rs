pub(crate) use crate::frame_capture::*;
#[cfg(any(test, feature = "legacy-oracle"))]
use mlua::{Function, Lua, Table, Value};
use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::BTreeMap;
use std::rc::Rc;
use std::time::Instant;

use crate::LUA_PLAYERS;
use crate::SONG_LUA_PLAYER_OPTIONS_KEYS;
use crate::SongLuaColumnOffsetBuildParams;
use crate::SongLuaColumnOffsetWindow;
use crate::SongLuaCompileContext;
use crate::SongLuaCompileInfo;
use crate::SongLuaEaseTarget;
use crate::SongLuaEaseWindow;
use crate::SongLuaMessageEvent;
#[cfg(any(test, feature = "legacy-oracle"))]
use crate::SongLuaOverlayCompileActor;
use crate::SongLuaOverlayEase;
use crate::SongLuaOverlayState;
use crate::SongLuaOverlayUpdateTarget;
use crate::SongLuaOverlayUpdateTrack;
use crate::SongLuaOverlayUpdateValue;
use crate::SongLuaSpanMode;
use crate::SongLuaStatefulMessageCapture;
use crate::SongLuaTimeUnit;
#[cfg(any(test, feature = "legacy-oracle"))]
use crate::SongLuaTrackedActor;
use crate::SongLuaTrackedActorTarget;
#[cfg(any(test, feature = "legacy-oracle"))]
use crate::actor_overlay_initial_state;
#[cfg(any(test, feature = "legacy-oracle"))]
use crate::actor_tree_has_update_functions;
use crate::append_column_transform_windows_from_samples;
#[cfg(any(test, feature = "legacy-oracle"))]
use crate::compile_song_runtime_delta_values;
#[cfg(any(test, feature = "legacy-oracle"))]
use crate::compile_song_runtime_values;
use crate::overlay_delta_pair_from_states;
use crate::overlay_state_after_blocks;
use crate::push_unique_compile_detail;
#[cfg(any(test, feature = "legacy-oracle"))]
use crate::read_f32;
#[cfg(any(test, feature = "legacy-oracle"))]
use crate::read_note_column_transform_samples;
#[cfg(any(test, feature = "legacy-oracle"))]
use crate::reset_actor_capture;
#[cfg(any(test, feature = "legacy-oracle"))]
use crate::reset_overlay_compile_actor_capture_tables;
#[cfg(any(test, feature = "legacy-oracle"))]
use crate::reset_tracked_capture_tables;
use crate::runtime_player_option_ease_target;
#[cfg(any(test, feature = "legacy-oracle"))]
use crate::set_actor_overlay_getter_state;
#[cfg(any(test, feature = "legacy-oracle"))]
use crate::set_compile_song_runtime_beat;
#[cfg(any(test, feature = "legacy-oracle"))]
use crate::set_compile_song_runtime_delta_values;
#[cfg(any(test, feature = "legacy-oracle"))]
use crate::set_compile_song_runtime_values;
use crate::song_beat_at_elapsed_seconds;
use crate::song_display_bps;
use crate::song_elapsed_seconds_at;
use crate::song_elapsed_seconds_for_beat;
#[cfg(any(test, feature = "legacy-oracle"))]
use crate::song_lua_side_effect_count;
use crate::song_music_rate;

pub const SONG_LUA_UPDATE_FUNCTION_MAX_SAMPLES: usize = 8192;
pub(crate) const SONG_LUA_UPDATE_REFERENCE_FPS: f32 = 60.0;

pub(crate) fn first_update_seconds(_context: &SongLuaCompileContext) -> f32 {
    #[cfg(feature = "test-support")]
    if let Some(frame) = _context.compile_clock_inputs.get(1) {
        let origin = _context.song_timing.as_ref()
            .map_or(0.0, |timing| f64::from(timing.get_time_for_beat_exact(0.0)));
        return ((frame[0] - origin) / f64::from(song_music_rate(_context))) as f32;
    }
    1.0 / SONG_LUA_UPDATE_REFERENCE_FPS
}

#[cfg(any(test, feature = "legacy-oracle"))]
pub(crate) fn apply_startup_states<Kind>(
    context: &SongLuaCompileContext,
    overlays: &mut [SongLuaOverlayCompileActor<Kind>],
    states: &std::collections::HashMap<usize, crate::lua_util::SongLuaStartupState>,
    messages: &mut Vec<SongLuaMessageEvent>,
) {
    const MESSAGE: &str = "__songlua_queued_startup";
    let beat = song_beat_at_elapsed_seconds(first_update_seconds(context), context);
    let mut changed = false;
    for overlay in overlays.iter_mut() {
        let Some(startup) = states.get(&(overlay.table.to_pointer() as usize)) else {
            continue;
        };
        let Some(command) = startup_command(startup, overlay.actor.initial_state) else {
            continue;
        };
        overlay.actor.initial_state = startup.initial;
        overlay.actor.message_commands.push(command);
        changed = true;
    }
    if changed {
        messages.push(SongLuaMessageEvent {
            beat,
            message: MESSAGE.to_string(),
            persists: true,
        });
    }
}
#[cfg(any(test, feature = "legacy-oracle"))]
pub(crate) fn apply_initial_updates<Kind>(
    overlays: &mut [SongLuaOverlayCompileActor<Kind>],
    states: &std::collections::HashMap<usize, crate::lua_util::SongLuaStartupState>,
) {
    for overlay in overlays {
        let Some(update) = states.get(&(overlay.table.to_pointer() as usize)) else {
            continue;
        };
        // Actor::Update(0) calls UpdateFunction without advancing queued tweens.
        // Its immediate writes already belong to the initial gameplay frame.
        for block in &update.blocks {
            if block.duration == 0.0
                && block.start <= 0.0
                && !block.queued
                && block.progress.is_none()
            {
                crate::apply_overlay_delta(&mut overlay.actor.initial_state, &block.delta);
            }
        }
    }
}

#[cfg(any(test, feature = "legacy-oracle"))]
pub(crate) fn apply_layer_startup(
    actors: &mut [SongLuaTrackedActor],
    states: &std::collections::HashMap<usize, crate::lua_util::SongLuaStartupState>,
    messages: &mut Vec<SongLuaMessageEvent>,
) {
    let mut changed = false;
    for actor in actors
        .iter_mut()
        .filter(|actor| matches!(actor.target, SongLuaTrackedActorTarget::ScreenLayer(_)))
    {
        let Some(startup) = states.get(&(actor.table.to_pointer() as usize)) else {
            continue;
        };
        if startup.blocks.is_empty() {
            continue;
        }
        // Screen children are outside the song actor tree. Their OnCommand
        // tweens start at time zero, while immediate hides already apply there.
        actor.actor.initial_state =
            overlay_state_after_blocks(startup.initial, &startup.blocks, 0.0);
        actor
            .actor
            .message_commands
            .push(crate::SongLuaOverlayMessageCommand {
                frame_advance: 0.0,
                message: "__songlua_screen_startup".to_string(),
                blocks: startup.blocks.clone(),
            });
        changed = true;
    }
    if changed
        && !messages
            .iter()
            .any(|message| message.message == "__songlua_screen_startup")
    {
        messages.push(SongLuaMessageEvent {
            beat: 0.0,
            message: "__songlua_screen_startup".to_string(),
            persists: true,
        });
    }
}

#[cfg(any(test, feature = "legacy-oracle"))]
pub(crate) fn apply_startup_tweens<Kind>(
    overlays: &mut [SongLuaOverlayCompileActor<Kind>],
    states: &std::collections::HashMap<usize, crate::lua_util::SongLuaStartupState>,
    messages: &mut Vec<SongLuaMessageEvent>,
) {
    let mut changed = false;
    for overlay in overlays {
        let Some(startup) = states.get(&(overlay.table.to_pointer() as usize)) else {
            continue;
        };
        if !startup.blocks.iter().any(|block| {
            (block.duration > 0.0
                || block
                    .progress
                    .as_ref()
                    .is_some_and(|samples| samples.first().is_some_and(|sample| sample[0] > 0.0)))
                && block.delta != crate::SongLuaOverlayStateDelta::default()
        }) {
            continue;
        }
        overlay.actor.initial_state =
            overlay_state_after_blocks(startup.initial, &startup.blocks, 0.0);
        overlay
            .actor
            .message_commands
            .push(crate::SongLuaOverlayMessageCommand {
                frame_advance: 0.0,
                message: "__songlua_actor_startup".to_string(),
                blocks: startup.blocks.clone(),
            });
        changed = true;
    }
    if changed {
        messages.push(SongLuaMessageEvent {
            beat: 0.0,
            message: "__songlua_actor_startup".to_string(),
            persists: true,
        });
    }
}

pub(crate) fn startup_command(
    startup: &crate::lua_util::SongLuaStartupState,
    ready: SongLuaOverlayState,
) -> Option<crate::SongLuaOverlayMessageCommand> {
    let mut blocks = startup.blocks.clone();
    if blocks.is_empty() {
        let (_, delta) = overlay_delta_pair_from_states(startup.initial, ready, ready)?;
        blocks.push(crate::SongLuaOverlayCommandBlock {
            progress: None,
            queued: false,
            start: 0.0,
            duration: 0.0,
            easing: None,
            opt1: None,
            opt2: None,
            delta,
        });
    }
    // Native zero-time queues consume the first positive frame's delta.
    for block in &mut blocks {
        if block.queued
            && block.delta.effect_mode.is_some_and(|mode| {
                matches!(
                    mode,
                    deadlib_present::anim::EffectMode::Bob
                        | deadlib_present::anim::EffectMode::Bounce
                        | deadlib_present::anim::EffectMode::Wag
                )
            })
        {
            // Immediate motion macros run after the timer advance on their
            // first positive dispatch frame, including an exact sleep boundary.
            block.start = (block.start * SONG_LUA_UPDATE_REFERENCE_FPS + 0.000_01).floor()
                / SONG_LUA_UPDATE_REFERENCE_FPS;
        } else {
            block.start -= 1.0 / SONG_LUA_UPDATE_REFERENCE_FPS;
        }
    }
    Some(crate::SongLuaOverlayMessageCommand {
        frame_advance: 0.0,
        message: "__songlua_queued_startup".to_string(),
        blocks,
    })
}

pub(crate) const PLAYER_TRANSFORM_CAPTURE_KEYS: [&str; 11] = [
    "x",
    "y",
    "z",
    "rot_x_deg",
    "rot_z_deg",
    "rot_y_deg",
    "zoom_x",
    "zoom_y",
    "zoom_z",
    "skew_x",
    "skew_y",
];

#[cfg(any(test, feature = "legacy-oracle"))]
pub struct SongLuaPerframeEntry {
    pub start: f32,
    pub end: f32,
    pub function: Function,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SongLuaPerframeSample {
    pub beat: f32,
    pub eval_beat: f32,
    pub delta_beats: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SongLuaPerframePlayerState {
    pub x: Option<f32>,
    pub y: Option<f32>,
    pub z: Option<f32>,
    pub rotation_x: Option<f32>,
    pub rotation_z: Option<f32>,
    pub rotation_y: Option<f32>,
    pub zoom_x: Option<f32>,
    pub zoom_y: Option<f32>,
    pub zoom_z: Option<f32>,
    pub skew_x: Option<f32>,
    pub skew_y: Option<f32>,
}

pub type SongLuaUpdateModState = BTreeMap<String, f32>;

pub(crate) trait ModState {
    fn entries(&self) -> impl Iterator<Item = (&str, &f32)>;
    fn get(&self, key: &str) -> Option<&f32>;
    fn is_empty(&self) -> bool;
}

impl ModState for SongLuaUpdateModState {
    fn entries(&self) -> impl Iterator<Item = (&str, &f32)> {
        self.iter().map(|(key, value)| (key.as_str(), value))
    }

    fn get(&self, key: &str) -> Option<&f32> {
        BTreeMap::get(self, key)
    }

    fn is_empty(&self) -> bool {
        BTreeMap::is_empty(self)
    }
}

// Immutable frame snapshots share names for the lifetime of one compilation.
// Sorted, compact storage preserves BTreeMap traversal and lookup semantics.
#[derive(Clone, Default)]
pub(crate) struct ModSnapshot(Box<[(Rc<str>, f32)]>);

impl ModState for ModSnapshot {
    fn entries(&self) -> impl Iterator<Item = (&str, &f32)> {
        self.0.iter().map(|(key, value)| (key.as_ref(), value))
    }

    fn get(&self, key: &str) -> Option<&f32> {
        self.0
            .binary_search_by(|(name, _)| name.as_ref().cmp(key))
            .ok()
            .map(|index| &self.0[index].1)
    }

    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[cfg(any(test, feature = "legacy-oracle"))]
#[derive(Default)]
pub(crate) struct ModSnapshotScratch {
    names: FxHashSet<Rc<str>>,
    lua_names: FxHashMap<usize, (mlua::LuaString, Rc<str>)>,
    entries: Vec<(Rc<str>, f32)>,
}

#[cfg(any(test, feature = "legacy-oracle"))]
fn snapshot_name_error(error: mlua::Error) -> mlua::Error {
    // Borrow the name on success, but retain String's conversion diagnostics.
    match error {
        mlua::Error::FromLuaConversionError { from, to, message } if to == "string" => {
            mlua::Error::FromLuaConversionError {
                from,
                to: "String".into(),
                message,
            }
        }
        error => error,
    }
}

#[cfg(any(test, feature = "legacy-oracle"))]
struct SnapshotName(mlua::LuaString);

#[cfg(any(test, feature = "legacy-oracle"))]
impl mlua::FromLua for SnapshotName {
    fn from_lua(value: Value, lua: &Lua) -> mlua::Result<Self> {
        let name = mlua::LuaString::from_lua(value, lua).map_err(snapshot_name_error)?;
        Ok(Self(name))
    }
}

#[cfg(any(test, feature = "legacy-oracle"))]
impl ModSnapshotScratch {
    fn lua_name(&mut self, name: SnapshotName) -> mlua::Result<Rc<str>> {
        let pointer = name.0.to_pointer() as usize;
        if let Some((_, key)) = self.lua_names.get(&pointer) {
            return Ok(Rc::clone(key));
        }
        let text = name.0.to_str()?;
        let key = if let Some(key) = self.names.get(text.as_ref()) {
            Rc::clone(key)
        } else {
            let key: Rc<str> = Rc::from(text.as_ref());
            self.names.insert(Rc::clone(&key));
            key
        };
        drop(text);
        // Retain the Lua string so GC cannot recycle its pointer for a new name.
        self.lua_names.insert(pointer, (name.0, Rc::clone(&key)));
        Ok(key)
    }

    fn insert(&mut self, key: &str, value: f32) {
        let name = if let Some(name) = self.names.get(key) {
            Rc::clone(name)
        } else {
            let name: Rc<str> = Rc::from(key);
            self.names.insert(Rc::clone(&name));
            name
        };
        self.entries.push((name, value));
    }

    fn finish(&mut self) -> ModSnapshot {
        self.entries.sort_by(|a, b| a.0.cmp(&b.0));
        // Lua numeric keys can coerce to the same name as string keys.
        // Keep the last iteration value, just as BTreeMap::insert did.
        self.entries.dedup_by(|next, previous| {
            if next.0 == previous.0 {
                std::mem::swap(next, previous);
                true
            } else {
                false
            }
        });
        ModSnapshot(self.entries.as_slice().into())
    }

    fn override_value(&mut self, key: &str, value: f32) {
        if let Some((_, previous)) = self
            .entries
            .iter_mut()
            .rev()
            .find(|(name, _)| &**name == key)
        {
            *previous = value;
        } else {
            self.insert(key, value);
        }
    }

    fn sample(&mut self, table: &Table) -> Result<ModSnapshot, String> {
        self.entries.clear();
        if let Some(state) = table
            .raw_get::<Option<Table>>("__songlua_player_option_state")
            .map_err(|err| err.to_string())?
        {
            state
                .for_each::<SnapshotName, Value>(|key, value| {
                    let key = self.lua_name(key)?;
                    let value = match value {
                        Value::Boolean(value) => f32::from(value),
                        value => match read_f32(value) {
                            Some(value) => value,
                            None => return Ok(()),
                        },
                    };
                    self.entries.push((key, value));
                    Ok(())
                })
                .map_err(|err| err.to_string())?;
        }
        let active_speed = table
            .raw_get::<Option<SnapshotName>>("__songlua_speedmod_active")
            .map_err(|err| err.to_string())?;
        let active_speed = active_speed
            .map(|name| self.lua_name(name))
            .transpose()
            .map_err(|err| {
                // raw_get<String> uses String's stack conversion diagnostics.
                let err = match err {
                    mlua::Error::FromLuaConversionError { from, message, .. } => {
                        mlua::Error::FromLuaConversionError {
                            from,
                            to: "String".into(),
                            message,
                        }
                    }
                    err => err,
                };
                err.to_string()
            })?;
        for (key, state_key) in [
            ("xmod", "__songlua_speedmod_xmod"),
            ("cmod", "__songlua_speedmod_cmod"),
            ("mmod", "__songlua_speedmod_mmod"),
        ] {
            if active_speed.as_deref().is_some_and(|active| active != key) {
                continue;
            }
            if let Some(value) = table
                .get::<Option<f32>>(state_key)
                .map_err(|err| err.to_string())?
            {
                self.override_value(key, value);
            }
        }
        Ok(self.finish())
    }

    fn speeds(&mut self, lua: &Lua, table: &Table) -> Result<ModSnapshot, String> {
        self.entries.clear();
        if let Some(table) = table
            .raw_get::<Option<Table>>("__songlua_player_option_speeds")
            .map_err(|err| err.to_string())?
        {
            // Convert the key before the speed, matching pairs<String, f32>.
            table
                .for_each::<SnapshotName, Value>(|key, value| {
                    let key = self.lua_name(key)?;
                    let value = <f32 as mlua::FromLua>::from_lua(value, lua)?;
                    self.entries.push((key, value));
                    Ok(())
                })
                .map_err(|err| err.to_string())?;
        }
        Ok(self.finish())
    }

    pub(crate) fn states(
        &mut self,
        tables: &[Table; LUA_PLAYERS],
    ) -> Result<[ModSnapshot; LUA_PLAYERS], String> {
        Ok([self.sample(&tables[0])?, self.sample(&tables[1])?])
    }

    pub(crate) fn player_speeds(
        &mut self,
        lua: &Lua,
        tables: &[Table; LUA_PLAYERS],
    ) -> Result<[ModSnapshot; LUA_PLAYERS], String> {
        Ok([self.speeds(lua, &tables[0])?, self.speeds(lua, &tables[1])?])
    }
}

fn frame_buffer<T>(first: T, samples: usize) -> Vec<T> {
    let mut buffer = Vec::with_capacity(samples);
    buffer.push(first);
    buffer
}

#[cfg(any(test, feature = "legacy-oracle"))]
pub fn read_perframe_entries(table: Option<Table>) -> Result<Vec<SongLuaPerframeEntry>, String> {
    let Some(table) = table else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for value in table.sequence_values::<Value>() {
        let Value::Table(entry) = value.map_err(|err| err.to_string())? else {
            continue;
        };
        let Some(start) = read_f32(entry.raw_get::<Value>(1).map_err(|err| err.to_string())?)
        else {
            continue;
        };
        let Some(end) = read_f32(entry.raw_get::<Value>(2).map_err(|err| err.to_string())?) else {
            continue;
        };
        let Value::Function(function) = entry.raw_get::<Value>(3).map_err(|err| err.to_string())?
        else {
            continue;
        };
        if !start.is_finite() || !end.is_finite() || end <= start {
            continue;
        }
        out.push(SongLuaPerframeEntry {
            start,
            end,
            function,
        });
    }
    Ok(out)
}

#[cfg(any(test, feature = "legacy-oracle"))]
pub fn perframe_boundaries(entries: &[SongLuaPerframeEntry]) -> Vec<f32> {
    let mut boundaries = entries
        .iter()
        .flat_map(|entry| [entry.start, entry.end])
        .filter(|value| value.is_finite())
        .collect::<Vec<_>>();
    boundaries.sort_by(f32::total_cmp);
    boundaries.dedup_by(|left, right| (*left - *right).abs() <= f32::EPSILON);
    boundaries
}

#[cfg(any(test, feature = "legacy-oracle"))]
pub fn actor_perframe_player_state(actor: &Table) -> Result<SongLuaPerframePlayerState, String> {
    let zoom = actor
        .get::<Option<f32>>("__songlua_state_zoom")
        .map_err(|err| err.to_string())?;
    Ok(SongLuaPerframePlayerState {
        x: actor
            .get::<Option<f32>>("__songlua_state_x")
            .map_err(|err| err.to_string())?,
        y: actor
            .get::<Option<f32>>("__songlua_state_y")
            .map_err(|err| err.to_string())?,
        z: actor
            .get::<Option<f32>>("__songlua_state_z")
            .map_err(|err| err.to_string())?,
        rotation_x: actor
            .get::<Option<f32>>("__songlua_state_rot_x_deg")
            .map_err(|err| err.to_string())?,
        rotation_z: actor
            .get::<Option<f32>>("__songlua_state_rot_z_deg")
            .map_err(|err| err.to_string())?,
        rotation_y: actor
            .get::<Option<f32>>("__songlua_state_rot_y_deg")
            .map_err(|err| err.to_string())?,
        zoom_x: actor
            .get::<Option<f32>>("__songlua_state_zoom_x")
            .map_err(|err| err.to_string())?
            .or(zoom),
        zoom_y: actor
            .get::<Option<f32>>("__songlua_state_zoom_y")
            .map_err(|err| err.to_string())?
            .or(zoom),
        zoom_z: actor
            .get::<Option<f32>>("__songlua_state_zoom_z")
            .map_err(|err| err.to_string())?
            .or(zoom),
        skew_x: actor
            .get::<Option<f32>>("__songlua_state_skew_x")
            .map_err(|err| err.to_string())?,
        skew_y: actor
            .get::<Option<f32>>("__songlua_state_skew_y")
            .map_err(|err| err.to_string())?,
    })
}

#[cfg(any(test, feature = "legacy-oracle"))]
pub fn current_perframe_player_states(
    lua: &Lua,
    player_tables: &[Option<Table>; LUA_PLAYERS],
) -> Result<[SongLuaPerframePlayerState; LUA_PLAYERS], String> {
    let mut out = [SongLuaPerframePlayerState::default(); LUA_PLAYERS];
    for player in 0..LUA_PLAYERS {
        let Some(actor) = player_tables[player].as_ref() else {
            continue;
        };
        out[player] = actor_perframe_player_state(actor)?;
        let mut state = SongLuaOverlayState::default();
        if crate::lua_util::replay_tween_pose(lua, actor, &mut state) {
            sample_player_pose(&mut out[player], &state, None);
        }
    }
    Ok(out)
}

pub(crate) fn sample_player_pose(
    output: &mut SongLuaPerframePlayerState,
    state: &SongLuaOverlayState,
    mask: Option<u16>,
) {
    macro_rules! sample {
        ($bit:literal, $out:ident, $field:ident) => {
            if mask.map_or(output.$out.is_some(), |mask| mask & (1 << $bit) != 0) {
                output.$out = Some(state.$field);
            }
        };
    }
    sample!(0, x, x);
    sample!(1, y, y);
    sample!(2, z, z);
    sample!(3, rotation_x, rot_x_deg);
    sample!(4, rotation_z, rot_z_deg);
    sample!(5, rotation_y, rot_y_deg);
    sample!(6, zoom_x, zoom_x);
    sample!(7, zoom_y, zoom_y);
    sample!(8, zoom_z, zoom_z);
    sample!(9, skew_x, skew_x);
    sample!(10, skew_y, skew_y);
}

#[cfg(any(test, feature = "legacy-oracle"))]
fn capture_transform_mask(block: &Table) -> Result<u16, String> {
    let mut mask = 0;
    for (index, key) in PLAYER_TRANSFORM_CAPTURE_KEYS.iter().enumerate() {
        if !matches!(
            block
                .raw_get::<Value>(*key)
                .map_err(|err| err.to_string())?,
            Value::Nil
        ) {
            mask |= 1 << index;
        }
    }
    Ok(mask)
}

#[cfg(any(test, feature = "legacy-oracle"))]
fn actor_transform_mask(lua: &Lua, actor: &Table) -> Result<u16, String> {
    let mut mask = 0;
    if let Some(block) = actor
        .get::<Option<Table>>("__songlua_capture_block")
        .map_err(|err| err.to_string())?
    {
        mask |= capture_transform_mask(&block)?;
    }
    if let Some(blocks) = actor
        .get::<Option<Table>>("__songlua_capture_blocks")
        .map_err(|err| err.to_string())?
    {
        for block in blocks.sequence_values::<Table>() {
            mask |= capture_transform_mask(&block.map_err(|err| err.to_string())?)?;
        }
    }
    let captured = crate::lua_util::captured_update_target_mask(lua, actor);
    for (index, target) in PLAYER_TRANSFORM_TARGETS.into_iter().enumerate() {
        if captured & (1_u128 << target as usize) != 0 {
            mask |= 1 << index;
        }
    }
    Ok(mask)
}

#[cfg(any(test, feature = "legacy-oracle"))]
fn player_transform_masks(
    lua: &Lua,
    player_tables: &[Option<Table>; LUA_PLAYERS],
) -> Result<[u16; LUA_PLAYERS], String> {
    let mut masks = [0; LUA_PLAYERS];
    for (player, actor) in player_tables.iter().enumerate() {
        if let Some(actor) = actor {
            masks[player] = actor_transform_mask(lua, actor)?;
        }
    }
    Ok(masks)
}

#[cfg(any(test, feature = "legacy-oracle"))]
pub fn tracked_player_tables(
    tracked_actors: &[SongLuaTrackedActor],
) -> [Option<Table>; LUA_PLAYERS] {
    let mut out = std::array::from_fn(|_| None);
    for tracked in tracked_actors {
        if let SongLuaTrackedActorTarget::Player(player) = tracked.target {
            out[player] = Some(tracked.table.clone());
        }
    }
    out
}

#[cfg(any(test, feature = "legacy-oracle"))]
pub fn update_player_option_tables(lua: &Lua) -> Result<[Table; LUA_PLAYERS], String> {
    let globals = lua.globals();
    Ok([
        globals
            .get::<Table>(SONG_LUA_PLAYER_OPTIONS_KEYS[0])
            .map_err(|err| err.to_string())?,
        globals
            .get::<Table>(SONG_LUA_PLAYER_OPTIONS_KEYS[1])
            .map_err(|err| err.to_string())?,
    ])
}

#[cfg(any(test, feature = "legacy-oracle"))]
pub fn player_option_sample(table: &Table) -> Result<SongLuaUpdateModState, String> {
    let mut out = SongLuaUpdateModState::new();
    if let Some(state) = table
        .raw_get::<Option<Table>>("__songlua_player_option_state")
        .map_err(|err| err.to_string())?
    {
        for pair in state.pairs::<String, Value>() {
            let (key, value) = pair.map_err(|err| err.to_string())?;
            let value = match value {
                Value::Boolean(value) => f32::from(value),
                value => match read_f32(value) {
                    Some(value) => value,
                    None => continue,
                },
            };
            out.insert(key, value);
        }
    }
    let active_speed = table
        .raw_get::<Option<String>>("__songlua_speedmod_active")
        .map_err(|err| err.to_string())?;
    for (key, state_key) in [
        ("xmod", "__songlua_speedmod_xmod"),
        ("cmod", "__songlua_speedmod_cmod"),
        ("mmod", "__songlua_speedmod_mmod"),
    ] {
        // The host retains previous values, but only the last selected
        // speed mode may drive playback.
        if active_speed.as_deref().is_some_and(|active| active != key) {
            continue;
        }
        if let Some(value) = table
            .get::<Option<f32>>(state_key)
            .map_err(|err| err.to_string())?
        {
            out.insert(key.to_string(), value);
        }
    }
    Ok(out)
}

#[cfg(any(test, feature = "legacy-oracle"))]
pub fn current_update_mod_states(
    tables: &[Table; LUA_PLAYERS],
) -> Result<[SongLuaUpdateModState; LUA_PLAYERS], String> {
    Ok([
        player_option_sample(&tables[0])?,
        player_option_sample(&tables[1])?,
    ])
}

#[cfg(any(test, feature = "legacy-oracle"))]
pub fn active_perframe_entries(
    entries: &[SongLuaPerframeEntry],
    start: f32,
    end: f32,
) -> Vec<&SongLuaPerframeEntry> {
    let mid = 0.5f32.mul_add(end - start, start);
    entries
        .iter()
        .filter(|entry| mid > entry.start && mid < entry.end)
        .collect()
}

#[inline(always)]
#[must_use]
pub fn perframe_segment_step(len: f32) -> f32 {
    (len / 96.0).clamp(1.0 / 192.0, 0.125)
}

#[inline(always)]
#[must_use]
pub fn perframe_delta_seconds(context: &SongLuaCompileContext, delta_beats: f32) -> f32 {
    song_elapsed_seconds_for_beat(
        delta_beats,
        song_display_bps(context),
        song_music_rate(context),
    )
}

#[inline(always)]
fn beat_span_seconds(context: &SongLuaCompileContext, start: f32, end: f32) -> f32 {
    (song_elapsed_seconds_at(end, context) - song_elapsed_seconds_at(start, context)).max(0.0)
}

#[inline(always)]
#[must_use]
pub fn relative_player_target(value: Option<f32>, baseline: Option<f32>) -> Option<f32> {
    value.map(|value| value - baseline.unwrap_or(0.0))
}

#[cfg(any(test, feature = "legacy-oracle"))]
pub fn call_perframe_entry(
    lua: &Lua,
    entry: &SongLuaPerframeEntry,
    beat: f32,
    delta_beats: f32,
    delta_seconds: f32,
) -> Result<bool, String> {
    let previous = compile_song_runtime_values(lua).map_err(|err| err.to_string())?;
    let previous_delta = compile_song_runtime_delta_values(lua).map_err(|err| err.to_string())?;
    let side_effect_before = song_lua_side_effect_count(lua).map_err(|err| err.to_string())?;
    set_compile_song_runtime_beat(lua, beat).map_err(|err| err.to_string())?;
    set_compile_song_runtime_delta_values(lua, delta_beats, delta_seconds)
        .map_err(|err| err.to_string())?;
    let result = entry
        .function
        .call::<Value>((beat, delta_seconds))
        .map(|_| ())
        .map_err(|err| err.to_string());
    set_compile_song_runtime_values(lua, previous.0, previous.1).map_err(|err| err.to_string())?;
    set_compile_song_runtime_delta_values(lua, previous_delta.0, previous_delta.1)
        .map_err(|err| err.to_string())?;
    let saw_side_effect =
        song_lua_side_effect_count(lua).map_err(|err| err.to_string())? > side_effect_before;
    result?;
    Ok(saw_side_effect)
}

#[must_use]
pub fn update_function_end_beat(context: &SongLuaCompileContext) -> f32 {
    let seconds = context.music_length_seconds.max(0.0);
    let seconds = if context.song_timing.is_some() {
        seconds / song_music_rate(context)
    } else {
        seconds
    };
    song_beat_at_elapsed_seconds(seconds, context).max(0.0)
}

#[must_use]
pub fn update_function_sample_step(len: f32) -> f32 {
    if len <= 0.0 {
        return 0.0;
    }
    (len / SONG_LUA_UPDATE_FUNCTION_MAX_SAMPLES as f32).max(1.0 / 192.0)
}

pub(crate) fn update_function_replay_beats(
    context: &SongLuaCompileContext,
    start: f32,
    end: f32,
) -> Vec<(f64, f64)> {
    #[cfg(feature = "test-support")]
    if !context.compile_clock_inputs.is_empty() {
        let origin = context.song_timing.as_ref()
            .map_or(0.0, |timing| f64::from(timing.get_time_for_beat_exact(0.0)));
        let rate = f64::from(song_music_rate(context));
        let mut previous = 0.0;
        return context.compile_clock_inputs.iter().map(|&[music, beat, _]| {
            let seconds = (music - origin) / rate;
            let delta = seconds - previous;
            previous = seconds;
            (beat, delta)
        }).collect();
    }
    let start_seconds = f64::from(song_elapsed_seconds_at(start, context));
    let end_seconds = f64::from(if end == update_function_end_beat(context) {
        // Keep the supplied horizon. Converting its rounded beat back to
        // seconds can truncate the final frame and skip boundary callbacks.
        context.music_length_seconds / song_music_rate(context)
    } else {
        song_elapsed_seconds_at(end, context)
    });
    let frame_count = ((end_seconds - start_seconds) * f64::from(SONG_LUA_UPDATE_REFERENCE_FPS))
        .ceil()
        .max(0.0) as usize;
    // Empty timing maps have no prefix to reuse. Keep their original build
    // path: pre-sizing this case regressed allocator-sensitive benchmarks.
    if context.song_timing_bpms.is_empty() || context.song_timing.is_some() {
        let mut out = vec![(f64::from(start), 0.0)];
        let mut previous_seconds = start_seconds;
        for frame in 1..=frame_count {
            let seconds = (start_seconds + frame as f64 / f64::from(SONG_LUA_UPDATE_REFERENCE_FPS))
                .min(end_seconds);
            out.push((
                crate::song_beat_at_seconds64(seconds, context),
                seconds - previous_seconds,
            ));
            previous_seconds = seconds;
        }
        return out;
    }
    let mut out = Vec::with_capacity(frame_count.saturating_add(1));
    out.push((f64::from(start), 0.0));
    if frame_count == 0 {
        return out;
    }
    let mut previous_seconds = start_seconds;
    let rate = f64::from(song_music_rate(context));
    let mut cursor_beat = 0.0;
    let mut cursor_seconds = 0.0;
    let mut bpm = context
        .song_timing_bpms
        .first()
        .filter(|(beat, bpm)| *beat <= 0.0 && *bpm > 0.0)
        .map_or_else(
            || {
                f64::from(context.song_display_bpms[0].max(context.song_display_bpms[1]))
                    .max(f64::from(f32::EPSILON) * 60.0)
            },
            |segment| f64::from(segment.1),
        );
    let mut segments = context.song_timing_bpms.as_slice();
    let ordered = rate.is_finite()
        && rate > 0.0
        && bpm.is_finite()
        && segments
            .iter()
            .all(|(beat, bpm)| beat.is_finite() && bpm.is_finite() && *bpm > 0.0)
        && segments.is_sorted_by(|left, right| left.0 <= right.0);
    // Each timestamp uses the original frame formula. Only the immutable BPM
    // prefix is retained; accumulated delta rounding cannot move a boundary.
    for index in 0..frame_count {
        let frame = index + 1;
        let seconds = (start_seconds + frame as f64 / f64::from(SONG_LUA_UPDATE_REFERENCE_FPS))
            .min(end_seconds);
        let beat = if ordered {
            let target = seconds * rate;
            while let Some((&(segment_beat, segment_bpm), rest)) = segments.split_first() {
                let segment_beat = f64::from(segment_beat);
                let next_seconds = cursor_seconds + (segment_beat - cursor_beat) * 60.0 / bpm;
                if next_seconds > target {
                    break;
                }
                cursor_beat = segment_beat;
                cursor_seconds = next_seconds;
                bpm = f64::from(segment_bpm).max(f64::EPSILON);
                segments = rest;
            }
            cursor_beat + (target - cursor_seconds) * bpm / 60.0
        } else {
            // Public compile contexts can contain unordered or invalid timing.
            // Preserve their existing conversion semantics.
            crate::song_beat_at_seconds64(seconds, context)
        };
        let delta = seconds - previous_seconds;
        previous_seconds = seconds;
        out.push((beat, delta));
    }
    out
}

pub(crate) fn reference_actor_frames(_context: &SongLuaCompileContext) -> Option<Vec<[f32; 2]>> {
    #[cfg(feature = "test-support")]
    if !_context.compile_clock_inputs.is_empty() {
        let rate = f64::from(song_music_rate(_context));
        return Some(_context.compile_clock_inputs.iter().enumerate().map(|(index, &[music, _, delta])| {
            let span = if index == 0 { 0.0 } else {
                (music - _context.compile_clock_inputs[index - 1][0]) / rate
            };
            [delta as f32, span as f32]
        }).collect());
    }
    None
}

#[must_use]
pub fn update_function_samples(start: f32, end: f32) -> Vec<SongLuaPerframeSample> {
    let step = update_function_sample_step(end - start);
    let mut out = Vec::new();
    let mut beat = (start + step).min(end);
    let mut prev_eval = Some(start);

    loop {
        let eval_beat = beat;
        let delta_beats = prev_eval
            .map(|prev| (eval_beat - prev).abs())
            .unwrap_or(0.0);
        out.push(SongLuaPerframeSample {
            beat,
            eval_beat,
            delta_beats,
        });
        prev_eval = Some(eval_beat);
        if beat >= end - f32::EPSILON {
            break;
        }
        beat = (beat + step).min(end);
    }
    out
}

#[must_use]
pub fn perframe_samples(start: f32, end: f32) -> Vec<SongLuaPerframeSample> {
    perframe_sample_iter(start, end).collect()
}

pub(crate) fn perframe_sample_iter(
    start: f32,
    end: f32,
) -> impl Iterator<Item = SongLuaPerframeSample> {
    let step = perframe_segment_step(end - start);
    let eps = (0.5 * step).min(0.25 * (end - start)).max(1.0e-4_f32);
    let mut beat = start;
    let mut prev_eval = None::<f32>;
    let mut done = false;
    std::iter::from_fn(move || {
        if done {
            return None;
        }
        let eval_beat = if beat <= start + f32::EPSILON {
            (start + eps).min(end - eps)
        } else if beat >= end - f32::EPSILON {
            (end - eps).max(start + eps)
        } else {
            beat
        };
        let delta_beats = prev_eval
            .map(|prev| (eval_beat - prev).abs())
            .unwrap_or(0.0);
        let sample = SongLuaPerframeSample {
            beat,
            eval_beat,
            delta_beats,
        };
        prev_eval = Some(eval_beat);
        if beat >= end - f32::EPSILON {
            done = true;
        } else {
            beat = (beat + step).min(end);
            if beat > end {
                beat = end;
            }
        }
        Some(sample)
    })
}

#[cfg(any(test, feature = "legacy-oracle"))]
pub fn unsupported_perframe_info(entries: &[SongLuaPerframeEntry]) -> SongLuaCompileInfo {
    let mut info = SongLuaCompileInfo {
        unsupported_perframes: entries.len(),
        ..SongLuaCompileInfo::default()
    };
    for entry in entries {
        push_unique_compile_detail(
            &mut info.unsupported_perframe_captures,
            format!("perframe start={:.3} end={:.3}", entry.start, entry.end),
        );
    }
    info
}

pub fn push_perframe_overlay_targets(
    out: &mut Vec<SongLuaOverlayEase>,
    start: f32,
    end: f32,
    from_overlays: &[SongLuaOverlayState],
    to_overlays: &[SongLuaOverlayState],
    baseline_overlays: &[SongLuaOverlayState],
    skip_unchanged: bool,
) {
    for overlay_index in 0..from_overlays.len().min(to_overlays.len()) {
        if skip_unchanged && from_overlays[overlay_index] == to_overlays[overlay_index] {
            continue;
        }
        let Some((from, to)) = overlay_delta_pair_from_states(
            baseline_overlays[overlay_index],
            from_overlays[overlay_index],
            to_overlays[overlay_index],
        ) else {
            continue;
        };
        out.push(SongLuaOverlayEase {
            overlay_index,
            unit: SongLuaTimeUnit::Beat,
            start,
            limit: end - start,
            span_mode: SongLuaSpanMode::Len,
            from,
            to,
            easing: Some("linear".to_string()),
            sustain: None,
            opt1: None,
            opt2: None,
        });
    }
}

pub fn push_perframe_player_target(
    out: &mut Vec<SongLuaEaseWindow>,
    start: f32,
    end: f32,
    from: Option<f32>,
    to: Option<f32>,
    baseline: Option<f32>,
    neutral: f32,
    target: SongLuaEaseTarget,
    player: usize,
) {
    if end <= start {
        return;
    }
    let baseline = baseline.unwrap_or(neutral);
    let from = from.unwrap_or(baseline);
    let to = to.unwrap_or(baseline);
    if !from.is_finite() || !to.is_finite() {
        return;
    }
    if (from - baseline).abs() <= f32::EPSILON && (to - baseline).abs() <= f32::EPSILON {
        return;
    }
    out.push(SongLuaEaseWindow {
        approach_speed: None,
        unit: SongLuaTimeUnit::Beat,
        start,
        limit: end - start,
        span_mode: SongLuaSpanMode::Len,
        from,
        to,
        target,
        easing: Some("linear".to_string()),
        player: Some((player + 1) as u8),
        sustain: None,
        opt1: None,
        opt2: None,
    });
}

pub fn push_perframe_player_targets(
    out: &mut Vec<SongLuaEaseWindow>,
    start: f32,
    end: f32,
    from_players: &[SongLuaPerframePlayerState; LUA_PLAYERS],
    to_players: &[SongLuaPerframePlayerState; LUA_PLAYERS],
    baseline_players: &[SongLuaPerframePlayerState; LUA_PLAYERS],
) {
    for player in 0..LUA_PLAYERS {
        push_perframe_player_target(
            out,
            start,
            end,
            from_players[player].x,
            to_players[player].x,
            baseline_players[player].x,
            0.0,
            SongLuaEaseTarget::PlayerX,
            player,
        );
        push_perframe_player_target(
            out,
            start,
            end,
            from_players[player].y,
            to_players[player].y,
            baseline_players[player].y,
            0.0,
            SongLuaEaseTarget::PlayerY,
            player,
        );
        push_perframe_player_target(
            out,
            start,
            end,
            relative_player_target(from_players[player].z, baseline_players[player].z),
            relative_player_target(to_players[player].z, baseline_players[player].z),
            Some(0.0),
            0.0,
            SongLuaEaseTarget::PlayerZ,
            player,
        );
        push_perframe_player_target(
            out,
            start,
            end,
            from_players[player].rotation_x,
            to_players[player].rotation_x,
            baseline_players[player].rotation_x,
            0.0,
            SongLuaEaseTarget::PlayerRotationX,
            player,
        );
        push_perframe_player_target(
            out,
            start,
            end,
            from_players[player].rotation_z,
            to_players[player].rotation_z,
            baseline_players[player].rotation_z,
            0.0,
            SongLuaEaseTarget::PlayerRotationZ,
            player,
        );
        push_perframe_player_target(
            out,
            start,
            end,
            from_players[player].rotation_y,
            to_players[player].rotation_y,
            baseline_players[player].rotation_y,
            0.0,
            SongLuaEaseTarget::PlayerRotationY,
            player,
        );
        push_perframe_player_target(
            out,
            start,
            end,
            from_players[player].zoom_x,
            to_players[player].zoom_x,
            baseline_players[player].zoom_x,
            1.0,
            SongLuaEaseTarget::PlayerZoomX,
            player,
        );
        push_perframe_player_target(
            out,
            start,
            end,
            from_players[player].zoom_y,
            to_players[player].zoom_y,
            baseline_players[player].zoom_y,
            1.0,
            SongLuaEaseTarget::PlayerZoomY,
            player,
        );
        push_perframe_player_target(
            out,
            start,
            end,
            from_players[player].zoom_z,
            to_players[player].zoom_z,
            baseline_players[player].zoom_z,
            1.0,
            SongLuaEaseTarget::PlayerZoomZ,
            player,
        );
        push_perframe_player_target(
            out,
            start,
            end,
            from_players[player].skew_x,
            to_players[player].skew_x,
            baseline_players[player].skew_x,
            0.0,
            SongLuaEaseTarget::PlayerSkewX,
            player,
        );
        push_perframe_player_target(
            out,
            start,
            end,
            from_players[player].skew_y,
            to_players[player].skew_y,
            baseline_players[player].skew_y,
            0.0,
            SongLuaEaseTarget::PlayerSkewY,
            player,
        );
    }
}

#[inline(always)]
fn update_mod_runtime_value(key: &str, value: f32) -> f32 {
    if matches!(key, "xmod" | "cmod" | "mmod") {
        value
    } else {
        value * 100.0
    }
}

pub fn push_update_mod_targets(
    out: &mut Vec<SongLuaEaseWindow>,
    start: f32,
    end: f32,
    from_players: &[SongLuaUpdateModState; LUA_PLAYERS],
    to_players: &[SongLuaUpdateModState; LUA_PLAYERS],
    baseline_players: &[SongLuaUpdateModState; LUA_PLAYERS],
    speeds: &[SongLuaUpdateModState; LUA_PLAYERS],
    last_windows: &mut BTreeMap<(usize, String), usize>,
) {
    push_update_mod_targets_with_key(
        out,
        start,
        end,
        from_players,
        to_players,
        baseline_players,
        speeds,
        None,
        last_windows,
        &mut (0, String::new()),
        SongLuaTimeUnit::Beat,
    );
}

pub(crate) fn push_update_mod_targets_with_key<S: ModState>(
    out: &mut Vec<SongLuaEaseWindow>,
    start: f32,
    end: f32,
    from_players: &[S; LUA_PLAYERS],
    to_players: &[S; LUA_PLAYERS],
    baseline_players: &[S; LUA_PLAYERS],
    speeds: &[S; LUA_PLAYERS],
    next_speeds: Option<&[S; LUA_PLAYERS]>,
    last_windows: &mut BTreeMap<(usize, String), usize>,
    lookup_key: &mut (usize, String),
    unit: SongLuaTimeUnit,
) {
    if from_players.iter().all(ModState::is_empty) {
        return;
    }
    for player in 0..LUA_PLAYERS {
        for (key, &from) in from_players[player].entries() {
            // Classify without allocating the name of a coalesced speed target.
            let Some(mut target) = runtime_player_option_ease_target(key, "") else {
                continue;
            };
            let speed = speeds[player].get(key).copied();
            // The first native setter supplies its approach speed at the next
            // sample. Until then this is still the baseline, not a ramp toward
            // a future write (which would move Mini before the paired XMod).
            if speed.is_none() && next_speeds.is_some_and(|next| next[player].get(key).is_some()) {
                continue;
            }
            // These switches have no native approach speed, but initial values and
            // later writes are still step targets, never interpolated samples.
            if speed.is_some()
                || matches!(
                    key,
                    "cosecant" | "dizzyholds" | "stealthtype" | "zbuffer" | "modtimersetting" | "failsetting"
                )
            {
                if !from.is_finite() || speed.is_some_and(|speed| !speed.is_finite()) {
                    continue;
                }
                let value = update_mod_runtime_value(key, from);
                // Reuse this lookup buffer across keys and compiler samples.
                // Only newly indexed windows need an owned cache key.
                lookup_key.0 = player;
                lookup_key.1.clear();
                lookup_key.1.push_str(key);
                if let Some(index) = last_windows.get_mut(lookup_key) {
                    if let Some(window) = out.get_mut(*index)
                        && window.to == value
                        && window.approach_speed == speed
                    {
                        window.limit = end - window.start;
                        continue;
                    }
                    *index = out.len();
                } else {
                    last_windows.insert(lookup_key.clone(), out.len());
                }
                if let SongLuaEaseTarget::Mod(name) = &mut target {
                    *name = key.to_owned();
                }
                // Current approaches float targets at the authored speed;
                // switches change immediately. Never tween toward a future write.
                out.push(SongLuaEaseWindow {
                    approach_speed: speed,
                    unit,
                    start,
                    limit: end - start,
                    span_mode: SongLuaSpanMode::Len,
                    from: value,
                    to: value,
                    target,
                    easing: None,
                    player: Some(player as u8 + 1),
                    sustain: None,
                    opt1: None,
                    opt2: None,
                });
                continue;
            }
            let baseline = baseline_players[player].get(key).copied().unwrap_or(0.0);
            let to = to_players[player].get(key).copied().unwrap_or(baseline);
            if let SongLuaEaseTarget::Mod(name) = &mut target {
                *name = key.to_owned();
            }
            let first = out.len();
            push_perframe_player_target(
                out,
                start,
                end,
                Some(update_mod_runtime_value(key, from)),
                Some(update_mod_runtime_value(key, to)),
                Some(update_mod_runtime_value(key, baseline)),
                0.0,
                target,
                player,
            );
            for window in &mut out[first..] {
                window.unit = unit;
            }
        }
    }
}

pub fn push_perframe_static_targets(
    out_eases: &mut Vec<SongLuaEaseWindow>,
    out_overlay_eases: &mut Vec<SongLuaOverlayEase>,
    start: f32,
    end: f32,
    current_players: &[SongLuaPerframePlayerState; LUA_PLAYERS],
    current_overlays: &[SongLuaOverlayState],
    baseline_players: &[SongLuaPerframePlayerState; LUA_PLAYERS],
    baseline_overlays: &[SongLuaOverlayState],
) {
    push_perframe_player_targets(
        out_eases,
        start,
        end,
        current_players,
        current_players,
        baseline_players,
    );
    push_perframe_overlay_targets(
        out_overlay_eases,
        start,
        end,
        current_overlays,
        current_overlays,
        baseline_overlays,
        false,
    );
}

pub fn push_sampled_perframe_targets(
    out_eases: &mut Vec<SongLuaEaseWindow>,
    out_overlay_eases: &mut Vec<SongLuaOverlayEase>,
    end: f32,
    sample_beats: &[f32],
    player_samples: &[[SongLuaPerframePlayerState; LUA_PLAYERS]],
    overlay_samples: &[Vec<SongLuaOverlayState>],
    baseline_players: &[SongLuaPerframePlayerState; LUA_PLAYERS],
    baseline_overlays: &[SongLuaOverlayState],
) {
    for index in 0..sample_beats.len() {
        let seg_start = sample_beats[index];
        let seg_end = sample_beats.get(index + 1).copied().unwrap_or(end);
        if seg_end <= seg_start {
            continue;
        }
        let from_players = player_samples[index];
        let to_players = player_samples
            .get(index + 1)
            .copied()
            .unwrap_or(from_players);
        push_perframe_player_targets(
            out_eases,
            seg_start,
            seg_end,
            &from_players,
            &to_players,
            baseline_players,
        );
        let from_overlays = &overlay_samples[index];
        let to_overlays = overlay_samples.get(index + 1).unwrap_or(from_overlays);
        push_perframe_overlay_targets(
            out_overlay_eases,
            seg_start,
            seg_end,
            from_overlays,
            to_overlays,
            baseline_overlays,
            false,
        );
    }
}

#[cfg(any(test, feature = "legacy-oracle"))]
pub fn current_overlay_compile_actor_states<Kind>(
    overlays: &[SongLuaOverlayCompileActor<Kind>],
) -> Result<Vec<SongLuaOverlayState>, String> {
    let mut out = Vec::with_capacity(overlays.len());
    for overlay in overlays {
        out.push(actor_overlay_initial_state(&overlay.table)?);
    }
    Ok(out)
}

#[cfg(any(test, feature = "legacy-oracle"))]
fn capture_update_overlay_samples<Actor: std::borrow::Borrow<Table>>(
    lua: &Lua,
    context: &SongLuaCompileContext,
    overlays: &[Actor],
    baseline: &[SongLuaOverlayState],
    from_states: &[SongLuaOverlayState],
    update_states: &mut [SongLuaOverlayState],
    to_states: &mut [SongLuaOverlayState],
    restored_indices: &[usize],
    tracks: &mut Vec<SongLuaOverlayUpdateTrack>,
    track_indices: &mut std::collections::HashMap<
        (usize, crate::SongLuaOverlayUpdateTarget),
        usize,
        impl std::hash::BuildHasher,
    >,
    time: f32,
    next_time: f32,
    next_seconds: f64,
    scheduled_samples: &mut Vec<SongLuaScheduledOverlaySample>,
    scratch: &mut OverlaySampleScratch,
) -> Result<(), String> {
    let mut source = NativeFrameCapture { lua, overlays };
    crate::frame_capture::capture_update_overlay_samples(
        &mut source,
        crate::frame_capture::CaptureFrameArgs {
            context,
            baseline,
            from_states,
            update_states,
            to_states,
            restored_indices,
            tracks,
            track_indices,
            time,
            next_time,
            next_seconds,
            scheduled_samples,
            scratch,
        },
    )
}

#[cfg(any(test, feature = "legacy-oracle"))]
pub(crate) struct NativeFrameCapture<'a, Actor> {
    pub(crate) lua: &'a Lua,
    pub(crate) overlays: &'a [Actor],
}
#[cfg(any(test, feature = "legacy-oracle"))]
impl<Actor: std::borrow::Borrow<Table>> crate::frame_capture::CaptureSource
    for NativeFrameCapture<'_, Actor>
{
    fn actor_count(&self) -> usize {
        self.overlays.len()
    }
    fn chronological(&self) -> bool {
        self.lua
            .app_data_ref::<crate::lua_util::SongLuaCompileFrames>()
            .is_some()
    }
    fn drain(
        &mut self,
        visit: &mut crate::frame_capture::CaptureVisitor<'_>,
    ) -> Result<(), String> {
        let overlays = self.overlays;
        crate::lua_util::drain_overlay_update_capture(
            self.lua,
            |index, values, scheduled, final_values, reset| {
                let mut player = || {
                    let actor = overlays[index].borrow();
                    Ok(actor
                        .raw_get::<Option<i64>>("__songlua_player_index")
                        .map_err(|e| e.to_string())?
                        .is_some()
                        && actor
                            .raw_get::<Option<String>>("__songlua_player_child_name")
                            .map_err(|e| e.to_string())?
                            .is_none())
                };
                visit(index, values, scheduled, final_values, reset, &mut player)
            },
        )
    }
    fn reset(&mut self, index: usize) -> Result<(), String> {
        reset_actor_capture(self.lua, self.overlays[index].borrow()).map_err(|e| e.to_string())
    }
}

#[cfg(any(test, feature = "legacy-oracle"))]
fn apply_scheduled_overlay_states<Actor: std::borrow::Borrow<Table>>(
    lua: &Lua,
    overlays: &[Actor],
    states: &mut [SongLuaOverlayState],
    scheduled: &[SongLuaScheduledOverlaySample],
    seconds: f64,
) -> Result<(), String> {
    crate::frame_capture::apply_scheduled_overlay_states(
        states,
        scheduled,
        seconds,
        |index, target, value| {
            if let Some(actor) = overlays.get(index) {
                crate::lua_util::set_actor_overlay_update_getter_value(
                    lua,
                    actor.borrow(),
                    target,
                    value,
                )?;
            }
            Ok(())
        },
    )
}

#[cfg(any(test, feature = "legacy-oracle"))]
pub fn call_update_functions_at(
    lua: &Lua,
    root: &Value,
    beat: f64,
    seconds: f64,
    delta_beats: f32,
    delta_seconds: f64,
) -> Result<(), String> {
    let previous = compile_song_runtime_values(lua).map_err(|err| err.to_string())?;
    let previous_delta = compile_song_runtime_delta_values(lua).map_err(|err| err.to_string())?;
    set_compile_song_runtime_beat(lua, beat as f32).map_err(|err| err.to_string())?;
    set_compile_song_runtime_delta_values(lua, delta_beats, delta_seconds as f32)
        .map_err(|err| err.to_string())?;
    let runtime = lua
        .globals()
        .get::<Table>(crate::SONG_LUA_RUNTIME_KEY)
        .map_err(|err| err.to_string())?;
    runtime
        .set(crate::SONG_LUA_RUNTIME_BEAT_KEY, beat)
        .map_err(|err| err.to_string())?;
    runtime
        .set(crate::SONG_LUA_RUNTIME_SECONDS_KEY, seconds)
        .map_err(|err| err.to_string())?;
    if let Some(clock) = lua.app_data_ref::<crate::runtime::SongLuaClock>() {
        let rate = runtime
            .get::<f32>(crate::SONG_LUA_RUNTIME_RATE_KEY)
            .map_err(|err| err.to_string())?;
        let position = clock.0.get_song_position(
            (seconds * f64::from(rate)) as f32 + clock.0.get_time_for_beat_exact(0.0),
        );
        runtime
            .set(crate::SONG_LUA_RUNTIME_BPS_KEY, position.bpm / 60.0)
            .map_err(|err| err.to_string())?;
        runtime
            .set("__songlua_freeze", position.is_in_freeze)
            .map_err(|err| err.to_string())?;
        runtime
            .set("__songlua_delay", position.is_in_delay)
            .map_err(|err| err.to_string())?;
    }
    crate::song_tables::advance_option_levels(lua, seconds).map_err(|err| err.to_string())?;
    let result =
        crate::lua_util::run_actor_compile_update_functions_with_delta(lua, root, delta_seconds)
            .and_then(|()| {
                if let Value::Table(root) = root {
                    crate::lua_util::run_actor_draw_functions_for_table(lua, root)?;
                }
                Ok(())
            })
            .map_err(|err| err.to_string());
    set_compile_song_runtime_values(lua, previous.0, previous.1).map_err(|err| err.to_string())?;
    set_compile_song_runtime_delta_values(lua, previous_delta.0, previous_delta.1)
        .map_err(|err| err.to_string())?;
    result
}

#[derive(Default)]
struct ColumnSplineLane {
    frames: Vec<deadsync_gameplay::SongLuaColumnSplineFrame>,
    // NaN coefficients are deliberately non-reflexive under PartialEq.
    // Inspect a solved buffer only on its first identity comparison.
    // Geometry that changes every sample never needs this scan.
    reflexive: [Option<bool>; 2],
}

fn captured_spline_matches(
    previous: &Option<deadsync_gameplay::SongLuaSplineData>,
    next: &Option<deadsync_gameplay::SongLuaSplineData>,
    reflexive: &mut Option<bool>,
) -> bool {
    match (previous, next) {
        (Some(previous), Some(next)) => {
            if previous.constant != next.constant
                || previous.beats_per_t != next.beats_per_t
                || previous.receptor_t != next.receptor_t
                || previous.subtract_song_beat != next.subtract_song_beat
            {
                return false;
            }
            if std::sync::Arc::ptr_eq(&previous.coefficients, &next.coefficients) {
                *reflexive.get_or_insert_with(|| {
                    previous
                        .coefficients
                        .iter()
                        .flatten()
                        .flatten()
                        .all(|value| !value.is_nan())
                })
            } else {
                previous.coefficients == next.coefficients
            }
        }
        (None, None) => true,
        _ => false,
    }
}

fn captured_spline_reflexive(
    next: &Option<deadsync_gameplay::SongLuaSplineData>,
    previous: Option<&Option<deadsync_gameplay::SongLuaSplineData>>,
    previous_reflexive: Option<bool>,
) -> Option<bool> {
    if previous
        .and_then(Option::as_ref)
        .zip(next.as_ref())
        .is_some_and(|(previous, next)| {
            std::sync::Arc::ptr_eq(&previous.coefficients, &next.coefficients)
        })
    {
        previous_reflexive
    } else {
        None
    }
}

#[derive(Default)]
/// Compile-lifetime reader/solver buffers and one sample buffer for the lanes.
/// Equal geometry shares immutable coefficients after validating Lua reads.
/// One recent buffer is retained; there is no global cache or eviction policy.
pub(crate) struct ColumnSplineCapture {
    points: crate::lua_util::ColumnSplineReadScratch,
    sampled: Vec<(usize, usize, deadsync_gameplay::SongLuaColumnSplineFrame)>,
    lanes: BTreeMap<(usize, usize), ColumnSplineLane>,
    bytes: usize,
}

#[cfg(any(test, feature = "legacy-oracle"))]
impl ColumnSplineCapture {
    pub(crate) fn capture(&mut self, lua: &Lua, second: f32) -> Result<(), String> {
        crate::lua_util::read_column_position_splines(
            lua,
            |player, column| {
                self.lanes
                    .get(&(player, column))
                    .and_then(|lane| lane.frames.last())
            },
            &mut self.points,
            &mut self.sampled,
        )?;
        for (player, column, mut frame) in self.sampled.drain(..) {
            let lane = self.lanes.entry((player, column)).or_default();
            let last = lane.frames.last();
            if last.is_some_and(|last| {
                captured_spline_matches(&last.position, &frame.position, &mut lane.reflexive[0])
                    && captured_spline_matches(&last.zoom, &frame.zoom, &mut lane.reflexive[1])
            }) {
                continue;
            }
            self.bytes += [&frame.position, &frame.zoom]
                .into_iter()
                .flatten()
                .map(|spline| std::mem::size_of_val(spline.coefficients.as_ref()))
                .sum::<usize>();
            if self.bytes > 128 * 1024 * 1024 {
                return Err("Position spline tracks exceed 128 MiB per layer".into());
            }
            lane.reflexive = [
                captured_spline_reflexive(
                    &frame.position,
                    last.map(|last| &last.position),
                    lane.reflexive[0],
                ),
                captured_spline_reflexive(
                    &frame.zoom,
                    last.map(|last| &last.zoom),
                    lane.reflexive[1],
                ),
            ];
            frame.second = second;
            lane.frames.push(frame);
        }
        Ok(())
    }

    pub(crate) fn finish(self, out: &mut Vec<deadsync_gameplay::SongLuaColumnSplineTrack>) {
        log::debug!(
            "Compiled Position splines: lanes={} frames={} coefficients_bytes={}",
            self.lanes.len(),
            self.lanes
                .values()
                .map(|lane| lane.frames.len())
                .sum::<usize>(),
            self.bytes
        );
        out.extend(
            self.lanes
                .into_iter()
                .filter_map(|((player, column), lane)| {
                    let frames = lane.frames;
                    frames
                        .iter()
                        .any(|frame| frame.position.is_some() || frame.zoom.is_some())
                        .then(|| deadsync_gameplay::SongLuaColumnSplineTrack {
                            player,
                            column,
                            time_offset: 0.0,
                            frames: frames.into(),
                        })
                }),
        );
    }
}

#[cfg(any(test, feature = "legacy-oracle"))]
fn append_wrapper_overlays<Slot, Vertex, Attribute>(
    overlays: &mut Vec<
        SongLuaOverlayCompileActor<crate::SongLuaOverlayKind<Slot, Vertex, Attribute>>,
    >,
    wrappers: Vec<(usize, Table, SongLuaOverlayState)>,
) -> Result<(), String> {
    let mut indices: FxHashMap<_, _> = overlays
        .iter()
        .enumerate()
        .map(|(index, overlay)| (overlay.table.to_pointer() as usize, index))
        .collect();
    for (owner, table, initial_state) in wrappers {
        let Some(&owner_index) = indices.get(&owner) else {
            return Err("Runtime wrapper owner has no compiled overlay".into());
        };
        // New states are outside all earlier states owned by this actor.
        // Stop at its normal parent, which can have wrappers of its own.
        let mut inner = owner_index;
        while let Some(parent) = overlays[inner].actor.parent_index {
            if !matches!(
                overlays[parent].actor.kind,
                crate::SongLuaOverlayKind::WrapperState
            ) || overlays[parent]
                .table
                .raw_get::<Option<Table>>("__songlua_parent")
                .map_err(|err| err.to_string())?
                .is_none_or(|actor| actor.to_pointer() as usize != owner)
            {
                break;
            }
            inner = parent;
        }
        let parent_index = overlays[inner].actor.parent_index;
        let index = overlays.len();
        overlays[inner].actor.parent_index = Some(index);
        indices.insert(table.to_pointer() as usize, index);
        overlays.push(SongLuaOverlayCompileActor {
            actor: crate::SongLuaOverlayActor {
                kind: crate::SongLuaOverlayKind::WrapperState,
                name: None,
                parent_index,
                initial_state,
                message_commands: Vec::new(),
            },
            table,
            message_sounds: Vec::new(),
        });
    }
    Ok(())
}

// Worker-owned song capture, capped across both players. Sparse frames keep
// the last unchanged sample before each change so interpolation cannot start
// a wrapper animation early. Playback only borrows the immutable frames.
#[cfg(any(test, feature = "legacy-oracle"))]
pub(crate) fn capture_field_wrappers(
    lua: &Lua,
    tracked: &mut [SongLuaTrackedActor],
    prior_second: f32,
    clock: [f32; 2],
    bytes: &mut usize,
) -> Result<(), String> {
    for player in tracked.iter_mut().filter(|actor| {
        matches!(actor.target, SongLuaTrackedActorTarget::Player(_))
    }) {
        let children = crate::lua_util::actor_children(lua, &player.table)
            .map_err(|err| err.to_string())?;
        let Some(field) = children.raw_get::<Option<Table>>("NoteField")
            .map_err(|err| err.to_string())? else { continue; };
        let wrappers = field.raw_get::<Option<Table>>("__songlua_wrappers")
            .map_err(|err| err.to_string())?;
        let mut states = Vec::new();
        if let Some(wrappers) = wrappers {
            for index in (1..=wrappers.raw_len()).rev() {
                let wrapper = wrappers.raw_get::<Table>(index).map_err(|err| err.to_string())?;
                crate::lua_util::ensure_tween_replay(lua, &wrapper).map_err(|err| err.to_string())?;
                let mut state = actor_overlay_initial_state(&wrapper)?;
                crate::lua_util::replay_tween_pose(lua, &wrapper, &mut state);
                if let Some(rotation) = crate::lua_util::spin_render_pose(&wrapper)
                    .map_err(|err| err.to_string())? {
                    [state.rot_x_deg, state.rot_y_deg, state.rot_z_deg] = rotation;
                    state.spin_baked = true;
                }
                if let Some(effect) = crate::lua_util::effect_render_time(&wrapper, clock)
                    .map_err(|err| err.to_string())? {
                    state.effect_time = Some(effect);
                }
                states.push(state);
            }
        }
        let frames = &mut player.actor.note_field_frames;
        if states.is_empty() && frames.is_empty() { continue; }
        if frames.last().is_some_and(|frame| frame.wrappers.as_ref() == states) {
            continue;
        }
        let additional = states.len() * std::mem::size_of::<SongLuaOverlayState>()
            + 2 * std::mem::size_of::<crate::SongLuaNoteFieldFrame>();
        *bytes = bytes.saturating_add(additional);
        if *bytes > 256 * 1024 * 1024 {
            return Err("NoteField wrapper capture exceeds 256 MiB".into());
        }
        if let Some(previous) = frames.last().filter(|frame| frame.second < prior_second) {
            frames.push(crate::SongLuaNoteFieldFrame {
                second: prior_second, wrappers: previous.wrappers.clone(),
            });
        }
        frames.push(crate::SongLuaNoteFieldFrame { second: clock[0], wrappers: states.into() });
    }
    Ok(())
}

#[cfg(any(test, feature = "legacy-oracle"))]
pub fn compile_update_functions<Slot, Vertex, Attribute>(
    lua: &Lua,
    root: &Value,
    context: &SongLuaCompileContext,
    overlays: &mut Vec<SongLuaOverlayCompileActor<
        crate::SongLuaOverlayKind<Slot, Vertex, Attribute>,
    >>,
    tracked_actors: &mut [SongLuaTrackedActor],
    messages: &[SongLuaMessageEvent],
    startup_tweens: &crate::lua_util::SongLuaStartupStates,
    sound_events: &mut Vec<crate::SongLuaSoundEvent>,
    judgment_textures: &mut Vec<(usize, crate::SongLuaJudgmentTexture)>,
    sprite_textures: &mut Vec<(usize, crate::SongLuaSpriteTexture)>,
    column_splines: &mut Vec<deadsync_gameplay::SongLuaColumnSplineTrack>,
) -> Result<
    (
        Vec<SongLuaEaseWindow>,
        Vec<SongLuaOverlayEase>,
        Vec<SongLuaOverlayUpdateTrack>,
        Vec<SongLuaColumnOffsetWindow>,
        Vec<SongLuaStatefulMessageCapture>,
        Vec<(f32, String, bool)>,
    ),
    String,
> {
    let profile = std::env::var_os("DEADSYNC_SONG_LUA_TIMING_STDERR").is_some();
    let mut reset_ms = 0.0;
    let mut message_ms = 0.0;
    let mut update_ms = 0.0;
    let mut player_ms = 0.0;
    let mut mod_ms = 0.0;
    let mut column_ms = 0.0;
    let mut overlay_ms = 0.0;
    let mut spline_capture = ColumnSplineCapture::default();
    spline_capture.capture(lua, 0.0)?;
    let start = song_beat_at_elapsed_seconds(0.0, context);
    #[cfg(feature = "test-support")]
    let start = context.compile_clock_inputs.first().map_or(start, |frame| frame[1] as f32);
    let end = update_function_end_beat(context);
    if (!actor_tree_has_update_functions(lua, root).map_err(|err| err.to_string())?
        && !crate::model_texture::active(lua)) || end <= start
    {
        // Native Song options persist even when the foreground never updates.
        // Bake the startup targets using the same approach speeds and clock as
        // chronological capture, without replaying a static tree every frame.
        let tables = update_player_option_tables(lua)?;
        let mut scratch = ModSnapshotScratch::default();
        let states = scratch.states(&tables)?;
        let speeds = scratch.player_speeds(lua, &tables)?;
        let origin = context.song_timing.as_ref()
            .map_or(0.0, |timing| timing.get_time_for_beat_exact(0.0));
        let limit = origin + song_elapsed_seconds_at(end, context) * song_music_rate(context);
        let unit = if context.song_timing.is_some() {
            SongLuaTimeUnit::Second
        } else {
            SongLuaTimeUnit::BeatClock
        };
        let mut eases = Vec::new();
        push_update_mod_targets_with_key(&mut eases, origin, limit.max(origin.next_up()),
            &states, &states, &states, &speeds, None, &mut BTreeMap::new(),
            &mut (0, String::new()), unit);
        // Column handlers retain startup writes too. A one-knot native
        // rotation spline needs neither Solve nor an update callback.
        let columns = read_note_column_transform_samples(lua)?;
        let mut column_transforms = Vec::new();
        append_column_transform_windows_from_samples(
            &mut column_transforms, &columns, &columns,
            SongLuaColumnOffsetBuildParams {
                unit, start: origin, limit: limit.max(origin).next_up() - origin,
                span_mode: SongLuaSpanMode::Len, easing: None, sustain: None,
                opt1: None, opt2: None,
            },
        );
        spline_capture.finish(column_splines);
        return Ok((
            eases,
            Vec::new(),
            Vec::new(),
            column_transforms,
            Vec::new(),
            Vec::new(),
        ));
    }

    crate::lua_util::restore_startup_locals(lua).map_err(|err| err.to_string())?;
    // Queued discovery sounds describe future bodies. Chronological execution
    // records the calls actually reached; keep only real startup calls here.
    sound_events.retain(|event| !event.queued_startup);
    let player_tables = tracked_player_tables(tracked_actors);
    let option_tables = update_player_option_tables(lua)?;
    reset_overlay_compile_actor_capture_tables(lua, overlays)?;
    reset_tracked_capture_tables(lua, tracked_actors)?;
    for overlay in overlays.iter() {
        if let Some(binding) = overlay
            .table
            .raw_get::<Option<Table>>("__songlua_startup_sprite")
            .map_err(|err| err.to_string())?
        {
            for key in [
                "Texture",
                "Frames",
                "__songlua_state_sprite_default_states",
                "__songlua_state_sprite_texture_epoch",
                "__songlua_state_sprite_frame_sheet",
            ] {
                overlay
                    .table
                    .raw_set(
                        key,
                        binding
                            .raw_get::<Value>(key)
                            .map_err(|err| err.to_string())?,
                    )
                    .map_err(|err| err.to_string())?;
            }
        }
        // Speculative queue/action captures leave destination values in Lua.
        // Chronological replay must begin at the actual zero-delta pose for
        // every actor, including sleeping parents and their paused children.
        set_actor_overlay_getter_state(lua, &overlay.table, overlay.actor.initial_state)?;
    }
    crate::lua_util::begin_tween_replay(lua, overlays, startup_tweens);
    let overlay_count = overlays.len();
    // Player transforms use the same chronological tween capture as song
    // actors. Their temporary indices never become drawable overlay tracks.
    let mut capture_actors: Vec<Table> = overlays.iter().map(|actor| actor.table.clone()).collect();
    let mut baseline_overlays = current_overlay_compile_actor_states(overlays)?;
    let mut player_capture_indices = [None; LUA_PLAYERS];
    for (player, actor) in player_tables.iter().enumerate() {
        if let Some(actor) = actor {
            player_capture_indices[player] = Some(capture_actors.len());
            capture_actors.push(actor.clone());
            // A startup queue retains its destination in the Lua table.
            // Frame zero still renders the current pose before positive delta.
            let mut state = actor_overlay_initial_state(actor)?;
            crate::lua_util::replay_tween_pose(lua, actor, &mut state);
            baseline_overlays.push(state);
        }
    }
    // Screen layers sit outside the returned song tree but queued callbacks
    // can still write them. Capture those writes on the same chronological
    // clock, retaining them as timed commands on the existing captured actor.
    let mut layer_capture_indices = Vec::new();
    for (tracked_index, tracked) in tracked_actors.iter().enumerate() {
        if matches!(
            tracked.target,
            SongLuaTrackedActorTarget::SongForeground | SongLuaTrackedActorTarget::ScreenLayer(_)
        ) {
            layer_capture_indices.push((tracked_index, capture_actors.len()));
            capture_actors.push(tracked.table.clone());
            baseline_overlays.push(actor_overlay_initial_state(&tracked.table)?);
        }
    }
    let mut layer_broadcasts = Vec::new();
    crate::lua_util::begin_overlay_update_capture_from_indices(
        lua,
        capture_actors
            .iter()
            .enumerate()
            .map(|(index, actor)| (actor.to_pointer() as usize, index)),
    );
    crate::lua_util::begin_wrapper_capture(lua);
    let wrapper_capture_start = capture_actors.len();
    let mut message_replay = SongLuaPerframeMessageReplay::new(messages, overlays.len());
    #[cfg(feature = "test-support")]
    lua.set_app_data(crate::song_tables::SongLuaBoolWrites::default());
    #[cfg(feature = "test-support")]
    lua.set_app_data(crate::song_tables::SongLuaSpeedWrites::default());
    #[cfg(feature = "test-support")]
    lua.set_app_data(crate::host::SongLuaStateCalls::default());
    #[cfg(feature = "test-support")]
    lua.set_app_data(crate::song_tables::SongLuaSkinWrites::default());
    let mut replay_overlays = baseline_overlays.clone();
    let started = message_replay.advance(lua, context, overlays, &mut replay_overlays, start)?;
    restore_started_message_states(lua, overlays, &replay_overlays, started)?;
    crate::lua_util::append_wrapper_actors(lua, &mut capture_actors,
        &mut [&mut baseline_overlays, &mut replay_overlays]);
    // Message discovery leaves player destinations in Lua. Frame zero uses
    // their current pose, including immediate writes and unadvanced queues.
    for tracked in tracked_actors.iter_mut() {
        if let SongLuaTrackedActorTarget::Player(player) = tracked.target
            && let Some(index) = player_capture_indices[player]
        {
            let mut state = replay_overlays[index];
            crate::lua_util::replay_tween_pose(lua, &tracked.table, &mut state);
            set_actor_overlay_getter_state(lua, &tracked.table, state)?;
            baseline_overlays[index] = state;
            replay_overlays[index] = state;
            tracked.actor.initial_state = state;
        }
    }
    let mut update_overlays = replay_overlays.clone();
    let baseline_players = current_perframe_player_states(lua, &player_tables)?;
    let mut mod_scratch = ModSnapshotScratch::default();
    let baseline_mods = mod_scratch.states(&option_tables)?;
    let baseline_columns = read_note_column_transform_samples(lua)?;
    let replay = update_function_replay_beats(context, start, end);
    let sample_count = replay.len();
    let mut sample_beats = frame_buffer(start, sample_count);
    // Text is not tween state. Retain changes after the complete native-order
    // update, including parent callbacks that run after their children.
    let mut text_samples = Vec::new();
    for (index, overlay) in overlays.iter_mut().enumerate() {
        if let crate::SongLuaOverlayKind::BitmapText { text, .. } = &mut overlay.actor.kind {
            overlay
                .table
                .raw_set("Text", text.as_ref())
                .map_err(|err| err.to_string())?;
            text_samples.push((index, Vec::<(f32, std::sync::Arc<str>)>::new()));
        }
    }
    let rate = f64::from(song_music_rate(context));
    let origin = context
        .song_timing
        .as_ref()
        .map(|timing| timing.get_time_for_beat_exact(0.0));
    let frame_time =
        |beat, seconds: f64| origin.map_or(beat, |origin| (seconds * rate) as f32 + origin);
    let mut sample_seconds = frame_buffer(
        (f64::from(song_elapsed_seconds_at(start, context)) * rate) as f32,
        sample_count,
    );
    let fallback_bpms = [(0.0, song_display_bps(context) * 60.0)];
    let bpms = if context.song_timing_bpms.is_empty() {
        fallback_bpms.as_slice()
    } else {
        &context.song_timing_bpms
    };
    // Pauses, warps and split chart timing need the player's beat conversion.
    // A matching continuous clock can retain the exact sampled frame instead.
    let use_mod_clock = context
        .player_timing
        .iter()
        .flatten()
        .all(|timing| timing.matches_bpm_clock(bpms));
    let mut player_samples = frame_buffer(baseline_players, sample_count);
    let mut mod_samples = frame_buffer(baseline_mods.clone(), sample_count);
    let mut mod_speed_samples = frame_buffer(
        mod_scratch.player_speeds(lua, &option_tables)?,
        sample_count,
    );
    let mut column_samples = frame_buffer(baseline_columns, sample_count);
    let mut overlay_tracks = Vec::new();
    // Keys are compiler-owned actor indices and target enum discriminants.
    let mut overlay_track_indices = FxHashMap::default();
    let mut scheduled_overlay_samples = Vec::new();
    let mut overlay_sample_scratch = OverlaySampleScratch::default();
    let mut current_overlays = replay_overlays.clone();
    crate::lua_util::set_compile_frames(lua, replay.iter().copied())
        .map_err(|err| err.to_string())?;
    let actor_frames = reference_actor_frames(context);
    crate::lua_util::set_compile_actor_frames(lua, actor_frames.clone());
    let start_seconds = f64::from(song_elapsed_seconds_at(start, context));
    capture_update_overlay_samples(
        lua,
        context,
        &capture_actors,
        &baseline_overlays,
        &baseline_overlays,
        &mut update_overlays,
        &mut current_overlays,
        started,
        &mut overlay_tracks,
        &mut overlay_track_indices,
        frame_time(start, f64::from(song_elapsed_seconds_at(start, context))),
        frame_time(start, f64::from(song_elapsed_seconds_at(start, context))),
        f64::from(song_elapsed_seconds_at(start, context)),
        &mut scheduled_overlay_samples,
        &mut overlay_sample_scratch,
    )?;
    message_replay.stop(&overlay_sample_scratch.stopped_indices);
    let music_seconds = crate::runtime::song_music_time(lua, start_seconds, song_music_rate(context));
    let start_time = frame_time(start, start_seconds);
    let mut field_capture_bytes = 0;
    let mut prior_field_second = music_seconds;
    capture_field_wrappers(lua, tracked_actors, music_seconds, [music_seconds, start],
        &mut field_capture_bytes)?;
    for (index, actor) in capture_actors.iter().enumerate() {
        if let Some(clock) = crate::lua_util::effect_render_time(actor, [music_seconds, start])
            .map_err(|err| err.to_string())?
        {
            let target = SongLuaOverlayUpdateTarget::EffectTime;
            let value = SongLuaOverlayUpdateValue::Vec2(clock);
            set_overlay_state_update_value(&mut replay_overlays[index], target, &value);
            push_captured_overlay_value(&mut overlay_tracks, &mut overlay_track_indices,
                index, target, start_time, &current_overlays[index], start_time, &value);
            set_overlay_state_update_value(&mut current_overlays[index], target, &value);
        }
    }

    let mut beat = start;
    let mut seconds = f64::from(song_elapsed_seconds_at(start, context));
    let mut scheduled_states = baseline_overlays.clone();
    let mut player_capture_masks = player_transform_masks(lua, &player_tables)?;
    let mut frame_count = 0;
    for (exact_beat, delta_seconds) in replay.into_iter().skip(1) {
        let next_beat = exact_beat as f32;
        frame_count += 1;
        crate::lua_util::set_compile_frame(lua, frame_count);
        let delta_beats = next_beat - beat;
        let prior_time = frame_time(beat, seconds);
        seconds += delta_seconds;
        let next_time = frame_time(next_beat, seconds);
        let stage = profile.then(Instant::now);
        reset_tracked_capture_tables(lua, tracked_actors)?;
        reset_ms += stage.map_or(0.0, |started| started.elapsed().as_secs_f64() * 1000.0);
        let stage = profile.then(Instant::now);
        // Reuse state buffers; callbacks can extend them with new wrappers.
        // Keep prior, message-replayed and updated states apart.
        replay_overlays.copy_from_slice(&current_overlays);
        let started =
            message_replay.advance(lua, context, overlays, &mut replay_overlays, next_beat)?;
        message_ms += stage.map_or(0.0, |started| started.elapsed().as_secs_f64() * 1000.0);
        let stage = profile.then(Instant::now);
        apply_scheduled_overlay_states(
            lua,
            &capture_actors,
            &mut scheduled_states,
            &scheduled_overlay_samples,
            seconds,
        )?;
        crate::lua_util::set_pending_tweens(
            lua,
            scheduled_overlay_samples
                .iter()
                .filter(|sample| sample.end_seconds > scheduled_overlay_clock(sample, seconds))
                .map(|sample| (sample.overlay_index, sample.target, sample.value.clone())),
        );
        crate::lua_util::set_prior_positions(lua, &current_overlays);
        let actor_delta = f64::from(seconds as f32 - (seconds - delta_seconds) as f32);
        let actor_delta = actor_frames.as_ref().map_or(actor_delta, |frames| f64::from(frames[frame_count][0]));
        call_update_functions_at(lua, root, exact_beat, seconds, delta_beats, actor_delta)?;
        crate::model_texture::sample(lua, seconds as f32);
        crate::lua_util::append_wrapper_actors(lua, &mut capture_actors, &mut [
            &mut baseline_overlays, &mut current_overlays, &mut replay_overlays,
            &mut update_overlays, &mut scheduled_states,
        ]);
        for (index, samples) in &mut text_samples {
            let overlay = &overlays[*index];
            let text = overlay
                .table
                .get::<String>("Text")
                .map_err(|err| err.to_string())?;
            let crate::SongLuaOverlayKind::BitmapText { text: initial, .. } = &overlay.actor.kind
            else {
                unreachable!("text actor kind is unchanged during replay")
            };
            if samples
                .last()
                .map_or(initial.as_ref(), |sample| sample.1.as_ref())
                != text
            {
                samples.push((next_time, text.into()));
            }
        }
        crate::lua_util::sample_tween_replays(lua);
        for (overlay, state) in overlays.iter().zip(&mut replay_overlays) {
            crate::lua_util::replay_tween_pose(lua, &overlay.table, state);
        }
        crate::lua_util::capture_tween_poses(lua, &capture_actors);
        update_ms += stage.map_or(0.0, |started| started.elapsed().as_secs_f64() * 1000.0);
        let stage = profile.then(Instant::now);
        restore_started_message_states(lua, overlays, &replay_overlays, started)?;
        update_overlays.copy_from_slice(&replay_overlays);
        let next_masks = player_transform_masks(lua, &player_tables)?;
        capture_update_overlay_samples(
            lua,
            context,
            &capture_actors,
            &baseline_overlays,
            &current_overlays,
            &mut update_overlays,
            &mut replay_overlays,
            started,
            &mut overlay_tracks,
            &mut overlay_track_indices,
            prior_time,
            next_time,
            seconds,
            &mut scheduled_overlay_samples,
            &mut overlay_sample_scratch,
        )?;
        message_replay.stop(&overlay_sample_scratch.stopped_indices);
        apply_scheduled_overlay_states(
            lua,
            &capture_actors,
            &mut replay_overlays,
            &scheduled_overlay_samples,
            seconds,
        )?;
        for (index, target, value) in overlay_sample_scratch.retargeted_states.drain(..) {
            set_overlay_state_update_value(&mut replay_overlays[index], target, &value);
            push_captured_overlay_value(
                &mut overlay_tracks,
                &mut overlay_track_indices,
                index,
                target,
                prior_time,
                &current_overlays[index],
                next_time,
                &value,
            );
        }
        // Bake current render values, including linear queues whose destinations
        // can be edited by subsequent callbacks. Endpoint merges retain them.
        for sample in &scheduled_overlay_samples {
            if (sample.overlay_index >= overlay_count && sample.overlay_index < wrapper_capture_start)
                || scheduled_overlay_clock(sample, seconds) > sample.end_seconds
            {
                continue;
            }
            let value =
                overlay_state_update_value(&replay_overlays[sample.overlay_index], sample.target);
            push_captured_overlay_value(
                &mut overlay_tracks,
                &mut overlay_track_indices,
                sample.overlay_index,
                sample.target,
                prior_time,
                &current_overlays[sample.overlay_index],
                next_time,
                &value,
            );
        }
        // Preserve the intervening frames of directly written transforms.
        // A recurring command can skip a frame; that frame holds its last value.
        for index in 0..overlay_tracks.len() {
            let (actor_index, target) = (
                overlay_tracks[index].overlay_index,
                overlay_tracks[index].target,
            );
            if (actor_index >= overlay_count && actor_index < wrapper_capture_start)
                || (target != SongLuaOverlayUpdateTarget::Zoom
                    && !PLAYER_TRANSFORM_TARGETS.contains(&target))
            {
                continue;
            }
            let value = overlay_state_update_value(&replay_overlays[actor_index], target);
            push_captured_overlay_value(
                &mut overlay_tracks,
                &mut overlay_track_indices,
                actor_index,
                target,
                prior_time,
                &current_overlays[actor_index],
                next_time,
                &value,
            );
        }
        overlay_ms += stage.map_or(0.0, |started| started.elapsed().as_secs_f64() * 1000.0);
        let stage = profile.then(Instant::now);
        // Playback consumes raw music time; record clock anchors in that same
        // coordinate system even when the actor accumulates a local timer.
        let music_seconds = crate::runtime::song_music_time(lua, seconds, song_music_rate(context));
        for (index, actor) in capture_actors.iter().enumerate() {
            if let Some(clock) =
                crate::lua_util::effect_render_time(actor, [music_seconds, next_beat])
                    .map_err(|err| err.to_string())?
            {
                // Keep a reference while it predicts the exact native float.
                // Wraps, restarts, or changed rounding publish a new sample.
                let clock = current_overlays[index]
                    .effect_time
                    .filter(|[at, units]| (units + (clock[0] - at)).to_bits() == clock[1].to_bits())
                    .unwrap_or(clock);
                let target = SongLuaOverlayUpdateTarget::EffectTime;
                let value = SongLuaOverlayUpdateValue::Vec2(clock);
                set_overlay_state_update_value(&mut replay_overlays[index], target, &value);
                push_captured_overlay_value(
                    &mut overlay_tracks,
                    &mut overlay_track_indices,
                    index,
                    target,
                    prior_time,
                    &current_overlays[index],
                    next_time,
                    &value,
                );
            }
            if let Some(rotation) =
                crate::lua_util::spin_render_pose(actor).map_err(|err| err.to_string())?
            {
                for player in 0..LUA_PLAYERS {
                    if player_capture_indices[player] == Some(index) {
                        player_capture_masks[player] |= (1 << 3) | (1 << 4) | (1 << 5);
                    }
                }
                for (target, angle) in [
                    SongLuaOverlayUpdateTarget::RotationX,
                    SongLuaOverlayUpdateTarget::RotationY,
                    SongLuaOverlayUpdateTarget::RotationZ,
                ]
                .into_iter()
                .zip(rotation)
                {
                    let value = SongLuaOverlayUpdateValue::F32(angle);
                    set_overlay_state_update_value(&mut replay_overlays[index], target, &value);
                    if !overlay_state_matches_update_value(&current_overlays[index], target, &value)
                    {
                        push_captured_overlay_value(
                            &mut overlay_tracks,
                            &mut overlay_track_indices,
                            index,
                            target,
                            prior_time,
                            &current_overlays[index],
                            next_time,
                            &value,
                        );
                    }
                }
            }
        }
        let mut next_players = current_perframe_player_states(lua, &player_tables)?;
        for player in 0..LUA_PLAYERS {
            player_capture_masks[player] |= next_masks[player];
            if let Some(index) = player_capture_indices[player] {
                let state = &replay_overlays[index];
                let mask = player_capture_masks[player];
                let output = &mut next_players[player];
                sample_player_pose(output, state, Some(mask));
            }
        }
        sample_beats.push(next_beat);
        sample_seconds.push((seconds * rate) as f32);
        player_samples.push(next_players);
        player_ms += stage.map_or(0.0, |started| started.elapsed().as_secs_f64() * 1000.0);
        let stage = profile.then(Instant::now);
        mod_samples.push(mod_scratch.states(&option_tables)?);
        mod_speed_samples.push(mod_scratch.player_speeds(lua, &option_tables)?);
        mod_ms += stage.map_or(0.0, |started| started.elapsed().as_secs_f64() * 1000.0);
        let stage = profile.then(Instant::now);
        column_samples.push(read_note_column_transform_samples(lua)?);
        spline_capture.capture(lua, (seconds * rate) as f32)?;
        column_ms += stage.map_or(0.0, |started| started.elapsed().as_secs_f64() * 1000.0);
        capture_field_wrappers(lua, tracked_actors, prior_field_second,
            [music_seconds, next_beat], &mut field_capture_bytes)?;
        prior_field_second = music_seconds;
        let mut layer_message = None;
        for &(tracked_index, index) in &layer_capture_indices {
            let prior = current_overlays[index];
            let Some((_, delta)) =
                overlay_delta_pair_from_states(prior, prior, replay_overlays[index])
            else {
                continue;
            };
            let message =
                layer_message.get_or_insert_with(|| format!("__songlua_layer_frame_{frame_count}"));
            tracked_actors[tracked_index].actor.message_commands.push(
                crate::SongLuaOverlayMessageCommand {
                    frame_advance: 0.0,
                    message: message.clone(),
                    blocks: vec![crate::SongLuaOverlayCommandBlock {
                        progress: None,
                        queued: false,
                        start: 0.0,
                        duration: 0.0,
                        easing: None,
                        opt1: None,
                        opt2: None,
                        delta,
                    }],
                },
            );
        }
        if let Some(message) = layer_message {
            layer_broadcasts.push((next_beat, message, true));
        }
        std::mem::swap(&mut current_overlays, &mut replay_overlays);
        beat = next_beat;
    }
    if profile {
        eprintln!(
            "Song lua update timing: frames={frame_count} reset_ms={reset_ms:.3} message_ms={message_ms:.3} update_ms={update_ms:.3} player_ms={player_ms:.3} mod_ms={mod_ms:.3} column_ms={column_ms:.3} overlay_ms={overlay_ms:.3}"
        );
    }

    let mut eases = Vec::new();
    let mut column_transforms = Vec::new();
    let mut last_mod_windows = BTreeMap::new();
    let mut last_mod_lookup_key = (0, String::new());
    for index in 0..sample_beats.len() {
        let seg_start = sample_beats[index];
        let seg_end = sample_beats.get(index + 1).copied().unwrap_or(end);
        // Preserve the last sampled target even when the song ends between
        // reference frames; the gameplay window builder retains its tail.
        let from_mods = &mod_samples[index];
        let to_mods = mod_samples.get(index + 1).unwrap_or(from_mods);
        let (mod_start, mod_end, mod_unit) = if let Some(timing) = &context.song_timing {
            let origin = timing.get_time_for_beat_exact(0.0);
            let start = sample_seconds[index] + origin;
            (
                start,
                sample_seconds
                    .get(index + 1)
                    .copied()
                    .map_or_else(|| start.next_up(), |second| second + origin),
                SongLuaTimeUnit::Second,
            )
        } else if use_mod_clock {
            (
                sample_seconds[index],
                sample_seconds
                    .get(index + 1)
                    .copied()
                    .unwrap_or_else(|| sample_seconds[index].next_up()),
                SongLuaTimeUnit::BeatClock,
            )
        } else {
            (
                seg_start,
                seg_end.max(seg_start.next_up()),
                SongLuaTimeUnit::Beat,
            )
        };
        // Dropping a speed mode ends its coalescing run. Selecting the same
        // value again later must start a new window after the intervening mode.
        for player in 0..LUA_PLAYERS {
            for key in ["xmod", "cmod", "mmod"] {
                if from_mods[player].get(key).is_none() {
                    last_mod_lookup_key.0 = player;
                    last_mod_lookup_key.1.clear();
                    last_mod_lookup_key.1.push_str(key);
                    last_mod_windows.remove(&last_mod_lookup_key);
                }
            }
        }
        push_update_mod_targets_with_key(
            &mut eases,
            mod_start,
            mod_end,
            from_mods,
            to_mods,
            &baseline_mods,
            &mod_speed_samples[index],
            mod_speed_samples.get(index + 1),
            &mut last_mod_windows,
            &mut last_mod_lookup_key,
            // Keep the update clock: narrowing to a beat and converting back
            // can move a step target past its own frame's timestamp.
            mod_unit,
        );
        let (frame_start, frame_end, frame_unit) = if origin.is_some() {
            (mod_start, mod_end, SongLuaTimeUnit::Second)
        } else {
            // Actor::UpdateTweening uses seconds regardless of split chart
            // timing. A rounded beat/time round trip can move a captured pose
            // past its own frame and interpolate towards the following pose.
            (
                sample_seconds[index],
                sample_seconds
                    .get(index + 1)
                    .copied()
                    .unwrap_or_else(|| sample_seconds[index].next_up()),
                SongLuaTimeUnit::BeatClock,
            )
        };
        if frame_end <= frame_start {
            continue;
        }
        let from_players = player_samples[index];
        let to_players = player_samples
            .get(index + 1)
            .copied()
            .unwrap_or(from_players);
        let first_player_window = eases.len();
        push_perframe_player_targets(
            &mut eases,
            frame_start,
            frame_end,
            &from_players,
            &to_players,
            &baseline_players,
        );
        for window in &mut eases[first_player_window..] {
            window.unit = frame_unit;
        }
        let from_columns = &column_samples[index];
        let to_columns = column_samples.get(index + 1).unwrap_or(from_columns);
        append_column_transform_windows_from_samples(
            &mut column_transforms,
            from_columns,
            to_columns,
            SongLuaColumnOffsetBuildParams {
                unit: frame_unit,
                start: frame_start,
                limit: frame_end - frame_start,
                span_mode: SongLuaSpanMode::Len,
                easing: None,
                sustain: None,
                opt1: None,
                opt2: None,
            },
        );
    }
    // Replay captured every rendered frame; future queued work lies beyond
    // this song and must not introduce interpolated endpoints into its tracks.
    for track in &mut overlay_tracks {
        sort_overlay_update_samples(&mut track.samples);
    }

    let (wrapper_start, wrappers) = crate::lua_util::finish_wrapper_capture(lua, overlay_count);
    debug_assert_eq!(wrapper_start, wrapper_capture_start);
    append_wrapper_overlays(overlays, wrappers)?;
    overlay_tracks.retain_mut(|track| {
        if track.overlay_index < overlay_count { return true; }
        if track.overlay_index >= wrapper_start {
            track.overlay_index = overlay_count + track.overlay_index - wrapper_start;
            return true;
        }
        false
    });
    for (index, samples) in text_samples {
        if let crate::SongLuaOverlayKind::BitmapText {
            text_changes,
            text_changes_in_seconds,
            ..
        } = &mut overlays[index].actor.kind
        {
            *text_changes = samples.into();
            *text_changes_in_seconds = context.song_timing.is_some();
        }
    }
    let mut stateful_messages = crate::lua_util::stateful_message_captures(lua);
    for capture in &mut stateful_messages {
        capture
            .overlay_targets
            .retain(|(index, _)| *index < overlays.len());
        capture
            .writes
            .retain(|write| write.overlay_index < overlays.len());
    }
    stateful_messages
        .retain(|capture| !capture.overlay_targets.is_empty() || !capture.writes.is_empty());
    let mut runtime_broadcasts = crate::lua_util::runtime_broadcast_captures(lua);
    runtime_broadcasts.extend(layer_broadcasts);
    spline_capture.finish(column_splines);
    sound_events.extend(crate::lua_util::take_runtime_sounds(lua));
    judgment_textures.extend(crate::lua_util::take_judgment_textures(lua));
    sprite_textures.extend(crate::lua_util::take_sprite_textures(lua));
    crate::lua_util::apply_message_advances(lua, overlays);
    crate::lua_util::finish_tween_replay(lua, overlays, startup_tweens);
    lua.remove_app_data::<crate::lua_util::SongLuaCompileFrames>();
    crate::lua_util::end_overlay_update_capture(lua);
    Ok((
        eases,
        Vec::new(),
        overlay_tracks,
        column_transforms,
        stateful_messages,
        runtime_broadcasts,
    ))
}

#[derive(Clone, Copy)]
struct SongLuaPerframeActiveMessage {
    command_index: usize,
    start_beat: f32,
    elapsed: Option<f32>,
    base: SongLuaOverlayState,
}

// Reused for one replay. Result indices are unique and sorted for binary search.
#[derive(Default)]
struct StartedOverlayIndices {
    indices: Vec<usize>,
    seen: Vec<bool>,
}

impl StartedOverlayIndices {
    fn clear(&mut self) {
        for &index in &self.indices {
            self.seen[index] = false;
        }
        self.indices.clear();
    }

    fn insert(&mut self, index: usize, overlay_count: usize) {
        if self.seen.is_empty() {
            self.seen.resize(overlay_count, false);
        }
        if !self.seen[index] {
            self.seen[index] = true;
            self.indices.push(index);
        }
    }

    fn as_sorted_slice(&mut self) -> &[usize] {
        self.indices.sort_unstable();
        &self.indices
    }
}

struct SongLuaPerframeMessageReplay<'a> {
    messages: &'a [SongLuaMessageEvent],
    order: Vec<usize>,
    next: usize,
    active: Vec<Option<SongLuaPerframeActiveMessage>>,
    started: StartedOverlayIndices,
}

#[cfg(any(test, feature = "legacy-oracle"))]
impl<'a> SongLuaPerframeMessageReplay<'a> {
    fn stop(&mut self, indices: &[usize]) {
        // Startup and message blocks share the actor's native tween queue.
        // A callback cancellation must prevent them from resuming next frame.
        for &index in indices {
            if let Some(active) = self.active.get_mut(index) {
                *active = None;
            }
        }
    }

    fn new(messages: &'a [SongLuaMessageEvent], overlay_count: usize) -> Self {
        let mut order = (0..messages.len()).collect::<Vec<_>>();
        order.sort_by(|&a, &b| messages[a].beat.total_cmp(&messages[b].beat));
        Self {
            messages,
            order,
            next: 0,
            active: vec![None; overlay_count],
            started: StartedOverlayIndices::default(),
        }
    }

    fn advance<Kind>(
        &mut self,
        lua: &Lua,
        context: &SongLuaCompileContext,
        overlays: &mut [SongLuaOverlayCompileActor<Kind>],
        states: &mut [SongLuaOverlayState],
        beat: f32,
    ) -> Result<&[usize], String> {
        self.started.clear();
        while let Some(&event_index) = self.order.get(self.next) {
            let event = &self.messages[event_index];
            let event_beat = event.beat;
            if event_beat > beat + f32::EPSILON {
                break;
            }
            for (overlay_index, overlay) in overlays.iter_mut().enumerate() {
                let was_active = self.active[overlay_index].is_some();
                apply_perframe_active_message(
                    lua,
                    context,
                    overlay,
                    &mut self.active[overlay_index],
                    event_beat,
                )?;
                if was_active && let Some(state) = states.get_mut(overlay_index) {
                    *state = actor_overlay_initial_state(&overlay.table)?;
                }
                let Some(command_index) = overlay
                    .actor
                    .message_commands
                    .iter()
                    .position(|command| command.message == event.message)
                else {
                    continue;
                };
                let base = match states.get(overlay_index) {
                    Some(state) => *state,
                    None => actor_overlay_initial_state(&overlay.table)?,
                };
                self.active[overlay_index] = Some(SongLuaPerframeActiveMessage {
                    command_index,
                    start_beat: event_beat,
                    elapsed: None,
                    base,
                });
                self.started.insert(overlay_index, self.active.len());
                apply_perframe_active_message(
                    lua,
                    context,
                    overlay,
                    &mut self.active[overlay_index],
                    event_beat,
                )?;
                if let Some(state) = states.get_mut(overlay_index) {
                    *state = actor_overlay_initial_state(&overlay.table)?;
                }
            }
            self.next += 1;
        }
        for (overlay_index, overlay) in overlays.iter_mut().enumerate() {
            let was_active = self.active[overlay_index].is_some();
            apply_perframe_active_message(
                lua,
                context,
                overlay,
                &mut self.active[overlay_index],
                beat,
            )?;
            if was_active && let Some(state) = states.get_mut(overlay_index) {
                *state = actor_overlay_initial_state(&overlay.table)?;
            }
        }
        Ok(self.started.as_sorted_slice())
    }
}

#[cfg(any(test, feature = "legacy-oracle"))]
fn restore_started_message_states<Kind>(
    lua: &Lua,
    overlays: &[SongLuaOverlayCompileActor<Kind>],
    states: &[SongLuaOverlayState],
    started: &[usize],
) -> Result<(), String> {
    for &index in started {
        let (Some(overlay), Some(state)) = (overlays.get(index), states.get(index)) else {
            continue;
        };
        set_actor_overlay_getter_state(lua, &overlay.table, *state)?;
    }
    Ok(())
}

#[cfg(any(test, feature = "legacy-oracle"))]
fn apply_perframe_active_message<Kind>(
    lua: &Lua,
    context: &SongLuaCompileContext,
    overlay: &SongLuaOverlayCompileActor<Kind>,
    active: &mut Option<SongLuaPerframeActiveMessage>,
    beat: f32,
) -> Result<(), String> {
    let Some(message) = *active else {
        return Ok(());
    };
    let Some(command) = overlay.actor.message_commands.get(message.command_index) else {
        *active = None;
        return Ok(());
    };
    let elapsed = beat_span_seconds(context, message.start_beat, beat);
    let keep = |block: &&crate::SongLuaOverlayCommandBlock| {
        !block.queued || command.message.starts_with("__songlua_")
    };
    let mut state =
        overlay_state_after_blocks(message.base, command.blocks.iter().filter(keep), elapsed);
    // Actor.h stores effects outside TweenState. A queued effect survives later
    // tween frames; only a newly reached command block can replace it.
    let current = actor_overlay_initial_state(&overlay.table)?;
    use SongLuaOverlayUpdateTarget as Target;
    for target in [
        Target::Hibernating,
        Target::HAlign,
        Target::VAlign,
        Target::TextAlign,
        Target::Size,
        Target::BaseZoom,
        Target::BaseZoomX,
        Target::BaseZoomY,
        Target::BaseZoomZ,
        Target::BaseAlpha,
        Target::Vibrate,
        Target::EffectMagnitude,
        Target::EffectClock,
        Target::EffectTimer,
        Target::EffectNoOffset,
        Target::EffectMode,
        Target::EffectColor1,
        Target::EffectColor2,
        Target::EffectPeriod,
        Target::EffectOffset,
        Target::EffectTime,
        Target::EffectTiming,
        Target::Rainbow,
    ] {
        let written = command.blocks.iter().filter(keep).any(|block| {
            block.delta.has_update_target(target)
                && crate::overlay_block_factor(block, elapsed).is_some()
                && message
                    .elapsed
                    .is_none_or(|prior| crate::overlay_block_factor(block, prior).is_none())
        });
        if !written {
            let value = overlay_state_update_value(&current, target);
            set_overlay_state_update_value(&mut state, target, &value);
        }
    }
    if command.message == "__songlua_actor_startup" {
        crate::lua_util::replay_tween_pose(lua, &overlay.table, &mut state);
    }
    active
        .as_mut()
        .expect("active message was read above")
        .elapsed = Some(elapsed);
    if let Some(epoch) = crate::motion_restart_epoch(
        message.base,
        &command.blocks,
        elapsed,
        song_elapsed_seconds_at(message.start_beat, context),
    ) {
        crate::lua_util::sync_motion_restart(
            lua,
            &overlay.table,
            epoch,
            song_elapsed_seconds_at(beat, context),
        )
        .map_err(|err| err.to_string())?;
    }
    set_actor_overlay_getter_state(lua, &overlay.table, state)?;
    let duration = command
        .blocks
        .iter()
        .filter(keep)
        .map(crate::overlay_block_end)
        .fold(0.0_f32, f32::max);
    if elapsed >= duration {
        *active = None;
    }
    Ok(())
}

// Only adjacent snapshots are needed to emit a sampled segment. Keep their
// capacity across windows instead of retaining every sample until the end.
#[derive(Default)]
struct PerframeSnapshot {
    beat: f32,
    players: [SongLuaPerframePlayerState; LUA_PLAYERS],
    overlays: Vec<SongLuaOverlayState>,
}

#[cfg(any(test, feature = "legacy-oracle"))]
impl PerframeSnapshot {
    fn capture<Kind>(
        &mut self,
        lua: &Lua,
        beat: f32,
        players: &[Option<Table>; LUA_PLAYERS],
        overlays: &[SongLuaOverlayCompileActor<Kind>],
    ) -> Result<(), String> {
        self.beat = beat;
        self.players = current_perframe_player_states(lua, players)?;
        self.overlays.clear();
        self.overlays.reserve_exact(overlays.len());
        for overlay in overlays {
            self.overlays
                .push(actor_overlay_initial_state(&overlay.table)?);
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn push_segment(
        &self,
        next: &Self,
        end: f32,
        out_eases: &mut Vec<SongLuaEaseWindow>,
        out_overlay_eases: &mut Vec<SongLuaOverlayEase>,
        baseline_players: &[SongLuaPerframePlayerState; LUA_PLAYERS],
        baseline_overlays: &[SongLuaOverlayState],
    ) {
        if end <= self.beat {
            return;
        }
        push_perframe_player_targets(
            out_eases,
            self.beat,
            end,
            &self.players,
            &next.players,
            baseline_players,
        );
        push_perframe_overlay_targets(
            out_overlay_eases,
            self.beat,
            end,
            &self.overlays,
            &next.overlays,
            baseline_overlays,
            false,
        );
    }
}

#[cfg(any(test, feature = "legacy-oracle"))]
pub fn compile_perframes<Kind>(
    lua: &Lua,
    prefix_table: Option<Table>,
    global_table: Option<Table>,
    context: &SongLuaCompileContext,
    overlays: &mut [SongLuaOverlayCompileActor<Kind>],
    tracked_actors: &[SongLuaTrackedActor],
    messages: &[SongLuaMessageEvent],
) -> Result<
    (
        Vec<SongLuaEaseWindow>,
        Vec<SongLuaOverlayEase>,
        SongLuaCompileInfo,
    ),
    String,
> {
    let mut entries = read_perframe_entries(prefix_table)?;
    entries.extend(read_perframe_entries(global_table)?);
    if entries.is_empty() {
        return Ok((Vec::new(), Vec::new(), SongLuaCompileInfo::default()));
    }

    let boundaries = perframe_boundaries(&entries);
    if boundaries.len() < 2 {
        return Ok((Vec::new(), Vec::new(), SongLuaCompileInfo::default()));
    }

    let player_tables = tracked_player_tables(tracked_actors);
    let baseline_players = current_perframe_player_states(lua, &player_tables)?;
    let baseline_overlays = current_overlay_compile_actor_states(overlays)?;
    let mut out_eases = Vec::new();
    let mut out_overlay_eases = Vec::new();
    let mut saw_recognized_side_effect = false;
    let mut message_replay = SongLuaPerframeMessageReplay::new(messages, overlays.len());
    let mut message_states = baseline_overlays.clone();
    let mut previous = PerframeSnapshot::default();
    let mut current = PerframeSnapshot::default();

    for window in boundaries.windows(2) {
        let [start, end] = [window[0], window[1]];
        if end <= start {
            continue;
        }
        let active = active_perframe_entries(&entries, start, end);
        if active.is_empty() {
            current.capture(lua, start, &player_tables, overlays)?;
            message_states.clone_from(&current.overlays);
            push_perframe_static_targets(
                &mut out_eases,
                &mut out_overlay_eases,
                start,
                end,
                &current.players,
                &current.overlays,
                &baseline_players,
                &baseline_overlays,
            );
            continue;
        }

        let mut has_previous = false;
        for sample in perframe_sample_iter(start, end) {
            let delta_seconds = beat_span_seconds(
                context,
                sample.eval_beat - sample.delta_beats,
                sample.eval_beat,
            );
            let _ = message_replay.advance(
                lua,
                context,
                overlays,
                &mut message_states,
                sample.eval_beat,
            )?;
            reset_overlay_compile_actor_capture_tables(lua, overlays)?;
            reset_tracked_capture_tables(lua, tracked_actors)?;
            for entry in &active {
                saw_recognized_side_effect |= call_perframe_entry(
                    lua,
                    entry,
                    sample.eval_beat,
                    sample.delta_beats,
                    delta_seconds,
                )?;
            }
            current.capture(lua, sample.beat, &player_tables, overlays)?;
            message_states.clone_from(&current.overlays);
            if has_previous {
                previous.push_segment(
                    &current,
                    current.beat,
                    &mut out_eases,
                    &mut out_overlay_eases,
                    &baseline_players,
                    &baseline_overlays,
                );
            }
            std::mem::swap(&mut previous, &mut current);
            has_previous = true;
        }

        if has_previous {
            previous.push_segment(
                &previous,
                end,
                &mut out_eases,
                &mut out_overlay_eases,
                &baseline_players,
                &baseline_overlays,
            );
        }
    }

    let mut info = SongLuaCompileInfo::default();
    if out_eases.is_empty() && out_overlay_eases.is_empty() && !saw_recognized_side_effect {
        info = unsupported_perframe_info(&entries);
    }
    Ok((out_eases, out_overlay_eases, info))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scheduled_native_lerp() {
        use SongLuaOverlayUpdateValue as Value;
        // Native near-camera Actor capture at 101/60 seconds of a 3s tween.
        let expected = 85.5555419921875_f32;
        let factor = (101.0_f32 / 60.0) / 3.0;
        let actual = lerp_scheduled_value(&Value::F32(-700.0), &Value::F32(700.0), factor);
        let Value::F32(actual) = actual else {
            panic!("scalar tween")
        };
        assert_eq!(actual.to_bits(), expected.to_bits());
        let actual = lerp_scheduled_value(
            &Value::Vec3([516.0, 31.0, -700.0]),
            &Value::Vec3([516.0, 31.0, 700.0]),
            factor,
        );
        let Value::Vec3(actual) = actual else {
            panic!("vector tween")
        };
        assert_eq!(actual, [516.0, 31.0, expected]);
    }

    #[test]
    fn completed_update_tweens_persist_into_following_state() {
        let baseline = vec![SongLuaOverlayState::default()];
        let mut update_states = baseline.clone();
        let mut next_states = baseline.clone();
        let mut tracks = Vec::new();
        let mut track_indices = std::collections::HashMap::new();
        let mut scheduled = vec![SongLuaScheduledOverlaySample {
            progress: None,
            duration: 1.0,
            dispatch_seconds: None,
            frame_advance: 0.0,
            overlay_index: 0,
            target: SongLuaOverlayUpdateTarget::X,
            start_seconds: 1.0,
            end_seconds: 2.0,
            start_beat: 1.0,
            end_beat: 2.0,
            easing: Some("accelerate".to_owned()),
            opt1: None,
            from: SongLuaOverlayUpdateValue::F32(0.0),
            value: SongLuaOverlayUpdateValue::F32(427.0),
        }];

        merge_completed_scheduled_overlay_samples(
            &mut tracks,
            &mut track_indices,
            &baseline,
            &mut update_states,
            &mut next_states,
            &mut scheduled,
            2.0,
        );

        assert!(scheduled.is_empty());
        assert_eq!(update_states[0].x, 427.0);
        assert_eq!(next_states[0].x, 427.0);
        assert!(tracks[0].samples.last().is_some_and(|sample| {
            sample.time == 2.0 && sample.value == SongLuaOverlayUpdateValue::F32(427.0)
        }));
    }
}
