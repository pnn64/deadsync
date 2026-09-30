// Frozen from a082c2239 (0.5.1627); visibility and formatting only.
use super::*;

pub(super) fn actor_overlay_initial_state(actor: &Table) -> Result<SongLuaOverlayState, String> {
    let mut state = SongLuaOverlayState::default();
    if let Some(visible) = actor
        .get::<Option<bool>>("__songlua_visible")
        .map_err(|err| err.to_string())?
    {
        state.visible = visible;
    }
    if let Some(diffuse) = actor
        .get::<Option<Table>>("__songlua_state_diffuse")
        .map_err(|err| err.to_string())?
        .and_then(|value| table_vec4(&value))
    {
        state.diffuse = diffuse;
    }
    if let Some(colors) = actor
        .get::<Option<Table>>("__songlua_state_vertex_colors")
        .map_err(|err| err.to_string())?
        .and_then(|value| table_vertex_colors(&value))
    {
        state.vertex_colors = Some(colors);
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_x")
        .map_err(|err| err.to_string())?
    {
        state.x = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_y")
        .map_err(|err| err.to_string())?
    {
        state.y = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_z")
        .map_err(|err| err.to_string())?
    {
        state.z = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_z_bias")
        .map_err(|err| err.to_string())?
    {
        state.z_bias = value;
    }
    if let Some(value) = actor
        .get::<Option<i32>>("__songlua_state_draw_order")
        .map_err(|err| err.to_string())?
    {
        state.draw_order = value;
    }
    if let Some(value) = actor
        .get::<Option<bool>>("__songlua_state_draw_by_z_position")
        .map_err(|err| err.to_string())?
    {
        state.draw_by_z_position = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_halign")
        .map_err(|err| err.to_string())?
    {
        state.halign = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_valign")
        .map_err(|err| err.to_string())?
    {
        state.valign = value;
    }
    if let Some(value) = actor
        .get::<Option<String>>("__songlua_state_text_align")
        .map_err(|err| err.to_string())?
        .as_deref()
        .and_then(parse_overlay_text_align)
    {
        state.text_align = value;
    }
    if let Some(value) = actor
        .get::<Option<bool>>("__songlua_state_uppercase")
        .map_err(|err| err.to_string())?
    {
        state.uppercase = value;
    }
    if let Some(value) = actor
        .get::<Option<Table>>("__songlua_state_shadow_len")
        .map_err(|err| err.to_string())?
        .and_then(|value| table_vec2(&value))
    {
        state.shadow_len = value;
    }
    if let Some(value) = actor
        .get::<Option<Table>>("__songlua_state_shadow_color")
        .map_err(|err| err.to_string())?
        .and_then(|value| table_vec4(&value))
    {
        state.shadow_color = value;
    }
    if let Some(value) = actor
        .get::<Option<Table>>("__songlua_state_glow")
        .map_err(|err| err.to_string())?
        .and_then(|value| table_vec4(&value))
    {
        state.glow = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_fov")
        .map_err(|err| err.to_string())?
    {
        state.fov = Some(value);
    }
    if let Some(value) = actor
        .get::<Option<Table>>("__songlua_state_vanishpoint")
        .map_err(|err| err.to_string())?
        .and_then(|value| table_vec2(&value))
    {
        state.vanishpoint = Some(value);
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_cropleft")
        .map_err(|err| err.to_string())?
    {
        state.cropleft = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_cropright")
        .map_err(|err| err.to_string())?
    {
        state.cropright = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_croptop")
        .map_err(|err| err.to_string())?
    {
        state.croptop = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_cropbottom")
        .map_err(|err| err.to_string())?
    {
        state.cropbottom = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_fadeleft")
        .map_err(|err| err.to_string())?
    {
        state.fadeleft = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_faderight")
        .map_err(|err| err.to_string())?
    {
        state.faderight = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_fadetop")
        .map_err(|err| err.to_string())?
    {
        state.fadetop = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_fadebottom")
        .map_err(|err| err.to_string())?
    {
        state.fadebottom = value;
    }
    if let Some(value) = actor
        .get::<Option<bool>>("__songlua_state_mask_source")
        .map_err(|err| err.to_string())?
    {
        state.mask_source = value;
    }
    if let Some(value) = actor
        .get::<Option<bool>>("__songlua_state_mask_dest")
        .map_err(|err| err.to_string())?
    {
        state.mask_dest = value;
    }
    if let Some(value) = actor
        .get::<Option<bool>>("__songlua_state_depth_test")
        .map_err(|err| err.to_string())?
    {
        state.depth_test = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_zoom")
        .map_err(|err| err.to_string())?
    {
        state.zoom = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_zoom_x")
        .map_err(|err| err.to_string())?
    {
        state.zoom_x = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_zoom_y")
        .map_err(|err| err.to_string())?
    {
        state.zoom_y = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_zoom_z")
        .map_err(|err| err.to_string())?
    {
        state.zoom_z = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_basezoom")
        .map_err(|err| err.to_string())?
    {
        state.basezoom = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_basezoom_x")
        .map_err(|err| err.to_string())?
    {
        state.basezoom_x = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_basezoom_y")
        .map_err(|err| err.to_string())?
    {
        state.basezoom_y = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_basezoom_z")
        .map_err(|err| err.to_string())?
    {
        state.basezoom_z = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_rot_x_deg")
        .map_err(|err| err.to_string())?
    {
        state.rot_x_deg = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_rot_y_deg")
        .map_err(|err| err.to_string())?
    {
        state.rot_y_deg = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_rot_z_deg")
        .map_err(|err| err.to_string())?
    {
        state.rot_z_deg = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_skew_x")
        .map_err(|err| err.to_string())?
    {
        state.skew_x = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_skew_y")
        .map_err(|err| err.to_string())?
    {
        state.skew_y = value;
    }
    if let Some(value) = actor
        .get::<Option<bool>>("__songlua_state_vibrate")
        .map_err(|err| err.to_string())?
    {
        state.vibrate = value;
    }
    if let Some(value) = actor
        .get::<Option<Table>>("__songlua_state_effect_magnitude")
        .map_err(|err| err.to_string())?
        .and_then(|value| table_vec3(&value))
    {
        state.effect_magnitude = value;
    }
    if let Some(value) = actor
        .get::<Option<String>>("__songlua_state_effect_clock")
        .map_err(|err| err.to_string())?
        .as_deref()
        .and_then(parse_overlay_effect_clock)
    {
        state.effect_clock = value;
    }
    if let Some(value) = actor
        .get::<Option<Table>>("__songlua_state_effect_color1")
        .map_err(|err| err.to_string())?
        .and_then(|value| table_vec4(&value))
    {
        state.effect_color1 = value;
    }
    if let Some(value) = actor
        .get::<Option<Table>>("__songlua_state_effect_color2")
        .map_err(|err| err.to_string())?
        .and_then(|value| table_vec4(&value))
    {
        state.effect_color2 = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_effect_period")
        .map_err(|err| err.to_string())?
    {
        state.effect_period = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_effect_offset")
        .map_err(|err| err.to_string())?
    {
        state.effect_offset = value;
    }
    if let Some(value) = actor
        .get::<Option<Table>>("__songlua_state_effect_timing")
        .map_err(|err| err.to_string())?
        .and_then(|value| table_vec5(&value))
    {
        state.effect_timing = Some(value);
    }
    if let Some(value) = actor
        .get::<Option<bool>>("__songlua_state_rainbow")
        .map_err(|err| err.to_string())?
    {
        state.rainbow = value;
    }
    if let Some(value) = actor
        .get::<Option<bool>>("__songlua_state_rainbow_scroll")
        .map_err(|err| err.to_string())?
    {
        state.rainbow_scroll = value;
    }
    if let Some(value) = actor
        .get::<Option<bool>>("__songlua_state_text_jitter")
        .map_err(|err| err.to_string())?
    {
        state.text_jitter = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_text_distortion")
        .map_err(|err| err.to_string())?
    {
        state.text_distortion = value;
    }
    if let Some(value) = actor
        .get::<Option<String>>("__songlua_state_text_glow_mode")
        .map_err(|err| err.to_string())?
        .as_deref()
        .and_then(parse_overlay_text_glow_mode)
    {
        state.text_glow_mode = value;
    }
    if let Some(value) = actor
        .get::<Option<bool>>("__songlua_state_mult_attrs_with_diffuse")
        .map_err(|err| err.to_string())?
    {
        state.mult_attrs_with_diffuse = value;
    }
    if let Some(value) = actor
        .get::<Option<bool>>("__songlua_state_sprite_animate")
        .map_err(|err| err.to_string())?
    {
        state.sprite_animate = value;
    }
    if let Some(value) = actor
        .get::<Option<bool>>("__songlua_state_sprite_loop")
        .map_err(|err| err.to_string())?
    {
        state.sprite_loop = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_sprite_playback_rate")
        .map_err(|err| err.to_string())?
    {
        state.sprite_playback_rate = value;
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_sprite_state_delay")
        .map_err(|err| err.to_string())?
    {
        state.sprite_state_delay = value;
    }
    if let Some(value) = actor
        .get::<Option<i32>>("__songlua_state_vert_spacing")
        .map_err(|err| err.to_string())?
    {
        state.vert_spacing = Some(value);
    }
    if let Some(value) = actor
        .get::<Option<i32>>("__songlua_state_wrap_width_pixels")
        .map_err(|err| err.to_string())?
    {
        state.wrap_width_pixels = Some(value);
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_max_width")
        .map_err(|err| err.to_string())?
    {
        state.max_width = Some(value);
    }
    if let Some(value) = actor
        .get::<Option<f32>>("__songlua_state_max_height")
        .map_err(|err| err.to_string())?
    {
        state.max_height = Some(value);
    }
    if let Some(value) = actor
        .get::<Option<bool>>("__songlua_state_max_w_pre_zoom")
        .map_err(|err| err.to_string())?
    {
        state.max_w_pre_zoom = value;
    }
    if let Some(value) = actor
        .get::<Option<bool>>("__songlua_state_max_h_pre_zoom")
        .map_err(|err| err.to_string())?
    {
        state.max_h_pre_zoom = value;
    }
    if let Some(value) = actor
        .get::<Option<bool>>("__songlua_state_max_dimension_uses_zoom")
        .map_err(|err| err.to_string())?
    {
        state.max_dimension_uses_zoom = value;
    }
    if let Some(value) = actor
        .get::<Option<u32>>("__songlua_state_sprite_state_index")
        .map_err(|err| err.to_string())?
    {
        state.sprite_state_index = Some(value);
    }
    if let Some(value) = actor
        .get::<Option<bool>>("__songlua_state_decode_movie")
        .map_err(|err| err.to_string())?
    {
        state.decode_movie = value;
    }
    if let Some(value) = actor
        .get::<Option<bool>>("__songlua_state_texture_filtering")
        .map_err(|err| err.to_string())?
    {
        state.texture_filtering = value;
    }
    if let Some(value) = actor
        .get::<Option<bool>>("__songlua_state_texture_wrapping")
        .map_err(|err| err.to_string())?
    {
        state.texture_wrapping = value;
    }
    if let Some(value) = actor
        .get::<Option<Table>>("__songlua_state_texcoord_offset")
        .map_err(|err| err.to_string())?
        .and_then(|value| table_vec2(&value))
    {
        state.texcoord_offset = Some(value);
    }
    if let Some(value) = actor
        .get::<Option<Table>>("__songlua_state_custom_texture_rect")
        .map_err(|err| err.to_string())?
        .and_then(|value| table_vec4(&value))
    {
        state.custom_texture_rect = Some(value);
    }
    if let Some(value) = actor
        .get::<Option<Table>>("__songlua_state_texcoord_velocity")
        .map_err(|err| err.to_string())?
        .and_then(|value| table_vec2(&value))
    {
        state.texcoord_velocity = Some(value);
    }
    if let Some(value) = actor
        .get::<Option<Table>>("__songlua_state_size")
        .map_err(|err| err.to_string())?
        .and_then(|value| table_vec2(&value))
    {
        state.size = Some(value);
    }
    if let Some(value) = actor
        .get::<Option<Table>>("__songlua_state_stretch_rect")
        .map_err(|err| err.to_string())?
        .and_then(|value| table_vec4(&value))
    {
        state.stretch_rect = Some(value);
    }
    if let Some(raw) = actor
        .get::<Option<String>>("__songlua_state_blend")
        .map_err(|err| err.to_string())?
        .as_deref()
        .and_then(parse_overlay_blend_mode)
    {
        state.blend = raw;
    }
    if let Some(raw) = actor
        .get::<Option<String>>("__songlua_state_effect_mode")
        .map_err(|err| err.to_string())?
        .as_deref()
        .and_then(parse_overlay_effect_mode)
    {
        state.effect_mode = raw;
    }
    Ok(state)
}
