//! Compiler initial state: native Rust properties plus authored raw overrides.
//! This reads the existing preparation representation without invoking property
//! getters or advancing tweens. Captured values own no Lua allocations.
use super::*;
use crate::SongLuaOverlayState;
use fields::Field;

pub(super) fn read_live(
    call: &mut HostCall<'_>,
    actor: TableId,
) -> Result<SongLuaOverlayState, Error> {
    let mut state = read_with_metadata(call, actor, true, true)?;
    let pose = call
        .store()
        .actors
        .get(&actor)
        .map(|a| (a.current, *a.destination()));
    if let Some((current, dest)) = pose
        && state.stretch_rect.is_some()
    {
        state.x = dest.x;
        state.y = dest.y;
        crate::move_overlay(&mut state, current.x, current.y);
    }
    state.spin_baked = chronological::render_spin(call, actor)?.is_some();
    Ok(state)
}

fn get(call: &mut HostCall<'_>, actor: TableId, key: Field, live: bool) -> Result<Value, Error> {
    // During playback the live pose owns these properties; the raw fields
    // are the preparation representation and are not consulted.
    if live && key.native() {
        return Ok(Value::Nil);
    }
    fields::get(call, actor, key)
}

pub(super) fn read(call: &mut HostCall<'_>, actor: TableId) -> Result<SongLuaOverlayState, Error> {
    read_with_metadata(call, actor, true, false)
}

pub(super) fn read_reset(
    call: &mut HostCall<'_>,
    actor: TableId,
) -> Result<SongLuaOverlayState, Error> {
    read_with_metadata(call, actor, false, false)
}

fn read_with_metadata(
    call: &mut HostCall<'_>,
    actor: TableId,
    metadata: bool,
    live: bool,
) -> Result<SongLuaOverlayState, Error> {
    let mut state = {
        let guard = call.store();
        guard
            .actors
            .get(&actor)
            .map(|actor| {
                let mut state = actor.state.render;
                if live {
                    actor.current.write_to(&mut state);
                } else {
                    actor.destination().write_to(&mut state);
                    [state.rot_x_deg, state.rot_y_deg, state.rot_z_deg] = actor.state.rotation;
                }
                state.visible = actor.state.visible;
                state.draw_order = actor.state.draw_order;
                state.hibernating = actor.hibernate > 0.;
                state
            })
            .unwrap_or_default()
    };
    if !metadata {
        // InitState clears native Actor fields, not resources, dimensions or
        // the other renderer properties retained in Rust rather than raw Lua.
        let defaults = SongLuaOverlayState::default();
        state.stretch_rect = None;
        TweenPose::from(defaults).write_to(&mut state);
        macro_rules! reset {
            ($($field:ident),* $(,)?) => { $(state.$field = defaults.$field;)* };
        }
        reset!(
            z_bias,
            draw_order,
            halign,
            valign,
            visible,
            basezoom,
            basezoom_x,
            basezoom_y,
            basezoom_z,
            base_rotation,
            base_alpha,
            blend,
            shadow_len,
            shadow_color,
            depth_test,
            depth_write,
            depth_test_on_fail,
            cull_mode,
            texture_filtering,
            texture_wrapping,
            texcoord_offset,
            vibrate,
            rainbow,
            rainbow_scroll,
            spin_baked,
            effect_clock,
            effect_timer,
            effect_no_offset,
            effect_mode,
            effect_magnitude,
            effect_color1,
            effect_color2,
            effect_period,
            effect_offset,
            effect_time,
            effect_timing,
            hibernating
        );
    }
    let class = text(call, actor, Field::ACTOR_TYPE, live)?;
    if !metadata || !call.store().actors.contains_key(&actor) {
        let model = class
            .as_deref()
            .is_some_and(|class| class.eq_ignore_ascii_case("Model"));
        state.depth_test = model;
        state.depth_write = model;
        state.texture_wrapping = model;
        state.cull_mode = if model {
            deadlib_render_core::CullMode::Back
        } else {
            deadlib_render_core::CullMode::None
        };
    }
    if let Some(mode) = integer(call, actor, Field::STATE_CULL_MODE, 0., 255., live)? {
        state.cull_mode = match mode {
            1 => deadlib_render_core::CullMode::Back,
            2 => deadlib_render_core::CullMode::Front,
            _ => deadlib_render_core::CullMode::None,
        };
    }
    if let Some(left) = number(call, actor, Field::HIBERNATE_SECONDS, live)? {
        state.hibernating = left > 0.;
    }
    state.hibernating |= flag(call, actor, Field::THEME_HIBERNATING, live)?.unwrap_or(false);
    if let Some(aux) = number(call, actor, Field::STATE_AUX, live)? {
        state.aux = aux;
    }
    if let Some(visible) = flag(call, actor, Field::VISIBLE, live)? {
        state.visible = visible;
    }
    if let Some(value) = flag(call, actor, Field::STATE_SPRITE_TEXTURE, live)? {
        state.sprite_texture = value;
    } else if class
        .as_deref()
        .is_some_and(|class| class.eq_ignore_ascii_case("Sprite"))
    {
        let texture = sprites::field(call, actor, "Texture")?;
        let has_texture = value_text(call, texture)?.is_some();
        let capture = if has_texture {
            None
        } else {
            let allocation = get(call, actor, Field::AFT_ALLOCATION, live)?;
            let value = if allocation == Value::Nil {
                sprites::field(call, actor, "__songlua_aft_capture_name")?
            } else {
                sprites::field(call, table(call, allocation)?, "name")?
            };
            value_text(call, value)?
        };
        state.sprite_texture = has_texture || capture.is_some();
    }
    if let Some(value) = number(call, actor, Field::STATE_X, live)? {
        state.x = value;
    }
    if let Some(value) = number(call, actor, Field::STATE_Y, live)? {
        state.y = value;
    }
    if let Some(value) = number(call, actor, Field::STATE_Z, live)? {
        state.z = value;
    }
    if let Some(value) = number(call, actor, Field::STATE_Z_BIAS, live)? {
        state.z_bias = value;
    }
    if let Some(value) = number(call, actor, Field::STATE_HALIGN, live)? {
        state.halign = value;
    }
    if let Some(value) = number(call, actor, Field::STATE_VALIGN, live)? {
        state.valign = value;
    }
    if let Some(value) = number(call, actor, Field::STATE_CROPLEFT, live)? {
        state.cropleft = value;
    }
    if let Some(value) = number(call, actor, Field::STATE_CROPRIGHT, live)? {
        state.cropright = value;
    }
    if let Some(value) = number(call, actor, Field::STATE_CROPTOP, live)? {
        state.croptop = value;
    }
    if let Some(value) = number(call, actor, Field::STATE_CROPBOTTOM, live)? {
        state.cropbottom = value;
    }
    if let Some(value) = number(call, actor, Field::STATE_FADELEFT, live)? {
        state.fadeleft = value;
    }
    if let Some(value) = number(call, actor, Field::STATE_FADERIGHT, live)? {
        state.faderight = value;
    }
    if let Some(value) = number(call, actor, Field::STATE_FADETOP, live)? {
        state.fadetop = value;
    }
    if let Some(value) = number(call, actor, Field::STATE_FADEBOTTOM, live)? {
        state.fadebottom = value;
    }
    if let Some(value) = number(call, actor, Field::STATE_ZOOM, live)? {
        state.zoom = value;
    }
    if let Some(value) = number(call, actor, Field::STATE_ZOOM_X, live)? {
        state.zoom_x = value;
    }
    if let Some(value) = number(call, actor, Field::STATE_ZOOM_Y, live)? {
        state.zoom_y = value;
    }
    if let Some(value) = number(call, actor, Field::STATE_ZOOM_Z, live)? {
        state.zoom_z = value;
    }
    if let Some(value) = number(call, actor, Field::STATE_BASEZOOM, live)? {
        state.basezoom = value;
    }
    if let Some(value) = number(call, actor, Field::STATE_BASEZOOM_X, live)? {
        state.basezoom_x = value;
    }
    if let Some(value) = number(call, actor, Field::STATE_BASEZOOM_Y, live)? {
        state.basezoom_y = value;
    }
    if let Some(value) = number(call, actor, Field::STATE_BASEZOOM_Z, live)? {
        state.basezoom_z = value;
    }
    if let Some(value) = number(call, actor, Field::STATE_BASE_ALPHA, live)? {
        state.base_alpha = value;
    }
    if let Some(value) = number(call, actor, Field::STATE_ROT_X_DEG, live)? {
        state.rot_x_deg = value;
    }
    if let Some(value) = number(call, actor, Field::STATE_ROT_Y_DEG, live)? {
        state.rot_y_deg = value;
    }
    if let Some(value) = number(call, actor, Field::STATE_ROT_Z_DEG, live)? {
        state.rot_z_deg = value;
    }
    if let Some(value) = number(call, actor, Field::STATE_SKEW_X, live)? {
        state.skew_x = value;
    }
    if let Some(value) = number(call, actor, Field::STATE_SKEW_Y, live)? {
        state.skew_y = value;
    }
    if let Some(value) = number(call, actor, Field::STATE_EFFECT_PERIOD, live)? {
        state.effect_period = value;
    }
    if let Some(value) = number(call, actor, Field::STATE_EFFECT_OFFSET, live)? {
        state.effect_offset = value;
    }
    if let Some(value) = number(call, actor, Field::STATE_TEXT_DISTORTION, live)? {
        state.text_distortion = value;
    }
    if let Some(value) = number(call, actor, Field::STATE_SPRITE_PLAYBACK_RATE, live)? {
        state.sprite_playback_rate = value;
    }
    if let Some(value) = number(call, actor, Field::STATE_SPRITE_STATE_DELAY, live)? {
        state.sprite_state_delay = value;
    }
    if let Some(value) = number(call, actor, Field::STATE_MAX_WIDTH, live)? {
        state.max_width = Some(value);
    }
    if let Some(value) = number(call, actor, Field::STATE_MAX_HEIGHT, live)? {
        state.max_height = Some(value);
    }
    if let Some(value) = flag(call, actor, Field::STATE_DRAW_BY_Z_POSITION, live)? {
        state.draw_by_z_position = value;
    }
    if let Some(value) = flag(call, actor, Field::STATE_AFT_PRESERVE, live)? {
        state.aft_preserve = value;
    }
    if let Some(value) = flag(call, actor, Field::STATE_AFT_CREATED, live)? {
        state.aft_created = value;
    }
    if let Some(value) = flag(call, actor, Field::STATE_UPPERCASE, live)? {
        state.uppercase = value;
    }
    if let Some(value) = flag(call, actor, Field::STATE_MASK_SOURCE, live)? {
        state.mask_source = value;
    }
    if let Some(value) = flag(call, actor, Field::STATE_MASK_DEST, live)? {
        state.mask_dest = value;
    }
    if let Some(value) = flag(call, actor, Field::STATE_DEPTH_TEST, live)? {
        state.depth_test = value;
    }
    if let Some(value) = flag(call, actor, Field::STATE_DEPTH_WRITE, live)? {
        state.depth_write = value;
    }
    if let Some(value) = flag(call, actor, Field::STATE_DEPTH_TEST_ON_FAIL, live)? {
        state.depth_test_on_fail = value;
    }
    if let Some(value) = flag(call, actor, Field::STATE_VIBRATE, live)? {
        state.vibrate = value;
    }
    if let Some(value) = flag(call, actor, Field::STATE_RAINBOW, live)? {
        state.rainbow = value;
    }
    if let Some(value) = flag(call, actor, Field::STATE_RAINBOW_SCROLL, live)? {
        state.rainbow_scroll = value;
    }
    if let Some(value) = flag(call, actor, Field::STATE_TEXT_JITTER, live)? {
        state.text_jitter = value;
    }
    if let Some(value) = flag(call, actor, Field::STATE_MULT_ATTRS_WITH_DIFFUSE, live)? {
        state.mult_attrs_with_diffuse = value;
    }
    if let Some(value) = flag(call, actor, Field::STATE_SPRITE_ANIMATE, live)? {
        state.sprite_animate = value;
    }
    if let Some(value) = flag(call, actor, Field::STATE_SPRITE_LOOP, live)? {
        state.sprite_loop = value;
    }
    if let Some(value) = flag(call, actor, Field::STATE_MAX_W_PRE_ZOOM, live)? {
        state.max_w_pre_zoom = value;
    }
    if let Some(value) = flag(call, actor, Field::STATE_MAX_H_PRE_ZOOM, live)? {
        state.max_h_pre_zoom = value;
    }
    if let Some(value) = flag(call, actor, Field::STATE_MAX_DIMENSION_USES_ZOOM, live)? {
        state.max_dimension_uses_zoom = value;
    }
    if let Some(value) = flag(call, actor, Field::STATE_DECODE_MOVIE, live)? {
        state.decode_movie = value;
    }
    if let Some(value) = flag(call, actor, Field::STATE_TEXTURE_FILTERING, live)? {
        state.texture_filtering = value;
    }
    if let Some(value) = flag(call, actor, Field::STATE_TEXTURE_WRAPPING, live)? {
        state.texture_wrapping = value;
    }
    if let Some(value) = flag(call, actor, Field::STATE_EFFECT_TIMER, live)? {
        state.effect_timer = value;
    }
    if let Some(value) = flag(call, actor, Field::STATE_EFFECT_NO_OFFSET, live)? {
        state.effect_no_offset = value;
    }
    let fov = number(call, actor, Field::STATE_FOV, live)?;
    let declaration = number(call, actor, Field::FOV, live)?;
    // Materialized frames already consumed FOV. A later fov(-1) leaves None;
    // falling back to the declaration here would accidentally enable it again.
    let fallback = if call.store().actors.contains_key(&actor) {
        state.fov
    } else {
        declaration
    };
    state.fov = fov.or(fallback).filter(|value| *value != -1.);
    if let Some(value) = integer(
        call,
        actor,
        Field::STATE_DRAW_ORDER,
        i32::MIN as f64,
        i32::MAX as f64,
        live,
    )? {
        state.draw_order = value as i32;
    }
    if let Some(value) = integer(
        call,
        actor,
        Field::STATE_VERT_SPACING,
        i32::MIN as f64,
        i32::MAX as f64,
        live,
    )? {
        state.vert_spacing = Some(value as i32);
    }
    if let Some(value) = integer(
        call,
        actor,
        Field::STATE_WRAP_WIDTH_PIXELS,
        i32::MIN as f64,
        i32::MAX as f64,
        live,
    )? {
        state.wrap_width_pixels = Some(value as i32);
    }
    if let Some(value) = integer(
        call,
        actor,
        Field::STATE_SPRITE_STATE_INDEX,
        u32::MIN as f64,
        u32::MAX as f64,
        live,
    )? {
        state.sprite_state_index = Some(value as u32);
    }
    if let Some(value) = vector::<4>(call, actor, Field::STATE_DIFFUSE, live)? {
        state.diffuse = value;
    }
    if let Some(value) = vector::<2>(call, actor, Field::STATE_SHADOW_LEN, live)? {
        state.shadow_len = value;
    }
    if let Some(value) = vector::<4>(call, actor, Field::STATE_SHADOW_COLOR, live)? {
        state.shadow_color = value;
    }
    if let Some(value) = vector::<4>(call, actor, Field::STATE_GLOW, live)? {
        state.glow = value;
    }
    if let Some(value) = vector::<2>(call, actor, Field::STATE_VANISHPOINT, live)? {
        state.vanishpoint = Some(value);
    }
    if let Some(value) = vector::<4>(call, actor, Field::STATE_QUATERNION, live)? {
        state.quaternion = value;
    }
    if let Some(value) = vector::<3>(call, actor, Field::STATE_BASE_ROTATION, live)? {
        state.base_rotation = value;
    }
    if let Some(value) = vector::<3>(call, actor, Field::STATE_EFFECT_MAGNITUDE, live)? {
        state.effect_magnitude = value;
    }
    if let Some(value) = vector::<4>(call, actor, Field::STATE_EFFECT_COLOR1, live)? {
        state.effect_color1 = value;
    }
    if let Some(value) = vector::<4>(call, actor, Field::STATE_EFFECT_COLOR2, live)? {
        state.effect_color2 = value;
    }
    if let Some(value) = vector::<5>(call, actor, Field::STATE_EFFECT_TIMING, live)? {
        state.effect_timing = Some(value);
    }
    if let Some(value) = vector::<2>(call, actor, Field::STATE_TEXCOORD_OFFSET, live)? {
        state.texcoord_offset = Some(value);
    }
    if let Some(value) = vector::<4>(call, actor, Field::STATE_CUSTOM_TEXTURE_RECT, live)? {
        state.custom_texture_rect = Some(value);
    }
    if let Some(value) = vector::<2>(call, actor, Field::STATE_TEXCOORD_VELOCITY, live)? {
        state.texcoord_velocity = Some(value);
    }
    if let Some(value) = vector::<2>(call, actor, Field::STATE_SIZE, live)? {
        state.size = Some(value);
    }
    if let Some(value) = vector::<4>(call, actor, Field::STATE_STRETCH_RECT, live)? {
        state.stretch_rect = Some(value);
    }
    if let Some(colors) = optional_table(call, actor, Field::STATE_VERTEX_COLORS, live)? {
        if let Some(out) = vertex_colors(call, colors)? {
            state.vertex_colors = Some(out);
        }
    }
    if let Some(value) = text(call, actor, Field::STATE_TEXT_ALIGN, live)?
        .as_deref()
        .and_then(crate::parse_overlay_text_align)
    {
        state.text_align = value;
    }
    if let Some(value) = text(call, actor, Field::STATE_EFFECT_CLOCK, live)?
        .as_deref()
        .and_then(crate::parse_overlay_effect_clock)
    {
        state.effect_clock = value;
    }
    if let Some(value) = text(call, actor, Field::STATE_TEXT_GLOW_MODE, live)?
        .as_deref()
        .and_then(crate::parse_overlay_text_glow_mode)
    {
        state.text_glow_mode = value;
    }
    if let Some(value) = text(call, actor, Field::STATE_BLEND, live)?
        .as_deref()
        .and_then(crate::parse_overlay_blend_mode)
    {
        state.blend = value;
    }
    if let Some(value) = text(call, actor, Field::STATE_EFFECT_MODE, live)?
        .as_deref()
        .and_then(crate::parse_overlay_effect_mode)
    {
        state.effect_mode = value;
    }
    Ok(state)
}
fn number(
    call: &mut HostCall<'_>,
    actor: TableId,
    key: Field,
    live: bool,
) -> Result<Option<f32>, Error> {
    let value = get(call, actor, key, live)?;
    if value == Value::Nil {
        return Ok(None);
    }
    sprites::number(call, value).map(|value| Some(value as f32))
}
fn integer(
    call: &mut HostCall<'_>,
    actor: TableId,
    key: Field,
    min: f64,
    max: f64,
    live: bool,
) -> Result<Option<i64>, Error> {
    let value = get(call, actor, key, live)?;
    if value == Value::Nil {
        return Ok(None);
    }
    let value = sprites::number(call, value)?.trunc();
    if !(min..=max).contains(&value) {
        return Err(call.error(format!("{}: integer out of range", key.name()).as_bytes()));
    }
    Ok(Some(value as i64))
}
fn flag(
    call: &mut HostCall<'_>,
    actor: TableId,
    key: Field,
    live: bool,
) -> Result<Option<bool>, Error> {
    let value = get(call, actor, key, live)?;
    Ok((value != Value::Nil).then_some(!matches!(value, Value::Nil | Value::Boolean(false))))
}
pub(super) fn value_text(call: &mut HostCall<'_>, value: Value) -> Result<Option<String>, Error> {
    if value == Value::Nil {
        return Ok(None);
    }
    let id = call
        .coerce_string(value)?
        .ok_or_else(|| call.error(b"actor state: string expected"))?;
    let text = std::str::from_utf8(call.string_bytes(id).unwrap())
        .map_err(|e| call.error(e.to_string().as_bytes()))?;
    Ok(Some(text.to_owned()))
}
fn text(
    call: &mut HostCall<'_>,
    actor: TableId,
    key: Field,
    live: bool,
) -> Result<Option<StateText>, Error> {
    let value = get(call, actor, key, live)?;
    if value == Value::Nil {
        return Ok(None);
    }
    let id = call
        .coerce_string(value)?
        .ok_or_else(|| call.error(b"actor state: string expected"))?;
    let bytes = call.string_bytes(id).unwrap();
    std::str::from_utf8(bytes).map_err(|e| call.error(e.to_string().as_bytes()))?;
    Ok(Some(StateText(smallvec::SmallVec::from_slice(bytes))))
}
// Actor class and enum names usually fit inline. Spill without truncation for
// authored long strings, retaining the same UTF-8 validation and coercions.
struct StateText(smallvec::SmallVec<[u8; 64]>);
impl std::ops::Deref for StateText {
    type Target = str;
    fn deref(&self) -> &str {
        std::str::from_utf8(&self.0).expect("validated actor state text")
    }
}
fn optional_table(
    call: &mut HostCall<'_>,
    actor: TableId,
    key: Field,
    live: bool,
) -> Result<Option<TableId>, Error> {
    let value = get(call, actor, key, live)?;
    if value == Value::Nil {
        return Ok(None);
    }
    table(call, value).map(Some)
}
fn vector<const N: usize>(
    call: &mut HostCall<'_>,
    actor: TableId,
    key: Field,
    live: bool,
) -> Result<Option<[f32; N]>, Error> {
    match optional_table(call, actor, key, live)? {
        Some(t) => components(call, t),
        None => Ok(None),
    }
}
pub(super) fn components<const N: usize>(
    call: &mut HostCall<'_>,
    t: TableId,
) -> Result<Option<[f32; N]>, Error> {
    let mut values = [0.; N];
    for (i, dest) in values.iter_mut().enumerate() {
        let value = call.raw_table_get(t, Value::Number((i + 1) as f64))?;
        // Vector conversion ignores malformed components; scalar properties error.
        let converted = if matches!(value, Value::Number(_)) {
            value
        } else {
            call.call(helper(call, 6)?, &[value])?[0]
        };
        let Value::Number(value) = converted else {
            return Ok(None);
        };
        *dest = value as f32;
    }
    Ok(Some(values))
}

/// Decode the existing four raw rows without invoking authored indexing hooks.
pub(super) fn vertex_colors(
    call: &mut HostCall<'_>,
    colors: TableId,
) -> Result<Option<[[f32; 4]; 4]>, Error> {
    let mut out = [[0.; 4]; 4];
    for (i, row) in out.iter_mut().enumerate() {
        let Value::Table(t) = call.raw_table_get(colors, Value::Number((i + 1) as f64))? else {
            return Ok(None);
        };
        let Some(value) = components(call, t)? else {
            return Ok(None);
        };
        *row = value;
    }
    Ok(Some(out))
}
