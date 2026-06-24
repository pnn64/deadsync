use crate::lua::{
    itg_find_function_end, itg_find_matching, itg_parse_lua_float_expr,
    itg_parse_self_chain_commands, itg_skip_ws, itg_split_call_args,
};
use crate::{
    ModelDrawState, ModelEffectClock, ModelEffectMode, ModelEffectState, ModelTweenSegment,
    SpriteDefinition, TweenType,
};
use log::warn;
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptTween {
    Linear,
    Accelerate,
    Decelerate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptControl {
    StopTweening,
    FinishTweening,
    PlayCommand,
    Animate,
    SetState,
    SetStateProperties,
    SetTextureFiltering,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScriptActorMod {
    X(f32),
    Y(f32),
    Z(f32),
    AddX(f32),
    AddY(f32),
    AddZ(f32),
    RotationX(f32),
    RotationY(f32),
    RotationZ(f32),
    AddRotationX(f32),
    AddRotationY(f32),
    AddRotationZ(f32),
    Zoom(f32),
    ZoomX(f32),
    ZoomY(f32),
    ZoomZ(f32),
    Diffuse([f32; 4]),
    DiffuseAlpha(f32),
    Glow([f32; 4]),
    VertAlign(f32),
    BlendAdd(bool),
    Visible(bool),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScriptEffectMod {
    DiffuseRamp,
    DiffuseShift,
    GlowShift,
    Pulse,
    Spin,
    StopEffect,
    EffectColor1([f32; 4]),
    EffectColor2([f32; 4]),
    EffectPeriod(f32),
    EffectOffset(f32),
    EffectTiming([f32; 5]),
    EffectMagnitude([f32; 3]),
}

#[inline(always)]
fn split_script_call_args(raw: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut depth = 0usize;
    let mut quote = 0u8;
    let bytes = raw.as_bytes();
    let mut idx = 0usize;
    while idx < bytes.len() {
        let b = bytes[idx];
        if quote != 0 {
            if b == quote {
                quote = 0;
            }
            idx += 1;
            continue;
        }
        match b {
            b'"' | b'\'' => {
                quote = b;
            }
            b'(' | b'{' | b'[' => {
                depth += 1;
            }
            b')' | b'}' | b']' => {
                depth = depth.saturating_sub(1);
            }
            b',' if depth == 0 => {
                let part = raw[start..idx].trim();
                if !part.is_empty() {
                    out.push(part.to_string());
                }
                start = idx + 1;
            }
            _ => {}
        }
        idx += 1;
    }
    let tail = raw[start..].trim();
    if !tail.is_empty() {
        out.push(tail.to_string());
    }
    out
}

#[inline(always)]
pub fn split_script_token(token: &str) -> Option<(String, Vec<String>)> {
    let token = token.trim();
    let parts = split_script_call_args(token);
    if parts.is_empty() {
        return None;
    }
    let command = parts[0].trim().to_ascii_lowercase();
    if command.is_empty() {
        return None;
    }
    let args = parts
        .iter()
        .skip(1)
        .map(|part| part.trim().to_string())
        .collect::<Vec<_>>();
    Some((command, args))
}

#[inline(always)]
pub fn parse_script_number(raw: &str) -> Option<f32> {
    itg_parse_lua_float_expr(raw)
}

#[inline(always)]
pub fn parse_script_bool(raw: &str) -> bool {
    let t = raw.trim().trim_matches('"').trim_matches('\'');
    t.eq_ignore_ascii_case("true") || t == "1"
}

pub fn parse_linear_frames_expr(raw: &str) -> Option<(usize, Vec<f32>)> {
    let value = raw.trim().trim_end_matches(';').trim();
    let open = value.find('(')?;
    let head = value[..open].trim();
    if !head.eq_ignore_ascii_case("Sprite.LinearFrames") {
        return None;
    }
    let close = itg_find_matching(value, open, '(', ')')?;
    let args = itg_split_call_args(&value[open + 1..close]);
    if args.len() < 2 {
        return None;
    }
    let frame_count = args[0]
        .trim()
        .parse::<usize>()
        .ok()
        .or_else(|| itg_parse_lua_float_expr(&args[0]).map(|v| v as usize))?
        .max(1);
    let seconds = itg_parse_lua_float_expr(&args[1])?;
    let delay = (seconds / frame_count as f32).max(0.0);
    Some((frame_count, vec![delay; frame_count]))
}

pub fn parse_script_state_properties(args: &[String]) -> Option<(usize, Vec<f32>)> {
    args.first().and_then(|expr| parse_linear_frames_expr(expr))
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpriteStatePropertiesPlan {
    pub frame_count: usize,
    pub frame_delays: Vec<f32>,
}

pub fn sprite_state_properties_plans(script: &str) -> Vec<SpriteStatePropertiesPlan> {
    let script = normalized_script_command(script);
    let mut plans = Vec::new();
    for raw_token in script.split(';') {
        let token = raw_token.trim();
        if token.is_empty() {
            continue;
        }
        let Some((command, args)) = split_script_token(token) else {
            continue;
        };
        if command != "setstateproperties" {
            continue;
        }
        if let Some((frame_count, frame_delays)) = parse_script_state_properties(&args) {
            plans.push(SpriteStatePropertiesPlan {
                frame_count,
                frame_delays,
            });
        }
    }
    plans
}

pub fn sprite_state_properties_command_plans(
    commands: &HashMap<String, String>,
    default_is_beat_based: bool,
) -> (bool, Vec<SpriteStatePropertiesPlan>) {
    if commands.is_empty() {
        return (default_is_beat_based, Vec::new());
    }
    let mut sorted = commands.iter().collect::<Vec<_>>();
    sorted.sort_unstable_by(|a, b| a.0.cmp(b.0));

    let mut beat_based = default_is_beat_based;
    for (_, script) in sorted.iter().copied() {
        if let Some(script_clock) = parse_script_effectclock_from_commands(script) {
            beat_based = script_clock;
        }
    }

    let plans = sorted
        .into_iter()
        .flat_map(|(_, script)| sprite_state_properties_plans(script))
        .collect();
    (beat_based, plans)
}

#[inline(always)]
fn parse_script_f32_list(raw: &str) -> Vec<f32> {
    raw.split(',').filter_map(parse_script_number).collect()
}

#[inline(always)]
const fn script_rgba8(r: u8, g: u8, b: u8) -> [f32; 4] {
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0]
}

#[inline(always)]
fn parse_script_judgment_line_color(raw: &str) -> Option<[f32; 4]> {
    let trimmed = raw.trim();
    let open = trimmed.find('(')?;
    if !trimmed.ends_with(')') || open + 1 >= trimmed.len() {
        return None;
    }
    let name = trimmed[..open].trim();
    let stroke = if name.eq_ignore_ascii_case("JudgmentLineToStrokeColor") {
        true
    } else if name.eq_ignore_ascii_case("JudgmentLineToColor") {
        false
    } else {
        return None;
    };
    let key = trimmed[open + 1..trimmed.len() - 1]
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .to_ascii_lowercase();
    let mut color = match key.as_str() {
        "judgmentline_w1" => script_rgba8(0xbf, 0xea, 0xff),
        "judgmentline_w2" => script_rgba8(0xff, 0xf5, 0x68),
        "judgmentline_w3" => script_rgba8(0xa4, 0xff, 0x00),
        "judgmentline_w4" => script_rgba8(0x34, 0xbf, 0xff),
        "judgmentline_w5" => script_rgba8(0xe4, 0x4d, 0xff),
        "judgmentline_held" => script_rgba8(0xff, 0xff, 0xff),
        "judgmentline_miss" => script_rgba8(0xff, 0x3c, 0x3c),
        "judgmentline_maxcombo" => script_rgba8(0xff, 0xc6, 0x00),
        _ => script_rgba8(0x00, 0x00, 0x00),
    };
    if stroke {
        color[0] *= 0.5;
        color[1] *= 0.5;
        color[2] *= 0.5;
    }
    Some(color)
}

#[inline(always)]
fn parse_script_color(raw: &str) -> Option<[f32; 4]> {
    let trimmed = raw.trim();
    if let Some(color) = parse_script_judgment_line_color(trimmed) {
        return Some(color);
    }
    let lower = trimmed.to_ascii_lowercase();
    let value = if lower.starts_with("color(") && trimmed.ends_with(')') {
        let inner = &trimmed[6..trimmed.len().saturating_sub(1)];
        inner.trim().trim_matches('"').trim_matches('\'')
    } else {
        trimmed.trim_matches('"').trim_matches('\'')
    };
    if let Some(color) = parse_script_hex_color(value) {
        return Some(color);
    }
    let values = parse_script_f32_list(value);
    if values.len() < 4 {
        return None;
    }
    Some([values[0], values[1], values[2], values[3]])
}

fn parse_script_hex_color(raw: &str) -> Option<[f32; 4]> {
    let hex = raw.trim().strip_prefix('#')?;
    if hex.len() != 6 && hex.len() != 8 {
        return None;
    }
    let byte = |idx: usize| u8::from_str_radix(&hex[idx..idx + 2], 16).ok();
    Some([
        byte(0)? as f32 / 255.0,
        byte(2)? as f32 / 255.0,
        byte(4)? as f32 / 255.0,
        if hex.len() == 8 {
            byte(6)? as f32 / 255.0
        } else {
            1.0
        },
    ])
}

#[inline(always)]
fn parse_script_color_args(args: &[String]) -> Option<[f32; 4]> {
    if args.len() == 1 {
        let raw = args[0].as_str();
        if let Some(color) = parse_script_color(raw) {
            return Some(color);
        }
        let values = parse_script_f32_list(raw);
        if values.len() >= 4 {
            return Some([values[0], values[1], values[2], values[3]]);
        }
    }
    if args.len() < 4 {
        return None;
    }
    let mut values = [0.0f32; 4];
    for (idx, arg) in args.iter().take(4).enumerate() {
        values[idx] = parse_script_number(arg)?;
    }
    Some(values)
}

#[inline(always)]
pub fn parse_script_vertalign(raw: &str) -> Option<f32> {
    let value = raw.trim().trim_matches('"').trim_matches('\'');
    if let Ok(v) = value.parse::<f32>() {
        return Some(v);
    }
    match value.to_ascii_lowercase().as_str() {
        "top" => Some(0.0),
        "middle" | "center" => Some(0.5),
        "bottom" => Some(1.0),
        _ => None,
    }
}

#[inline(always)]
pub fn parse_script_tween(cmd: &str, args: &[String]) -> Option<(ScriptTween, f32)> {
    let tween = match cmd {
        "linear" => ScriptTween::Linear,
        "accelerate" => ScriptTween::Accelerate,
        "decelerate" => ScriptTween::Decelerate,
        _ => return None,
    };
    args.first()
        .and_then(|arg| parse_script_number(arg))
        .map(|duration| (tween, duration))
}

#[inline(always)]
pub fn parse_script_sleep(cmd: &str, args: &[String]) -> Option<f32> {
    if cmd != "sleep" {
        return None;
    }
    args.first().and_then(|arg| parse_script_number(arg))
}

#[inline(always)]
pub fn parse_script_control(cmd: &str) -> Option<ScriptControl> {
    match cmd {
        "stoptweening" => Some(ScriptControl::StopTweening),
        "finishtweening" => Some(ScriptControl::FinishTweening),
        "playcommand" => Some(ScriptControl::PlayCommand),
        "animate" => Some(ScriptControl::Animate),
        "setstate" => Some(ScriptControl::SetState),
        "setstateproperties" => Some(ScriptControl::SetStateProperties),
        "settexturefiltering" => Some(ScriptControl::SetTextureFiltering),
        _ => None,
    }
}

#[inline(always)]
pub fn parse_script_actor_mod(cmd: &str, args: &[String]) -> Option<ScriptActorMod> {
    let first = args.first().and_then(|v| parse_script_number(v));
    let bool_first = args.first().map(|v| parse_script_bool(v));

    match cmd {
        "x" => first.map(ScriptActorMod::X),
        "y" => first.map(ScriptActorMod::Y),
        "z" => first.map(ScriptActorMod::Z),
        "addx" => first.map(ScriptActorMod::AddX),
        "addy" => first.map(ScriptActorMod::AddY),
        "addz" => first.map(ScriptActorMod::AddZ),
        "rotationx" => first.map(ScriptActorMod::RotationX),
        "rotationy" => first.map(ScriptActorMod::RotationY),
        "rotationz" => first.map(ScriptActorMod::RotationZ),
        "addrotationx" => first.map(ScriptActorMod::AddRotationX),
        "addrotationy" => first.map(ScriptActorMod::AddRotationY),
        "addrotationz" => first.map(ScriptActorMod::AddRotationZ),
        "zoom" => first.map(ScriptActorMod::Zoom),
        "zoomx" => first.map(ScriptActorMod::ZoomX),
        "zoomy" => first.map(ScriptActorMod::ZoomY),
        "zoomz" => first.map(ScriptActorMod::ZoomZ),
        "diffuse" => parse_script_color_args(args).map(ScriptActorMod::Diffuse),
        "diffusealpha" => first.map(ScriptActorMod::DiffuseAlpha),
        "glow" => parse_script_color_args(args).map(ScriptActorMod::Glow),
        "vertalign" | "valign" => args
            .first()
            .and_then(|v| parse_script_vertalign(v))
            .map(ScriptActorMod::VertAlign),
        "blend" => {
            if args.iter().any(|a| {
                let lower = a.to_ascii_lowercase();
                lower.contains("blendmode_add") || lower.contains("blend.add")
            }) {
                Some(ScriptActorMod::BlendAdd(true))
            } else if !args.is_empty() {
                Some(ScriptActorMod::BlendAdd(false))
            } else {
                None
            }
        }
        "visible" => bool_first.map(ScriptActorMod::Visible),
        _ => None,
    }
}

#[inline(always)]
pub fn parse_script_effect_clock(raw: &str) -> Option<ModelEffectClock> {
    let lower = raw
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .to_ascii_lowercase();
    match lower.as_str() {
        "beat" | "beatnooffset" | "bgm" => Some(ModelEffectClock::Beat),
        "timer" | "timerglobal" | "music" | "musicnooffset" | "time" | "seconds" => {
            Some(ModelEffectClock::Time)
        }
        _ if lower.contains("beat") => Some(ModelEffectClock::Beat),
        _ => None,
    }
}

#[inline(always)]
pub fn parse_script_effect_mod(cmd: &str, args: &[String]) -> Option<ScriptEffectMod> {
    match cmd {
        "diffuseramp" => Some(ScriptEffectMod::DiffuseRamp),
        "diffuseshift" => Some(ScriptEffectMod::DiffuseShift),
        "glowshift" => Some(ScriptEffectMod::GlowShift),
        "pulse" => Some(ScriptEffectMod::Pulse),
        "spin" => Some(ScriptEffectMod::Spin),
        "stopeffect" => Some(ScriptEffectMod::StopEffect),
        "effectcolor1" => parse_script_color_args(args).map(ScriptEffectMod::EffectColor1),
        "effectcolor2" => parse_script_color_args(args).map(ScriptEffectMod::EffectColor2),
        "effectperiod" => args
            .first()
            .and_then(|v| parse_script_number(v))
            .map(ScriptEffectMod::EffectPeriod),
        "effectoffset" => args
            .first()
            .and_then(|v| parse_script_number(v))
            .map(ScriptEffectMod::EffectOffset),
        "effecttiming" => {
            if args.len() < 4 {
                return None;
            }
            let mut values = [0.0f32; 5];
            values[0] = parse_script_number(&args[0])?;
            values[1] = parse_script_number(&args[1])?;
            values[2] = parse_script_number(&args[2])?;
            let hold_at_zero = parse_script_number(&args[3])?;
            if args.len() >= 5 {
                values[3] = parse_script_number(&args[4])?;
                values[4] = hold_at_zero;
            } else {
                values[3] = 0.0;
                values[4] = hold_at_zero;
            }
            Some(ScriptEffectMod::EffectTiming(values))
        }
        "effectmagnitude" => {
            if args.len() < 3 {
                return None;
            }
            let mut values = [0.0f32; 3];
            for (idx, arg) in args.iter().take(3).enumerate() {
                values[idx] = parse_script_number(arg)?;
            }
            Some(ScriptEffectMod::EffectMagnitude(values))
        }
        _ => None,
    }
}

pub fn normalized_script_command(script: &str) -> Cow<'_, str> {
    let trimmed = script.trim();
    if !trimmed.contains("self:") {
        return Cow::Borrowed(script);
    }
    if let Some(command) = normalized_lua_function_command(trimmed) {
        return Cow::Owned(command);
    }
    itg_parse_self_chain_commands(trimmed).map_or(Cow::Borrowed(script), Cow::Owned)
}

fn normalized_lua_function_command(script: &str) -> Option<String> {
    if !script.starts_with("function") {
        return None;
    }
    let mut cursor = "function".len();
    cursor = itg_skip_ws(script, cursor);
    let open = *script.as_bytes().get(cursor)?;
    if open != b'(' {
        return None;
    }
    let params_close = itg_find_matching(script, cursor, '(', ')')?;
    let body_start = params_close + 1;
    let body_end = itg_find_function_end(script, body_start)?;
    itg_parse_self_chain_commands(&script[body_start..body_end])
}

#[inline(always)]
pub fn parse_script_effectclock_from_commands(script: &str) -> Option<bool> {
    let mut out = None;
    let script = normalized_script_command(script);
    for raw in script.split(';') {
        let token = raw.trim();
        if token.is_empty() {
            continue;
        }
        let Some((cmd, args)) = split_script_token(token) else {
            continue;
        };
        if cmd != "effectclock" {
            continue;
        }
        let clock = args.first().map(String::as_str).unwrap_or("time");
        if let Some(parsed) = parse_script_effect_clock(clock) {
            out = Some(matches!(parsed, ModelEffectClock::Beat));
        }
    }
    out
}

pub fn sprite_animation_is_beat_based(
    commands: &HashMap<String, String>,
    default_is_beat_based: bool,
) -> bool {
    let mut clock = None;
    let preferred = ["initcommand", "nonecommand", "oncommand", "offcommand"];
    for key in preferred {
        if let Some(script) = commands.get(key)
            && let Some(is_beat) = parse_script_effectclock_from_commands(script)
        {
            clock = Some(is_beat);
        }
    }
    let mut extras = commands
        .iter()
        .filter(|(key, _)| !preferred.contains(&key.as_str()))
        .map(|(key, script)| (key.as_str(), script.as_str()))
        .collect::<Vec<_>>();
    extras.sort_unstable_by(|a, b| a.0.cmp(b.0));
    for (_, script) in extras {
        if let Some(is_beat) = parse_script_effectclock_from_commands(script) {
            clock = Some(is_beat);
        }
    }
    clock.unwrap_or(default_is_beat_based)
}

#[derive(Debug, Clone, Copy)]
pub struct ItgCommandEffect {
    pub start_alpha: Option<f32>,
    pub target_alpha: Option<f32>,
    pub start_zoom: Option<f32>,
    pub target_zoom: Option<f32>,
    pub duration: f32,
    pub tween: TweenType,
    pub blend_add: Option<bool>,
    pub interrupts: bool,
}

impl Default for ItgCommandEffect {
    fn default() -> Self {
        Self {
            start_alpha: None,
            target_alpha: None,
            start_zoom: None,
            target_zoom: None,
            duration: 0.0,
            tween: TweenType::Linear,
            blend_add: None,
            interrupts: false,
        }
    }
}

pub fn itg_parse_command_effect(script: &str) -> ItgCommandEffect {
    let mut out = ItgCommandEffect::default();
    let mut pending_duration = 0.0f32;
    let mut pending_tween = TweenType::Linear;
    let script = normalized_script_command(script);
    for raw in script.split(';') {
        let token = raw.trim();
        if token.is_empty() {
            continue;
        }
        let Some((cmd, args)) = split_script_token(token) else {
            continue;
        };
        if let Some((tween, duration)) = parse_script_tween(cmd.as_str(), &args) {
            pending_duration = duration.max(0.0);
            pending_tween = tween_type_from_script_tween(tween);
            continue;
        }
        if let Some(duration) = parse_script_sleep(cmd.as_str(), &args) {
            pending_duration = duration.max(0.0);
            pending_tween = TweenType::Linear;
            continue;
        }
        if matches!(cmd.as_str(), "stoptweening" | "finishtweening") {
            out.interrupts = true;
            continue;
        }
        if let Some(mod_cmd) = parse_script_actor_mod(cmd.as_str(), &args) {
            match mod_cmd {
                ScriptActorMod::DiffuseAlpha(alpha) => {
                    if pending_duration > f32::EPSILON {
                        out.target_alpha = Some(alpha);
                        out.duration = pending_duration;
                        out.tween = pending_tween;
                        pending_duration = 0.0;
                    } else {
                        out.start_alpha = Some(alpha);
                        out.target_alpha = Some(alpha);
                    }
                }
                ScriptActorMod::Zoom(zoom) => {
                    if pending_duration > f32::EPSILON {
                        out.target_zoom = Some(zoom);
                        out.duration = pending_duration;
                        out.tween = pending_tween;
                        pending_duration = 0.0;
                    } else {
                        out.start_zoom = Some(zoom);
                        out.target_zoom = Some(zoom);
                    }
                }
                ScriptActorMod::BlendAdd(v) => {
                    out.blend_add = Some(v);
                }
                _ => {}
            }
        }
    }
    out
}

#[inline(always)]
pub fn tween_type_from_script_tween(tween: ScriptTween) -> TweenType {
    match tween {
        ScriptTween::Linear => TweenType::Linear,
        ScriptTween::Accelerate => TweenType::Accelerate,
        ScriptTween::Decelerate => TweenType::Decelerate,
    }
}

pub type ItgActorMod = ScriptActorMod;

pub fn itg_apply_parent_zoom(
    def: &mut SpriteDefinition,
    draw: &mut ModelDrawState,
    axis: usize,
    zoom: f32,
) {
    if zoom < 0.0 {
        match axis {
            0 => def.mirror_h = !def.mirror_h,
            1 => def.mirror_v = !def.mirror_v,
            _ => {}
        }
    }
    draw.zoom[axis] *= zoom.abs();
}

pub fn itg_apply_parent_actor_mod(
    def: &mut SpriteDefinition,
    draw: &mut ModelDrawState,
    actor_mod: ScriptActorMod,
) {
    match actor_mod {
        ScriptActorMod::X(v) | ScriptActorMod::AddX(v) => draw.pos[0] += v,
        ScriptActorMod::Y(v) | ScriptActorMod::AddY(v) => draw.pos[1] += v,
        ScriptActorMod::Z(v) | ScriptActorMod::AddZ(v) => draw.pos[2] += v,
        ScriptActorMod::RotationX(v) | ScriptActorMod::AddRotationX(v) => draw.rot[0] += v,
        ScriptActorMod::RotationY(v) | ScriptActorMod::AddRotationY(v) => draw.rot[1] += v,
        ScriptActorMod::RotationZ(v) | ScriptActorMod::AddRotationZ(v) => draw.rot[2] += v,
        ScriptActorMod::Zoom(v) => {
            itg_apply_parent_zoom(def, draw, 0, v);
            itg_apply_parent_zoom(def, draw, 1, v);
            itg_apply_parent_zoom(def, draw, 2, v);
        }
        ScriptActorMod::ZoomX(v) => itg_apply_parent_zoom(def, draw, 0, v),
        ScriptActorMod::ZoomY(v) => itg_apply_parent_zoom(def, draw, 1, v),
        ScriptActorMod::ZoomZ(v) => itg_apply_parent_zoom(def, draw, 2, v),
        ScriptActorMod::Diffuse(color) => {
            for (dst, src) in draw.tint.iter_mut().zip(color) {
                *dst *= src;
            }
        }
        ScriptActorMod::DiffuseAlpha(alpha) => draw.tint[3] *= alpha,
        ScriptActorMod::Glow(color) => draw.glow = color,
        ScriptActorMod::VertAlign(v) => draw.vert_align = v,
        ScriptActorMod::BlendAdd(v) => draw.blend_add = v,
        ScriptActorMod::Visible(v) => draw.visible &= v,
    }
}

pub fn itg_apply_parent_command(
    def: &mut SpriteDefinition,
    draw: &mut ModelDrawState,
    script: &str,
) {
    let script = normalized_script_command(script);
    for raw_token in script.split(';') {
        let token = raw_token.trim();
        if token.is_empty() {
            continue;
        }
        let Some((command, args)) = split_script_token(token) else {
            continue;
        };
        if let Some(actor_mod) = parse_script_actor_mod(&command, &args) {
            itg_apply_parent_actor_mod(def, draw, actor_mod);
        }
    }
}

pub fn itg_apply_actor_mods(state: &mut ModelDrawState, mods: &[ItgActorMod]) {
    for m in mods {
        match *m {
            ItgActorMod::X(v) => state.pos[0] = v,
            ItgActorMod::Y(v) => state.pos[1] = v,
            ItgActorMod::Z(v) => state.pos[2] = v,
            ItgActorMod::AddX(v) => state.pos[0] += v,
            ItgActorMod::AddY(v) => state.pos[1] += v,
            ItgActorMod::AddZ(v) => state.pos[2] += v,
            ItgActorMod::RotationX(v) => state.rot[0] = v,
            ItgActorMod::RotationY(v) => state.rot[1] = v,
            ItgActorMod::RotationZ(v) => state.rot[2] = v,
            ItgActorMod::AddRotationX(v) => state.rot[0] += v,
            ItgActorMod::AddRotationY(v) => state.rot[1] += v,
            ItgActorMod::AddRotationZ(v) => state.rot[2] += v,
            ItgActorMod::Zoom(v) => state.zoom = [v, v, v],
            ItgActorMod::ZoomX(v) => state.zoom[0] = v,
            ItgActorMod::ZoomY(v) => state.zoom[1] = v,
            ItgActorMod::ZoomZ(v) => state.zoom[2] = v,
            ItgActorMod::Diffuse(v) => state.tint = v,
            ItgActorMod::DiffuseAlpha(v) => state.tint[3] = v,
            ItgActorMod::Glow(v) => state.glow = v,
            ItgActorMod::VertAlign(v) => state.vert_align = v,
            ItgActorMod::BlendAdd(v) => state.blend_add = v,
            ItgActorMod::Visible(v) => state.visible = v,
        }
    }
}

pub fn itg_active_model_commands(
    commands: &HashMap<String, String>,
    active_key: &str,
) -> HashMap<String, String> {
    let mut out = HashMap::new();
    if let Some(value) = commands.get("initcommand") {
        out.insert("initcommand".to_string(), value.clone());
    }
    if let Some(value) = commands.get(active_key) {
        out.insert("nonecommand".to_string(), value.clone());
    }
    out
}

pub fn model_draw_program(
    commands: &HashMap<String, String>,
) -> (ModelDrawState, Arc<[ModelTweenSegment]>, ModelEffectState) {
    let mut state = ModelDrawState::default();
    let mut effect = ModelEffectState::default();
    let mut timeline: Vec<ModelTweenSegment> = Vec::new();
    let mut cursor_time = 0.0f32;
    let mut pending_tween: Option<(f32, TweenType)> = None;
    let mut grouped_mods: Vec<ItgActorMod> = Vec::new();

    let flush_group = |state: &mut ModelDrawState,
                       timeline: &mut Vec<ModelTweenSegment>,
                       cursor_time: &mut f32,
                       pending_tween: &mut Option<(f32, TweenType)>,
                       grouped_mods: &mut Vec<ItgActorMod>| {
        if grouped_mods.is_empty() {
            return;
        }
        if let Some((duration, tween)) = pending_tween.take()
            && duration > f32::EPSILON
        {
            let from = *state;
            let mut to = from;
            itg_apply_actor_mods(&mut to, grouped_mods);
            timeline.push(ModelTweenSegment {
                start: *cursor_time,
                duration,
                tween,
                from,
                to,
            });
            *state = to;
            *cursor_time += duration;
            grouped_mods.clear();
            return;
        }
        itg_apply_actor_mods(state, grouped_mods);
        grouped_mods.clear();
    };

    for key in ["initcommand", "nonecommand"] {
        let Some(script) = commands.get(key) else {
            continue;
        };
        let script = normalized_script_command(script);
        for raw in script.split(';') {
            let token = raw.trim();
            if token.is_empty() {
                continue;
            }
            let Some((cmd, args)) = split_script_token(token) else {
                continue;
            };
            if let Some((tween, duration)) = parse_script_tween(cmd.as_str(), &args) {
                flush_group(
                    &mut state,
                    &mut timeline,
                    &mut cursor_time,
                    &mut pending_tween,
                    &mut grouped_mods,
                );
                pending_tween = Some((duration.max(0.0), tween_type_from_script_tween(tween)));
                continue;
            }
            if let Some(duration) = parse_script_sleep(cmd.as_str(), &args) {
                flush_group(
                    &mut state,
                    &mut timeline,
                    &mut cursor_time,
                    &mut pending_tween,
                    &mut grouped_mods,
                );
                cursor_time += duration.max(0.0);
                continue;
            }
            if cmd == "effectclock" {
                flush_group(
                    &mut state,
                    &mut timeline,
                    &mut cursor_time,
                    &mut pending_tween,
                    &mut grouped_mods,
                );
                let raw_clock = args.first().map(String::as_str).unwrap_or("time");
                effect.clock = if let Some(clock) = parse_script_effect_clock(raw_clock) {
                    clock
                } else {
                    warn!("unsupported effectclock '{raw_clock}' in model DSL path");
                    ModelEffectClock::Time
                };
                continue;
            }
            if let Some(effect_mod) = parse_script_effect_mod(cmd.as_str(), &args) {
                flush_group(
                    &mut state,
                    &mut timeline,
                    &mut cursor_time,
                    &mut pending_tween,
                    &mut grouped_mods,
                );
                match effect_mod {
                    ScriptEffectMod::DiffuseRamp => {
                        effect.mode = ModelEffectMode::DiffuseRamp;
                        effect.period = 1.0;
                        effect.timing = [0.5, 0.0, 0.5, 0.0, 0.0];
                        effect.color1 = [0.0, 0.0, 0.0, 1.0];
                        effect.color2 = [1.0, 1.0, 1.0, 1.0];
                    }
                    ScriptEffectMod::DiffuseShift => {
                        effect.mode = ModelEffectMode::DiffuseShift;
                        effect.period = 1.0;
                        effect.timing = [0.5, 0.0, 0.5, 0.0, 0.0];
                        effect.color1 = [0.0, 0.0, 0.0, 1.0];
                        effect.color2 = [1.0, 1.0, 1.0, 1.0];
                    }
                    ScriptEffectMod::GlowShift => {
                        effect.mode = ModelEffectMode::GlowShift;
                        effect.period = 1.0;
                        effect.timing = [0.5, 0.0, 0.5, 0.0, 0.0];
                        effect.color1 = [1.0, 1.0, 1.0, 0.2];
                        effect.color2 = [1.0, 1.0, 1.0, 0.8];
                    }
                    ScriptEffectMod::Pulse => {
                        effect.mode = ModelEffectMode::Pulse;
                        effect.period = 2.0;
                        effect.timing = [1.0, 0.0, 1.0, 0.0, 0.0];
                        effect.magnitude = [0.5, 1.0, 0.0];
                    }
                    ScriptEffectMod::Spin => {
                        effect.mode = ModelEffectMode::Spin;
                        effect.magnitude = [0.0, 0.0, 180.0];
                    }
                    ScriptEffectMod::StopEffect => {
                        effect.mode = ModelEffectMode::None;
                    }
                    ScriptEffectMod::EffectColor1(c) => {
                        effect.color1 = c;
                    }
                    ScriptEffectMod::EffectColor2(c) => {
                        effect.color2 = c;
                    }
                    ScriptEffectMod::EffectPeriod(v) => {
                        if v > 0.0 {
                            effect.period = v;
                            effect.timing = [v * 0.5, 0.0, v * 0.5, 0.0, 0.0];
                        }
                    }
                    ScriptEffectMod::EffectOffset(v) => {
                        effect.offset = v;
                    }
                    ScriptEffectMod::EffectTiming(v) => {
                        let timing = [
                            v[0].max(0.0),
                            v[1].max(0.0),
                            v[2].max(0.0),
                            v[3].max(0.0),
                            v[4].max(0.0),
                        ];
                        let total = timing[0] + timing[1] + timing[2] + timing[3] + timing[4];
                        if total > 0.0 {
                            effect.timing = timing;
                            effect.period = total;
                        }
                    }
                    ScriptEffectMod::EffectMagnitude(v) => {
                        effect.magnitude = v;
                    }
                }
                continue;
            }
            if parse_script_control(cmd.as_str()).is_some() {
                flush_group(
                    &mut state,
                    &mut timeline,
                    &mut cursor_time,
                    &mut pending_tween,
                    &mut grouped_mods,
                );
                continue;
            }
            if let Some(mod_cmd) = parse_script_actor_mod(cmd.as_str(), &args) {
                grouped_mods.push(mod_cmd);
            } else {
                warn!("unsupported noteskin actor command in model DSL path: '{cmd}'");
            }
        }
    }

    flush_group(
        &mut state,
        &mut timeline,
        &mut cursor_time,
        &mut pending_tween,
        &mut grouped_mods,
    );

    state.zoom[0] = state.zoom[0].max(0.0);
    state.zoom[1] = state.zoom[1].max(0.0);
    state.zoom[2] = state.zoom[2].max(0.0);
    state.tint[0] = state.tint[0].clamp(0.0, 1.0);
    state.tint[1] = state.tint[1].clamp(0.0, 1.0);
    state.tint[2] = state.tint[2].clamp(0.0, 1.0);
    state.tint[3] = state.tint[3].clamp(0.0, 1.0);
    state.glow[0] = state.glow[0].clamp(0.0, 1.0);
    state.glow[1] = state.glow[1].clamp(0.0, 1.0);
    state.glow[2] = state.glow[2].clamp(0.0, 1.0);
    state.glow[3] = state.glow[3].clamp(0.0, 1.0);

    (state, Arc::from(timeline), effect)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parent_actor_mod_negative_zoom_flips_sprite_definition() {
        let mut def = SpriteDefinition::default();
        let mut draw = ModelDrawState::default();

        itg_apply_parent_actor_mod(&mut def, &mut draw, ScriptActorMod::ZoomX(-2.0));
        itg_apply_parent_actor_mod(&mut def, &mut draw, ScriptActorMod::ZoomY(-0.5));

        assert!(def.mirror_h);
        assert!(def.mirror_v);
        assert_eq!(draw.zoom, [2.0, 0.5, 1.0]);
    }

    #[test]
    fn parent_actor_mod_accumulates_and_multiplies_values() {
        let mut def = SpriteDefinition::default();
        let mut draw = ModelDrawState {
            tint: [0.5, 0.75, 1.0, 0.8],
            ..ModelDrawState::default()
        };

        itg_apply_parent_actor_mod(&mut def, &mut draw, ScriptActorMod::X(4.0));
        itg_apply_parent_actor_mod(&mut def, &mut draw, ScriptActorMod::AddX(2.0));
        itg_apply_parent_actor_mod(
            &mut def,
            &mut draw,
            ScriptActorMod::Diffuse([0.5, 0.5, 0.25, 0.5]),
        );
        itg_apply_parent_actor_mod(&mut def, &mut draw, ScriptActorMod::DiffuseAlpha(0.5));
        itg_apply_parent_actor_mod(&mut def, &mut draw, ScriptActorMod::Visible(false));
        itg_apply_parent_actor_mod(&mut def, &mut draw, ScriptActorMod::Visible(true));

        assert_eq!(draw.pos[0], 6.0);
        assert_eq!(draw.tint, [0.25, 0.375, 0.25, 0.2]);
        assert!(!draw.visible);
    }

    #[test]
    fn parent_command_parses_and_applies_actor_mods() {
        let mut def = SpriteDefinition::default();
        let mut draw = ModelDrawState {
            tint: [1.0, 0.5, 0.25, 1.0],
            ..ModelDrawState::default()
        };

        itg_apply_parent_command(
            &mut def,
            &mut draw,
            "zoomx,-2;addy,8;diffusealpha,0.25;visible,false;finishtweening",
        );

        assert!(def.mirror_h);
        assert_eq!(draw.zoom[0], 2.0);
        assert_eq!(draw.pos[1], 8.0);
        assert_eq!(draw.tint[3], 0.25);
        assert!(!draw.visible);
    }

    #[test]
    fn model_draw_program_builds_tween_and_effect() {
        let commands = HashMap::from([
            (
                "initcommand".to_string(),
                "zoom,0.5;diffuse,#ff000080;effectclock,beat;glowshift;".to_string(),
            ),
            (
                "nonecommand".to_string(),
                "linear,0.25;addx,8;rotationz,45;".to_string(),
            ),
        ]);

        let (draw, timeline, effect) = model_draw_program(&commands);

        assert_eq!(timeline.len(), 1);
        assert_eq!(timeline[0].duration, 0.25);
        assert_eq!(timeline[0].to.pos[0], 8.0);
        assert_eq!(timeline[0].to.rot[2], 45.0);
        assert_eq!(draw.zoom, [0.5, 0.5, 0.5]);
        assert_eq!(draw.tint, [1.0, 0.0, 0.0, 0.5019608]);
        assert_eq!(effect.clock, ModelEffectClock::Beat);
        assert_eq!(effect.mode, ModelEffectMode::GlowShift);
    }

    #[test]
    fn active_model_commands_keep_init_and_remap_active_command() {
        let commands = HashMap::from([
            ("initcommand".to_string(), "zoom,0.5".to_string()),
            (
                "holdingoncommand".to_string(),
                "linear,0.2;diffusealpha,1".to_string(),
            ),
            (
                "rolloncommand".to_string(),
                "linear,0.2;diffusealpha,0".to_string(),
            ),
        ]);

        let active = itg_active_model_commands(&commands, "holdingoncommand");

        assert_eq!(
            active.get("initcommand").map(String::as_str),
            Some("zoom,0.5")
        );
        assert_eq!(
            active.get("nonecommand").map(String::as_str),
            Some("linear,0.2;diffusealpha,1")
        );
        assert!(!active.contains_key("rolloncommand"));
    }

    #[test]
    fn itg_parse_command_effect_tracks_alpha_zoom_and_interrupts() {
        let effect = itg_parse_command_effect(
            "diffusealpha,0.25;linear,0.1;diffusealpha,1;zoom,1.5;stoptweening;blend,BlendMode_Add;",
        );

        assert_eq!(effect.start_alpha, Some(0.25));
        assert_eq!(effect.target_alpha, Some(1.0));
        assert_eq!(effect.duration, 0.1);
        assert_eq!(effect.start_zoom, Some(1.5));
        assert_eq!(effect.target_zoom, Some(1.5));
        assert_eq!(effect.blend_add, Some(true));
        assert!(effect.interrupts);
    }

    #[test]
    fn sprite_animation_clock_uses_preferred_then_sorted_extra_commands() {
        let commands = HashMap::from([
            ("zcommand".to_string(), "effectclock,time".to_string()),
            ("initcommand".to_string(), "effectclock,beat".to_string()),
            ("acommand".to_string(), "effectclock,beat".to_string()),
        ]);

        assert!(!sprite_animation_is_beat_based(&commands, true));
        assert!(sprite_animation_is_beat_based(&HashMap::new(), true));
    }

    #[test]
    fn sprite_state_properties_plans_parse_direct_and_lua_commands() {
        let lua_plans = sprite_state_properties_plans(
            "function(self) self:SetStateProperties(Sprite.LinearFrames(4, 0.2)) end",
        );
        let direct_plans =
            sprite_state_properties_plans("SetStateProperties, Sprite.LinearFrames(2, 0.5);");

        assert_eq!(lua_plans.len(), 1);
        assert_eq!(lua_plans[0].frame_count, 4);
        assert_eq!(lua_plans[0].frame_delays, vec![0.05; 4]);
        assert_eq!(direct_plans.len(), 1);
        assert_eq!(direct_plans[0].frame_count, 2);
        assert_eq!(direct_plans[0].frame_delays, vec![0.25; 2]);
    }

    #[test]
    fn sprite_state_properties_command_plans_sort_and_select_clock() {
        let commands = HashMap::from([
            (
                "zcommand".to_string(),
                "effectclock,time;SetStateProperties,Sprite.LinearFrames(3,0.3)".to_string(),
            ),
            (
                "acommand".to_string(),
                "effectclock,beat;SetStateProperties,Sprite.LinearFrames(2,0.2)".to_string(),
            ),
        ]);

        let (beat_based, plans) = sprite_state_properties_command_plans(&commands, true);

        assert!(!beat_based);
        assert_eq!(plans.len(), 2);
        assert_eq!(plans[0].frame_count, 2);
        assert_eq!(plans[1].frame_count, 3);
    }

    #[test]
    fn parse_linear_frames_expr_builds_equal_frame_delays() {
        let (frames, delays) =
            parse_linear_frames_expr("Sprite.LinearFrames(64,(64/60))").expect("linear frames");

        assert_eq!(frames, 64);
        assert_eq!(delays.len(), 64);
        assert!((delays[0] - (1.0 / 60.0)).abs() < 1e-6);
    }
}
