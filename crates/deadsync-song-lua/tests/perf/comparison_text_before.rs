// Frozen from 142996521; unchanged downstream helpers remain shared.
use super::*;

pub(super) fn set_actor_texture_from_value(
    actor: &Table,
    value: Option<&Value>,
    clear_on_nil: bool,
) -> mlua::Result<()> {
    if actor_type_is(actor, "ActorMultiVertex")? {
        // Native SetTexture retains the copied RageTexture, including its
        // allocation and frame metadata, independently of this actor's size.
        match value {
            Some(Value::Table(texture))
                if texture
                    .raw_get::<Option<String>>("__songlua_native_type")?
                    .as_deref()
                    == Some("RageTexture") =>
            {
                actor.raw_set("__songlua_amv_texture_handle", texture.clone())?;
            }
            Some(Value::String(_)) => actor.raw_set("__songlua_amv_texture_handle", Value::Nil)?,
            Some(Value::Nil) | None if clear_on_nil => {
                actor.raw_set("__songlua_amv_texture_handle", Value::Nil)?;
            }
            _ => (),
        }
    }
    match value {
        Some(Value::String(texture)) => {
            if actor.weak_lua() == texture.weak_lua() {
                let _validated = texture.to_str()?;
                actor.set("Texture", texture)?;
            } else {
                actor.set("Texture", texture.to_str()?.as_ref())?;
            }
            actor.raw_set("__songlua_default_texture", false)?;
            actor.set("__songlua_aft_capture_name", Value::Nil)?;
            set_actor_decode_movie_for_texture(actor)?;
        }
        Some(Value::Table(texture)) => {
            if texture.raw_get::<bool>("__songlua_default_texture")? {
                actor.raw_set("__songlua_default_texture", true)?;
                actor.set("Texture", Value::Nil)?;
                actor.set("__songlua_aft_capture_name", Value::Nil)?;
                actor.set("__songlua_state_decode_movie", false)?;
            } else if let Some(capture_name) =
                texture.get::<Option<String>>("__songlua_aft_capture_name")?
            {
                actor.raw_set("__songlua_default_texture", false)?;
                actor.set("__songlua_aft_capture_name", capture_name)?;
                actor.set("Texture", Value::Nil)?;
                set_actor_decode_movie_for_texture(actor)?;
            } else if let Some(texture_path) = texture
                .get::<Option<String>>("__songlua_texture_path")?
                .filter(|path| !path.is_empty())
            {
                actor.set("Texture", texture_path)?;
                actor.raw_set("__songlua_default_texture", false)?;
                actor.set("__songlua_aft_capture_name", Value::Nil)?;
                set_actor_decode_movie_for_texture(actor)?;
            }
        }
        Some(Value::Nil) | None if clear_on_nil => {
            actor.raw_set("__songlua_default_texture", false)?;
            actor.set("Texture", Value::Nil)?;
            actor.set("__songlua_aft_capture_name", Value::Nil)?;
            actor.set("__songlua_state_decode_movie", false)?;
        }
        _ => {}
    }
    Ok(())
}

pub(super) fn classify_function_ease_probe(
    calls: &Table,
) -> mlua::Result<Option<SongLuaEaseTarget>> {
    let mut saw_x = false;
    let mut saw_y = false;
    let mut saw_z = false;
    let mut saw_rotation_x = false;
    let mut saw_rotation_z = false;
    let mut saw_rotation_y = false;
    let mut saw_skew_x = false;
    let mut saw_skew_y = false;
    let mut saw_zoom = false;
    let mut saw_zoom_x = false;
    let mut saw_zoom_y = false;
    let mut saw_zoom_z = false;
    for value in calls.sequence_values::<String>() {
        let value = value?;
        let (target_kind, method_name) =
            value.split_once('.').unwrap_or(("player", value.as_str()));
        if !matches!(target_kind, "player" | "overlay") {
            return Ok(None);
        }
        match method_name {
            "x" if target_kind == "player" => saw_x = true,
            "y" if target_kind == "player" => saw_y = true,
            "z" if target_kind != "overlay" => saw_z = true,
            "rotationx" if target_kind != "overlay" => saw_rotation_x = true,
            "rotationz" if target_kind != "overlay" => saw_rotation_z = true,
            "rotationy" if target_kind != "overlay" => saw_rotation_y = true,
            "skewx" if target_kind == "player" => saw_skew_x = true,
            "skewy" if target_kind == "player" => saw_skew_y = true,
            "zoom" if target_kind != "overlay" => saw_zoom = true,
            "zoomx" if target_kind != "overlay" => saw_zoom_x = true,
            "zoomy" if target_kind != "overlay" => saw_zoom_y = true,
            "zoomz" if target_kind != "overlay" => saw_zoom_z = true,
            _ => return Ok(None),
        }
    }
    Ok(
        match (
            saw_x,
            saw_y,
            saw_z,
            saw_rotation_x,
            saw_rotation_z,
            saw_rotation_y,
            saw_skew_x,
            saw_skew_y,
            saw_zoom,
            saw_zoom_x,
            saw_zoom_y,
            saw_zoom_z,
        ) {
            (true, false, false, false, false, false, false, false, false, false, false, false) => {
                Some(SongLuaEaseTarget::PlayerX)
            }
            (false, true, false, false, false, false, false, false, false, false, false, false) => {
                Some(SongLuaEaseTarget::PlayerY)
            }
            (false, false, true, false, false, false, false, false, false, false, false, false) => {
                Some(SongLuaEaseTarget::PlayerZ)
            }
            (false, false, false, true, false, false, false, false, false, false, false, false) => {
                Some(SongLuaEaseTarget::PlayerRotationX)
            }
            (false, false, false, false, true, false, false, false, false, false, false, false) => {
                Some(SongLuaEaseTarget::PlayerRotationZ)
            }
            (false, false, false, false, false, true, false, false, false, false, false, false) => {
                Some(SongLuaEaseTarget::PlayerRotationY)
            }
            (false, false, false, false, false, false, true, false, false, false, false, false) => {
                Some(SongLuaEaseTarget::PlayerSkewX)
            }
            (false, false, false, false, false, false, false, true, false, false, false, false) => {
                Some(SongLuaEaseTarget::PlayerSkewY)
            }
            (false, false, false, false, false, false, false, false, true, false, false, false) => {
                Some(SongLuaEaseTarget::PlayerZoom)
            }
            (false, false, false, false, false, false, false, false, false, true, false, false) => {
                Some(SongLuaEaseTarget::PlayerZoomX)
            }
            (false, false, false, false, false, false, false, false, false, false, true, false) => {
                Some(SongLuaEaseTarget::PlayerZoomY)
            }
            (false, false, false, false, false, false, false, false, false, false, false, true) => {
                Some(SongLuaEaseTarget::PlayerZoomZ)
            }
            _ => None,
        },
    )
}
