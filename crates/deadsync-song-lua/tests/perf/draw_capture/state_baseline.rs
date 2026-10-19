// Frozen state decoder from add695545; helpers and scalar-field enums are shared.
use super::*;

pub(crate) fn initial_state(actor: &Table) -> Result<SongLuaOverlayState, String> {
    read_actor_overlay_state(actor,
        |field| actor.raw_get::<Option<f32>>(field.key()),
        |field| actor.raw_get::<Option<bool>>(field.key()))
}

fn read_actor_overlay_state(
    actor: &Table,
    number: impl Fn(ActorNumberField) -> mlua::Result<Option<f32>>,
    flag: impl Fn(ActorFlagField) -> mlua::Result<Option<bool>>,
) -> Result<SongLuaOverlayState, String> {
    // These fields belong to this instance. Missing render state must not
    // invoke Lua class lookup for every default property on every frame.
    let mut state = SongLuaOverlayState::default();
    // Model::Model enables the Z buffer and texture wrapping before Lua overrides.
    state.depth_test = actor
        .raw_get::<Option<String>>("__songlua_actor_type")
        .map_err(|err| err.to_string())?
        .is_some_and(|kind| kind.eq_ignore_ascii_case("Model"));
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
                .raw_get::<Option<String>>("__songlua_actor_type")
                .map_err(|err| err.to_string())?
                .is_some_and(|kind| kind.eq_ignore_ascii_case("Sprite"))
                || actor
                    .get::<Option<String>>("Texture")
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
        .raw_get::<Option<Table>>("__songlua_state_shadow_len")
        .map_err(|err| err.to_string())?
        .and_then(|value| table_vec2(&value))
    {
        state.shadow_len = value;
    }
    if let Some(value) = actor
        .raw_get::<Option<Table>>("__songlua_state_shadow_color")
        .map_err(|err| err.to_string())?
        .and_then(|value| table_vec4(&value))
    {
        state.shadow_color = value;
    }
    if let Some(value) = actor
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
        .raw_get::<Option<Table>>("__songlua_state_base_rotation")
        .map_err(|err| err.to_string())?
        .and_then(|value| table_vec3(&value))
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
        .raw_get::<Option<Table>>("__songlua_state_effect_magnitude")
        .map_err(|err| err.to_string())?
        .and_then(|value| table_vec3(&value))
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
        .raw_get::<Option<Table>>("__songlua_state_effect_color1")
        .map_err(|err| err.to_string())?
        .and_then(|value| table_vec4(&value))
    {
        state.effect_color1 = value;
    }
    if let Some(value) = actor
        .raw_get::<Option<Table>>("__songlua_state_effect_color2")
        .map_err(|err| err.to_string())?
        .and_then(|value| table_vec4(&value))
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
        .raw_get::<Option<Table>>("__songlua_state_effect_timing")
        .map_err(|err| err.to_string())?
        .and_then(|value| table_vec5(&value))
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
        .raw_get::<Option<Table>>("__songlua_state_size")
        .map_err(|err| err.to_string())?
        .and_then(|value| table_vec2(&value))
    {
        state.size = Some(value);
    }
    if let Some(value) = actor
        .raw_get::<Option<Table>>("__songlua_state_stretch_rect")
        .map_err(|err| err.to_string())?
        .and_then(|value| table_vec4(&value))
    {
        state.stretch_rect = Some(value);
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
