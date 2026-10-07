use mlua::{Function, Lua, Table, Value};
use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::BTreeMap;
use std::rc::Rc;
use std::time::Instant;

use crate::{
    LUA_PLAYERS, SONG_LUA_PLAYER_OPTIONS_KEYS, SongLuaColumnOffsetBuildParams,
    SongLuaColumnOffsetWindow, SongLuaCompileContext, SongLuaCompileInfo, SongLuaEaseTarget,
    SongLuaEaseWindow, SongLuaMessageEvent, SongLuaOverlayCompileActor, SongLuaOverlayEase,
    SongLuaOverlayState, SongLuaOverlayUpdateSample, SongLuaOverlayUpdateTarget,
    SongLuaOverlayUpdateTrack, SongLuaOverlayUpdateValue, SongLuaSpanMode,
    SongLuaStatefulMessageCapture, SongLuaTimeUnit, SongLuaTrackedActor, SongLuaTrackedActorTarget,
    actor_overlay_initial_state, actor_tree_has_update_functions,
    append_column_transform_windows_from_samples, compile_song_runtime_delta_values,
    compile_song_runtime_values, overlay_delta_pair_from_states, overlay_state_after_blocks,
    push_unique_compile_detail, read_f32, read_note_column_transform_samples, reset_actor_capture,
    reset_overlay_compile_actor_capture_tables, reset_tracked_capture_tables,
    runtime_player_option_ease_target, set_actor_overlay_getter_state,
    set_compile_song_runtime_beat, set_compile_song_runtime_delta_values,
    set_compile_song_runtime_values, song_beat_at_elapsed_seconds, song_display_bps,
    song_elapsed_seconds_at, song_elapsed_seconds_for_beat, song_lua_side_effect_count,
    song_music_rate,
};

pub const SONG_LUA_UPDATE_FUNCTION_MAX_SAMPLES: usize = 8192;
pub(crate) const SONG_LUA_UPDATE_REFERENCE_FPS: f32 = 60.0;

pub(crate) fn apply_startup_states<Kind>(
    context: &SongLuaCompileContext,
    overlays: &mut [SongLuaOverlayCompileActor<Kind>],
    states: &std::collections::HashMap<usize, crate::lua_util::SongLuaStartupState>,
    messages: &mut Vec<SongLuaMessageEvent>,
) {
    const MESSAGE: &str = "__songlua_queued_startup";
    let beat = song_beat_at_elapsed_seconds(1.0 / SONG_LUA_UPDATE_REFERENCE_FPS, context);
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
                aux: None,
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
                aux: None,
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

fn startup_command(
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
        aux: None,
        blocks,
    })
}

const PLAYER_TRANSFORM_CAPTURE_KEYS: [&str; 11] = [
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
const PLAYER_TRANSFORM_TARGETS: [SongLuaOverlayUpdateTarget; 11] = {
    use SongLuaOverlayUpdateTarget as Target;
    [
        Target::X,
        Target::Y,
        Target::Z,
        Target::RotationX,
        Target::RotationZ,
        Target::RotationY,
        Target::ZoomX,
        Target::ZoomY,
        Target::ZoomZ,
        Target::SkewX,
        Target::SkewY,
    ]
};

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

trait ModState {
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
struct ModSnapshot(Box<[(Rc<str>, f32)]>);

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

#[derive(Default)]
struct ModSnapshotScratch {
    names: FxHashSet<Rc<str>>,
    lua_names: FxHashMap<usize, (mlua::LuaString, Rc<str>)>,
    entries: Vec<(Rc<str>, f32)>,
}

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

struct SnapshotName(mlua::LuaString);

impl mlua::FromLua for SnapshotName {
    fn from_lua(value: Value, lua: &Lua) -> mlua::Result<Self> {
        let name = mlua::LuaString::from_lua(value, lua).map_err(snapshot_name_error)?;
        Ok(Self(name))
    }
}

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

    fn states(
        &mut self,
        tables: &[Table; LUA_PLAYERS],
    ) -> Result<[ModSnapshot; LUA_PLAYERS], String> {
        Ok([self.sample(&tables[0])?, self.sample(&tables[1])?])
    }

    fn player_speeds(
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

pub fn current_perframe_player_states(
    player_tables: &[Option<Table>; LUA_PLAYERS],
) -> Result<[SongLuaPerframePlayerState; LUA_PLAYERS], String> {
    let mut out = [SongLuaPerframePlayerState::default(); LUA_PLAYERS];
    for player in 0..LUA_PLAYERS {
        let Some(actor) = player_tables[player].as_ref() else {
            continue;
        };
        out[player] = actor_perframe_player_state(actor)?;
    }
    Ok(out)
}

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

pub fn current_update_mod_states(
    tables: &[Table; LUA_PLAYERS],
) -> Result<[SongLuaUpdateModState; LUA_PLAYERS], String> {
    Ok([
        player_option_sample(&tables[0])?,
        player_option_sample(&tables[1])?,
    ])
}

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

fn perframe_sample_iter(start: f32, end: f32) -> impl Iterator<Item = SongLuaPerframeSample> {
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
        last_windows,
        &mut (0, String::new()),
        SongLuaTimeUnit::Beat,
    );
}

fn push_update_mod_targets_with_key<S: ModState>(
    out: &mut Vec<SongLuaEaseWindow>,
    start: f32,
    end: f32,
    from_players: &[S; LUA_PLAYERS],
    to_players: &[S; LUA_PLAYERS],
    baseline_players: &[S; LUA_PLAYERS],
    speeds: &[S; LUA_PLAYERS],
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
            // These switches have no native approach speed, but initial values and
            // later writes are still step targets, never interpolated samples.
            if speed.is_some()
                || matches!(
                    key,
                    "cosecant" | "dizzyholds" | "stealthtype" | "zbuffer" | "modtimersetting"
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

pub fn current_overlay_compile_actor_states<Kind>(
    overlays: &[SongLuaOverlayCompileActor<Kind>],
) -> Result<Vec<SongLuaOverlayState>, String> {
    let mut out = Vec::with_capacity(overlays.len());
    for overlay in overlays {
        out.push(actor_overlay_initial_state(&overlay.table)?);
    }
    Ok(out)
}

// Close an idle interval before a command that has already changed the current
// state. Keep that command at its original frame rather than ramping across the
// idle interval or delaying it until the following capture frame.
fn push_overlay_gap(
    samples: &mut Vec<SongLuaOverlayUpdateSample>,
    time: f32,
    next_time: f32,
    current: impl FnOnce() -> SongLuaOverlayUpdateValue,
) {
    let Some(last) = samples.last() else {
        return;
    };
    if last.time >= time - f32::EPSILON {
        return;
    }
    let current = current();
    let hold_time = time - (next_time - time);
    if last.value != current && hold_time > last.time + f32::EPSILON {
        let value = last.value.clone();
        samples.push(SongLuaOverlayUpdateSample {
            time: hold_time,
            value,
        });
    }
    samples.push(SongLuaOverlayUpdateSample {
        time,
        value: current,
    });
}

fn push_update_overlay_value(
    tracks: &mut Vec<SongLuaOverlayUpdateTrack>,
    track_indices: &mut std::collections::HashMap<
        (usize, crate::SongLuaOverlayUpdateTarget),
        usize,
        impl std::hash::BuildHasher,
    >,
    overlay_index: usize,
    target: crate::SongLuaOverlayUpdateTarget,
    time: f32,
    current: crate::SongLuaOverlayUpdateValue,
    next_time: f32,
    next: crate::SongLuaOverlayUpdateValue,
) -> usize {
    let key = (overlay_index, target);
    let track_index = match track_indices.get(&key).copied() {
        Some(index) => index,
        None => {
            let index = tracks.len();
            track_indices.insert(key, index);
            tracks.push(SongLuaOverlayUpdateTrack {
                overlay_index,
                target,
                // A target enters capture only when UpdateCommand writes it.
                // Do not invent a ramp from the actor default before that first
                // write; the runtime applies the first sampled value as a step.
                samples: vec![SongLuaOverlayUpdateSample {
                    time: next_time,
                    value: next,
                }],
            });
            return index;
        }
    };
    if current == next {
        return track_index;
    }
    let track = &mut tracks[track_index];
    push_overlay_gap(&mut track.samples, time, next_time, || current);
    track.samples.push(SongLuaOverlayUpdateSample {
        time: next_time,
        value: next,
    });
    track_index
}

#[allow(clippy::too_many_arguments)]
fn push_captured_overlay_value(
    tracks: &mut Vec<SongLuaOverlayUpdateTrack>,
    track_indices: &mut std::collections::HashMap<
        (usize, SongLuaOverlayUpdateTarget),
        usize,
        impl std::hash::BuildHasher,
    >,
    overlay_index: usize,
    target: SongLuaOverlayUpdateTarget,
    time: f32,
    current: &SongLuaOverlayState,
    next_time: f32,
    next: &SongLuaOverlayUpdateValue,
) -> usize {
    let key = (overlay_index, target);
    let track_index = match track_indices.get(&key).copied() {
        Some(index) => index,
        None => {
            let index = tracks.len();
            track_indices.insert(key, index);
            // The first write is a step. Its unused current value needs no
            // snapshot, including when that value contains vertex colors.
            tracks.push(SongLuaOverlayUpdateTrack {
                overlay_index,
                target,
                samples: vec![SongLuaOverlayUpdateSample {
                    time: next_time,
                    value: next.clone(),
                }],
            });
            return index;
        }
    };
    if overlay_state_matches_update_value(current, target, next)
        && tracks[track_index]
            .samples
            .last()
            .is_some_and(|sample| &sample.value == next)
    {
        return track_index;
    }
    let track = &mut tracks[track_index];
    push_overlay_gap(&mut track.samples, time, next_time, || {
        overlay_state_update_value(current, target)
    });
    track.samples.push(SongLuaOverlayUpdateSample {
        time: next_time,
        value: next.clone(),
    });
    track_index
}

fn overlay_state_update_value(
    state: &SongLuaOverlayState,
    target: crate::SongLuaOverlayUpdateTarget,
) -> crate::SongLuaOverlayUpdateValue {
    use crate::{SongLuaOverlayUpdateTarget as Target, SongLuaOverlayUpdateValue as Value};
    macro_rules! value {
        ($variant:ident, $field:ident) => {
            Value::$variant(state.$field)
        };
    }
    macro_rules! option {
        ($variant:ident, $field:ident) => {
            state.$field.map(Value::$variant).unwrap_or(Value::None)
        };
    }
    match target {
        Target::X => value!(F32, x),
        Target::Y => value!(F32, y),
        Target::Z => value!(F32, z),
        Target::ZBias => value!(F32, z_bias),
        Target::DrawOrder => value!(I32, draw_order),
        Target::DrawByZPosition => value!(Bool, draw_by_z_position),
        Target::AftCreated => value!(Bool, aft_created),
        Target::AftPreserve => value!(Bool, aft_preserve),
        Target::HAlign => value!(F32, halign),
        Target::VAlign => value!(F32, valign),
        Target::TextAlign => value!(TextAlign, text_align),
        Target::Uppercase => value!(Bool, uppercase),
        Target::ShadowLen => value!(Vec2, shadow_len),
        Target::ShadowColor => value!(Vec4, shadow_color),
        Target::Glow => value!(Vec4, glow),
        Target::Fov => option!(F32, fov),
        Target::Vanishpoint => option!(Vec2, vanishpoint),
        Target::Diffuse => value!(Vec4, diffuse),
        Target::VertexColors => state
            .vertex_colors
            .map(|value| Value::VertexColors(std::sync::Arc::new(value)))
            .unwrap_or(Value::None),
        Target::Visible => value!(Bool, visible),
        Target::CropLeft => value!(F32, cropleft),
        Target::CropRight => value!(F32, cropright),
        Target::CropTop => value!(F32, croptop),
        Target::CropBottom => value!(F32, cropbottom),
        Target::FadeLeft => value!(F32, fadeleft),
        Target::FadeRight => value!(F32, faderight),
        Target::FadeTop => value!(F32, fadetop),
        Target::FadeBottom => value!(F32, fadebottom),
        Target::MaskSource => value!(Bool, mask_source),
        Target::MaskDest => value!(Bool, mask_dest),
        Target::DepthTest => value!(Bool, depth_test),
        Target::Zoom => value!(F32, zoom),
        Target::ZoomX => value!(F32, zoom_x),
        Target::ZoomY => value!(F32, zoom_y),
        Target::ZoomZ => value!(F32, zoom_z),
        Target::BaseZoom => value!(F32, basezoom),
        Target::BaseZoomX => value!(F32, basezoom_x),
        Target::BaseZoomY => value!(F32, basezoom_y),
        Target::BaseZoomZ => value!(F32, basezoom_z),
        Target::RotationX => value!(F32, rot_x_deg),
        Target::RotationY => value!(F32, rot_y_deg),
        Target::RotationZ => value!(F32, rot_z_deg),
        Target::BaseRotation => value!(Vec3, base_rotation),
        Target::SkewX => value!(F32, skew_x),
        Target::SkewY => value!(F32, skew_y),
        Target::Blend => value!(Blend, blend),
        Target::Vibrate => value!(Bool, vibrate),
        Target::EffectMagnitude => value!(Vec3, effect_magnitude),
        Target::EffectClock => value!(EffectClock, effect_clock),
        Target::EffectTimer => value!(Bool, effect_timer),
        Target::EffectMode => value!(EffectMode, effect_mode),
        Target::EffectColor1 => value!(Vec4, effect_color1),
        Target::EffectColor2 => value!(Vec4, effect_color2),
        Target::EffectPeriod => value!(F32, effect_period),
        Target::EffectOffset => value!(F32, effect_offset),
        Target::EffectTime => option!(Vec2, effect_time),
        Target::EffectTiming => option!(Vec5, effect_timing),
        Target::Rainbow => value!(Bool, rainbow),
        Target::RainbowScroll => value!(Bool, rainbow_scroll),
        Target::TextJitter => value!(Bool, text_jitter),
        Target::TextDistortion => value!(F32, text_distortion),
        Target::TextGlowMode => value!(TextGlowMode, text_glow_mode),
        Target::MultAttrsWithDiffuse => value!(Bool, mult_attrs_with_diffuse),
        Target::SpriteTexture => value!(Bool, sprite_texture),
        Target::SpriteAnimate => value!(Bool, sprite_animate),
        Target::SpriteLoop => value!(Bool, sprite_loop),
        Target::SpritePlaybackRate => value!(F32, sprite_playback_rate),
        Target::SpriteStateDelay => value!(F32, sprite_state_delay),
        Target::SpriteStateIndex => option!(U32, sprite_state_index),
        Target::VertSpacing => option!(I32, vert_spacing),
        Target::WrapWidthPixels => option!(I32, wrap_width_pixels),
        Target::MaxWidth => option!(F32, max_width),
        Target::MaxHeight => option!(F32, max_height),
        Target::MaxWPreZoom => value!(Bool, max_w_pre_zoom),
        Target::MaxHPreZoom => value!(Bool, max_h_pre_zoom),
        Target::MaxDimensionUsesZoom => value!(Bool, max_dimension_uses_zoom),
        Target::TextureFiltering => value!(Bool, texture_filtering),
        Target::TextureWrapping => value!(Bool, texture_wrapping),
        Target::TexcoordOffset => option!(Vec2, texcoord_offset),
        Target::CustomTextureRect => option!(Vec4, custom_texture_rect),
        Target::TexcoordVelocity => option!(Vec2, texcoord_velocity),
        Target::Size => option!(Vec2, size),
        Target::StretchRect => option!(Vec4, stretch_rect),
    }
}

fn overlay_state_matches_update_value(
    state: &SongLuaOverlayState,
    target: SongLuaOverlayUpdateTarget,
    value: &SongLuaOverlayUpdateValue,
) -> bool {
    if target == SongLuaOverlayUpdateTarget::VertexColors {
        // Compare the state's inline colors with the existing track's owned
        // colors. Only a changed track needs new owned samples.
        return match value {
            SongLuaOverlayUpdateValue::None => state.vertex_colors.is_none(),
            SongLuaOverlayUpdateValue::VertexColors(colors) => {
                state.vertex_colors.as_ref() == Some(colors.as_ref())
            }
            _ => false,
        };
    }
    overlay_state_update_value(state, target) == *value
}

fn set_overlay_state_update_value(
    state: &mut SongLuaOverlayState,
    target: crate::SongLuaOverlayUpdateTarget,
    value: &crate::SongLuaOverlayUpdateValue,
) {
    use crate::{SongLuaOverlayUpdateTarget as Target, SongLuaOverlayUpdateValue as Value};
    macro_rules! set_value {
        ($target:ident, $variant:ident, $field:ident) => {
            if target == Target::$target {
                if let Value::$variant(value) = value {
                    state.$field = *value;
                }
                return;
            }
        };
    }
    macro_rules! set_option {
        ($target:ident, $variant:ident, $field:ident) => {
            if target == Target::$target {
                state.$field = match value {
                    Value::$variant(value) => Some(*value),
                    Value::None => None,
                    _ => state.$field,
                };
                return;
            }
        };
    }
    set_value!(X, F32, x);
    set_value!(Y, F32, y);
    set_value!(Z, F32, z);
    set_value!(ZBias, F32, z_bias);
    set_value!(DrawOrder, I32, draw_order);
    set_value!(DrawByZPosition, Bool, draw_by_z_position);
    set_value!(AftCreated, Bool, aft_created);
    set_value!(AftPreserve, Bool, aft_preserve);
    set_value!(HAlign, F32, halign);
    set_value!(VAlign, F32, valign);
    set_value!(TextAlign, TextAlign, text_align);
    set_value!(Uppercase, Bool, uppercase);
    set_value!(ShadowLen, Vec2, shadow_len);
    set_value!(ShadowColor, Vec4, shadow_color);
    set_value!(Glow, Vec4, glow);
    set_option!(Fov, F32, fov);
    set_option!(Vanishpoint, Vec2, vanishpoint);
    set_value!(Diffuse, Vec4, diffuse);
    if target == Target::VertexColors {
        state.vertex_colors = match value {
            Value::VertexColors(value) => Some(**value),
            Value::None => None,
            _ => state.vertex_colors,
        };
        return;
    }
    set_value!(Visible, Bool, visible);
    set_value!(CropLeft, F32, cropleft);
    set_value!(CropRight, F32, cropright);
    set_value!(CropTop, F32, croptop);
    set_value!(CropBottom, F32, cropbottom);
    set_value!(FadeLeft, F32, fadeleft);
    set_value!(FadeRight, F32, faderight);
    set_value!(FadeTop, F32, fadetop);
    set_value!(FadeBottom, F32, fadebottom);
    set_value!(MaskSource, Bool, mask_source);
    set_value!(MaskDest, Bool, mask_dest);
    set_value!(DepthTest, Bool, depth_test);
    set_value!(Zoom, F32, zoom);
    set_value!(ZoomX, F32, zoom_x);
    set_value!(ZoomY, F32, zoom_y);
    set_value!(ZoomZ, F32, zoom_z);
    set_value!(BaseZoom, F32, basezoom);
    set_value!(BaseZoomX, F32, basezoom_x);
    set_value!(BaseZoomY, F32, basezoom_y);
    set_value!(BaseZoomZ, F32, basezoom_z);
    set_value!(RotationX, F32, rot_x_deg);
    set_value!(RotationY, F32, rot_y_deg);
    set_value!(RotationZ, F32, rot_z_deg);
    set_value!(BaseRotation, Vec3, base_rotation);
    set_value!(SkewX, F32, skew_x);
    set_value!(SkewY, F32, skew_y);
    set_value!(Blend, Blend, blend);
    set_value!(Vibrate, Bool, vibrate);
    set_value!(EffectMagnitude, Vec3, effect_magnitude);
    set_value!(EffectClock, EffectClock, effect_clock);
    set_value!(EffectTimer, Bool, effect_timer);
    set_value!(EffectMode, EffectMode, effect_mode);
    set_value!(EffectColor1, Vec4, effect_color1);
    set_value!(EffectColor2, Vec4, effect_color2);
    set_value!(EffectPeriod, F32, effect_period);
    set_value!(EffectOffset, F32, effect_offset);
    set_option!(EffectTime, Vec2, effect_time);
    set_option!(EffectTiming, Vec5, effect_timing);
    set_value!(Rainbow, Bool, rainbow);
    set_value!(RainbowScroll, Bool, rainbow_scroll);
    set_value!(TextJitter, Bool, text_jitter);
    set_value!(TextDistortion, F32, text_distortion);
    set_value!(TextGlowMode, TextGlowMode, text_glow_mode);
    set_value!(MultAttrsWithDiffuse, Bool, mult_attrs_with_diffuse);
    set_value!(SpriteTexture, Bool, sprite_texture);
    set_value!(SpriteAnimate, Bool, sprite_animate);
    set_value!(SpriteLoop, Bool, sprite_loop);
    set_value!(SpritePlaybackRate, F32, sprite_playback_rate);
    set_value!(SpriteStateDelay, F32, sprite_state_delay);
    set_option!(SpriteStateIndex, U32, sprite_state_index);
    set_option!(VertSpacing, I32, vert_spacing);
    set_option!(WrapWidthPixels, I32, wrap_width_pixels);
    set_option!(MaxWidth, F32, max_width);
    set_option!(MaxHeight, F32, max_height);
    set_value!(MaxWPreZoom, Bool, max_w_pre_zoom);
    set_value!(MaxHPreZoom, Bool, max_h_pre_zoom);
    set_value!(MaxDimensionUsesZoom, Bool, max_dimension_uses_zoom);
    set_value!(TextureFiltering, Bool, texture_filtering);
    set_value!(TextureWrapping, Bool, texture_wrapping);
    set_option!(TexcoordOffset, Vec2, texcoord_offset);
    set_option!(CustomTextureRect, Vec4, custom_texture_rect);
    set_option!(TexcoordVelocity, Vec2, texcoord_velocity);
    set_option!(Size, Vec2, size);
    set_option!(StretchRect, Vec4, stretch_rect);
}

// Scratch belongs to one compilation and retains only its high-water storage.
// Track indices stay stable because update tracks are append-only during capture.
#[derive(Default)]
struct OverlaySampleScratch {
    completed: Vec<SongLuaScheduledOverlaySample>,
    blocked_actors: Vec<bool>,
    reset_indices: Vec<usize>,
    stopped_indices: Vec<usize>,
    captured_tracks: Vec<bool>,
    message_targets: Vec<(
        usize,
        SongLuaOverlayUpdateTarget,
        SongLuaOverlayUpdateValue,
        SongLuaOverlayUpdateValue,
    )>,
    retargeted_states: Vec<(usize, SongLuaOverlayUpdateTarget, SongLuaOverlayUpdateValue)>,
}

fn retarget_actor_tween(
    scheduled: &mut Vec<SongLuaScheduledOverlaySample>,
    overlay_index: usize,
    target: SongLuaOverlayUpdateTarget,
    value: &SongLuaOverlayUpdateValue,
    current: &SongLuaOverlayState,
    seconds: f64,
) -> Option<SongLuaOverlayUpdateValue> {
    // Keep current-state sampling, back-tween mutation and inherited axes in
    // one state transition: a setter must never restart the active tween.
    if !matches!(
        target,
        SongLuaOverlayUpdateTarget::Zoom | SongLuaOverlayUpdateTarget::Diffuse
    ) && !PLAYER_TRANSFORM_TARGETS.contains(&target)
    {
        return None;
    }
    let pending = scheduled.iter().rposition(|sample| {
        sample.overlay_index == overlay_index
            && sample.end_seconds > scheduled_overlay_clock(sample, seconds)
    })?;
    let (start, end) = (
        scheduled[pending].start_seconds,
        scheduled[pending].end_seconds,
    );
    let rendered = scheduled
        .iter()
        .rev()
        .find(|sample| {
            sample.overlay_index == overlay_index
                && sample.target == target
                && sample
                    .dispatch_seconds
                    .is_none_or(|dispatch| dispatch <= seconds + 1.0e-7)
                && sample.start_seconds <= scheduled_overlay_clock(sample, seconds)
                && sample.end_seconds > scheduled_overlay_clock(sample, seconds)
        })
        .map_or_else(
            || overlay_state_update_value(current, target),
            |sample| {
                lerp_scheduled_value(
                    &sample.from,
                    // A setter changes the back tween. Earlier active tweens
                    // retain their own destinations on this frame.
                    if sample.frame_advance > 0.0
                        && sample.start_seconds == start
                        && sample.end_seconds == end
                    {
                        value
                    } else {
                        &sample.value
                    },
                    scheduled_overlay_factor(sample, scheduled_overlay_clock(sample, seconds)),
                )
            },
        );
    if let Some(sample) = scheduled.iter_mut().rev().find(|sample| {
        sample.overlay_index == overlay_index
            && sample.target == target
            && sample.start_seconds == start
            && sample.end_seconds == end
    }) {
        sample.value = value.clone();
        return Some(rendered);
    }
    let pending = &scheduled[pending];
    let from = scheduled
        .iter()
        .rev()
        .find(|sample| {
            sample.overlay_index == overlay_index
                && sample.target == target
                && sample.end_seconds <= pending.start_seconds
        })
        .map_or_else(
            || overlay_state_update_value(current, target),
            |sample| sample.value.clone(),
        );
    let replacement = SongLuaScheduledOverlaySample {
        progress: pending.progress.clone(),
        duration: pending.duration,
        dispatch_seconds: pending.dispatch_seconds,
        frame_advance: pending.frame_advance,
        overlay_index,
        target,
        start_seconds: pending.start_seconds,
        end_seconds: pending.end_seconds,
        start_beat: pending.start_beat,
        end_beat: pending.end_beat,
        easing: pending.easing.clone(),
        opt1: pending.opt1,
        from,
        value: value.clone(),
    };
    scheduled.push(replacement);
    if scheduled.last().is_some_and(|sample| {
        sample.frame_advance > 0.0
            && sample.start_seconds <= scheduled_overlay_clock(sample, seconds)
    }) {
        let sample = scheduled.last().expect("replacement appended");
        return Some(lerp_scheduled_value(
            &sample.from,
            &sample.value,
            scheduled_overlay_factor(sample, scheduled_overlay_clock(sample, seconds)),
        ));
    }
    Some(rendered)
}

fn append_scheduled_overlay_updates(
    scheduled_samples: &mut Vec<SongLuaScheduledOverlaySample>,
    context: &SongLuaCompileContext,
    update_states: &[SongLuaOverlayState],
    overlay_index: usize,
    scheduled: &[crate::lua_util::SongLuaScheduledOverlayUpdate],
    message_seconds: f64,
) {
    if scheduled.is_empty() {
        return;
    }
    // Targets are contiguous enum discriminants through StretchRect.
    // Borrow prior writes; only emitted samples need owned values.
    let mut scheduled_values: [Option<&SongLuaOverlayUpdateValue>;
        SongLuaOverlayUpdateTarget::StretchRect as usize + 1] =
        [None; SongLuaOverlayUpdateTarget::StretchRect as usize + 1];
    let mut previous_times: Option<((u32, u32), (f32, f32))> = None;
    let reuse_times = scheduled.len() > 1;
    for update in scheduled {
        let (start_seconds, end_seconds) = if let Some(samples) = &update.progress {
            let start = samples.first().expect("baked tween has frames")[0];
            let end = samples.last().expect("baked tween has frames");
            (f64::from(start), f64::from(end[0] + end[1]))
        } else {
            let start = message_seconds + f64::from(update.delay_seconds);
            (start, start + f64::from(update.duration_seconds))
        };
        let advance = if update.progress.is_some() {
            0.0
        } else {
            update.frame_advance
        };
        let start = (start_seconds - f64::from(advance)) as f32;
        let end = (end_seconds - f64::from(advance)) as f32;
        let time_bits = (start.to_bits(), end.to_bits());
        // Adjacent properties in one tween share their time bounds. Keep only
        // the preceding pair, keyed by bits to retain signed zero and NaNs.
        let (start_beat, end_beat) = if reuse_times {
            match previous_times {
                Some((bits, beats)) if bits == time_bits => beats,
                _ => {
                    let beats = (
                        song_beat_at_elapsed_seconds(start, context),
                        song_beat_at_elapsed_seconds(end, context),
                    );
                    previous_times = Some((time_bits, beats));
                    beats
                }
            }
        } else {
            (
                song_beat_at_elapsed_seconds(start, context),
                song_beat_at_elapsed_seconds(end, context),
            )
        };
        let from = scheduled_values[update.target as usize]
            .cloned()
            .or_else(|| update.initial_value.clone())
            .or_else(|| {
                update_states
                    .get(overlay_index)
                    .map(|state| overlay_state_update_value(state, update.target))
            })
            .unwrap_or(SongLuaOverlayUpdateValue::None);
        scheduled_samples.push(SongLuaScheduledOverlaySample {
            progress: update.progress.clone(),
            duration: update.duration_seconds,
            dispatch_seconds: update.dispatch_seconds,
            frame_advance: update.frame_advance,
            overlay_index,
            target: update.target,
            start_seconds,
            end_seconds,
            start_beat,
            end_beat,
            easing: update.easing.clone(),
            opt1: update.opt1,
            from,
            value: update.value.clone(),
        });
        scheduled_values[update.target as usize] = Some(&update.value);
    }
}

fn apply_captured_final_values(
    update_states: &mut [SongLuaOverlayState],
    to_states: &mut [SongLuaOverlayState],
    overlay_index: usize,
    scheduled: &[crate::lua_util::SongLuaScheduledOverlayUpdate],
    final_values: &[(SongLuaOverlayUpdateTarget, SongLuaOverlayUpdateValue)],
    restored_indices: &[usize],
) {
    // Small batches retain their original short scan. Wide batches reconcile
    // target membership once, without heap storage or a branch per target.
    if scheduled.len() < 16 || final_values.len() < 16 {
        for (target, value) in final_values {
            if scheduled.iter().any(|update| update.target == *target) {
                continue;
            }
            if let Some(state) = update_states.get_mut(overlay_index) {
                set_overlay_state_update_value(state, *target, value);
            }
            if restored_indices.binary_search(&overlay_index).is_err()
                && let Some(state) = to_states.get_mut(overlay_index)
            {
                set_overlay_state_update_value(state, *target, value);
            }
        }
        return;
    }
    const _: () = assert!((SongLuaOverlayUpdateTarget::StretchRect as usize) < 128);
    let scheduled_targets = scheduled.iter().fold(0_u128, |mask, update| {
        mask | (1_u128 << update.target as usize)
    });
    let restored = restored_indices.binary_search(&overlay_index).is_ok();
    for (target, value) in final_values {
        if scheduled_targets & (1_u128 << *target as usize) != 0 {
            continue;
        }
        if let Some(state) = update_states.get_mut(overlay_index) {
            set_overlay_state_update_value(state, *target, value);
        }
        if !restored && let Some(state) = to_states.get_mut(overlay_index) {
            set_overlay_state_update_value(state, *target, value);
        }
    }
}

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
    if lua
        .app_data_ref::<crate::lua_util::SongLuaCompileFrames>()
        .is_some()
    {
        scratch.completed.clear();
        scratch.blocked_actors.resize(overlays.len(), false);
        scratch.blocked_actors.fill(false);
        scratch
            .completed
            .extend(scheduled_samples.extract_if(.., |sample| {
                // A later zero-time state cannot pass its pending actor tween
                // when cursor rounding puts that state's endpoint first.
                if scratch.blocked_actors[sample.overlay_index] {
                    return false;
                }
                let complete = sample
                    .dispatch_seconds
                    .is_none_or(|dispatch| dispatch <= next_seconds + 1.0e-7)
                    && sample.end_seconds <= scheduled_overlay_clock(sample, next_seconds);
                scratch.blocked_actors[sample.overlay_index] = !complete;
                complete
            }));
        // Actor queues are captured in enqueue order. Rounded cursor times
        // can put a trailing zero-time state just before its preceding tween;
        // sorting completed endpoints would then restore the wrong state.
        for sample in &scratch.completed {
            set_overlay_state_update_value(
                &mut update_states[sample.overlay_index],
                sample.target,
                &sample.value,
            );
            set_overlay_state_update_value(
                &mut to_states[sample.overlay_index],
                sample.target,
                &sample.value,
            );
            push_captured_overlay_value(
                tracks,
                track_indices,
                sample.overlay_index,
                sample.target,
                time,
                &from_states[sample.overlay_index],
                next_time,
                &sample.value,
            );
        }
    } else {
        merge_completed_scheduled_overlay_samples_into(
            tracks,
            track_indices,
            baseline,
            update_states,
            to_states,
            scheduled_samples,
            next_seconds,
            &mut scratch.completed,
        );
    }
    scratch.reset_indices.clear();
    scratch.stopped_indices.clear();
    scratch.captured_tracks.clear();
    scratch.captured_tracks.resize(tracks.len(), false);
    scratch.message_targets.clear();
    scratch.retargeted_states.clear();
    let OverlaySampleScratch {
        reset_indices,
        stopped_indices,
        captured_tracks,
        message_targets,
        retargeted_states,
        ..
    } = scratch;
    crate::lua_util::drain_overlay_update_capture(
        lua,
        |overlay_index, values, scheduled, final_values, tween_reset| {
            let Some(baseline) = baseline.get(overlay_index) else {
                return Ok(());
            };
            debug_assert!(overlay_index < overlays.len());
            reset_indices.push(overlay_index);
            if tween_reset {
                stopped_indices.push(overlay_index);
                for sample in scheduled_samples
                    .iter()
                    .filter(|sample| sample.overlay_index == overlay_index)
                {
                    let clock = scheduled_overlay_clock(sample, next_seconds);
                    let current = if clock >= sample.start_seconds
                        && sample
                            .dispatch_seconds
                            .is_none_or(|dispatch| dispatch <= next_seconds + 1.0e-7)
                    {
                        lerp_scheduled_value(
                            &sample.from,
                            &sample.value,
                            scheduled_overlay_factor(sample, clock),
                        )
                    } else {
                        overlay_state_update_value(&from_states[overlay_index], sample.target)
                    };
                    set_overlay_state_update_value(
                        &mut update_states[overlay_index],
                        sample.target,
                        &current,
                    );
                    set_overlay_state_update_value(
                        &mut to_states[overlay_index],
                        sample.target,
                        &current,
                    );
                    push_captured_overlay_value(
                        tracks,
                        track_indices,
                        overlay_index,
                        sample.target,
                        time,
                        &from_states[overlay_index],
                        next_time,
                        &current,
                    );
                }
                scheduled_samples.retain(|sample| sample.overlay_index != overlay_index);
            }
            let actor = overlays[overlay_index].borrow();
            let player = actor
                .raw_get::<Option<i64>>("__songlua_player_index")
                .map_err(|err| err.to_string())?
                .is_some()
                && actor
                    .raw_get::<Option<String>>("__songlua_player_child_name")
                    .map_err(|err| err.to_string())?
                    .is_none();
            if player
                || lua
                    .app_data_ref::<crate::lua_util::SongLuaCompileFrames>()
                    .is_some()
            {
                // Player Lua getters retain destinations. Their render state
                // must also retain immediate writes before a delayed return.
                for (target, value) in values {
                    if let Some(rendered) = retarget_actor_tween(
                        scheduled_samples,
                        overlay_index,
                        *target,
                        value,
                        from_states.get(overlay_index).unwrap_or(baseline),
                        next_seconds,
                    ) {
                        // Actor setters modify the back tween's destination.
                        // This frame has already advanced its current state.
                        retargeted_states.push((overlay_index, *target, rendered));
                    }
                    if let Some(state) = update_states.get_mut(overlay_index) {
                        set_overlay_state_update_value(state, *target, value);
                    }
                    if let Some(state) = to_states.get_mut(overlay_index) {
                        set_overlay_state_update_value(state, *target, value);
                    }
                }
            }
            apply_captured_final_values(
                update_states,
                to_states,
                overlay_index,
                scheduled,
                final_values,
                restored_indices,
            );
            for (target, next) in values {
                let current = from_states.get(overlay_index).unwrap_or(baseline);
                let rendered = retargeted_states
                    .iter()
                    .rev()
                    .find(|(index, property, _)| *index == overlay_index && *property == *target)
                    .map(|(_, _, value)| value);
                let next = rendered.unwrap_or(next);
                let track_index = push_captured_overlay_value(
                    tracks,
                    track_indices,
                    overlay_index,
                    *target,
                    time,
                    current,
                    next_time,
                    next,
                );
                // Tracks only append. Mark unchanged writes too: they still
                // take precedence over a restored message on this tick.
                if track_index >= captured_tracks.len() {
                    captured_tracks.resize(track_index + 1, false);
                }
                captured_tracks[track_index] = true;
            }
            append_scheduled_overlay_updates(
                scheduled_samples,
                context,
                update_states,
                overlay_index,
                scheduled,
                next_seconds,
            );
            Ok(())
        },
    )?;
    message_targets.extend(
        track_indices
            .iter()
            .filter(|(_, index)| !captured_tracks[**index])
            .filter_map(|(&(overlay_index, target), &track_index)| {
                let current_state = from_states.get(overlay_index)?;
                let message_state = to_states.get(overlay_index)?;
                let tracked = tracks
                    .get(track_index)
                    .and_then(|track| track.samples.last())
                    .map(|sample| &sample.value)?;
                if overlay_state_matches_update_value(message_state, target, tracked) {
                    return None;
                }
                Some((
                    overlay_index,
                    target,
                    overlay_state_update_value(current_state, target),
                    overlay_state_update_value(message_state, target),
                ))
            }),
    );
    // Runtime update tracks are applied after message commands. Keep an existing
    // track synchronized when a message changes its target, otherwise a stale
    // sampled value can overwrite a later persistent action (notably Player
    // ActorProxy visibility in Step Your Game Up).
    for (overlay_index, target, current, message) in message_targets.drain(..) {
        push_update_overlay_value(
            tracks,
            track_indices,
            overlay_index,
            target,
            time,
            current,
            next_time,
            message,
        );
    }
    for &overlay_index in reset_indices.iter() {
        reset_actor_capture(lua, overlays[overlay_index].borrow())
            .map_err(|err| err.to_string())?;
    }
    Ok(())
}

#[cfg_attr(test, derive(Clone))]
struct SongLuaScheduledOverlaySample {
    progress: Option<std::sync::Arc<[[f32; 2]]>>,
    duration: f32,
    dispatch_seconds: Option<f64>,
    frame_advance: f32,
    overlay_index: usize,
    target: SongLuaOverlayUpdateTarget,
    start_seconds: f64,
    end_seconds: f64,
    start_beat: f32,
    end_beat: f32,
    easing: Option<String>,
    opt1: Option<f32>,
    from: SongLuaOverlayUpdateValue,
    value: SongLuaOverlayUpdateValue,
}

fn lerp_scheduled_value(
    from: &SongLuaOverlayUpdateValue,
    to: &SongLuaOverlayUpdateValue,
    factor: f32,
) -> SongLuaOverlayUpdateValue {
    use SongLuaOverlayUpdateValue as Value;
    let lerp = |from: f32, to: f32| crate::actor_lerp(from, to, factor);
    match (from, to) {
        (Value::F32(from), Value::F32(to)) => Value::F32(lerp(*from, *to)),
        (Value::Vec2(from), Value::Vec2(to)) => {
            Value::Vec2([lerp(from[0], to[0]), lerp(from[1], to[1])])
        }
        (Value::Vec3(from), Value::Vec3(to)) => Value::Vec3([
            lerp(from[0], to[0]),
            lerp(from[1], to[1]),
            lerp(from[2], to[2]),
        ]),
        (Value::Vec4(from), Value::Vec4(to)) => Value::Vec4([
            lerp(from[0], to[0]),
            lerp(from[1], to[1]),
            lerp(from[2], to[2]),
            lerp(from[3], to[3]),
        ]),
        _ => {
            if factor >= 1.0 {
                to.clone()
            } else {
                from.clone()
            }
        }
    }
}

#[inline(always)]
fn scheduled_overlay_factor(sample: &SongLuaScheduledOverlaySample, seconds: f64) -> f32 {
    let linear_factor = if let Some(frames) = &sample.progress {
        crate::tween_frame_factor(frames, sample.duration, seconds as f32).unwrap_or(0.0)
    } else if sample.end_seconds <= sample.start_seconds + f64::EPSILON {
        1.0
    } else {
        ((seconds - sample.start_seconds) / (sample.end_seconds - sample.start_seconds))
            .clamp(0.0, 1.0) as f32
    };
    crate::overlay_command_ease_factor(sample.easing.as_deref(), linear_factor, sample.opt1)
}

fn scheduled_overlay_clock(sample: &SongLuaScheduledOverlaySample, seconds: f64) -> f64 {
    if sample.progress.is_some() {
        // Native frame samples and Actor::Update use float timestamps. Compare
        // on that clock too, so a rounded-up zero-time state is not deferred.
        f64::from(seconds as f32)
    } else if sample.frame_advance == 0.0 {
        seconds
    } else {
        seconds + f64::from(sample.frame_advance)
    }
}

fn scheduled_overlay_value(
    sample: &SongLuaScheduledOverlaySample,
    factor: f32,
) -> SongLuaOverlayUpdateValue {
    if sample.duration <= 0.0 {
        // Zero-time destinations are copied, not interpolated. Even at 1.0,
        // subtraction and addition can change a timer period's float bits.
        sample.value.clone()
    } else {
        lerp_scheduled_value(&sample.from, &sample.value, factor)
    }
}

fn scheduled_clocks_match(
    left: &SongLuaScheduledOverlaySample,
    right: &SongLuaScheduledOverlaySample,
) -> bool {
    left.start_seconds.to_bits() == right.start_seconds.to_bits()
        && left.end_seconds.to_bits() == right.end_seconds.to_bits()
        && left.duration.to_bits() == right.duration.to_bits()
        && left.frame_advance == right.frame_advance
        && match (&left.progress, &right.progress) {
            (Some(left), Some(right)) => std::sync::Arc::ptr_eq(left, right),
            (None, None) => true,
            _ => false,
        }
}

fn apply_scheduled_overlay_states_uncached<Actor: std::borrow::Borrow<Table>>(
    lua: &Lua,
    overlays: &[Actor],
    states: &mut [SongLuaOverlayState],
    scheduled: &[SongLuaScheduledOverlaySample],
    seconds: f64,
) -> Result<(), String> {
    for sample in scheduled {
        let clock = scheduled_overlay_clock(sample, seconds);
        if sample
            .dispatch_seconds
            .is_some_and(|dispatch| seconds + 1.0e-7 < dispatch)
            || clock + f64::EPSILON < sample.start_seconds
        {
            continue;
        }
        let factor = scheduled_overlay_factor(sample, clock);
        let Some(state) = states.get_mut(sample.overlay_index) else {
            continue;
        };
        let value = scheduled_overlay_value(sample, factor);
        set_overlay_state_update_value(state, sample.target, &value);
        if let Some(overlay) = overlays.get(sample.overlay_index) {
            crate::lua_util::set_actor_overlay_update_getter_value(
                lua,
                overlay.borrow(),
                sample.target,
                &value,
            )?;
        }
    }
    Ok(())
}

fn apply_scheduled_overlay_states<Actor: std::borrow::Borrow<Table>>(
    lua: &Lua,
    overlays: &[Actor],
    states: &mut [SongLuaOverlayState],
    scheduled: &[SongLuaScheduledOverlaySample],
    seconds: f64,
) -> Result<(), String> {
    // Probe the first pair once. Homogeneous scalar/unique-timing batches
    // keep the original loop; grouped expensive tweens amortize the cache.
    let reuse_factors = if let [first, second, ..] = scheduled {
        matches!(first.easing.as_deref(), Some("spring" | "outElastic"))
            && scheduled_clocks_match(first, second)
            && first.easing == second.easing
            && (first.easing.as_deref() == Some("spring")
                || first.opt1.map(f32::to_bits) == second.opt1.map(f32::to_bits))
    } else {
        false
    };
    if !reuse_factors {
        return apply_scheduled_overlay_states_uncached(lua, overlays, states, scheduled, seconds);
    }
    let mut last_factor: Option<(&SongLuaScheduledOverlaySample, u8, f32)> = None;
    for sample in scheduled {
        let clock = scheduled_overlay_clock(sample, seconds);
        if sample
            .dispatch_seconds
            .is_some_and(|dispatch| seconds + 1.0e-7 < dispatch)
            || clock + f64::EPSILON < sample.start_seconds
        {
            continue;
        }
        // Spring/elastic curves use transcendental functions. Adjacent
        // properties of one tween share their factor; keep only the last one.
        let curve = match sample.easing.as_deref() {
            Some("spring") => 1,
            Some("outElastic") => 2,
            _ => 0,
        };
        let factor = if curve != 0 {
            if let Some((previous, previous_curve, factor)) = last_factor
                && scheduled_clocks_match(previous, sample)
                && previous_curve == curve
                && (curve == 1 || previous.opt1.map(f32::to_bits) == sample.opt1.map(f32::to_bits))
            {
                factor
            } else {
                let factor = scheduled_overlay_factor(sample, clock);
                last_factor = Some((sample, curve, factor));
                factor
            }
        } else {
            scheduled_overlay_factor(sample, clock)
        };
        let Some(state) = states.get_mut(sample.overlay_index) else {
            continue;
        };
        let value = scheduled_overlay_value(sample, factor);
        set_overlay_state_update_value(state, sample.target, &value);
        if let Some(overlay) = overlays.get(sample.overlay_index) {
            crate::lua_util::set_actor_overlay_update_getter_value(
                lua,
                overlay.borrow(),
                sample.target,
                &value,
            )?;
        }
    }
    Ok(())
}

#[cfg(test)]
fn merge_completed_scheduled_overlay_samples(
    tracks: &mut Vec<SongLuaOverlayUpdateTrack>,
    track_indices: &mut std::collections::HashMap<
        (usize, SongLuaOverlayUpdateTarget),
        usize,
        impl std::hash::BuildHasher,
    >,
    baseline: &[SongLuaOverlayState],
    update_states: &mut [SongLuaOverlayState],
    to_states: &mut [SongLuaOverlayState],
    scheduled: &mut Vec<SongLuaScheduledOverlaySample>,
    beat: f32,
) {
    merge_completed_scheduled_overlay_samples_into(
        tracks,
        track_indices,
        baseline,
        update_states,
        to_states,
        scheduled,
        f64::from(beat),
        &mut Vec::new(),
    );
}

#[allow(clippy::too_many_arguments)]
fn merge_completed_scheduled_overlay_samples_into(
    tracks: &mut Vec<SongLuaOverlayUpdateTrack>,
    track_indices: &mut std::collections::HashMap<
        (usize, SongLuaOverlayUpdateTarget),
        usize,
        impl std::hash::BuildHasher,
    >,
    baseline: &[SongLuaOverlayState],
    update_states: &mut [SongLuaOverlayState],
    to_states: &mut [SongLuaOverlayState],
    scheduled: &mut Vec<SongLuaScheduledOverlaySample>,
    seconds: f64,
    completed: &mut Vec<SongLuaScheduledOverlaySample>,
) {
    completed.clear();
    if scheduled.is_empty() {
        return;
    }
    // Keep pending tweens (and their capacity) in place across sample ticks.
    // The common case where none have completed does not allocate or move them.
    // Beats do not advance during a pause. Actor delays and tweens still do.
    completed.extend(scheduled.extract_if(.., |sample| {
        // Progress samples use a float song clock. Complete on that same
        // clock; a double comparison can leave a rounded-up empty tail alive
        // for one more frame and overwrite the next recurring tween.
        sample.end_seconds <= scheduled_overlay_clock(sample, seconds)
            && sample
                .dispatch_seconds
                .is_none_or(|dispatch| dispatch <= seconds + 1.0e-7)
    }));
    if !completed.is_empty() {
        completed.sort_by(|left, right| left.end_seconds.total_cmp(&right.end_seconds));
        for sample in completed.iter() {
            if let Some(state) = update_states.get_mut(sample.overlay_index) {
                set_overlay_state_update_value(state, sample.target, &sample.value);
            }
            if let Some(state) = to_states.get_mut(sample.overlay_index) {
                set_overlay_state_update_value(state, sample.target, &sample.value);
            }
        }
        merge_scheduled_overlay_samples_from_buffer(tracks, track_indices, baseline, completed);
    }
}

fn sort_overlay_update_samples(samples: &mut Vec<SongLuaOverlayUpdateSample>) {
    // Sample tracks are normally already ordered. Avoid the stable sort's
    // temporary allocation in that case; preserve stable ties when sorting.
    if !samples.is_sorted_by(|left, right| left.time.total_cmp(&right.time).is_le()) {
        samples.sort_by(|left, right| left.time.total_cmp(&right.time));
    }
    samples.dedup_by(|next, previous| {
        if (previous.time - next.time).abs() <= f32::EPSILON {
            // Keep the last value AND timestamp so epsilon-connected runs
            // collapse exactly as they do when replacing the last output item.
            std::mem::swap(previous, next);
            true
        } else {
            false
        }
    });
}

fn append_ordered_overlay_sample(
    samples: &mut Vec<SongLuaOverlayUpdateSample>,
    sample: SongLuaOverlayUpdateSample,
) {
    if let Some(last) = samples.last_mut()
        && (last.time - sample.time).abs() <= f32::EPSILON
    {
        *last = sample;
    } else {
        samples.push(sample);
    }
}

// The caller keeps samples in total_cmp order. Negative NaNs form a leading
// prefix; include them in the search partition, then reject them as values.
// Numeric comparison includes both signs of zero, matching the reverse scan.
fn overlay_sample_at_or_before(
    samples: &[SongLuaOverlayUpdateSample],
    time: f32,
) -> Option<&SongLuaOverlayUpdateSample> {
    let last = samples.last()?;
    if last.time <= time {
        return Some(last);
    }
    let earlier = &samples[..samples.len() - 1];
    // Short reverse scans time the binary-search setup. The last sample was
    // already rejected, so neither path needs to compare it again.
    if earlier.len() < 32 {
        return earlier.iter().rev().find(|sample| sample.time <= time);
    }
    let end = earlier.partition_point(|sample| {
        sample.time <= time || (sample.time.is_nan() && sample.time.is_sign_negative())
    });
    end.checked_sub(1)
        .and_then(|index| earlier.get(index))
        .filter(|sample| sample.time <= time)
}

fn merge_scheduled_overlay_samples_from_buffer(
    tracks: &mut Vec<SongLuaOverlayUpdateTrack>,
    track_indices: &mut std::collections::HashMap<
        (usize, SongLuaOverlayUpdateTarget),
        usize,
        impl std::hash::BuildHasher,
    >,
    baseline: &[SongLuaOverlayState],
    scheduled: &mut Vec<SongLuaScheduledOverlaySample>,
) {
    for track in tracks.iter_mut() {
        sort_overlay_update_samples(&mut track.samples);
    }
    scheduled.sort_by(|left, right| left.start_beat.total_cmp(&right.start_beat));
    for sample in scheduled.drain(..) {
        let track_index = *track_indices
            .entry((sample.overlay_index, sample.target))
            .or_insert_with(|| {
                let index = tracks.len();
                tracks.push(SongLuaOverlayUpdateTrack {
                    overlay_index: sample.overlay_index,
                    target: sample.target,
                    samples: vec![SongLuaOverlayUpdateSample {
                        time: 0.0,
                        value: overlay_state_update_value(
                            &baseline[sample.overlay_index],
                            sample.target,
                        ),
                    }],
                });
                index
            });
        let track = &mut tracks[track_index];
        let has_start = sample.end_beat > sample.start_beat + f32::EPSILON;
        // Step writes emit only their end value, so their start snapshot is
        // unused. Clone or construct one only when an anchor will own it.
        let current = has_start.then(|| {
            overlay_sample_at_or_before(&track.samples, sample.start_beat + f32::EPSILON)
                .map(|current| current.value.clone())
                .unwrap_or_else(|| {
                    overlay_state_update_value(&baseline[sample.overlay_index], sample.target)
                })
        });
        let first_beat = if has_start {
            sample.start_beat
        } else {
            sample.end_beat
        };
        let ordered = track
            .samples
            .last()
            .is_none_or(|last| last.time.total_cmp(&first_beat).is_le());
        if ordered {
            // The existing prefix is sorted and compacted. An ordered append
            // can only merge with its last sample, preserving last-write wins.
            if let Some(current) = current {
                append_ordered_overlay_sample(
                    &mut track.samples,
                    SongLuaOverlayUpdateSample {
                        time: sample.start_beat,
                        value: current,
                    },
                );
            }
            append_ordered_overlay_sample(
                &mut track.samples,
                SongLuaOverlayUpdateSample {
                    time: sample.end_beat,
                    value: sample.value,
                },
            );
            continue;
        }
        if let Some(current) = current {
            track.samples.push(SongLuaOverlayUpdateSample {
                time: sample.start_beat,
                value: current,
            });
        }
        track.samples.push(SongLuaOverlayUpdateSample {
            time: sample.end_beat,
            value: sample.value,
        });
        sort_overlay_update_samples(&mut track.samples);
    }
    // Every track was canonicalized above. Ordered appends keep that order
    // and compact ties; out-of-order appends canonicalize their track locally.
}

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
struct ColumnSplineCapture {
    points: crate::lua_util::ColumnSplineReadScratch,
    sampled: Vec<(usize, usize, deadsync_gameplay::SongLuaColumnSplineFrame)>,
    lanes: BTreeMap<(usize, usize), ColumnSplineLane>,
    bytes: usize,
}

impl ColumnSplineCapture {
    fn capture(&mut self, lua: &Lua, second: f32) -> Result<(), String> {
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

    fn finish(self, out: &mut Vec<deadsync_gameplay::SongLuaColumnSplineTrack>) {
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

pub fn compile_update_functions<Kind>(
    lua: &Lua,
    root: &Value,
    context: &SongLuaCompileContext,
    overlays: &mut [SongLuaOverlayCompileActor<Kind>],
    tracked_actors: &mut [SongLuaTrackedActor],
    messages: &[SongLuaMessageEvent],
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
    if !actor_tree_has_update_functions(lua, root).map_err(|err| err.to_string())? {
        spline_capture.finish(column_splines);
        return Ok((
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        ));
    }
    let start = song_beat_at_elapsed_seconds(0.0, context);
    let end = update_function_end_beat(context);
    if end <= start {
        spline_capture.finish(column_splines);
        return Ok((
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        ));
    }

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
            set_actor_overlay_getter_state(lua, &overlay.table, overlay.actor.initial_state)?;
        }
        if overlay
            .table
            .raw_get::<Option<Table>>("__songlua_state_motion_clock")
            .map_err(|err| err.to_string())?
            .is_some()
        {
            // Probing queued bodies leaves their future effect state in Lua.
            // Chronological replay begins with the state after Init/On.
            set_actor_overlay_getter_state(lua, &overlay.table, overlay.actor.initial_state)?;
        }
    }
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
            baseline_overlays.push(actor_overlay_initial_state(actor)?);
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
    let mut message_replay = SongLuaPerframeMessageReplay::new(messages, overlays.len());
    #[cfg(feature = "test-support")]
    lua.set_app_data(crate::song_tables::SongLuaBoolWrites::default());
    let mut replay_overlays = baseline_overlays.clone();
    let started = message_replay.advance(lua, context, overlays, &mut replay_overlays, start)?;
    restore_started_message_states(lua, overlays, &replay_overlays, started)?;
    let mut update_overlays = replay_overlays.clone();
    let baseline_players = current_perframe_player_states(&player_tables)?;
    let mut mod_scratch = ModSnapshotScratch::default();
    let baseline_mods = mod_scratch.states(&option_tables)?;
    let baseline_columns = read_note_column_transform_samples(lua)?;
    let replay = update_function_replay_beats(context, start, end);
    let sample_count = replay.len();
    let mut sample_beats = frame_buffer(start, sample_count);
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

    let mut beat = start;
    let mut seconds = f64::from(song_elapsed_seconds_at(start, context));
    let mut scheduled_states = baseline_overlays.clone();
    let mut player_capture_masks = player_transform_masks(lua, &player_tables)?;
    let mut frame_count = 0;
    crate::lua_util::set_compile_frames(lua, replay.iter().copied())
        .map_err(|err| err.to_string())?;
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
        // Overlay count is fixed for this compiler run. Reuse the three state
        // buffers while keeping prior, message-replayed and updated states apart.
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
        call_update_functions_at(lua, root, exact_beat, seconds, delta_beats, actor_delta)?;
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
            if sample.overlay_index >= overlay_count
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
            if actor_index >= overlay_count
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
        for (index, actor) in capture_actors.iter().enumerate() {
            if let Some(clock) =
                crate::lua_util::effect_render_time(actor, [seconds as f32, next_beat])
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
        let mut next_players = current_perframe_player_states(&player_tables)?;
        for player in 0..LUA_PLAYERS {
            player_capture_masks[player] |= next_masks[player];
            if let Some(index) = player_capture_indices[player] {
                let state = &replay_overlays[index];
                let mask = player_capture_masks[player];
                let output = &mut next_players[player];
                macro_rules! sample {
                    ($bit:literal, $out:ident, $field:ident) => {
                        if mask & (1 << $bit) != 0 {
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
                    aux: None,
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

    overlay_tracks.retain(|track| track.overlay_index < overlay_count);
    let mut stateful_messages = crate::lua_util::stateful_message_captures(lua);
    for capture in &mut stateful_messages {
        capture
            .overlay_targets
            .retain(|(index, _)| *index < overlay_count);
        capture
            .writes
            .retain(|write| write.overlay_index < overlay_count);
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
                if let Some(aux) = overlay.actor.message_commands[command_index].aux {
                    overlay
                        .table
                        .set("__songlua_aux", aux)
                        .map_err(|err| err.to_string())?;
                }
                let base = match states.get(overlay_index) {
                    Some(state) => *state,
                    None => actor_overlay_initial_state(&overlay.table)?,
                };
                self.active[overlay_index] = Some(SongLuaPerframeActiveMessage {
                    command_index,
                    start_beat: event_beat,
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
    let state =
        overlay_state_after_blocks(message.base, command.blocks.iter().filter(keep), elapsed);
    if let Some(epoch) = crate::motion_restart_epoch(
        message.base,
        &command.blocks,
        elapsed,
        song_elapsed_seconds_at(message.start_beat, context),
    ) {
        crate::lua_util::sync_motion_restart(
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

impl PerframeSnapshot {
    fn capture<Kind>(
        &mut self,
        beat: f32,
        players: &[Option<Table>; LUA_PLAYERS],
        overlays: &[SongLuaOverlayCompileActor<Kind>],
    ) -> Result<(), String> {
        self.beat = beat;
        self.players = current_perframe_player_states(players)?;
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
    let baseline_players = current_perframe_player_states(&player_tables)?;
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
            current.capture(start, &player_tables, overlays)?;
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
            current.capture(sample.beat, &player_tables, overlays)?;
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
