//! Lua-owned manual-draw snapshot for the compiler's existing state reader.
use super::*;
use crate::{
    SongLuaOverlayState, SongLuaOverlayUpdateTarget as Target, SongLuaOverlayUpdateValue as Update,
};
const FIELDS: &[(Target, &str)] = &[
    (Target::Aux, "__songlua_state_aux"),
    (Target::X, "__songlua_state_x"),
    (Target::Y, "__songlua_state_y"),
    (Target::Z, "__songlua_state_z"),
    (Target::ZBias, "__songlua_state_z_bias"),
    (Target::DrawOrder, "__songlua_state_draw_order"),
    (
        Target::DrawByZPosition,
        "__songlua_state_draw_by_z_position",
    ),
    (Target::AftPreserve, "__songlua_state_aft_preserve"),
    (Target::AftCreated, "__songlua_state_aft_created"),
    (Target::HAlign, "__songlua_state_halign"),
    (Target::VAlign, "__songlua_state_valign"),
    (Target::TextAlign, "__songlua_state_text_align"),
    (Target::Uppercase, "__songlua_state_uppercase"),
    (Target::ShadowLen, "__songlua_state_shadow_len"),
    (Target::ShadowColor, "__songlua_state_shadow_color"),
    (Target::Glow, "__songlua_state_glow"),
    (Target::Fov, "__songlua_state_fov"),
    (Target::Vanishpoint, "__songlua_state_vanishpoint"),
    (Target::Diffuse, "__songlua_state_diffuse"),
    (Target::VertexColors, "__songlua_state_vertex_colors"),
    (Target::Visible, "__songlua_state_visible"),
    (Target::Hibernating, "__songlua_state_hibernating"),
    (Target::CropLeft, "__songlua_state_cropleft"),
    (Target::CropRight, "__songlua_state_cropright"),
    (Target::CropTop, "__songlua_state_croptop"),
    (Target::CropBottom, "__songlua_state_cropbottom"),
    (Target::FadeLeft, "__songlua_state_fadeleft"),
    (Target::FadeRight, "__songlua_state_faderight"),
    (Target::FadeTop, "__songlua_state_fadetop"),
    (Target::FadeBottom, "__songlua_state_fadebottom"),
    (Target::MaskSource, "__songlua_state_mask_source"),
    (Target::MaskDest, "__songlua_state_mask_dest"),
    (Target::DepthTest, "__songlua_state_depth_test"),
    (Target::DepthWrite, "__songlua_state_depth_write"),
    (
        Target::DepthTestOnFail,
        "__songlua_state_depth_test_on_fail",
    ),
    (Target::CullMode, "__songlua_state_cull_mode"),
    (Target::Zoom, "__songlua_state_zoom"),
    (Target::ZoomX, "__songlua_state_zoom_x"),
    (Target::ZoomY, "__songlua_state_zoom_y"),
    (Target::ZoomZ, "__songlua_state_zoom_z"),
    (Target::BaseZoom, "__songlua_state_basezoom"),
    (Target::BaseZoomX, "__songlua_state_basezoom_x"),
    (Target::BaseZoomY, "__songlua_state_basezoom_y"),
    (Target::BaseZoomZ, "__songlua_state_basezoom_z"),
    (Target::RotationX, "__songlua_state_rot_x_deg"),
    (Target::RotationY, "__songlua_state_rot_y_deg"),
    (Target::RotationZ, "__songlua_state_rot_z_deg"),
    (Target::BaseRotation, "__songlua_state_base_rotation"),
    (Target::Quaternion, "__songlua_state_quaternion"),
    (Target::BaseAlpha, "__songlua_state_base_alpha"),
    (Target::SkewX, "__songlua_state_skew_x"),
    (Target::SkewY, "__songlua_state_skew_y"),
    (Target::Blend, "__songlua_state_blend"),
    (Target::Vibrate, "__songlua_state_vibrate"),
    (Target::EffectMagnitude, "__songlua_state_effect_magnitude"),
    (Target::EffectClock, "__songlua_state_effect_clock"),
    (Target::EffectTimer, "__songlua_state_effect_timer"),
    (Target::EffectNoOffset, "__songlua_state_effect_no_offset"),
    (Target::EffectMode, "__songlua_state_effect_mode"),
    (Target::EffectColor1, "__songlua_state_effect_color1"),
    (Target::EffectColor2, "__songlua_state_effect_color2"),
    (Target::EffectPeriod, "__songlua_state_effect_period"),
    (Target::EffectOffset, "__songlua_state_effect_offset"),
    (Target::EffectTiming, "__songlua_state_effect_timing"),
    (Target::Rainbow, "__songlua_state_rainbow"),
    (Target::RainbowScroll, "__songlua_state_rainbow_scroll"),
    (Target::TextJitter, "__songlua_state_text_jitter"),
    (Target::TextDistortion, "__songlua_state_text_distortion"),
    (Target::TextGlowMode, "__songlua_state_text_glow_mode"),
    (
        Target::MultAttrsWithDiffuse,
        "__songlua_state_mult_attrs_with_diffuse",
    ),
    (Target::SpriteTexture, "__songlua_state_sprite_texture"),
    (Target::SpriteAnimate, "__songlua_state_sprite_animate"),
    (Target::SpriteLoop, "__songlua_state_sprite_loop"),
    (
        Target::SpritePlaybackRate,
        "__songlua_state_sprite_playback_rate",
    ),
    (
        Target::SpriteStateDelay,
        "__songlua_state_sprite_state_delay",
    ),
    (
        Target::SpriteStateIndex,
        "__songlua_state_sprite_state_index",
    ),
    (Target::VertSpacing, "__songlua_state_vert_spacing"),
    (Target::WrapWidthPixels, "__songlua_state_wrap_width_pixels"),
    (Target::MaxWidth, "__songlua_state_max_width"),
    (Target::MaxHeight, "__songlua_state_max_height"),
    (Target::MaxWPreZoom, "__songlua_state_max_w_pre_zoom"),
    (Target::MaxHPreZoom, "__songlua_state_max_h_pre_zoom"),
    (
        Target::MaxDimensionUsesZoom,
        "__songlua_state_max_dimension_uses_zoom",
    ),
    (
        Target::TextureFiltering,
        "__songlua_state_texture_filtering",
    ),
    (Target::TextureWrapping, "__songlua_state_texture_wrapping"),
    (Target::TexcoordOffset, "__songlua_state_texcoord_offset"),
    (
        Target::CustomTextureRect,
        "__songlua_state_custom_texture_rect",
    ),
    (
        Target::TexcoordVelocity,
        "__songlua_state_texcoord_velocity",
    ),
    (Target::Size, "__songlua_state_size"),
    (Target::StretchRect, "__songlua_state_stretch_rect"),
];
const COPY: &[u8] = br#"
local type,next,rawget,rawset,sub,error = type,next,rawget,rawset,string.sub,error
local exact={__songlua_visible=true,__songlua_native_name=true,__songlua_wrappers=true,
 __songlua_hibernate_seconds=true,__songlua_theme_hibernating=true,__songlua_update_rate=true,
 __songlua_current_x=true,__songlua_current_y=true,__songlua_current_z=true,
 __songlua_diffuse=true,__songlua_text_attributes=true,__songlua_stroke_color=true,
 __songlua_aux=true,__songlua_stream_width=true,__songlua_sprite_animation_length_seconds=true,
 __songlua_sprite_effect_mode=true,Text=true,Texture=true,File=true,StreamWidth=true}
local function clone(v)
 if type(v)~='table' then return v end
 local out={};for k,x in next,v do rawset(out,clone(k),clone(x)) end;return out
end
return function(actor,mutable)
 local out={}
 for k,v in next,actor do
  if type(k)=='string' and (exact[k] or sub(k,1,16)=='__songlua_state_' or
   sub(k,1,24)=='__songlua_graph_display_' or sub(k,1,19)=='__songlua_scroller_' or
   sub(k,1,14)=='__songlua_aft_' or sub(k,1,23)=='__songlua_command_queue' or
   sub(k,1,27)=='__songlua_recurring_update_' or
   (mutable and sub(k,1,18)=='__songlua_capture_')) then
    if k=='__songlua_wrappers' and type(v)=='table' then
     local copy={};local i=1;while rawget(v,i)~=nil do
      local w=rawget(v,i);if type(w)~='table'then error('wrapper table expected')end
      copy[i]=w;i=i+1
     end;rawset(out,k,copy)
    else rawset(out,k,clone(v)) end
  end
 end
 return out
end
"#;
pub(super) fn install(vm: &mut Vm, helpers: TableId) -> Result<(), Error> {
    let code = compile_named(COPY, b"=manual actor draw snapshot")?;
    let function = vm.execute(&code)?[0];
    super::super::put(vm, helpers, "manual_state_snapshot", function)
}
pub(super) fn write(
    call: &mut HostCall<'_>,
    actor: TableId,
    state: &SongLuaOverlayState,
) -> Result<TableId, Error> {
    validate_keys(call, actor)?;
    let snapshot = copy_validated(call, actor)?;
    let value = Value::Table(snapshot);
    let live = call
        .store()
        .clock
        .live
        .load(std::sync::atomic::Ordering::Relaxed);
    call.with_roots(&[value], |call| {
        let default = SongLuaOverlayState::default();
        for &(target, key) in FIELDS {
            // The constructor infers an unbound Sprite in Rust metadata, but
            // native semantic snapshots omit that inferred flag. With no actor
            // type on the snapshot, its later reader defaults to a bound texture.
            // Explicit texture setters publish the flag into the raw table.
            if target == Target::SpriteTexture
                && get_static(call, snapshot, key.as_bytes())? == Value::Nil
            {
                continue;
            }
            let value = crate::perframe::overlay_state_update_value(state, target);
            let overrides = live && live_override(target);
            if !overrides
                && (get_static(call, snapshot, key.as_bytes())? != Value::Nil
                    || value == crate::perframe::overlay_state_update_value(&default, target))
            {
                continue;
            }
            let value = encode(call, &value)?;
            song_replay::raw_set(call, snapshot, key, value)?;
        }
        for (key, value, changed) in [
            (
                "__songlua_visible",
                Value::Boolean(state.visible),
                !state.visible,
            ),
            (
                "__songlua_hibernate_seconds",
                Value::Number(if state.hibernating { 1. } else { 0. }),
                state.hibernating,
            ),
            (
                "__songlua_state_decode_movie",
                Value::Boolean(state.decode_movie),
                state.decode_movie,
            ),
        ] {
            if changed && get_static(call, snapshot, key.as_bytes())? == Value::Nil {
                song_replay::raw_set(call, snapshot, key, value)?;
            }
        }
        Ok(snapshot)
    })
}
fn validate_keys(call: &mut HostCall<'_>, actor: TableId) -> Result<(), Error> {
    // Native snapshot iteration validates string keys but never calls authored
    // property getters. Preserve that boundary before copying raw semantic data.
    let mut key = Value::Nil;
    while let Some((next, _)) = call.raw_table_next(actor, key)? {
        key = next;
        if let Value::String(id) = key {
            let valid = call
                .string_bytes(id)
                .is_some_and(|bytes| std::str::from_utf8(bytes).is_ok());
            if !valid {
                return Err(call.error(b"actor snapshot key is not UTF-8"));
            }
        }
    }
    Ok(())
}

/// Copy only native semantic fields; inferred Rust defaults are kept separately.
pub(super) fn raw_copy(call: &mut HostCall<'_>, actor: TableId) -> Result<TableId, Error> {
    copy_selected(call, actor, false)
}
pub(super) fn raw_copy_mutable(call: &mut HostCall<'_>, actor: TableId) -> Result<TableId, Error> {
    copy_selected(call, actor, true)
}
fn copy_selected(call: &mut HostCall<'_>, actor: TableId, mutable: bool) -> Result<TableId, Error> {
    let validate = call.new_host_function(
        |call| {
            let actor = table(call, call.captures()[0])?;
            validate_keys(call, actor)
        },
        &[Value::Table(actor)],
    )?;
    call.call(validate, &[])?;
    copy(call, actor, mutable)
}
fn copy_validated(call: &mut HostCall<'_>, actor: TableId) -> Result<TableId, Error> {
    copy(call, actor, false)
}

/// The snapshot fields of `actor`, copied as the `manual_state_snapshot`
/// helper copies them.
pub(super) fn copy(
    call: &mut HostCall<'_>,
    actor: TableId,
    mutable: bool,
) -> Result<TableId, Error> {
    match copy_native(call, actor, mutable)? {
        Some(copy) => Ok(copy),
        None => copy_with_helper(call, actor, mutable),
    }
}
pub(super) fn copy_with_helper(
    call: &mut HostCall<'_>,
    actor: TableId,
    mutable: bool,
) -> Result<TableId, Error> {
    let function = native_children::helper_function(call, b"manual_state_snapshot")?;
    let value = call.call(function, &[Value::Table(actor), Value::Boolean(mutable)])?[0];
    table(call, value)
}

// How many tables deep a field's value is copied here. Actor state nests two
// (vertex colours) or three (text attributes).
const COPY_DEPTH: usize = 4;

/// The helper's copy without entering Lua, for the shapes actor tables have:
/// the same entries, written in the same order. `None` leaves the table to
/// the helper, which defines the result: a field name that is not UTF-8, a
/// wrapper that is not a table, a table used as a key, or a value nested
/// deeper than `COPY_DEPTH`.
pub(super) fn copy_native(
    call: &mut HostCall<'_>,
    actor: TableId,
    mutable: bool,
) -> Result<Option<TableId>, Error> {
    let out = call.try_new_table()?;
    call.with_roots(&[Value::Table(out)], |call| {
        let mut key = Value::Nil;
        while let Some((next, value)) = call.raw_table_next(actor, key)? {
            key = next;
            let Value::String(id) = key else {
                continue;
            };
            let (selected, wrappers) = match call.string_bytes(id).map(std::str::from_utf8) {
                Some(Ok(name)) => (
                    semantic(name) || mutable && name.starts_with("__songlua_capture_"),
                    name == "__songlua_wrappers",
                ),
                _ => return Ok(None),
            };
            if !selected {
                continue;
            }
            let copied = match value {
                Value::Table(list) if wrappers => wrapper_list(call, list)?,
                value => clone_value(call, value, COPY_DEPTH)?,
            };
            let Some(copied) = copied else {
                return Ok(None);
            };
            call.raw_table_set(out, key, copied)?;
        }
        Ok(Some(out))
    })
}
// A new list of the same wrapper tables.
fn wrapper_list(call: &mut HostCall<'_>, list: TableId) -> Result<Option<Value>, Error> {
    let out = call.try_new_table()?;
    for index in 1usize.. {
        let index = Value::Number(index as f64);
        match call.raw_table_get(list, index)? {
            Value::Nil => break,
            wrapper @ Value::Table(_) => call.raw_table_set(out, index, wrapper)?,
            _ => return Ok(None),
        }
    }
    Ok(Some(Value::Table(out)))
}
// The helper's `clone`: tables are copied entry by entry, other values kept.
fn clone_value(
    call: &mut HostCall<'_>,
    value: Value,
    depth: usize,
) -> Result<Option<Value>, Error> {
    let Value::Table(source) = value else {
        return Ok(Some(value));
    };
    if depth == 0 {
        return Ok(None);
    }
    let out = call.try_new_table()?;
    call.with_roots(&[Value::Table(out)], |call| {
        let mut key = Value::Nil;
        while let Some((next, value)) = call.raw_table_next(source, key)? {
            key = next;
            if matches!(key, Value::Table(_)) {
                return Ok(None);
            }
            let Some(value) = clone_value(call, value, depth - 1)? else {
                return Ok(None);
            };
            call.raw_table_set(out, key, value)?;
        }
        Ok(Some(Value::Table(out)))
    })
}
fn semantic(key: &str) -> bool {
    [
        "__songlua_state_",
        "__songlua_graph_display_",
        "__songlua_scroller_",
        "__songlua_aft_",
        "__songlua_command_queue",
        "__songlua_recurring_update_",
    ]
    .iter()
    .any(|prefix| key.starts_with(prefix))
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
/// Raw writes also permit cleanup after an execution limit. Ordinary rewinds
/// retain native __newindex callbacks and their ordering/error behavior.
pub(super) fn restore_raw(
    call: &mut HostCall<'_>,
    actor: TableId,
    saved: TableId,
    raw: bool,
) -> Result<(), Error> {
    restore_selected(call, actor, saved, raw, false)
}
pub(super) fn restore_mutable(
    call: &mut HostCall<'_>,
    actor: TableId,
    saved: TableId,
    raw: bool,
) -> Result<(), Error> {
    restore_selected(call, actor, saved, raw, true)
}
fn restore_selected(
    call: &mut HostCall<'_>,
    actor: TableId,
    saved: TableId,
    raw: bool,
    mutable: bool,
) -> Result<(), Error> {
    let output = Arc::new(Mutex::new(None::<RestoreEntries>));
    let scan = call.new_host_function_with_data(
        scan_restore,
        &[Value::Table(actor), Value::Table(saved)],
        (output.clone(), mutable),
    )?;
    call.call(scan, &[])?;
    let (keys, entries) = output.lock().unwrap().take().unwrap();
    for (key, value) in keys.into_iter().map(|key| (key, Value::Nil)).chain(entries) {
        if raw {
            call.raw_table_set(actor, key, value)?
        } else {
            call.table_set(Value::Table(actor), key, value)?
        }
    }
    Ok(())
}

type RestoreEntries = (Vec<Value>, Vec<(Value, Value)>);
fn scan_restore(call: &mut HostCall<'_>) -> Result<(), Error> {
    let actor = table(call, call.captures()[0])?;
    let saved = table(call, call.captures()[1])?;
    let (output, mutable) = call
        .function_data::<(Arc<Mutex<Option<RestoreEntries>>>, bool)>()
        .unwrap()
        .clone();
    let entries = {
        let mut stack = call.native_stack()?;
        let base = stack.top();
        let mut keys = Vec::new();
        let mut entries = Vec::new();
        stack.push(Value::Table(actor))?;
        let actor_index = stack.top_offset();
        stack.push(Value::Nil)?;
        while stack.next_table(actor_index)? {
            if matches!(stack.value(-2), Some(Value::String(_))) {
                let selected = std::str::from_utf8(stack.to_bytes(-2)?.unwrap())
                    .map(|key| semantic(key) || mutable && key.starts_with("__songlua_capture_"));
                match selected {
                    Ok(true) => keys.push(stack.value(-2).unwrap()),
                    Ok(false) => {}
                    Err(error) => return Err(stack.error(error.to_string().as_bytes())),
                }
            }
            stack.set_top(stack.top() - 1)?;
        }
        stack.set_top(base)?;
        stack.push(Value::Table(saved))?;
        let saved_index = stack.top_offset();
        stack.push(Value::Nil)?;
        while stack.next_table(saved_index)? {
            entries.push((stack.value(-2).unwrap(), stack.value(-1).unwrap()));
            stack.set_top(stack.top() - 1)?;
        }
        stack.set_top(base)?;
        (keys, entries)
    };
    *output.lock().unwrap() = Some(entries);
    Ok(())
}

fn live_override(target: Target) -> bool {
    matches!(
        target,
        Target::Aux
            | Target::X
            | Target::Y
            | Target::Z
            | Target::Zoom
            | Target::ZoomX
            | Target::ZoomY
            | Target::ZoomZ
            | Target::SkewX
            | Target::SkewY
            | Target::CropLeft
            | Target::CropRight
            | Target::CropTop
            | Target::CropBottom
            | Target::FadeLeft
            | Target::FadeRight
            | Target::FadeTop
            | Target::FadeBottom
            | Target::Diffuse
            | Target::Glow
            | Target::VertexColors
            | Target::DrawByZPosition
            | Target::MaskSource
            | Target::MaskDest
            | Target::DepthTest
            | Target::DepthWrite
            | Target::DepthTestOnFail
            | Target::TextureFiltering
            | Target::TextureWrapping
            | Target::BaseZoom
            | Target::BaseZoomX
            | Target::BaseZoomY
            | Target::BaseZoomZ
            | Target::HAlign
            | Target::VAlign
    )
}
pub(super) fn publish(
    call: &mut HostCall<'_>,
    actor: TableId,
    target: Target,
    value: &Update,
) -> Result<(), Error> {
    let key = FIELDS
        .iter()
        .find_map(|&(field, key)| (field == target).then_some(key))
        .ok_or_else(|| call.error(b"actor property has no raw state field"))?;
    let value = encode(call, value)?;
    call.with_roots(&[value], |call| sprites::set(call, actor, key, value))
}

fn encode(call: &mut HostCall<'_>, value: &Update) -> Result<Value, Error> {
    Ok(match value {
        Update::None => Value::Nil,
        Update::F32(n) => Value::Number(f64::from(*n)),
        Update::I32(n) => Value::Number(f64::from(*n)),
        Update::U32(n) => Value::Number(f64::from(*n)),
        Update::Bool(b) => Value::Boolean(*b),
        Update::Vec2(v) => sprites::vector_value(call, v)?,
        Update::Vec3(v) => sprites::vector_value(call, v)?,
        Update::Vec4(v) | Update::Quaternion(v) => sprites::vector_value(call, v)?,
        Update::Vec5(v) => sprites::vector_value(call, v)?,
        Update::VertexColors(v) => {
            let rows = call.try_new_table()?;
            call.with_roots(&[Value::Table(rows)], |call| {
                for (i, v) in v.iter().enumerate() {
                    let value = sprites::vector_value(call, v)?;
                    call.raw_table_set(rows, Value::Number((i + 1) as f64), value)?;
                }
                Ok(Value::Table(rows))
            })?
        }
        Update::CullMode(mode) => Value::Number(match mode {
            deadlib_render_core::CullMode::None => 0.,
            deadlib_render_core::CullMode::Back => 1.,
            deadlib_render_core::CullMode::Front => 2.,
        }),
        Update::TextAlign(v) => call.try_intern(crate::overlay_text_align_label(*v).as_bytes())?,
        Update::EffectClock(v) => call.try_intern(crate::effect_clock_label(*v).as_bytes())?,
        Update::EffectMode(v) => call.try_intern(crate::effect_mode_label(*v).as_bytes())?,
        Update::TextGlowMode(v) => call.try_intern(crate::text_glow_mode_label(*v).as_bytes())?,
        Update::Blend(v) => call.try_intern(format!("{v:?}").as_bytes())?,
    })
}
