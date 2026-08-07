use deadlib_present::anim::EffectClock;
use deadsync_profile::{PlayerSide, StepStatsExtra};
use log::{info, warn};
use mlua::{HookTriggers, Lua, LuaOptions, MultiValue, StdLib, Table, Value, VmState};
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::rc::Rc;
use std::sync::LazyLock;

const GIF_FOLDER: &str = "step_stats_gifs";
const MAX_GIFS: usize = 256;
const MAX_SCRIPT_BYTES: u64 = 256 * 1024;
const MAX_FRAMES: usize = 4096;
const MAX_LUA_MEMORY: usize = 8 * 1024 * 1024;
const MAX_LUA_INSTRUCTIONS: u32 = 1_000_000;
const LUA_HOOK_INTERVAL: u32 = 10_000;

#[derive(Clone, Copy, Debug, PartialEq)]
struct GifStyle {
    effect_clock: EffectClock,
    crop: [f32; 4],
    x: f32,
    y: f32,
    zoom: f32,
    align_x: f32,
}

impl Default for GifStyle {
    fn default() -> Self {
        Self {
            effect_clock: EffectClock::Time,
            crop: [0.0; 4],
            x: 0.0,
            y: 0.0,
            zoom: 1.0,
            align_x: 0.5,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct GifDefinition {
    name: String,
    texture: String,
    frames: Box<[u32]>,
    frame_ends: Box<[f32]>,
    cycle: f32,
    // [normal/wide][P1/P2]. Lua is evaluated only while the immutable catalog
    // is built, so player/aspect conditionals cost nothing during gameplay.
    styles: [[GifStyle; 2]; 2],
}

impl GifDefinition {
    #[inline(always)]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[inline(always)]
    fn style(&self, side: PlayerSide, wide: bool) -> GifStyle {
        self.styles[wide as usize][player_index(side)]
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ResolvedStepStatsExtra {
    #[default]
    None,
    ErrorStats,
    Gif(usize),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GifRenderParams {
    pub player_side: PlayerSide,
    pub wide: bool,
    pub aspect_ratio: f32,
    pub pane_x: f32,
    pub pane_y: f32,
    pub banner_data_zoom: f32,
    pub note_field_is_centered: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GifRenderLayout {
    pub texture: &'static str,
    pub crop: [f32; 4],
    pub x: f32,
    pub y: f32,
    pub zoom: f32,
    pub align_x: f32,
    frames: &'static [u32],
    frame_ends: &'static [f32],
    cycle: f32,
    effect_clock: EffectClock,
}

impl GifRenderLayout {
    pub fn frame_at(self, beat: f32, seconds: f32) -> u32 {
        let clock = match self.effect_clock {
            EffectClock::Time => seconds,
            EffectClock::Beat => beat,
        };
        mixed_frame(clock, self.frames, self.frame_ends, self.cycle)
    }
}

struct CapturedGif {
    texture: String,
    frames: Vec<u32>,
    delays: Vec<f32>,
    style: GifStyle,
}

/// Immutable, process-lifetime Simply Love Step Stats GIF catalog.
///
/// Owner/thread model: initialized once by the first menu/gameplay setup caller
/// through `LazyLock`, then shared read-only. Capacity is capped at `MAX_GIFS`,
/// scripts at `MAX_SCRIPT_BYTES`, and animations at `MAX_FRAMES`. Warmup reads
/// and compiles the Lua files outside live gameplay with strict memory and
/// instruction limits and without OS, I/O, or package libraries. Resolved
/// gameplay state is a fixed catalog index, so gameplay has no file-I/O,
/// parsing, allocation, insertion, miss recovery, eviction, or destruction
/// work. Invalid entries are logged and omitted. Entries live until process
/// exit, and catalog size is logged once. Worst-case per-frame work is one
/// indexed lookup plus a binary search over precomputed frame timing.
static CATALOG: LazyLock<Vec<GifDefinition>> = LazyLock::new(|| {
    let roots = catalog_roots();
    let catalog = discover_in_roots(&roots);
    info!("Loaded {} Step Stats GIF definitions", catalog.len());
    catalog
});

fn catalog_roots() -> Vec<PathBuf> {
    let roots = deadsync_assets::graphic_texture_roots(GIF_FOLDER);
    #[cfg(test)]
    let roots = {
        let mut roots = roots;
        let source_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("assets")
            .join("graphics")
            .join(GIF_FOLDER);
        if source_root.is_dir() && !roots.iter().any(|root| root == &source_root) {
            roots.insert(0, source_root);
        }
        roots
    };
    roots
}

static OPTION_SETTINGS: LazyLock<Vec<StepStatsExtra>> = LazyLock::new(|| {
    let mut dynamic: Vec<_> = catalog()
        .iter()
        .map(|gif| StepStatsExtra::gif(gif.name.clone()))
        .collect();
    dynamic.push(StepStatsExtra::Randomizer);
    dynamic.sort_by(|a, b| {
        a.as_str()
            .bytes()
            .map(|byte| byte.to_ascii_lowercase())
            .cmp(b.as_str().bytes().map(|byte| byte.to_ascii_lowercase()))
    });

    let mut out = Vec::with_capacity(dynamic.len() + 2);
    out.push(StepStatsExtra::None);
    out.push(StepStatsExtra::ErrorStats);
    out.extend(dynamic);
    out
});

#[inline(always)]
pub fn catalog() -> &'static [GifDefinition] {
    CATALOG.as_slice()
}

#[inline(always)]
pub fn option_settings() -> &'static [StepStatsExtra] {
    OPTION_SETTINGS.as_slice()
}

pub fn option_index(setting: &StepStatsExtra) -> usize {
    option_settings()
        .iter()
        .position(|candidate| candidate.as_str().eq_ignore_ascii_case(setting.as_str()))
        .unwrap_or(0)
}

pub fn resolve_extra(setting: &StepStatsExtra) -> ResolvedStepStatsExtra {
    match setting {
        StepStatsExtra::None => ResolvedStepStatsExtra::None,
        StepStatsExtra::ErrorStats => ResolvedStepStatsExtra::ErrorStats,
        StepStatsExtra::Randomizer => {
            if catalog().is_empty() {
                ResolvedStepStatsExtra::None
            } else {
                let index = (rand::random::<u64>() as usize) % catalog().len();
                ResolvedStepStatsExtra::Gif(index)
            }
        }
        StepStatsExtra::Gif(name) => catalog()
            .iter()
            .position(|gif| gif.name.eq_ignore_ascii_case(name))
            .map(ResolvedStepStatsExtra::Gif)
            .unwrap_or(ResolvedStepStatsExtra::None),
    }
}

pub fn gif_render_layout(
    extra: ResolvedStepStatsExtra,
    params: GifRenderParams,
) -> Option<GifRenderLayout> {
    let ResolvedStepStatsExtra::Gif(index) = extra else {
        return None;
    };
    let gif = catalog().get(index)?;
    let style = gif.style(params.player_side, params.wide);
    let side_sign = match params.player_side {
        PlayerSide::P1 => 1.0,
        PlayerSide::P2 => -1.0,
    };
    let mut local_x = if params.note_field_is_centered {
        -12.0
    } else {
        -25.0
    } * params.aspect_ratio
        * side_sign;
    if params.wide && params.aspect_ratio < 1.7 {
        local_x += 5.5;
    }

    let base_zoom = if params.wide && !params.note_field_is_centered {
        0.5
    } else {
        0.4
    };
    let actor_frame_zoom = base_zoom * params.banner_data_zoom;
    Some(GifRenderLayout {
        texture: gif.texture.as_str(),
        crop: style.crop,
        x: params.pane_x + local_x * params.banner_data_zoom + style.x * actor_frame_zoom,
        y: params.pane_y - 57.0 * params.banner_data_zoom + style.y * actor_frame_zoom,
        zoom: actor_frame_zoom * style.zoom,
        align_x: style.align_x,
        frames: &gif.frames,
        frame_ends: &gif.frame_ends,
        cycle: gif.cycle,
        effect_clock: style.effect_clock,
    })
}

fn player_index(side: PlayerSide) -> usize {
    match side {
        PlayerSide::P1 => 0,
        PlayerSide::P2 => 1,
    }
}

fn mixed_frame(clock: f32, frames: &[u32], frame_ends: &[f32], cycle: f32) -> u32 {
    if frames.is_empty() || frames.len() != frame_ends.len() {
        return 0;
    }
    if !clock.is_finite() || !cycle.is_finite() || cycle <= 0.0 {
        return frames[0];
    }
    let phase = clock.rem_euclid(cycle);
    let index = frame_ends.partition_point(|&end| end <= phase);
    frames.get(index).copied().unwrap_or(frames[0])
}

fn discover_in_roots(roots: &[PathBuf]) -> Vec<GifDefinition> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for root in roots {
        let Ok(entries) = fs::read_dir(root) else {
            continue;
        };
        let mut paths: Vec<_> = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| {
                path.is_file()
                    && path
                        .extension()
                        .and_then(|extension| extension.to_str())
                        .is_some_and(|extension| extension.eq_ignore_ascii_case("lua"))
            })
            .collect();
        paths.sort_by_key(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
                .to_ascii_lowercase()
        });

        for path in paths {
            if out.len() >= MAX_GIFS {
                warn!("Step Stats GIF catalog reached its {MAX_GIFS}-entry cap");
                break;
            }
            let Some(name) = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .map(str::trim)
                .filter(|name| !name.is_empty())
            else {
                continue;
            };
            if name.eq_ignore_ascii_case("Randomizer")
                || seen.contains(name.to_ascii_lowercase().as_str())
            {
                continue;
            }
            let size = fs::metadata(&path)
                .map(|meta| meta.len())
                .unwrap_or(u64::MAX);
            if size > MAX_SCRIPT_BYTES {
                warn!(
                    "Skipping oversized Step Stats GIF script '{}': {size} bytes",
                    path.display()
                );
                continue;
            }
            let content = match fs::read_to_string(&path) {
                Ok(content) => content,
                Err(error) => {
                    warn!(
                        "Failed to read Step Stats GIF '{}': {error}",
                        path.display()
                    );
                    continue;
                }
            };
            match compile_definition(name, &content, path.to_string_lossy().as_ref()).and_then(
                |mut gif| {
                    let texture_name = local_texture_name(&gif.texture)?;
                    if !roots.iter().any(|root| root.join(&texture_name).is_file()) {
                        return Err(format!("referenced texture '{texture_name}' was not found"));
                    }
                    gif.texture = format!("{GIF_FOLDER}/{texture_name}");
                    Ok(gif)
                },
            ) {
                Ok(gif) => {
                    seen.insert(name.to_ascii_lowercase());
                    out.push(gif);
                }
                Err(error) => warn!(
                    "Skipping invalid Step Stats GIF '{}': {error}",
                    path.display()
                ),
            }
        }
        if out.len() >= MAX_GIFS {
            break;
        }
    }
    out.sort_by(|a, b| {
        a.name
            .bytes()
            .map(|byte| byte.to_ascii_lowercase())
            .cmp(b.name.bytes().map(|byte| byte.to_ascii_lowercase()))
    });
    out
}

fn local_texture_name(raw: &str) -> Result<String, String> {
    let mut normal = None;
    for component in Path::new(raw.trim()).components() {
        match component {
            Component::CurDir => {}
            Component::Normal(name) if normal.is_none() => normal = Some(name),
            _ => return Err(format!("texture path '{raw}' must name a local file")),
        }
    }
    normal
        .and_then(|name| name.to_str())
        .map(str::to_string)
        .ok_or_else(|| format!("texture path '{raw}' is not valid UTF-8"))
}

fn compile_definition(name: &str, content: &str, source: &str) -> Result<GifDefinition, String> {
    let p1_normal = capture_script(content, source, PlayerSide::P1, false)?;
    let p2_normal = capture_script(content, source, PlayerSide::P2, false)?;
    let p1_wide = capture_script(content, source, PlayerSide::P1, true)?;
    let p2_wide = capture_script(content, source, PlayerSide::P2, true)?;
    for captured in [&p2_normal, &p1_wide, &p2_wide] {
        if captured.texture != p1_normal.texture
            || captured.frames != p1_normal.frames
            || captured.delays != p1_normal.delays
        {
            return Err("texture or animation frames vary by player/aspect context".to_string());
        }
    }
    let (frame_ends, cycle) = frame_timing(&p1_normal.delays)?;
    Ok(GifDefinition {
        name: name.to_string(),
        texture: p1_normal.texture,
        frames: p1_normal.frames.into_boxed_slice(),
        frame_ends,
        cycle,
        styles: [
            [p1_normal.style, p2_normal.style],
            [p1_wide.style, p2_wide.style],
        ],
    })
}

fn frame_timing(delays: &[f32]) -> Result<(Box<[f32]>, f32), String> {
    let mut cycle = 0.0;
    let mut ends = Vec::with_capacity(delays.len());
    for &delay in delays {
        if !delay.is_finite() || delay < 0.0 {
            return Err("animation delay must be a finite non-negative number".to_string());
        }
        cycle += delay;
        if !cycle.is_finite() {
            return Err("animation cycle duration is too large".to_string());
        }
        ends.push(cycle);
    }
    Ok((ends.into_boxed_slice(), cycle))
}

fn capture_script(
    content: &str,
    source: &str,
    side: PlayerSide,
    wide: bool,
) -> Result<CapturedGif, String> {
    let libs = StdLib::TABLE | StdLib::STRING | StdLib::MATH | StdLib::UTF8;
    let lua = Lua::new_with(libs, LuaOptions::default()).map_err(|error| error.to_string())?;
    lua.set_memory_limit(MAX_LUA_MEMORY)
        .map_err(|error| format!("failed to limit Lua memory: {error}"))?;
    let instructions = Rc::new(Cell::new(0));
    let hook_instructions = Rc::clone(&instructions);
    lua.set_hook(
        HookTriggers::new().every_nth_instruction(LUA_HOOK_INTERVAL),
        move |_, _| {
            let next = hook_instructions.get() + LUA_HOOK_INTERVAL;
            hook_instructions.set(next);
            if next > MAX_LUA_INSTRUCTIONS {
                return Err(mlua::Error::runtime(
                    "Step Stats GIF script exceeded its instruction limit",
                ));
            }
            Ok(VmState::Continue)
        },
    )
    .map_err(|error| format!("failed to limit Lua execution: {error}"))?;
    install_capture_host(&lua, wide).map_err(|error| error.to_string())?;
    let function = lua
        .load(content)
        .set_name(source)
        .into_function()
        .map_err(|error| format!("Lua compile failed: {error}"))?;
    let player = match side {
        PlayerSide::P1 => "PlayerNumber_P1",
        PlayerSide::P2 => "PlayerNumber_P2",
    };
    let root = function
        .call::<Value>(player)
        .map_err(|error| format!("Lua execution failed: {error}"))?;
    let mut sprites = Vec::new();
    collect_sprite_tables(root, &mut sprites).map_err(|error| error.to_string())?;
    if sprites.len() != 1 {
        return Err(format!(
            "expected exactly one Def.Sprite, found {}",
            sprites.len()
        ));
    }
    capture_sprite(&lua, sprites.pop().expect("one sprite captured"))
        .map_err(|error| error.to_string())
}

fn install_capture_host(lua: &Lua, wide: bool) -> mlua::Result<()> {
    let globals = lua.globals();
    let def = lua.create_table()?;
    def.set(
        "ActorFrame",
        lua.create_function(|_, table: Table| Ok(table))?,
    )?;
    def.set(
        "Sprite",
        lua.create_function(|_, table: Table| {
            table.set("__deadsync_sprite", true)?;
            Ok(table)
        })?,
    )?;
    globals.set("Def", def)?;
    globals.set(
        "IsUsingWideScreen",
        lua.create_function(move |_, _args: MultiValue| Ok(wide))?,
    )?;

    let sprite = lua.create_table()?;
    sprite.set(
        "LinearFrames",
        lua.create_function(|lua, (count, seconds): (usize, f32)| {
            let frames = lua.create_table()?;
            frames.set("__deadsync_count", count.max(1))?;
            frames.set("__deadsync_seconds", seconds.max(0.0))?;
            Ok(frames)
        })?,
    )?;
    globals.set("Sprite", sprite)?;
    Ok(())
}

fn collect_sprite_tables(value: Value, out: &mut Vec<Table>) -> mlua::Result<()> {
    let Value::Table(table) = value else {
        return Ok(());
    };
    if table.get::<bool>("__deadsync_sprite").unwrap_or(false) {
        out.push(table);
        return Ok(());
    }
    for child in table.sequence_values::<Value>() {
        collect_sprite_tables(child?, out)?;
    }
    Ok(())
}

fn capture_sprite(lua: &Lua, sprite: Table) -> mlua::Result<CapturedGif> {
    let texture = sprite.get::<String>("Texture")?;
    let (frames, delays) = capture_frames(&sprite)?;
    let state = Rc::new(RefCell::new(GifStyle::default()));
    let actor = lua.create_table()?;
    let metatable = lua.create_table()?;
    let method_state = Rc::clone(&state);
    metatable.set(
        "__index",
        lua.create_function(move |lua, (_actor, method): (Table, String)| {
            let method_state = Rc::clone(&method_state);
            lua.create_function(move |_, (actor, args): (Table, MultiValue)| {
                apply_actor_command(&mut method_state.borrow_mut(), &method, &args);
                Ok(actor)
            })
        })?,
    )?;
    actor.set_metatable(Some(metatable))?;
    for command_name in ["InitCommand", "OnCommand"] {
        if let Value::Function(command) = sprite.get::<Value>(command_name)? {
            command.call::<()>(actor.clone())?;
        }
    }
    let style = *state.borrow();
    Ok(CapturedGif {
        texture,
        frames,
        delays,
        style,
    })
}

fn capture_frames(sprite: &Table) -> mlua::Result<(Vec<u32>, Vec<f32>)> {
    let mut frames = BTreeMap::new();
    let mut delays = BTreeMap::new();
    for pair in sprite.clone().pairs::<Value, Value>() {
        let (key, value) = pair?;
        let Value::String(key) = key else {
            continue;
        };
        let key = key.to_str()?;
        if let Some(index) = numbered_key(key.as_ref(), "frame")
            && let Some(frame) = value_u32(&value)
        {
            frames.insert(index, frame);
        } else if let Some(index) = numbered_key(key.as_ref(), "delay")
            && let Some(delay) = value_f32(&value)
        {
            delays.insert(index, delay.max(0.0));
        }
    }

    if frames.is_empty()
        && let Value::Table(linear) = sprite.get::<Value>("Frames")?
    {
        let count = linear.get::<usize>("__deadsync_count")?.min(MAX_FRAMES);
        let seconds = linear.get::<f32>("__deadsync_seconds")?;
        let delay = seconds / count.max(1) as f32;
        return Ok(((0..count as u32).collect(), vec![delay; count]));
    }

    let count = frames
        .last_key_value()
        .map(|(&index, _)| index + 1)
        .unwrap_or(1);
    if count > MAX_FRAMES {
        return Err(mlua::Error::runtime(format!(
            "animation has {count} frames; limit is {MAX_FRAMES}"
        )));
    }
    let mut out_frames = Vec::with_capacity(count);
    let mut out_delays = Vec::with_capacity(count);
    let mut last_frame = 0;
    let default_delay = delays.get(&0).copied().unwrap_or(1.0);
    for index in 0..count {
        if let Some(&frame) = frames.get(&index) {
            last_frame = frame;
        }
        out_frames.push(last_frame);
        out_delays.push(delays.get(&index).copied().unwrap_or(default_delay));
    }
    Ok((out_frames, out_delays))
}

fn numbered_key(key: &str, prefix: &str) -> Option<usize> {
    let lower = key.to_ascii_lowercase();
    lower.strip_prefix(prefix)?.parse().ok()
}

fn value_f32(value: &Value) -> Option<f32> {
    let value = match value {
        Value::Integer(value) => Some(*value as f32),
        Value::Number(value) => Some(*value as f32),
        Value::String(value) => value.to_str().ok()?.parse().ok(),
        _ => None,
    }?;
    value.is_finite().then_some(value)
}

fn value_u32(value: &Value) -> Option<u32> {
    match value {
        Value::Integer(value) => u32::try_from(*value).ok(),
        Value::Number(value) if *value >= 0.0 => Some(*value as u32),
        Value::String(value) => value.to_str().ok()?.parse().ok(),
        _ => None,
    }
}

fn value_string(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.to_str().ok()?.to_string()),
        _ => None,
    }
}

fn align_value(value: &Value) -> Option<f32> {
    value_f32(value).or_else(
        || match value_string(value)?.to_ascii_lowercase().as_str() {
            "left" | "top" => Some(0.0),
            "center" | "middle" => Some(0.5),
            "right" | "bottom" => Some(1.0),
            _ => None,
        },
    )
}

fn apply_actor_command(style: &mut GifStyle, method: &str, args: &MultiValue) {
    let method = method.to_ascii_lowercase();
    let first = args.front();
    match method.as_str() {
        "effectclock" => {
            if let Some(clock) = first.and_then(value_string) {
                style.effect_clock = match clock.to_ascii_lowercase().as_str() {
                    "beat" | "beatnooffset" | "bgm" => EffectClock::Beat,
                    _ => EffectClock::Time,
                };
            }
        }
        "x" => style.x = first.and_then(value_f32).unwrap_or(style.x),
        "y" => style.y = first.and_then(value_f32).unwrap_or(style.y),
        "xy" => {
            style.x = first.and_then(value_f32).unwrap_or(style.x);
            style.y = args.get(1).and_then(value_f32).unwrap_or(style.y);
        }
        "addx" => style.x += first.and_then(value_f32).unwrap_or(0.0),
        "addy" => style.y += first.and_then(value_f32).unwrap_or(0.0),
        "zoom" => style.zoom = first.and_then(value_f32).unwrap_or(style.zoom),
        "halign" => style.align_x = first.and_then(align_value).unwrap_or(style.align_x),
        "align" => {
            style.align_x = first.and_then(align_value).unwrap_or(style.align_x);
        }
        "cropleft" => style.crop[0] = first.and_then(value_f32).unwrap_or(style.crop[0]),
        "cropright" => style.crop[1] = first.and_then(value_f32).unwrap_or(style.crop[1]),
        "croptop" => style.crop[2] = first.and_then(value_f32).unwrap_or(style.crop[2]),
        "cropbottom" => style.crop[3] = first.and_then(value_f32).unwrap_or(style.crop[3]),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROOT_REDUCER: &str = r#"
t = Def.ActorFrame {}
t[#t+1] = Def.Sprite {
    Texture="RootReducer Spooky 2x2.png",
    Frame0000=0, Delay0000=0.5,
    Frame0001=1, Delay0001=0.5,
    Frame0002=2, Delay0002=0.5,
    Frame0003=3, Delay0003=0.5,
    Frame0004=0, Delay0004=0.5,
    Frame0005=1, Delay0005=0.5,
    Frame0006=2, Delay0006=0.5,
    Frame0007=3, Delay0007=0.5,
    OnCommand=function(self)
        self:effectclock("bgm")
        self:cropright(0.02)
        self:cropleft(0.02)
        self:croptop(0.02)
        self:cropbottom(0.02)
    end
}
return t
"#;

    #[test]
    fn issue_641_actor_compiles_to_animation_data() {
        let gif = compile_definition("RootReducer spooky", ROOT_REDUCER, "fixture.lua").unwrap();
        assert_eq!(gif.texture, "RootReducer Spooky 2x2.png");
        assert_eq!(&*gif.frames, &[0, 1, 2, 3, 0, 1, 2, 3]);
        assert_eq!(&*gif.frame_ends, &[0.5, 1.0, 1.5, 2.0, 2.5, 3.0, 3.5, 4.0]);
        assert_eq!(gif.cycle, 4.0);
        assert_eq!(gif.styles[0][0].effect_clock, EffectClock::Beat);
        assert_eq!(gif.styles[0][0].crop, [0.02; 4]);
    }

    #[test]
    fn lua_player_and_aspect_conditionals_compile_to_four_styles() {
        let script = r#"
local player = ...
local pn = tonumber(player:sub(-1))
return Def.Sprite {
    Texture="conditional 1x1.png",
    OnCommand=function(self)
        self:halign(0.5 + 0.5*(pn*2-3))
        if IsUsingWideScreen() then
            self:x(220*(pn*2-3)):zoom(1.3)
        else
            self:x(150*(pn*2-3)):zoom(0.3)
        end
    end
}
"#;
        let gif = compile_definition("conditional", script, "fixture.lua").unwrap();
        assert_eq!(gif.styles[0][0].x, -150.0);
        assert_eq!(gif.styles[0][1].x, 150.0);
        assert_eq!(gif.styles[1][0].x, -220.0);
        assert_eq!(gif.styles[1][1].x, 220.0);
        assert_eq!(gif.styles[0][0].align_x, 0.0);
        assert_eq!(gif.styles[0][1].align_x, 1.0);
    }

    #[test]
    fn frame_lookup_respects_mixed_delays_and_wraps() {
        let (ends, cycle) = frame_timing(&[0.125, 0.25, 0.125]).unwrap();
        assert_eq!(mixed_frame(0.0, &[1, 2, 0], &ends, cycle), 1);
        assert_eq!(mixed_frame(0.124, &[1, 2, 0], &ends, cycle), 1);
        assert_eq!(mixed_frame(0.125, &[1, 2, 0], &ends, cycle), 2);
        assert_eq!(mixed_frame(0.5, &[1, 2, 0], &ends, cycle), 1);
    }

    #[test]
    fn lua_capture_excludes_os_and_io_libraries() {
        let script = r#"
assert(os == nil and io == nil and package == nil)
return Def.Sprite { Texture="safe 1x1.png" }
"#;
        capture_script(script, "fixture.lua", PlayerSide::P1, false).unwrap();
    }

    #[test]
    fn lua_capture_stops_runaway_scripts() {
        let error = capture_script("while true do end", "fixture.lua", PlayerSide::P1, false)
            .err()
            .expect("runaway script should fail");
        assert!(error.contains("instruction limit"));
    }

    #[test]
    fn catalog_contains_all_bundled_gifs() {
        let roots = catalog_roots();
        assert!(
            !roots.is_empty(),
            "Step Stats GIF roots were not discovered"
        );
        let among_path = roots
            .iter()
            .map(|root| root.join("AmongUs.lua"))
            .find(|path| path.is_file())
            .expect("bundled AmongUs.lua was not discovered");
        let among_source = fs::read_to_string(&among_path)
            .unwrap_or_else(|error| panic!("failed to read '{}': {error}", among_path.display()));
        compile_definition(
            "AmongUs",
            &among_source,
            among_path.to_string_lossy().as_ref(),
        )
        .unwrap();
        let names: Vec<_> = catalog().iter().map(GifDefinition::name).collect();
        assert_eq!(names.len(), 11);
        assert!(names.contains(&"AmongUs"));
        assert!(names.contains(&"Bocchi"));
        assert!(names.contains(&"BrodyQuest"));
        assert!(names.contains(&"CatJAM"));
        assert!(names.contains(&"Sonic"));
    }

    #[test]
    fn options_include_special_values_and_every_catalog_entry() {
        assert_eq!(option_settings().len(), catalog().len() + 3);
        assert_eq!(option_settings()[0], StepStatsExtra::None);
        assert_eq!(option_settings()[1], StepStatsExtra::ErrorStats);
        assert!(option_settings().contains(&StepStatsExtra::Randomizer));
        for gif in catalog() {
            assert!(
                option_settings()
                    .iter()
                    .any(|setting| setting.as_str() == gif.name())
            );
        }
    }

    #[test]
    fn render_layout_applies_compiled_child_transform() {
        let brody = resolve_extra(&StepStatsExtra::gif("BrodyQuest"));
        let p1 = gif_render_layout(
            brody,
            GifRenderParams {
                player_side: PlayerSide::P1,
                wide: true,
                aspect_ratio: 16.0 / 9.0,
                pane_x: 0.0,
                pane_y: 0.0,
                banner_data_zoom: 1.0,
                note_field_is_centered: false,
            },
        )
        .unwrap();
        assert_eq!(p1.align_x, 0.0);
        assert!(p1.x < 0.0);
        assert_eq!(p1.y, -77.0);
        assert_eq!(p1.zoom, 0.65);
    }
}
