// Frozen from 494323364; unchanged downstream helpers remain shared.
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
                actor.set("Texture", texture.to_str()?.to_string())?;
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

pub(super) fn set_actor_sound_file_from_value(
    actor: &Table,
    value: Option<&Value>,
    clear_on_nil: bool,
) -> mlua::Result<()> {
    match value {
        Some(Value::String(file)) => {
            if actor.weak_lua() == file.weak_lua() {
                let _validated = file.to_str()?;
                actor.set("File", file)?;
            } else {
                actor.set("File", file.to_str()?.to_string())?;
            }
        }
        Some(Value::Nil) | None if clear_on_nil => actor.set("File", Value::Nil)?,
        _ => {}
    }
    Ok(())
}
