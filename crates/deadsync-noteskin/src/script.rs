use crate::lua::{
    itg_call_args, itg_find_function_end, itg_find_matching, itg_parse_lua_float_expr,
    itg_parse_self_chain_commands, itg_skip_ws,
};
use crate::{
    ModelDrawState, ModelEffectClock, ModelEffectMode, ModelEffectState, ModelTweenSegment,
    SpriteDefinition, TweenType,
};
use log::warn;
use smallvec::SmallVec;
use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptTween {
    Linear,
    Accelerate,
    Decelerate,
    Smooth,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptControl {
    StopTweening,
    FinishTweening,
    PlayCommand,
    Animate,
    Play,
    Pause,
    SetState,
    SetStateProperties,
    SetAllStateDelays,
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
    FadeLeft(f32),
    FadeRight(f32),
    FadeTop(f32),
    FadeBottom(f32),
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
    Thump(f32),
    Spin,
    StopEffect,
    EffectColor1([f32; 4]),
    EffectColor2([f32; 4]),
    EffectPeriod(f32),
    EffectOffset(f32),
    EffectTiming([f32; 5]),
    EffectMagnitude([f32; 3]),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptCommand<'a> {
    Linear,
    Accelerate,
    Decelerate,
    Smooth,
    Sleep,
    StopTweening,
    FinishTweening,
    PlayCommand,
    Animate,
    Play,
    Pause,
    SetState,
    SetStateProperties,
    SetAllStateDelays,
    SetTextureFiltering,
    SetSecondsIntoAnimation,
    Rate,
    ZTest,
    ZWrite,
    ClearZBuffer,
    CustomTextureRect,
    TexCoordVelocity,
    X,
    Y,
    Z,
    AddX,
    AddY,
    AddZ,
    RotationX,
    RotationY,
    RotationZ,
    AddRotationX,
    AddRotationY,
    AddRotationZ,
    Zoom,
    ZoomX,
    ZoomY,
    ZoomZ,
    BaseZoom,
    BaseZoomX,
    BaseZoomY,
    BaseZoomZ,
    Diffuse,
    DiffuseAlpha,
    Glow,
    FadeLeft,
    FadeRight,
    FadeTop,
    FadeBottom,
    VertAlign,
    VAlign,
    Blend,
    Visible,
    DiffuseRamp,
    DiffuseShift,
    GlowShift,
    Pulse,
    Thump,
    Spin,
    StopEffect,
    EffectColor1,
    EffectColor2,
    EffectPeriod,
    EffectOffset,
    EffectTiming,
    EffectMagnitude,
    EffectClock,
    BaseRotationZ,
    Unknown(&'a str),
}

impl<'a> ScriptCommand<'a> {
    #[must_use]
    pub const fn as_str(self) -> &'a str {
        match self {
            Self::Linear => "linear",
            Self::Accelerate => "accelerate",
            Self::Decelerate => "decelerate",
            Self::Smooth => "smooth",
            Self::Sleep => "sleep",
            Self::StopTweening => "stoptweening",
            Self::FinishTweening => "finishtweening",
            Self::PlayCommand => "playcommand",
            Self::Animate => "animate",
            Self::Play => "play",
            Self::Pause => "pause",
            Self::SetState => "setstate",
            Self::SetStateProperties => "setstateproperties",
            Self::SetAllStateDelays => "setallstatedelays",
            Self::SetTextureFiltering => "settexturefiltering",
            Self::SetSecondsIntoAnimation => "SetSecondsIntoAnimation",
            Self::Rate => "rate",
            Self::ZTest => "ztest",
            Self::ZWrite => "zwrite",
            Self::ClearZBuffer => "clearzbuffer",
            Self::CustomTextureRect => "customtexturerect",
            Self::TexCoordVelocity => "texcoordvelocity",
            Self::X => "x",
            Self::Y => "y",
            Self::Z => "z",
            Self::AddX => "addx",
            Self::AddY => "addy",
            Self::AddZ => "addz",
            Self::RotationX => "rotationx",
            Self::RotationY => "rotationy",
            Self::RotationZ => "rotationz",
            Self::AddRotationX => "addrotationx",
            Self::AddRotationY => "addrotationy",
            Self::AddRotationZ => "addrotationz",
            Self::Zoom => "zoom",
            Self::ZoomX => "zoomx",
            Self::ZoomY => "zoomy",
            Self::ZoomZ => "zoomz",
            Self::BaseZoom => "basezoom",
            Self::BaseZoomX => "basezoomx",
            Self::BaseZoomY => "basezoomy",
            Self::BaseZoomZ => "basezoomz",
            Self::Diffuse => "diffuse",
            Self::DiffuseAlpha => "diffusealpha",
            Self::Glow => "glow",
            Self::FadeLeft => "fadeleft",
            Self::FadeRight => "faderight",
            Self::FadeTop => "fadetop",
            Self::FadeBottom => "fadebottom",
            Self::VertAlign => "vertalign",
            Self::VAlign => "valign",
            Self::Blend => "blend",
            Self::Visible => "visible",
            Self::DiffuseRamp => "diffuseramp",
            Self::DiffuseShift => "diffuseshift",
            Self::GlowShift => "glowshift",
            Self::Pulse => "pulse",
            Self::Thump => "thump",
            Self::Spin => "spin",
            Self::StopEffect => "stopeffect",
            Self::EffectColor1 => "effectcolor1",
            Self::EffectColor2 => "effectcolor2",
            Self::EffectPeriod => "effectperiod",
            Self::EffectOffset => "effectoffset",
            Self::EffectTiming => "effecttiming",
            Self::EffectMagnitude => "effectmagnitude",
            Self::EffectClock => "effectclock",
            Self::BaseRotationZ => "baserotationz",
            Self::Unknown(raw) => raw,
        }
    }
}

impl<'a> From<&'a str> for ScriptCommand<'a> {
    #[inline]
    fn from(raw: &'a str) -> Self {
        macro_rules! command {
            ($name:literal, $variant:ident) => {
                if raw.eq_ignore_ascii_case($name) {
                    return Self::$variant;
                }
            };
        }

        match raw.len() {
            1 => {
                command!("x", X);
                command!("y", Y);
                command!("z", Z);
            }
            4 => {
                command!("rate", Rate);
                command!("play", Play);
                command!("addx", AddX);
                command!("addy", AddY);
                command!("addz", AddZ);
                command!("zoom", Zoom);
                command!("glow", Glow);
                command!("spin", Spin);
            }
            5 => {
                command!("ztest", ZTest);
                command!("sleep", Sleep);
                command!("pause", Pause);
                command!("zoomx", ZoomX);
                command!("zoomy", ZoomY);
                command!("zoomz", ZoomZ);
                command!("blend", Blend);
                command!("pulse", Pulse);
                command!("thump", Thump);
            }
            6 => {
                command!("zwrite", ZWrite);
                command!("linear", Linear);
                command!("smooth", Smooth);
                command!("valign", VAlign);
            }
            7 => {
                command!("animate", Animate);
                command!("diffuse", Diffuse);
                command!("fadetop", FadeTop);
                command!("visible", Visible);
            }
            8 => {
                command!("basezoom", BaseZoom);
                command!("setstate", SetState);
                command!("fadeleft", FadeLeft);
            }
            9 => {
                command!("basezoomx", BaseZoomX);
                command!("basezoomy", BaseZoomY);
                command!("basezoomz", BaseZoomZ);
                command!("rotationx", RotationX);
                command!("rotationy", RotationY);
                command!("rotationz", RotationZ);
                command!("faderight", FadeRight);
                command!("vertalign", VertAlign);
                command!("glowshift", GlowShift);
            }
            10 => {
                command!("accelerate", Accelerate);
                command!("decelerate", Decelerate);
                command!("fadebottom", FadeBottom);
                command!("stopeffect", StopEffect);
            }
            11 => {
                command!("playcommand", PlayCommand);
                command!("diffuseramp", DiffuseRamp);
                command!("effectclock", EffectClock);
            }
            12 => {
                command!("clearzbuffer", ClearZBuffer);
                command!("stoptweening", StopTweening);
                command!("addrotationx", AddRotationX);
                command!("addrotationy", AddRotationY);
                command!("addrotationz", AddRotationZ);
                command!("diffusealpha", DiffuseAlpha);
                command!("diffuseshift", DiffuseShift);
                command!("effectcolor1", EffectColor1);
                command!("effectcolor2", EffectColor2);
                command!("effectperiod", EffectPeriod);
                command!("effectoffset", EffectOffset);
                command!("effecttiming", EffectTiming);
            }
            13 => command!("baserotationz", BaseRotationZ),
            14 => command!("finishtweening", FinishTweening),
            15 => command!("effectmagnitude", EffectMagnitude),
            16 => command!("texcoordvelocity", TexCoordVelocity),
            17 => {
                command!("setallstatedelays", SetAllStateDelays);
                command!("customtexturerect", CustomTextureRect);
            }
            18 => command!("setstateproperties", SetStateProperties),
            19 => command!("settexturefiltering", SetTextureFiltering),
            23 => command!("SetSecondsIntoAnimation", SetSecondsIntoAnimation),
            _ => {}
        }
        Self::Unknown(raw)
    }
}

impl fmt::Display for ScriptCommand<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptToken<'a> {
    command: ScriptCommand<'a>,
    args: SmallVec<[&'a str; 6]>,
}

impl<'a> ScriptToken<'a> {
    #[must_use]
    pub const fn command(&self) -> ScriptCommand<'a> {
        self.command
    }

    #[must_use]
    pub fn args(&self) -> &[&'a str] {
        &self.args
    }
}

#[inline(always)]
#[must_use]
pub fn split_script_token<'a>(token: &'a str) -> Option<ScriptToken<'a>> {
    let raw = token.trim();
    let mut command = None;
    let mut args = SmallVec::new();
    // Keep the command separate: arguments only need one buffer, and six
    // arguments fit inline without spilling for the command itself.
    let mut push_part = |part: &'a str| {
        let part = part.trim();
        if !part.is_empty() {
            if command.is_none() {
                command = Some(part);
            } else {
                args.push(part);
            }
        }
    };
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
                push_part(&raw[start..idx]);
                start = idx + 1;
            }
            _ => {}
        }
        idx += 1;
    }
    push_part(&raw[start..]);
    Some(ScriptToken {
        command: ScriptCommand::from(command?),
        args,
    })
}

#[inline(always)]
#[must_use]
pub fn parse_script_number(raw: &str) -> Option<f32> {
    itg_parse_lua_float_expr(raw)
}

pub(crate) fn script_random(seed: u64, sample: u32) -> f32 {
    let mut bits = seed.wrapping_add((u64::from(sample) + 1).wrapping_mul(0x9e3779b97f4a7c15));
    bits = (bits ^ (bits >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    bits = (bits ^ (bits >> 27)).wrapping_mul(0x94d049bb133111eb);
    bits ^= bits >> 31;
    (bits >> 40) as f32 / 16_777_216.0
}

/// Resolve actor-instance effect arguments on the load worker, after reading
/// the compiled cache. Preferences and random samples must not be baked into it.
pub fn resolve_effect_args(commands: &mut HashMap<String, String>, offset: f32, seed: u64) {
    for (key, script) in commands {
        if !script.contains("math.random") && !script.contains("PREFSMAN") {
            continue;
        }
        let seed = key.bytes().fold(seed, |seed, b| {
            seed.wrapping_mul(1099511628211) ^ u64::from(b)
        });
        let mut sample = 0;
        let mut out = String::with_capacity(script.len());
        let mut changed = false;
        for raw in script.split(';') {
            if !out.is_empty() {
                out.push(';');
            }
            let Some(token) = split_script_token(raw) else {
                out.push_str(raw);
                continue;
            };
            if !matches!(
                token.command(),
                ScriptCommand::EffectMagnitude | ScriptCommand::EffectOffset
            ) {
                out.push_str(raw);
                continue;
            }
            let mut values = SmallVec::<[f64; 3]>::new();
            for arg in token.args() {
                let Some(value) = crate::lua::parse_float_expr(arg, &mut |term| {
                    if let Some(args) = term
                        .strip_prefix("math.random(")
                        .and_then(|s| s.strip_suffix(')'))
                    {
                        let args = itg_call_args(args)
                            .map(|arg| crate::lua::parse_float_expr(arg, &mut |_| None))
                            .collect::<Option<SmallVec<[f64; 2]>>>()?;
                        let value = f64::from(script_random(seed, sample));
                        sample += 1;
                        let (min, max) = match args.as_slice() {
                            [] => return Some(value),
                            [max] => (1.0, max.trunc()),
                            [min, max] => (min.trunc(), max.trunc()),
                            _ => return None,
                        };
                        if !min.is_finite()
                            || !max.is_finite()
                            || min > max
                            || min < f64::from(i32::MIN)
                            || max > f64::from(i32::MAX)
                        {
                            return None;
                        }
                        return Some(min + (value * (max - min + 1.0)).floor());
                    }
                    let arg = term
                        .strip_prefix("PREFSMAN:GetPreference(")?
                        .strip_suffix(')')?
                        .trim();
                    (matches!(arg, "\"GlobalOffsetSeconds\"" | "'GlobalOffsetSeconds'"))
                        .then_some(f64::from(offset))
                }) else {
                    break;
                };
                values.push(value);
            }
            if values.len() != token.args().len() {
                out.push_str(raw);
                continue;
            }
            use std::fmt::Write;
            out.push_str(token.command().as_str());
            for value in values {
                let _ = write!(out, ",{value}");
            }
            changed = true;
        }
        if changed {
            *script = out;
        }
    }
}

#[must_use]
pub fn parse_script_float(raw: &str) -> f32 {
    let raw = raw.trim_start();
    let bytes = raw.as_bytes();
    let mut end = usize::from(matches!(bytes.first(), Some(b'+' | b'-')));
    let mut has_digit = false;
    while bytes.get(end).is_some_and(u8::is_ascii_digit) {
        has_digit = true;
        end += 1;
    }
    if bytes.get(end) == Some(&b'.') {
        end += 1;
        while bytes.get(end).is_some_and(u8::is_ascii_digit) {
            has_digit = true;
            end += 1;
        }
    }
    if !has_digit {
        return 0.0;
    }
    let exponent_start = end;
    if matches!(bytes.get(end), Some(b'e' | b'E')) {
        end += 1;
        if matches!(bytes.get(end), Some(b'+' | b'-')) {
            end += 1;
        }
        let exponent_digits = end;
        while bytes.get(end).is_some_and(u8::is_ascii_digit) {
            end += 1;
        }
        if end == exponent_digits {
            end = exponent_start;
        }
    }
    raw[..end].parse().unwrap_or(0.0)
}

#[must_use]
pub fn parse_script_int(raw: &str) -> i32 {
    let raw = raw.trim_start();
    let bytes = raw.as_bytes();
    let mut end = usize::from(matches!(bytes.first(), Some(b'+' | b'-')));
    let digit_start = end;
    while bytes.get(end).is_some_and(u8::is_ascii_digit) {
        end += 1;
    }
    if end == digit_start {
        return 0;
    }
    raw[..end].parse().unwrap_or(0)
}

#[inline(always)]
#[must_use]
pub fn parse_script_bool(raw: &str) -> bool {
    let t = raw.trim().trim_matches('"').trim_matches('\'');
    t.eq_ignore_ascii_case("true") || t == "1"
}

#[inline(always)]
fn script_contains_ignore_ascii_case(raw: &str, needle: &str) -> bool {
    raw.as_bytes()
        .windows(needle.len())
        .any(|window| window.eq_ignore_ascii_case(needle.as_bytes()))
}

#[must_use]
pub fn parse_linear_frames_expr(raw: &str) -> Option<(usize, Vec<f32>)> {
    let value = raw.trim().trim_end_matches(';').trim();
    let open = value.find('(')?;
    let head = value[..open].trim();
    if !head.eq_ignore_ascii_case("Sprite.LinearFrames") {
        return None;
    }
    let close = itg_find_matching(value, open, '(', ')')?;
    let mut args = itg_call_args(&value[open + 1..close]);
    let frame_expr = args.next()?;
    let seconds_expr = args.next()?;
    let frame_count = frame_expr
        .trim()
        .parse::<usize>()
        .ok()
        .or_else(|| itg_parse_lua_float_expr(frame_expr).map(|v| v as usize))?
        .max(1);
    let seconds = itg_parse_lua_float_expr(seconds_expr)?;
    let delay = (seconds / frame_count as f32).max(0.0);
    Some((frame_count, vec![delay; frame_count]))
}

#[must_use]
pub fn parse_script_state_properties<A: AsRef<str>>(args: &[A]) -> Option<(usize, Vec<f32>)> {
    args.first()
        .and_then(|expr| parse_linear_frames_expr(expr.as_ref()))
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpriteStatePropertiesPlan {
    pub frame_count: usize,
    pub frame_delays: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SpriteAnimationCommandPlan {
    StateProperties(SpriteStatePropertiesPlan),
    AllStateDelays(f32),
}

fn append_sprite_animation_command_plans(
    script: &str,
    plans: &mut Vec<SpriteAnimationCommandPlan>,
) {
    for_each_sprite_animation_command_plan(script, |plan| plans.push(plan));
}

fn for_each_sprite_animation_command_plan(
    script: &str,
    mut visit: impl FnMut(SpriteAnimationCommandPlan),
) {
    let script = normalized_script_command(script);
    for raw_token in script.split(';') {
        let token = raw_token.trim();
        if token.is_empty() {
            continue;
        }
        let Some(token) = split_script_token(token) else {
            continue;
        };
        match token.command() {
            ScriptCommand::SetStateProperties => {
                let args = token.args();
                if let Some((frame_count, frame_delays)) = parse_script_state_properties(args) {
                    visit(SpriteAnimationCommandPlan::StateProperties(
                        SpriteStatePropertiesPlan {
                            frame_count,
                            frame_delays,
                        },
                    ));
                }
            }
            ScriptCommand::SetAllStateDelays => {
                let args = token.args();
                if let Some(delay) = args.first().and_then(|arg| parse_script_number(arg)) {
                    visit(SpriteAnimationCommandPlan::AllStateDelays(delay.max(0.0)));
                }
            }
            _ => {}
        }
    }
}

#[must_use]
pub fn sprite_animation_command_plans(script: &str) -> Vec<SpriteAnimationCommandPlan> {
    let mut plans = Vec::new();
    append_sprite_animation_command_plans(script, &mut plans);
    plans
}

#[must_use]
pub fn sprite_state_properties_plans(script: &str) -> Vec<SpriteStatePropertiesPlan> {
    sprite_animation_command_plans(script)
        .into_iter()
        .filter_map(|plan| match plan {
            SpriteAnimationCommandPlan::StateProperties(plan) => Some(plan),
            SpriteAnimationCommandPlan::AllStateDelays(_) => None,
        })
        .collect()
}

#[must_use]
pub fn sprite_animation_command_plans_from_commands(
    commands: &HashMap<String, String>,
    default_is_beat_based: bool,
) -> (bool, Vec<SpriteAnimationCommandPlan>) {
    if commands.is_empty() {
        return (default_is_beat_based, Vec::new());
    }
    let sorted = sorted_sprite_animation_command_refs(commands);

    let mut beat_based = default_is_beat_based;
    for (_, script) in sorted.iter().copied() {
        if let Some(script_clock) = parse_script_effectclock_from_commands(script) {
            beat_based = script_clock;
        }
    }

    let mut plans = Vec::new();
    for (_, script) in sorted {
        append_sprite_animation_command_plans(script, &mut plans);
    }
    (beat_based, plans)
}

type SpriteAnimationCommandRefs<'a> = SmallVec<[(&'a String, &'a String); 8]>;

fn sorted_sprite_animation_command_refs(
    commands: &HashMap<String, String>,
) -> SpriteAnimationCommandRefs<'_> {
    let mut sorted = commands.iter().collect::<SpriteAnimationCommandRefs<'_>>();
    sorted.sort_unstable_by(|a, b| a.0.cmp(b.0));
    sorted
}

#[must_use]
pub fn sprite_state_properties_command_plans(
    commands: &HashMap<String, String>,
    default_is_beat_based: bool,
) -> (bool, Vec<SpriteStatePropertiesPlan>) {
    let (beat_based, plans) =
        sprite_animation_command_plans_from_commands(commands, default_is_beat_based);
    let plans = plans
        .into_iter()
        .filter_map(|plan| match plan {
            SpriteAnimationCommandPlan::StateProperties(plan) => Some(plan),
            SpriteAnimationCommandPlan::AllStateDelays(_) => None,
        })
        .collect();
    (beat_based, plans)
}

pub fn apply_sprite_animation_command_plans<T>(
    slot: &mut T,
    commands: &HashMap<String, String>,
    default_is_beat_based: bool,
    mut apply_plan: impl FnMut(&mut T, SpriteAnimationCommandPlan, bool),
) {
    let sorted = sorted_sprite_animation_command_refs(commands);
    // Resolve the final clock before applying any plan, preserving command
    // name order and the same clock for every callback.
    let mut beat_based = default_is_beat_based;
    for (_, script) in sorted.iter().copied() {
        if let Some(clock) = parse_script_effectclock_from_commands(script) {
            beat_based = clock;
        }
    }
    for (_, script) in sorted {
        for_each_sprite_animation_command_plan(script, |plan| apply_plan(slot, plan, beat_based));
    }
}

pub fn apply_sprite_animation_script_plans<T>(
    slot: &mut T,
    script: &str,
    beat_based: bool,
    mut apply_plan: impl FnMut(&mut T, SpriteAnimationCommandPlan, bool),
) {
    for_each_sprite_animation_command_plan(script, |plan| apply_plan(slot, plan, beat_based));
}

#[inline(always)]
fn parse_script_rgba_list(raw: &str) -> Option<[f32; 4]> {
    let mut values = raw.split(',').filter_map(parse_script_number);
    Some([
        values.next()?,
        values.next()?,
        values.next()?,
        values.next()?,
    ])
}

#[inline(always)]
const fn script_rgba8(r: u8, g: u8, b: u8) -> [f32; 4] {
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0]
}

#[inline(always)]
fn script_judgment_color(key: &str) -> [f32; 4] {
    match key.len() {
        15 if key.eq_ignore_ascii_case("judgmentline_w1") => script_rgba8(0xbf, 0xea, 0xff),
        15 if key.eq_ignore_ascii_case("judgmentline_w2") => script_rgba8(0xff, 0xf5, 0x68),
        15 if key.eq_ignore_ascii_case("judgmentline_w3") => script_rgba8(0xa4, 0xff, 0x00),
        15 if key.eq_ignore_ascii_case("judgmentline_w4") => script_rgba8(0x34, 0xbf, 0xff),
        15 if key.eq_ignore_ascii_case("judgmentline_w5") => script_rgba8(0xe4, 0x4d, 0xff),
        17 if key.eq_ignore_ascii_case("judgmentline_held") => script_rgba8(0xff, 0xff, 0xff),
        17 if key.eq_ignore_ascii_case("judgmentline_miss") => script_rgba8(0xff, 0x3c, 0x3c),
        21 if key.eq_ignore_ascii_case("judgmentline_maxcombo") => script_rgba8(0xff, 0xc6, 0x00),
        _ => script_rgba8(0x00, 0x00, 0x00),
    }
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
        .trim_matches('\'');
    let mut color = script_judgment_color(key);
    if stroke {
        color[0] *= 0.5;
        color[1] *= 0.5;
        color[2] *= 0.5;
    }
    Some(color)
}

#[inline(always)]
fn script_color_value(raw: &str) -> &str {
    let trimmed = raw.trim();
    let value = if trimmed.ends_with(')')
        && trimmed
            .get(..6)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("color("))
    {
        &trimmed[6..trimmed.len() - 1]
    } else {
        trimmed
    };
    value.trim().trim_matches('"').trim_matches('\'')
}

#[inline(always)]
fn parse_script_color(raw: &str) -> Option<[f32; 4]> {
    let trimmed = raw.trim();
    if let Some(color) = parse_script_judgment_line_color(trimmed) {
        return Some(color);
    }
    let value = script_color_value(trimmed);
    if let Some(color) = parse_script_hex_color(value) {
        return Some(color);
    }
    parse_script_rgba_list(value)
}

fn parse_script_hex_color(raw: &str) -> Option<[f32; 4]> {
    let hex = raw.trim().strip_prefix('#')?;
    if hex.len() != 6 && hex.len() != 8 {
        return None;
    }
    let byte = |idx: usize| u8::from_str_radix(&hex[idx..idx + 2], 16).ok();
    Some([
        f32::from(byte(0)?) / 255.0,
        f32::from(byte(2)?) / 255.0,
        f32::from(byte(4)?) / 255.0,
        if hex.len() == 8 {
            f32::from(byte(6)?) / 255.0
        } else {
            1.0
        },
    ])
}

#[inline(always)]
fn parse_script_color_args<A: AsRef<str>>(args: &[A]) -> Option<[f32; 4]> {
    if args.len() == 1 {
        let raw = args[0].as_ref();
        if let Some(color) = parse_script_color(raw) {
            return Some(color);
        }
    }
    if args.len() < 4 {
        return None;
    }
    let mut values = [0.0f32; 4];
    for (idx, arg) in args.iter().take(4).enumerate() {
        values[idx] = parse_script_number(arg.as_ref())?;
    }
    Some(values)
}

#[inline(always)]
#[must_use]
pub fn parse_script_vertalign(raw: &str) -> Option<f32> {
    let value = raw.trim().trim_matches('"').trim_matches('\'');
    if let Ok(v) = value.parse::<f32>() {
        return Some(v);
    }
    if value.eq_ignore_ascii_case("top") {
        Some(0.0)
    } else if value.eq_ignore_ascii_case("middle") || value.eq_ignore_ascii_case("center") {
        Some(0.5)
    } else if value.eq_ignore_ascii_case("bottom") {
        Some(1.0)
    } else {
        None
    }
}

#[inline(always)]
#[must_use]
pub fn parse_script_tween<'a, A: AsRef<str>>(
    cmd: impl Into<ScriptCommand<'a>>,
    args: &[A],
) -> Option<(ScriptTween, f32)> {
    let tween = match cmd.into() {
        ScriptCommand::Linear => ScriptTween::Linear,
        ScriptCommand::Accelerate => ScriptTween::Accelerate,
        ScriptCommand::Decelerate => ScriptTween::Decelerate,
        ScriptCommand::Smooth => ScriptTween::Smooth,
        _ => return None,
    };
    args.first()
        .and_then(|arg| parse_script_number(arg.as_ref()))
        .map(|duration| (tween, duration))
}

#[inline(always)]
#[must_use]
pub fn parse_script_sleep<'a, A: AsRef<str>>(
    cmd: impl Into<ScriptCommand<'a>>,
    args: &[A],
) -> Option<f32> {
    if cmd.into() != ScriptCommand::Sleep {
        return None;
    }
    args.first()
        .and_then(|arg| parse_script_number(arg.as_ref()))
}

#[inline(always)]
#[must_use]
pub fn parse_script_control<'a>(cmd: impl Into<ScriptCommand<'a>>) -> Option<ScriptControl> {
    match cmd.into() {
        ScriptCommand::StopTweening => Some(ScriptControl::StopTweening),
        ScriptCommand::FinishTweening => Some(ScriptControl::FinishTweening),
        ScriptCommand::PlayCommand => Some(ScriptControl::PlayCommand),
        ScriptCommand::Animate => Some(ScriptControl::Animate),
        ScriptCommand::Play => Some(ScriptControl::Play),
        ScriptCommand::Pause => Some(ScriptControl::Pause),
        ScriptCommand::SetState => Some(ScriptControl::SetState),
        ScriptCommand::SetStateProperties => Some(ScriptControl::SetStateProperties),
        ScriptCommand::SetAllStateDelays => Some(ScriptControl::SetAllStateDelays),
        ScriptCommand::SetTextureFiltering => Some(ScriptControl::SetTextureFiltering),
        _ => None,
    }
}

#[inline(always)]
pub fn parse_script_actor_mod<'a, A: AsRef<str>>(
    cmd: impl Into<ScriptCommand<'a>>,
    args: &[A],
) -> Option<ScriptActorMod> {
    let first = args
        .first()
        .and_then(|value| parse_script_number(value.as_ref()));
    let bool_first = args.first().map(|value| parse_script_bool(value.as_ref()));

    match cmd.into() {
        ScriptCommand::X => first.map(ScriptActorMod::X),
        ScriptCommand::Y => first.map(ScriptActorMod::Y),
        ScriptCommand::Z => first.map(ScriptActorMod::Z),
        ScriptCommand::AddX => first.map(ScriptActorMod::AddX),
        ScriptCommand::AddY => first.map(ScriptActorMod::AddY),
        ScriptCommand::AddZ => first.map(ScriptActorMod::AddZ),
        ScriptCommand::RotationX => first.map(ScriptActorMod::RotationX),
        ScriptCommand::RotationY => first.map(ScriptActorMod::RotationY),
        ScriptCommand::RotationZ => first.map(ScriptActorMod::RotationZ),
        ScriptCommand::AddRotationX => first.map(ScriptActorMod::AddRotationX),
        ScriptCommand::AddRotationY => first.map(ScriptActorMod::AddRotationY),
        ScriptCommand::AddRotationZ => first.map(ScriptActorMod::AddRotationZ),
        ScriptCommand::Zoom => first.map(ScriptActorMod::Zoom),
        ScriptCommand::ZoomX => first.map(ScriptActorMod::ZoomX),
        ScriptCommand::ZoomY => first.map(ScriptActorMod::ZoomY),
        ScriptCommand::ZoomZ => first.map(ScriptActorMod::ZoomZ),
        ScriptCommand::Diffuse => parse_script_color_args(args).map(ScriptActorMod::Diffuse),
        ScriptCommand::DiffuseAlpha => first.map(ScriptActorMod::DiffuseAlpha),
        ScriptCommand::Glow => parse_script_color_args(args).map(ScriptActorMod::Glow),
        ScriptCommand::FadeLeft => first.map(ScriptActorMod::FadeLeft),
        ScriptCommand::FadeRight => first.map(ScriptActorMod::FadeRight),
        ScriptCommand::FadeTop => first.map(ScriptActorMod::FadeTop),
        ScriptCommand::FadeBottom => first.map(ScriptActorMod::FadeBottom),
        ScriptCommand::VertAlign | ScriptCommand::VAlign => args
            .first()
            .and_then(|value| parse_script_vertalign(value.as_ref()))
            .map(ScriptActorMod::VertAlign),
        ScriptCommand::Blend => {
            if args.iter().any(|arg| {
                let raw = arg.as_ref();
                script_contains_ignore_ascii_case(raw, "blendmode_add")
                    || script_contains_ignore_ascii_case(raw, "blend.add")
            }) {
                Some(ScriptActorMod::BlendAdd(true))
            } else if !args.is_empty() {
                Some(ScriptActorMod::BlendAdd(false))
            } else {
                None
            }
        }
        ScriptCommand::Visible => bool_first.map(ScriptActorMod::Visible),
        _ => None,
    }
}

#[inline(always)]
#[must_use]
pub fn parse_script_effect_clock(raw: &str) -> Option<ModelEffectClock> {
    let value = raw.trim().trim_matches('"').trim_matches('\'');
    if value.eq_ignore_ascii_case("beat")
        || value.eq_ignore_ascii_case("beatnooffset")
        || value.eq_ignore_ascii_case("bgm")
    {
        Some(ModelEffectClock::Beat)
    } else if value.eq_ignore_ascii_case("timer")
        || value.eq_ignore_ascii_case("timerglobal")
        || value.eq_ignore_ascii_case("music")
        || value.eq_ignore_ascii_case("musicnooffset")
        || value.eq_ignore_ascii_case("time")
        || value.eq_ignore_ascii_case("seconds")
    {
        Some(ModelEffectClock::Time)
    } else if script_contains_ignore_ascii_case(value, "beat") {
        Some(ModelEffectClock::Beat)
    } else {
        None
    }
}

#[inline(always)]
pub fn parse_script_effect_mod<'a, A: AsRef<str>>(
    cmd: impl Into<ScriptCommand<'a>>,
    args: &[A],
) -> Option<ScriptEffectMod> {
    match cmd.into() {
        ScriptCommand::DiffuseRamp => Some(ScriptEffectMod::DiffuseRamp),
        ScriptCommand::DiffuseShift => Some(ScriptEffectMod::DiffuseShift),
        ScriptCommand::GlowShift => Some(ScriptEffectMod::GlowShift),
        ScriptCommand::Pulse => Some(ScriptEffectMod::Pulse),
        ScriptCommand::Thump => Some(ScriptEffectMod::Thump(
            args.first()
                .and_then(|arg| parse_script_number(arg.as_ref()))
                .unwrap_or(1.0),
        )),
        ScriptCommand::Spin => Some(ScriptEffectMod::Spin),
        ScriptCommand::StopEffect => Some(ScriptEffectMod::StopEffect),
        ScriptCommand::EffectColor1 => {
            parse_script_color_args(args).map(ScriptEffectMod::EffectColor1)
        }
        ScriptCommand::EffectColor2 => {
            parse_script_color_args(args).map(ScriptEffectMod::EffectColor2)
        }
        ScriptCommand::EffectPeriod => args
            .first()
            .and_then(|value| parse_script_number(value.as_ref()))
            .map(ScriptEffectMod::EffectPeriod),
        ScriptCommand::EffectOffset => args
            .first()
            .and_then(|value| parse_script_number(value.as_ref()))
            .map(ScriptEffectMod::EffectOffset),
        ScriptCommand::EffectTiming => {
            if args.len() < 4 {
                return None;
            }
            let mut values = [0.0f32; 5];
            values[0] = parse_script_number(args[0].as_ref())?;
            values[1] = parse_script_number(args[1].as_ref())?;
            values[2] = parse_script_number(args[2].as_ref())?;
            let hold_at_zero = parse_script_number(args[3].as_ref())?;
            if args.len() >= 5 {
                values[3] = parse_script_number(args[4].as_ref())?;
                values[4] = hold_at_zero;
            } else {
                values[3] = 0.0;
                values[4] = hold_at_zero;
            }
            Some(ScriptEffectMod::EffectTiming(values))
        }
        ScriptCommand::EffectMagnitude => {
            if args.len() < 3 {
                return None;
            }
            let mut values = [0.0f32; 3];
            for (idx, arg) in args.iter().take(3).enumerate() {
                values[idx] = parse_script_number(arg.as_ref())?;
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
        let Some(token) = split_script_token(token) else {
            continue;
        };
        if token.command() != ScriptCommand::EffectClock {
            continue;
        }
        let clock = token.args().first().copied().unwrap_or("time");
        if let Some(parsed) = parse_script_effect_clock(clock) {
            out = Some(matches!(parsed, ModelEffectClock::Beat));
        }
    }
    out
}

#[must_use]
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
        .collect::<SmallVec<[(&str, &str); 8]>>();
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
    pub finishes_tween: bool,
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
            finishes_tween: false,
        }
    }
}

#[must_use]
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
        let Some(token) = split_script_token(token) else {
            continue;
        };
        let command = token.command();
        let args = token.args();
        if let Some((tween, duration)) = parse_script_tween(command, args) {
            pending_duration = duration.max(0.0);
            pending_tween = tween_type_from_script_tween(tween);
            continue;
        }
        if let Some(duration) = parse_script_sleep(command, args) {
            pending_duration = duration.max(0.0);
            pending_tween = TweenType::Linear;
            continue;
        }
        if matches!(
            command,
            ScriptCommand::StopTweening | ScriptCommand::FinishTweening
        ) {
            out.interrupts = true;
            out.finishes_tween = command == ScriptCommand::FinishTweening;
            pending_duration = 0.0;
            continue;
        }
        if let Some(mod_cmd) = parse_script_actor_mod(command, args) {
            match mod_cmd {
                ScriptActorMod::DiffuseAlpha(alpha) | ScriptActorMod::Diffuse([_, _, _, alpha]) => {
                    // All properties after a tween command share its destination.
                    if pending_duration > f32::EPSILON {
                        out.target_alpha = Some(alpha);
                        out.duration = pending_duration;
                        out.tween = pending_tween;
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
#[must_use]
pub const fn tween_type_from_script_tween(tween: ScriptTween) -> TweenType {
    match tween {
        ScriptTween::Linear => TweenType::Linear,
        ScriptTween::Accelerate => TweenType::Accelerate,
        ScriptTween::Decelerate => TweenType::Decelerate,
        ScriptTween::Smooth => TweenType::Smooth,
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
        ScriptActorMod::FadeLeft(v) => draw.fade[0] = v,
        ScriptActorMod::FadeRight(v) => draw.fade[1] = v,
        ScriptActorMod::FadeTop(v) => draw.fade[2] = v,
        ScriptActorMod::FadeBottom(v) => draw.fade[3] = v,
        ScriptActorMod::VertAlign(v) => draw.vert_align = v,
        ScriptActorMod::BlendAdd(v) => draw.blend_add = v,
        ScriptActorMod::Visible(v) => draw.visible &= v,
    }
}

fn apply_base_zoom(zoom: &mut [f32; 3], command: ScriptCommand<'_>, args: &[&str]) -> bool {
    let target = match command {
        ScriptCommand::BaseZoom => &mut zoom[..],
        ScriptCommand::BaseZoomX => &mut zoom[0..1],
        ScriptCommand::BaseZoomY => &mut zoom[1..2],
        ScriptCommand::BaseZoomZ => &mut zoom[2..3],
        _ => return false,
    };
    let Some(value) = args.first().and_then(|arg| parse_script_number(arg)) else {
        return false;
    };
    target.fill(value);
    true
}

pub fn itg_apply_parent_command(
    def: &mut SpriteDefinition,
    draw: &mut ModelDrawState,
    script: &str,
) {
    let script = normalized_script_command(script);
    let mut base_zoom = [1.0; 3];
    for raw_token in script.split(';') {
        let token = raw_token.trim();
        if token.is_empty() {
            continue;
        }
        let Some(token) = split_script_token(token) else {
            continue;
        };
        if apply_base_zoom(&mut base_zoom, token.command(), token.args()) {
            continue;
        }
        if let Some(actor_mod) = parse_script_actor_mod(token.command(), token.args()) {
            itg_apply_parent_actor_mod(def, draw, actor_mod);
        }
    }
    for (axis, zoom) in base_zoom.into_iter().enumerate() {
        itg_apply_parent_zoom(def, draw, axis, zoom);
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
            ItgActorMod::FadeLeft(v) => state.fade[0] = v,
            ItgActorMod::FadeRight(v) => state.fade[1] = v,
            ItgActorMod::FadeTop(v) => state.fade[2] = v,
            ItgActorMod::FadeBottom(v) => state.fade[3] = v,
            ItgActorMod::VertAlign(v) => state.vert_align = v,
            ItgActorMod::BlendAdd(v) => state.blend_add = v,
            ItgActorMod::Visible(v) => state.visible = v,
        }
    }
}

#[must_use]
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
    model_draw_program_from_scripts([
        commands.get("initcommand").map(String::as_str),
        commands.get("nonecommand").map(String::as_str),
    ])
}

pub(crate) fn model_draw_program_from_scripts(
    scripts: [Option<&str>; 2],
) -> (ModelDrawState, Arc<[ModelTweenSegment]>, ModelEffectState) {
    let mut state = ModelDrawState::default();
    let mut base_zoom = [1.0; 3];
    let mut effect = ModelEffectState::default();
    let mut timeline: Vec<ModelTweenSegment> = Vec::new();
    let mut cursor_time = 0.0f32;
    let mut pending_tween: Option<(f32, TweenType)> = None;
    // Batch modifiers in inline storage; unusually large groups retain a heap fallback.
    let mut grouped_mods: SmallVec<[ItgActorMod; 8]> = SmallVec::new();

    let flush_group = |state: &mut ModelDrawState,
                       timeline: &mut Vec<ModelTweenSegment>,
                       cursor_time: &mut f32,
                       pending_tween: &mut Option<(f32, TweenType)>,
                       grouped_mods: &mut SmallVec<[ItgActorMod; 8]>| {
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
        let from = *state;
        itg_apply_actor_mods(state, grouped_mods);
        if !timeline.is_empty() {
            timeline.push(ModelTweenSegment {
                start: *cursor_time,
                duration: 0.0,
                tween: TweenType::Linear,
                from,
                to: *state,
            });
        }
        grouped_mods.clear();
    };

    for script in scripts.into_iter().flatten() {
        let script = normalized_script_command(script);
        for raw in script.split(';') {
            let token = raw.trim();
            if token.is_empty() {
                continue;
            }
            let Some(token) = split_script_token(token) else {
                continue;
            };
            let command = token.command();
            let args = token.args();
            // Actor base scale is immediate and independent of tweened zoom.
            if apply_base_zoom(&mut base_zoom, command, args) {
                continue;
            }
            // Sprite UV state and static depth masks are applied by the asset
            // loader. They are not model transforms and must not split a tween
            // group or be reported as unsupported on this second pass.
            if matches!(
                command,
                ScriptCommand::ZTest
                    | ScriptCommand::ZWrite
                    | ScriptCommand::ClearZBuffer
                    | ScriptCommand::CustomTextureRect
                    | ScriptCommand::TexCoordVelocity
                    | ScriptCommand::SetSecondsIntoAnimation
                    | ScriptCommand::Rate
            ) {
                continue;
            }
            if let Some((tween, duration)) = parse_script_tween(command, args) {
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
            if let Some(duration) = parse_script_sleep(command, args) {
                flush_group(
                    &mut state,
                    &mut timeline,
                    &mut cursor_time,
                    &mut pending_tween,
                    &mut grouped_mods,
                );
                let duration = duration.max(0.0);
                timeline.push(ModelTweenSegment {
                    start: cursor_time,
                    duration,
                    tween: TweenType::Linear,
                    from: state,
                    to: state,
                });
                cursor_time += duration;
                continue;
            }
            if command == ScriptCommand::EffectClock {
                flush_group(
                    &mut state,
                    &mut timeline,
                    &mut cursor_time,
                    &mut pending_tween,
                    &mut grouped_mods,
                );
                let raw_clock = args.first().copied().unwrap_or("time");
                effect.clock = if let Some(clock) = parse_script_effect_clock(raw_clock) {
                    clock
                } else {
                    warn!("unsupported effectclock '{raw_clock}' in model DSL path");
                    ModelEffectClock::Time
                };
                continue;
            }
            if let Some(effect_mod) = parse_script_effect_mod(command, args) {
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
                    ScriptEffectMod::Thump(period) => {
                        // Themes/_fallback/Scripts/02 Actor.lua: Actor:thump.
                        let period = period.max(f32::EPSILON);
                        effect.mode = ModelEffectMode::Pulse;
                        effect.period = period;
                        effect.timing = [0.0, 0.0, 0.75 * period, 0.0, 0.25 * period];
                        effect.magnitude = [1.0, 1.125, 1.0];
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
            if let Some(control) = parse_script_control(command) {
                if control != ScriptControl::SetAllStateDelays {
                    flush_group(
                        &mut state,
                        &mut timeline,
                        &mut cursor_time,
                        &mut pending_tween,
                        &mut grouped_mods,
                    );
                }
                continue;
            }
            if let Some(mod_cmd) = parse_script_actor_mod(command, args) {
                grouped_mods.push(mod_cmd);
            } else {
                warn!("unsupported noteskin actor command in model DSL path: '{command}'");
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

    // Bake the final base scale into every tween endpoint. SetBaseZoom changes
    // the whole actor immediately, even when invoked after a queued tween.
    for draw in std::iter::once(&mut state).chain(
        timeline
            .iter_mut()
            .flat_map(|segment| [&mut segment.from, &mut segment.to]),
    ) {
        for (zoom, base) in draw.zoom.iter_mut().zip(base_zoom) {
            *zoom *= base;
        }
    }

    // Preserve scale signs for sprite mirroring at load time. Model sampling
    // sanitizes scale after evaluating the timeline, including its final state.
    state.tint[0] = state.tint[0].clamp(0.0, 1.0);
    state.tint[1] = state.tint[1].clamp(0.0, 1.0);
    state.tint[2] = state.tint[2].clamp(0.0, 1.0);
    state.tint[3] = state.tint[3].clamp(0.0, 1.0);
    state.glow[0] = state.glow[0].clamp(0.0, 1.0);
    state.glow[1] = state.glow[1].clamp(0.0, 1.0);
    state.glow[2] = state.glow[2].clamp(0.0, 1.0);
    state.glow[3] = state.glow[3].clamp(0.0, 1.0);
    for fade in &mut state.fade {
        *fade = fade.clamp(0.0, 1.0);
    }

    (state, Arc::from(timeline), effect)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_token_reuses_split_semantics() {
        let source = " SetStateProperties, 4, { 0.1, 0.2 }, nested(1, 2) ";
        let token = split_script_token(source).unwrap();
        assert_eq!(token.command(), ScriptCommand::SetStateProperties);
        assert_eq!(token.args(), ["4", "{ 0.1, 0.2 }", "nested(1, 2)"]);
        let source_start = source.as_ptr() as usize;
        let source_end = source_start + source.len();
        for &value in token.args() {
            let value_start = value.as_ptr() as usize;
            assert!(value_start >= source_start && value_start < source_end);
        }
        assert_eq!(split_script_token(" , , "), None);
    }

    #[test]
    fn script_token_classification_is_case_insensitive_and_preserves_unknown_commands() {
        let mixed = split_script_token("eFfEcTtImInG,0.1,0.2,0.3,0.4,0.5").unwrap();
        let unknown = split_script_token("FutureCommand,enabled").unwrap();

        assert_eq!(mixed.command(), ScriptCommand::EffectTiming);
        assert_eq!(mixed.args(), ["0.1", "0.2", "0.3", "0.4", "0.5"]);
        assert_eq!(unknown.command(), ScriptCommand::Unknown("FutureCommand"));
        assert_eq!(unknown.command().as_str(), "FutureCommand");
    }

    #[test]
    fn script_token_preserves_arguments_beyond_inline_storage() {
        let token = split_script_token("unknown,a,b,c,d,e,f,g,nested(1,2)").unwrap();

        assert_eq!(
            token.args(),
            ["a", "b", "c", "d", "e", "f", "g", "nested(1,2)"]
        );
    }

    #[test]
    fn script_color_wrapper_is_ascii_case_insensitive_without_false_prefixes() {
        assert_eq!(
            parse_script_color("  CoLoR( '0.25, 0.5, 0.75, 1' )  "),
            Some([0.25, 0.5, 0.75, 1.0])
        );
        assert_eq!(
            parse_script_color("'0.1,0.2,0.3,0.4'"),
            Some([0.1, 0.2, 0.3, 0.4])
        );
        assert_eq!(parse_script_color("Colorful(1,2,3,4)"), None);
        assert_eq!(parse_script_color("Cölör(1,2,3,4)"), None);
    }

    #[test]
    fn script_judgment_colors_preserve_mixed_case_and_stroke_behavior() {
        assert_eq!(
            parse_script_color("jUdGmEnTlInEtOcOlOr('JuDgMeNtLiNe_W4')"),
            Some(script_rgba8(0x34, 0xbf, 0xff))
        );
        let mut stroke = script_rgba8(0xff, 0xf5, 0x68);
        stroke[0] *= 0.5;
        stroke[1] *= 0.5;
        stroke[2] *= 0.5;
        assert_eq!(
            parse_script_color("JudgmentLineToStrokeColor(\"JUDGMENTLINE_W2\")"),
            Some(stroke)
        );
        assert_eq!(
            parse_script_color("JudgmentLineToColor('unknown')"),
            Some([0.0, 0.0, 0.0, 1.0])
        );
    }

    #[test]
    fn script_rgba_parser_filters_invalid_values_and_takes_first_four() {
        assert_eq!(
            parse_script_rgba_list("ignored,0.1,0.2,0.3,0.4,99"),
            Some([0.1, 0.2, 0.3, 0.4])
        );
        assert_eq!(
            parse_script_color("0.0,0.25,0.5,1.0,42.0"),
            Some([0.0, 0.25, 0.5, 1.0])
        );
        assert_eq!(parse_script_rgba_list("0.1,bad,0.2,0.3"), None);
    }

    #[test]
    fn script_vertalign_preserves_numeric_quoted_and_mixed_case_values() {
        assert_eq!(parse_script_vertalign("0.375"), Some(0.375));
        assert_eq!(parse_script_vertalign(" 'ToP' "), Some(0.0));
        assert_eq!(parse_script_vertalign("MIDDLE"), Some(0.5));
        assert_eq!(parse_script_vertalign("Center"), Some(0.5));
        assert_eq!(parse_script_vertalign("\"bOtToM\""), Some(1.0));
        assert_eq!(parse_script_vertalign("Cënter"), None);
    }

    #[test]
    fn script_blend_detection_preserves_case_insensitive_substring_behavior() {
        assert_eq!(
            parse_script_actor_mod("blend", &["'BLENDMODE_ADD'"]),
            Some(ScriptActorMod::BlendAdd(true))
        );
        assert_eq!(
            parse_script_actor_mod("blend", &["prefix.BlEnD.AdD.suffix"]),
            Some(ScriptActorMod::BlendAdd(true))
        );
        assert_eq!(
            parse_script_actor_mod("blend", &["normal", "second_BLENDMODE_ADD_value"]),
            Some(ScriptActorMod::BlendAdd(true))
        );
        assert_eq!(
            parse_script_actor_mod("blend", &["BlendMode_Normal"]),
            Some(ScriptActorMod::BlendAdd(false))
        );
        assert_eq!(parse_script_actor_mod::<&str>("blend", &[]), None);
    }

    #[test]
    fn script_effect_clock_preserves_aliases_quotes_and_beat_fallback() {
        assert_eq!(
            parse_script_effect_clock(" 'BeAtNoOffset' "),
            Some(ModelEffectClock::Beat)
        );
        assert_eq!(
            parse_script_effect_clock("\"BGM\""),
            Some(ModelEffectClock::Beat)
        );
        assert_eq!(
            parse_script_effect_clock("MusicNoOffset"),
            Some(ModelEffectClock::Time)
        );
        assert_eq!(
            parse_script_effect_clock("customBEATclock"),
            Some(ModelEffectClock::Beat)
        );
        assert_eq!(parse_script_effect_clock("bëat"), None);
    }

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
    fn base_zoom_multiplies_regular_zoom_with_last_setter_winning() {
        for script in [
            "basezoom,2;basezoomx,0.9;BaseZoomX,0.8;basezoomy,0.6;basezoomz,1.5;zoom,2",
            "zoom,2;basezoom,2;basezoomx,0.8;basezoomy,0.6;basezoomz,1.5",
        ] {
            let commands = HashMap::from([("initcommand".into(), script.into())]);
            let (draw, timeline, _) = model_draw_program(&commands);
            assert_eq!(draw.zoom, [1.6, 1.2, 3.0]);
            assert!(timeline.is_empty());
            let mut parent = ModelDrawState::default();
            itg_apply_parent_command(&mut SpriteDefinition::default(), &mut parent, script);
            assert_eq!(parent.zoom, draw.zoom);
        }
        let mut def = SpriteDefinition::default();
        let mut draw = ModelDrawState::default();
        itg_apply_parent_command(&mut def, &mut draw, "basezoomx,-2;basezoomx,-0.8;zoom,2");
        assert!(def.mirror_h);
        assert!(!def.mirror_v);
        assert_eq!(draw.zoom, [1.6, 2.0, 2.0]);
    }

    #[test]
    fn base_zoom_is_immediate_and_preserved_by_tweens_and_pulses() {
        let commands = HashMap::from([(
            "initcommand".into(),
            "zoom,1;linear,2;zoom,3;basezoomx,0.8;basezoomy,0.6;diffusealpha,0".into(),
        )]);
        let (draw, timeline, effect) = model_draw_program(&commands);
        assert_eq!(timeline.len(), 1, "base zoom must not split the tween");
        assert_eq!(timeline[0].from.zoom, [0.8, 0.6, 1.0]);
        let mid = crate::draw::model_draw_at(draw, &timeline, effect, 0.0, &[], 1.0, 0.0);
        for (value, expected) in mid.zoom.into_iter().zip([1.6, 1.2, 2.0]) {
            assert!((value - expected).abs() < 1e-6);
        }
        assert_eq!(mid.tint[3], 0.5);

        let commands = HashMap::from([(
            "initcommand".into(),
            "basezoomx,0.8;basezoomy,0.6;pulse".into(),
        )]);
        let (draw, timeline, effect) = model_draw_program(&commands);
        for (time, multiplier) in [(0.0, 0.5), (1.0, 1.0)] {
            let sampled = crate::draw::model_draw_at(draw, &timeline, effect, 0.0, &[], time, 0.0);
            assert_eq!(
                sampled.zoom,
                [0.8 * multiplier, 0.6 * multiplier, multiplier]
            );
        }
    }

    #[test]
    fn effect_arguments_resolve_once_per_actor_with_injected_values() {
        let template = HashMap::from([("initcommand".to_string(),
            "pulse;effectmagnitude,math.random(0.75*100,0.85*100)/100,math.random(1),1;effectoffset,PREFSMAN:GetPreference('GlobalOffsetSeconds')".to_string())]);
        let mut samples = Vec::new();
        for seed in [0, 1, 2, 13] {
            let mut commands = template.clone();
            resolve_effect_args(&mut commands, -0.125, seed);
            let (_, _, effect) = model_draw_program(&commands);
            assert_eq!(effect.offset, -0.125);
            assert_eq!(&effect.magnitude[1..], &[1.0, 1.0]);
            assert!((0.75..=0.85).contains(&effect.magnitude[0]));
            assert!(
                (effect.magnitude[0] * 100.0 - (effect.magnitude[0] * 100.0).round()).abs() < 1e-5
            );
            samples.push(effect.magnitude[0]);
            let fixed = commands.clone();
            resolve_effect_args(&mut commands, 9.0, seed + 1);
            assert_eq!(
                commands, fixed,
                "resolved commands are stable for the actor lifetime"
            );
        }
        assert!(samples.windows(2).any(|pair| pair[0] != pair[1]));
        let mut fractional_bounds = HashMap::from([(
            "initcommand".into(),
            "pulse;effectmagnitude,math.random(1.15*100,1.15*100)/100,1,1".into(),
        )]);
        resolve_effect_args(&mut fractional_bounds, 0.0, 0);
        // Lua truncates the double result 114.999... before sampling.
        assert_eq!(model_draw_program(&fractional_bounds).2.magnitude[0], 1.14);
        let mut commands = template.clone();
        resolve_effect_args(&mut commands, 0.25, 0);
        assert_eq!(model_draw_program(&commands).2.offset, 0.25);
        assert!(
            template["initcommand"].contains("PREFSMAN"),
            "compiled source is reusable"
        );
        let mut invalid = HashMap::from([(
            "initcommand".into(),
            "effectmagnitude,math.random(10,1),unknown,1".into(),
        )]);
        let original = invalid.clone();
        resolve_effect_args(&mut invalid, 0.0, 0);
        assert_eq!(
            invalid, original,
            "invalid arguments still reach the diagnostic path"
        );
    }

    #[test]
    fn numeric_arithmetic_matches_lua_precedence() {
        for (expr, expected) in [
            ("8/4/2", 1.0),
            ("0.75*100", 75.0),
            ("-(2+3)*4", -20.0),
            ("1e-3 + 2*-3", -5.999),
        ] {
            assert_eq!(parse_script_number(expr), Some(expected), "{expr}");
        }
        for expr in ["1/0", "math.random(1,3)", "(2+3", "unknown*2"] {
            assert_eq!(parse_script_number(expr), None, "{expr}");
        }
    }

    #[test]
    fn model_sleep_retains_state_until_following_commands() {
        for script in [
            "diffusealpha,0;sleep,1;diffusealpha,1",
            "diffusealpha,0;sleep,1;linear,2;diffusealpha,1",
        ] {
            let commands = HashMap::from([("initcommand".into(), script.into())]);
            let (draw, timeline, effect) = model_draw_program(&commands);
            for (time, alpha) in [(0.0, 0.0), (0.5, 0.0), (3.0, 1.0)] {
                let sampled =
                    crate::draw::model_draw_at(draw, &timeline, effect, 0.0, &[], time, 0.0);
                assert_eq!(sampled.tint[3], alpha, "{script} at {time}");
            }
        }
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
        assert_eq!(draw.tint, [1.0, 0.0, 0.0, 0.501_960_8]);
        assert_eq!(effect.clock, ModelEffectClock::Beat);
        assert_eq!(effect.mode, ModelEffectMode::GlowShift);
    }

    #[test]
    fn model_draw_program_leaves_sprite_commands_to_asset_loading() {
        use std::cell::RefCell;
        thread_local! {
            static WARNINGS: RefCell<Option<Vec<String>>> = const { RefCell::new(None) };
        }
        struct CaptureWarnings;
        impl log::Log for CaptureWarnings {
            fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
                metadata.level() == log::Level::Warn
                    && matches!(
                        metadata.target(),
                        "deadsync_noteskin::script" | "deadsync_noteskin::explosion"
                    )
            }
            fn log(&self, record: &log::Record<'_>) {
                if self.enabled(record.metadata()) {
                    WARNINGS.with_borrow_mut(|warnings| {
                        if let Some(warnings) = warnings {
                            warnings.push(record.args().to_string());
                        }
                    });
                }
            }
            fn flush(&self) {}
        }
        static LOGGER: CaptureWarnings = CaptureWarnings;
        log::set_logger(&LOGGER).expect("this test binary has no other logger");
        log::set_max_level(log::LevelFilter::Warn);
        WARNINGS.with_borrow_mut(|warnings| *warnings = Some(Vec::new()));

        let commands = HashMap::from([(
            "initcommand".to_string(),
            "linear,0.5;x,8;ZTest,true;ZWrite,1;ClearZBuffer,true;\
             CustomTextureRect,0,1,0.5,1;TexCoordVelocity,0,-1;rate,2;SetSecondsIntoAnimation,0;\
             basezoomx,0.8;basezoomy,0.8;y,4"
                .to_string(),
        )]);
        let (draw, timeline, _) = model_draw_program(&commands);
        assert_eq!(timeline.len(), 1, "sprite commands must not split a tween");
        assert_eq!(timeline[0].duration, 0.5);
        assert_eq!(timeline[0].from.pos, [0.0; 3]);
        assert_eq!(timeline[0].to.pos, [8.0, 4.0, 0.0]);
        assert_eq!(draw.pos, [8.0, 4.0, 0.0]);
        let mut effects = HashMap::from([("initcommand".into(), "spin;effectmagnitude,0,0,math.random(70,190);effectoffset,PREFSMAN:GetPreference(\"GlobalOffsetSeconds\")".into())]);
        resolve_effect_args(&mut effects, 0.125, 13);
        model_draw_program(&effects);
        let animation = crate::explosion::parse_explosion_animation(
            "SetSecondsIntoAnimation,0;rate,2;linear,0.15;rotationz,90;rate,0.5;diffusealpha,0",
        );
        assert_eq!(animation.animation_seconds, Some(0.0));
        WARNINGS
            .with_borrow(|warnings| assert!(warnings.as_ref().unwrap().is_empty(), "{warnings:?}"));

        model_draw_program(&HashMap::from([(
            "initcommand".to_string(),
            "FutureCommand,1".to_string(),
        )]));
        let warnings = WARNINGS.with_borrow_mut(Option::take).unwrap();
        assert_eq!(
            warnings,
            ["unsupported noteskin actor command in model DSL path: 'FutureCommand'"]
        );
    }

    #[test]
    fn model_draw_program_parses_vertalign_and_base_glow() {
        let commands = HashMap::from([(
            "initcommand".to_string(),
            "SetTextureFiltering,false;vertalign,bottom;glow,0.1,0.2,0.3,0.4".to_string(),
        )]);

        let (draw, timeline, effect) = model_draw_program(&commands);

        assert!(parse_script_control("settexturefiltering").is_some());
        assert!(timeline.is_empty());
        assert!((draw.vert_align - 1.0).abs() <= f32::EPSILON);
        assert_eq!(draw.glow, [0.1, 0.2, 0.3, 0.4]);
        assert!(matches!(effect.mode, ModelEffectMode::None));
    }

    #[test]
    fn model_draw_program_parses_edge_fades_and_animation_controls() {
        let commands = HashMap::from([(
            "initcommand".to_string(),
            "pause;fadetop,0.5;fadeleft,2;faderight,-1;fadebottom,0.25".to_string(),
        )]);

        let (draw, timeline, effect) = model_draw_program(&commands);

        assert_eq!(draw.fade, [1.0, 0.0, 0.5, 0.25]);
        assert!(timeline.is_empty());
        assert!(matches!(effect.mode, ModelEffectMode::None));
        assert_eq!(parse_script_control("play"), Some(ScriptControl::Play));
        assert_eq!(parse_script_control("pause"), Some(ScriptControl::Pause));
    }

    #[test]
    fn model_draw_program_ignores_all_state_delay_control() {
        let commands = HashMap::from([(
            "nonecommand".to_string(),
            "linear,0.2;SetAllStateDelays,0.05;addx,8".to_string(),
        )]);

        let (_, timeline, _) = model_draw_program(&commands);

        assert_eq!(timeline.len(), 1);
        assert_eq!(timeline[0].duration, 0.2);
        assert_eq!(timeline[0].to.pos[0], 8.0);
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
        assert_eq!(effect.start_zoom, None);
        assert_eq!(effect.target_zoom, Some(1.5));
        assert_eq!(effect.blend_add, Some(true));
        assert!(effect.interrupts);
        assert!(!effect.finishes_tween);
    }

    #[test]
    fn itg_parse_command_effect_handles_lua_function_zoom_pulse() {
        let effect = itg_parse_command_effect(
            "function(self) self:finishtweening():zoom(0.75):linear(0.11):zoom(1.0)end",
        );

        assert!((effect.duration - 0.11).abs() <= 1e-6);
        assert!((effect.start_zoom.unwrap_or_default() - 0.75).abs() <= 1e-6);
        assert!((effect.target_zoom.unwrap_or_default() - 1.0).abs() <= 1e-6);
        assert!(effect.finishes_tween);
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
    fn sprite_animation_command_refs_preserve_sorted_order_when_inline_storage_spills() {
        let commands = (0..10)
            .rev()
            .map(|index| (format!("command{index:02}"), format!("script{index:02}")))
            .collect::<HashMap<_, _>>();

        let sorted = sorted_sprite_animation_command_refs(&commands);
        let keys = sorted
            .iter()
            .map(|(key, _)| key.as_str())
            .collect::<Vec<_>>();

        assert_eq!(
            keys,
            [
                "command00",
                "command01",
                "command02",
                "command03",
                "command04",
                "command05",
                "command06",
                "command07",
                "command08",
                "command09",
            ]
        );
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
    fn sprite_animation_command_plans_parse_all_state_delays() {
        let plans = sprite_animation_command_plans(
            "function(self) self:SetAllStateDelays(0.05):setstate(3) end",
        );

        assert_eq!(
            plans,
            vec![SpriteAnimationCommandPlan::AllStateDelays(0.05)]
        );
        assert_eq!(
            parse_script_control("setallstatedelays"),
            Some(ScriptControl::SetAllStateDelays)
        );
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
    fn apply_sprite_animation_commands_preserves_clock_and_plan_order() {
        let commands = HashMap::from([
            (
                "zcommand".to_string(),
                "effectclock,time;SetStateProperties,Sprite.LinearFrames(3,0.3)".to_string(),
            ),
            (
                "acommand".to_string(),
                "effectclock,beat;SetAllStateDelays,0.05".to_string(),
            ),
        ]);
        let mut applied = Vec::new();

        apply_sprite_animation_command_plans(&mut 0, &commands, true, |slot, plan, beat_based| {
            *slot += 1;
            match plan {
                SpriteAnimationCommandPlan::StateProperties(plan) => {
                    applied.push(format!("state:{}:{beat_based}", plan.frame_count));
                }
                SpriteAnimationCommandPlan::AllStateDelays(delay) => {
                    applied.push(format!("delay:{delay}:{beat_based}"));
                }
            }
        });

        assert_eq!(
            applied,
            ["delay:0.05:false", "state:3:false"].map(str::to_string)
        );
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
