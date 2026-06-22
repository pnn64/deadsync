use super::lua::{
    itg_find_function_end, itg_find_matching, itg_parse_lua_float_expr,
    itg_parse_self_chain_commands, itg_skip_ws,
};
use super::{ItgCommandEffect, ModelDrawState, TweenType};
use deadlib_present::anim as ui_anim;
use std::borrow::Cow;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ScriptTween {
    Linear,
    Accelerate,
    Decelerate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ScriptControl {
    StopTweening,
    FinishTweening,
    PlayCommand,
    Animate,
    SetState,
    SetStateProperties,
    SetTextureFiltering,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum ScriptActorMod {
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
pub(super) enum ScriptEffectMod {
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
pub(super) fn split_script_token(token: &str) -> Option<(String, Vec<String>)> {
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
pub(super) fn parse_script_number(raw: &str) -> Option<f32> {
    itg_parse_lua_float_expr(raw)
}

#[inline(always)]
pub(super) fn parse_script_bool(raw: &str) -> bool {
    let t = raw.trim().trim_matches('"').trim_matches('\'');
    t.eq_ignore_ascii_case("true") || t == "1"
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
pub(super) fn parse_script_vertalign(raw: &str) -> Option<f32> {
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
pub(super) fn parse_script_tween(cmd: &str, args: &[String]) -> Option<(ScriptTween, f32)> {
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
pub(super) fn parse_script_sleep(cmd: &str, args: &[String]) -> Option<f32> {
    if cmd != "sleep" {
        return None;
    }
    args.first().and_then(|arg| parse_script_number(arg))
}

#[inline(always)]
pub(super) fn parse_script_control(cmd: &str) -> Option<ScriptControl> {
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
pub(super) fn parse_script_actor_mod(cmd: &str, args: &[String]) -> Option<ScriptActorMod> {
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
pub(super) fn parse_script_effect_clock(raw: &str) -> Option<ui_anim::EffectClock> {
    let lower = raw
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .to_ascii_lowercase();
    match lower.as_str() {
        "beat" | "beatnooffset" | "bgm" => Some(ui_anim::EffectClock::Beat),
        "timer" | "timerglobal" | "music" | "musicnooffset" | "time" | "seconds" => {
            Some(ui_anim::EffectClock::Time)
        }
        _ if lower.contains("beat") => Some(ui_anim::EffectClock::Beat),
        _ => None,
    }
}

#[inline(always)]
pub(super) fn parse_script_effect_mod(cmd: &str, args: &[String]) -> Option<ScriptEffectMod> {
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

pub(super) fn normalized_script_command(script: &str) -> Cow<'_, str> {
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
pub(super) fn parse_script_effectclock_from_commands(script: &str) -> Option<bool> {
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
            out = Some(matches!(parsed, ui_anim::EffectClock::Beat));
        }
    }
    out
}

pub(super) fn itg_parse_command_effect(script: &str) -> ItgCommandEffect {
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
pub(super) fn tween_type_from_script_tween(tween: ScriptTween) -> TweenType {
    match tween {
        ScriptTween::Linear => TweenType::Linear,
        ScriptTween::Accelerate => TweenType::Accelerate,
        ScriptTween::Decelerate => TweenType::Decelerate,
    }
}

pub(super) type ItgActorMod = ScriptActorMod;

pub(super) fn itg_apply_actor_mods(state: &mut ModelDrawState, mods: &[ItgActorMod]) {
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
