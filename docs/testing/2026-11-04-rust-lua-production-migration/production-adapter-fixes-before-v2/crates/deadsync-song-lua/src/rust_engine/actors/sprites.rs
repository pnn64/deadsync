//! Sprite resources and immediate animation properties owned by the song host.
use super::*;
use std::path::{Path, PathBuf};

pub(super) const ACTOR_METHODS: &[&str] = &[
    "animate",
    "loop",
    "rate",
    "setstate",
    "GetNumStates",
    "texturetranslate",
    "texturewrapping",
];
pub(super) const SPRITE_METHODS: &[&str] = &[
    "Load",
    "LoadBanner",
    "LoadBackground",
    "LoadFromCached",
    "SetTexture",
    "GetTexture",
    "customtexturerect",
    "SetCustomImageRect",
    "stretchtexcoords",
    "addimagecoords",
    "texcoordvelocity",
    "scaletoclipped",
    "CropTo",
    "GetState",
    "SetStateProperties",
    "GetAnimationLengthSeconds",
    "SetSecondsIntoAnimation",
    "SetAllStateDelays",
    "GetDecodeMovie",
    "SetDecodeMovie",
    "SetEffectMode",
    "SetCustomPosCoords",
    "StopUsingCustomPosCoords",
    "get_use_effect_clock_for_texcoords",
    "set_use_effect_clock_for_texcoords",
];

pub(super) fn field(call: &mut HostCall<'_>, actor: TableId, key: &str) -> Result<Value, Error> {
    let key = call.try_intern(key.as_bytes())?;
    call.table_get(Value::Table(actor), key)
}
pub(super) fn set(
    call: &mut HostCall<'_>,
    actor: TableId,
    key: &str,
    value: Value,
) -> Result<(), Error> {
    let key = call.try_intern(key.as_bytes())?;
    call.table_set(Value::Table(actor), key, value)
}
fn raw_set(call: &mut HostCall<'_>, actor: TableId, key: &str, value: Value) -> Result<(), Error> {
    let key = call.try_intern(key.as_bytes())?;
    call.raw_table_set(actor, key, value)
}
pub(super) fn number(call: &mut HostCall<'_>, value: Value) -> Result<f64, Error> {
    if let Value::Number(n) = value {
        return Ok(n);
    }
    match call.call(helper(call, 6)?, &[value])?[0] {
        Value::Number(n) => Ok(n),
        _ => Err(call.error(b"number expected")),
    }
}
fn optional_number(
    call: &mut HostCall<'_>,
    actor: TableId,
    key: &str,
) -> Result<Option<f32>, Error> {
    let value = field(call, actor, key)?;
    if value == Value::Nil {
        Ok(None)
    } else {
        number(call, value).map(|n| Some(n as f32))
    }
}
fn uint(call: &mut HostCall<'_>, value: Value) -> Result<u32, Error> {
    let n = number(call, value)?;
    if n > -1.0 && n < 4294967296.0 {
        Ok(n as u32)
    } else {
        Err(call.error(b"unsigned integer out of range"))
    }
}
fn optional_uint(call: &mut HostCall<'_>, actor: TableId, key: &str) -> Result<Option<u32>, Error> {
    let value = field(call, actor, key)?;
    if value == Value::Nil {
        Ok(None)
    } else {
        uint(call, value).map(Some)
    }
}
pub(super) fn vector<const N: usize>(
    call: &mut HostCall<'_>,
    value: Value,
) -> Result<Option<[f32; N]>, Error> {
    if value == Value::Nil {
        return Ok(None);
    }
    let table = table(call, value)?;
    call.with_roots(&[value], |call| {
        let mut out = [0.0; N];
        for (i, item) in out.iter_mut().enumerate() {
            let value = call.raw_table_get(table, Value::Number((i + 1) as f64))?;
            let result = call.call(helper(call, 6)?, &[value])?[0];
            let Value::Number(n) = result else {
                return Ok(None);
            };
            *item = n as f32;
        }
        Ok(Some(out))
    })
}
pub(super) fn vector_value(call: &mut HostCall<'_>, values: &[f32]) -> Result<Value, Error> {
    let table = call.try_new_table()?;
    for (i, &value) in values.iter().enumerate() {
        call.raw_table_set(
            table,
            Value::Number((i + 1) as f64),
            Value::Number(value as f64),
        )?;
    }
    Ok(Value::Table(table))
}
fn put_vector(
    call: &mut HostCall<'_>,
    actor: TableId,
    key: &str,
    values: &[f32],
) -> Result<(), Error> {
    let value = vector_value(call, values)?;
    call.with_roots(&[value], |call| set(call, actor, key, value))
}
pub(super) fn default_texture(call: &mut HostCall<'_>, actor: TableId) -> Result<bool, Error> {
    let key = call.try_intern(b"__songlua_default_texture")?;
    Ok(!matches!(
        call.raw_table_get(actor, key)?,
        Value::Nil | Value::Boolean(false)
    ))
}
pub(super) fn sheet(call: &mut HostCall<'_>, actor: TableId) -> Result<Option<(u32, u32)>, Error> {
    if default_texture(call, actor)? {
        return Ok(Some((1, 1)));
    }
    if let Some(path) = assets::texture_path(call, actor)? {
        return Ok(Some(crate::parse_sprite_sheet_dims(
            &path.to_string_lossy(),
        )));
    }
    Ok(assets::field(call, actor, b"Texture")?
        .filter(|s| !s.trim().is_empty())
        .map(|s| crate::parse_sprite_sheet_dims(s.trim())))
}
pub(super) fn image_size(path: &Path, logical: bool) -> Option<[f32; 2]> {
    if !crate::is_song_lua_image_path(path) {
        return None;
    }
    let (width, height) =
        if let Some(texture) = deadlib_assets::generated_texture(&path.to_string_lossy()) {
            (texture.source_size[0], texture.source_size[1])
        } else {
            deadlib_assets::texture_source_size(path).ok()?
        };
    let (width, height) = if logical {
        deadlib_assets::texture_source_dims_from_real(&path.to_string_lossy(), width, height)
    } else {
        (width, height)
    };
    Some([width as f32, height as f32])
}
pub(super) fn actor_upload_size(
    call: &mut HostCall<'_>,
    actor: TableId,
) -> Result<Option<[f32; 2]>, Error> {
    // These are separate authored lookups in the production reader. Keep their
    // order even when a getter changes Texture, reenters the host or collects.
    let generated = assets::texture_path(call, actor)?
        .and_then(|path| deadlib_assets::generated_texture(&path.to_string_lossy()))
        .map(|texture| texture.source_size.map(|n| n as f32));
    let image = if generated.is_some() {
        generated
    } else {
        assets::texture_path(call, actor)?
            .as_deref()
            .and_then(|path| image_size(path, false))
    };
    let Some(image) = image else { return Ok(None) };
    let path = assets::texture_path(call, actor)?.unwrap_or_default();
    let mut hints = deadlib_assets::parse_texture_hints(&path.to_string_lossy());
    hints.max_size = Some(2048);
    hints.min_size = Some(8);
    deadlib_assets::texture_image_size(image.map(|n| n as u32), &hints)
        .map(|size| Some(size.map(|n| n as f32)))
        .map_err(|error| call.error(error.to_string().as_bytes()))
}
pub(super) fn movie_size(
    call: &mut HostCall<'_>,
    actor: TableId,
) -> Result<Option<[f32; 2]>, Error> {
    let Some(path) =
        assets::texture_path(call, actor)?.filter(|path| crate::is_song_lua_video_path(path))
    else {
        return Ok(None);
    };
    let path_text = path.to_string_lossy();
    if assets::field(call, actor, b"__songlua_movie_path")?.as_deref() == Some(path_text.as_ref()) {
        let value = field(call, actor, "__songlua_movie_size")?;
        return vector(call, value);
    }
    let value = call.try_intern(path_text.as_bytes())?;
    set(call, actor, "__songlua_movie_path", value)?;
    set(call, actor, "__songlua_movie_size", Value::Nil)?;
    match deadlib_video::dimensions(&path) {
        Ok((w, h)) => {
            let size = [w as f32, h as f32];
            put_vector(call, actor, "__songlua_movie_size", &size)?;
            Ok(Some(size))
        }
        Err(error) => {
            log::warn!("Song Lua movie metadata: {error}");
            Ok(None)
        }
    }
}
pub(super) fn frame_size(
    call: &mut HostCall<'_>,
    actor: TableId,
) -> Result<Option<[f32; 2]>, Error> {
    let path = assets::texture_path(call, actor)?;
    let dims = path
        .as_deref()
        .map(|p| crate::parse_sprite_sheet_dims(&p.to_string_lossy()));
    let movie = field(call, actor, "__songlua_movie_size")?;
    let source = vector::<2>(call, movie)?
        .or_else(|| path.as_deref().and_then(|path| image_size(path, true)));
    field(call, actor, "__songlua_state_sprite_animate")?;
    optional_uint(call, actor, "__songlua_state_sprite_state_index")?;
    Ok(
        crate::sprite_image_frame_size(source.map(|[w, h]| (w, h)), false, None, dims)
            .map(|(w, h)| [w, h]),
    )
}
pub(super) fn refresh_size(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
) -> Result<(), Error> {
    let image = frame_size(call, actor)?;
    let mut store = store.lock().unwrap();
    let actor = store.actors.get_mut(&actor).unwrap();
    actor.state.size = actor
        .state
        .render
        .size
        .or_else(|| {
            actor
                .state
                .render
                .stretch_rect
                .map(|r| [r[2] - r[0], r[3] - r[1]])
        })
        .or(image)
        .unwrap_or([1.0; 2]);
    Ok(())
}
pub(super) fn is_video(call: &mut HostCall<'_>, actor: TableId) -> Result<bool, Error> {
    if let Some(path) = assets::texture_path(call, actor)? {
        return Ok(crate::is_song_lua_video_path(&path));
    }
    Ok(assets::field(call, actor, b"Texture")?
        .is_some_and(|s| crate::is_song_lua_video_path(Path::new(s.trim()))))
}
pub(super) fn decode(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
    enabled: bool,
) -> Result<(), Error> {
    set(
        call,
        actor,
        "__songlua_state_decode_movie",
        Value::Boolean(enabled),
    )?;
    store
        .lock()
        .unwrap()
        .actors
        .get_mut(&actor)
        .unwrap()
        .state
        .render
        .decode_movie = enabled;
    Ok(())
}
pub(super) fn initialize(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
    template: bool,
) -> Result<(), Error> {
    let key = call.try_intern(b"__songlua_theme_path_actor")?;
    let theme_actor = call.raw_table_get(actor, key)? == Value::Boolean(true);
    // create_theme_path_actor fills Texture after constructing the empty
    // Sprite. It leaves the decoder field and movie metadata unset; its getter
    // can still infer a movie from Texture while the initial render flag is false.
    if !theme_actor {
        let enabled = is_video(call, actor)?;
        decode(call, store, actor, enabled)?;
        movie_size(call, actor)?;
    }
    let bound = store.lock().unwrap().actors[&actor].state.class == "Banner"
        || field(call, actor, "Texture")? != Value::Nil;
    if !template {
        let mut store = store.lock().unwrap();
        let render = &mut store.actors.get_mut(&actor).unwrap().state.render;
        render.sprite_animate = !theme_actor;
        render.sprite_texture = bound;
    }
    refresh_size(call, store, actor)
}
pub(super) fn set_texture(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
    value: Value,
    clear: bool,
) -> Result<(), Error> {
    match value {
        Value::String(_) => {
            assets::text(call, value)?;
            set(call, actor, "Texture", value)?;
            raw_set(
                call,
                actor,
                "__songlua_default_texture",
                Value::Boolean(false),
            )?;
            set(call, actor, "__songlua_aft_capture_name", Value::Nil)?;
            let video = is_video(call, actor)?;
            decode(call, store, actor, video)?;
        }
        Value::Table(texture) => {
            if default_texture(call, texture)? {
                raw_set(
                    call,
                    actor,
                    "__songlua_default_texture",
                    Value::Boolean(true),
                )?;
                set(call, actor, "Texture", Value::Nil)?;
                set(call, actor, "__songlua_aft_capture_name", Value::Nil)?;
                decode(call, store, actor, false)?;
            } else if let Some(name) = assets::field(call, texture, b"__songlua_aft_capture_name")?
            {
                raw_set(
                    call,
                    actor,
                    "__songlua_default_texture",
                    Value::Boolean(false),
                )?;
                let name = call.try_intern(name.as_bytes())?;
                set(call, actor, "__songlua_aft_capture_name", name)?;
                set(call, actor, "Texture", Value::Nil)?;
                let video = is_video(call, actor)?;
                decode(call, store, actor, video)?;
            } else if let Some(path) =
                assets::field(call, texture, b"__songlua_texture_path")?.filter(|p| !p.is_empty())
            {
                let path = call.try_intern(path.as_bytes())?;
                set_texture(call, store, actor, path, false)?;
            }
        }
        Value::Nil if clear => {
            raw_set(
                call,
                actor,
                "__songlua_default_texture",
                Value::Boolean(false),
            )?;
            set(call, actor, "Texture", Value::Nil)?;
            set(call, actor, "__songlua_aft_capture_name", Value::Nil)?;
            decode(call, store, actor, false)?;
        }
        _ => {}
    }
    Ok(())
}
fn size(store: &Shared, actor: TableId, size: [f32; 2]) {
    let mut store = store.lock().unwrap();
    let actor = store.actors.get_mut(&actor).unwrap();
    actor.state.size = size;
    actor.state.render.size = Some(size);
}
fn state(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
    index: u32,
    clear_rect: bool,
) -> Result<(), Error> {
    if clear_rect {
        action_capture::prepare_write(call, store, actor)?;
    }
    set(
        call,
        actor,
        "__songlua_state_sprite_state_index",
        Value::Number(index as f64),
    )?;
    if clear_rect {
        set(
            call,
            actor,
            "__songlua_state_custom_texture_rect",
            Value::Nil,
        )?;
    }
    let mut guard = store.lock().unwrap();
    let instance = guard.actors.get_mut(&actor).unwrap();
    if clear_rect {
        for clock in instance.model_materials.iter_mut().flatten() {
            clock.set_state(index.min(i32::MAX as u32) as i32);
        }
    }
    let render = &mut instance.state.render;
    render.sprite_state_index = Some(index);
    if clear_rect {
        render.custom_texture_rect = None;
    }
    drop(guard);
    if clear_rect {
        capture::write(
            call,
            store,
            actor,
            crate::SongLuaOverlayStateDelta {
                sprite_state_index: Some(index),
                ..Default::default()
            },
            true,
        );
        // Native SetState marks the current block when it clears its rectangle,
        // including a timed block with no other captured delta fields.
        let mut guard = store.lock().unwrap();
        let queued = guard.capture_scope.is_some();
        if let Some(capture) = guard
            .actors
            .get_mut(&actor)
            .and_then(|a| a.capture.as_mut())
        {
            capture.mark_current_changed(queued);
        }
    }
    Ok(())
}
fn rect(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
    rect: [f32; 4],
) -> Result<(), Error> {
    action_capture::prepare_write(call, store, actor)?;
    state(
        call,
        store,
        actor,
        crate::SONG_LUA_SPRITE_STATE_CLEAR,
        false,
    )?;
    capture::write(
        call,
        store,
        actor,
        crate::SongLuaOverlayStateDelta {
            sprite_state_index: Some(crate::SONG_LUA_SPRITE_STATE_CLEAR),
            ..Default::default()
        },
        false,
    );
    song_properties::record(
        call,
        store,
        actor,
        crate::SongLuaOverlayUpdateTarget::SpriteStateIndex,
        crate::SongLuaOverlayUpdateValue::U32(crate::SONG_LUA_SPRITE_STATE_CLEAR),
    );
    put_vector(call, actor, "__songlua_state_custom_texture_rect", &rect)?;
    store
        .lock()
        .unwrap()
        .actors
        .get_mut(&actor)
        .unwrap()
        .state
        .render
        .custom_texture_rect = Some(rect);
    song_media::record(call, store, actor, "custom_texture_rect");
    Ok(())
}
fn current_rect(call: &mut HostCall<'_>, actor: TableId) -> Result<[f32; 4], Error> {
    let custom = field(call, actor, "__songlua_state_custom_texture_rect")?;
    let custom = vector::<4>(call, custom)?;
    let state = optional_uint(call, actor, "__songlua_state_sprite_state_index")?;
    let sheet = sheet(call, actor)?;
    Ok(crate::sprite_texture_rect(custom, state, sheet))
}
fn store_sheet(call: &mut HostCall<'_>, actor: TableId) -> Result<(), Error> {
    if let Some((cols, rows)) = sheet(call, actor)? {
        let value = vector_value(call, &[cols as f32, rows as f32])?;
        raw_set(call, actor, "__songlua_state_sprite_frame_sheet", value)?;
    }
    Ok(())
}
pub(super) fn states(
    call: &mut HostCall<'_>,
    actor: TableId,
) -> Result<Vec<crate::SongLuaSpriteState>, Error> {
    let mut result = Vec::new();
    visit_states(call, actor, |state| result.push(state))?;
    Ok(result)
}

pub(super) fn visit_states(
    call: &mut HostCall<'_>,
    actor: TableId,
    mut visit: impl FnMut(crate::SongLuaSpriteState),
) -> Result<(), Error> {
    let key = call.try_intern(b"__songlua_state_sprite_default_states")?;
    if !matches!(
        call.raw_table_get(actor, key)?,
        Value::Nil | Value::Boolean(false)
    ) {
        return Ok(());
    }
    let key = call.try_intern(b"Frames")?;
    let frames = call.raw_table_get(actor, key)?;
    if frames != Value::Nil {
        let frames_table = table(call, frames)?;
        call.with_roots(&[frames], |call| {
            let mut frame = 0;
            for i in 1usize.. {
                let value = call.raw_table_get(frames_table, Value::Number(i as f64))?;
                if value == Value::Nil {
                    break;
                }
                let entry = table(call, value)?;
                call.with_roots(&[value], |call| {
                    frame = optional_uint(call, entry, "Frame")?.unwrap_or(frame);
                    let delay = optional_number(call, entry, "Delay")?.unwrap_or(0.1);
                    visit(crate::SongLuaSpriteState { frame, delay });
                    Ok(())
                })?;
            }
            Ok(())
        })?;
    } else {
        for i in 0i32.. {
            let Some(frame) = optional_uint(call, actor, &format!("Frame{i:04}"))? else {
                break;
            };
            let delay = optional_number(call, actor, &format!("Delay{i:04}"))?.unwrap_or(0.1);
            visit(crate::SongLuaSpriteState { frame, delay });
        }
    }
    Ok(())
}
fn capture(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
    previous: Option<PathBuf>,
) -> Result<(), Error> {
    if !assets::field(call, actor, b"__songlua_actor_type")?.is_some_and(|kind| {
        kind.eq_ignore_ascii_case("Sprite") || kind.eq_ignore_ascii_case("Banner")
    }) {
        return Ok(());
    }
    messages::side_effect(call)?;
    let Some(path) = assets::texture_path(call, actor)? else {
        return Ok(());
    };
    if previous.as_ref() != Some(&path) {
        state(call, store, actor, 0, false)?;
        capture::write(
            call,
            store,
            actor,
            crate::SongLuaOverlayStateDelta {
                sprite_state_index: Some(0),
                ..Default::default()
            },
            true,
        );
        let globals = table(call, helper(call, 5)?)?;
        let key = call.try_intern(crate::SONG_LUA_RUNTIME_KEY.as_bytes())?;
        let runtime = call.raw_table_get(globals, key)?;
        let seconds = if runtime == Value::Nil {
            None
        } else {
            let runtime_table = table(call, runtime)?;
            Some(call.with_roots(&[runtime], |call| {
                let value = field(call, runtime_table, crate::SONG_LUA_RUNTIME_SECONDS_KEY)?;
                number(call, value).map(|n| n as f32)
            })?)
        };
        set(
            call,
            actor,
            "__songlua_state_sprite_texture_epoch",
            seconds.map_or(Value::Nil, |n| Value::Number(n as f64)),
        )?;
    }
    let Some(queued) = store.lock().unwrap().sound_capture else {
        return Ok(());
    };
    if !queued {
        let second = effects::values(call, false)?[1];
        let states = states(call, actor)?;
        let epoch = optional_number(call, actor, "__songlua_state_sprite_texture_epoch")?;
        let key = call.try_intern(b"__songlua_state_sprite_frame_sheet")?;
        let value = call.raw_table_get(actor, key)?;
        let frame_sheet = if value == Value::Nil {
            None
        } else {
            let sheet = table(call, value)?;
            Some(call.with_roots(&[value], |call| {
                let cols = call.raw_table_get(sheet, Value::Number(1.0))?;
                let cols = uint(call, cols)?;
                let rows = call.raw_table_get(sheet, Value::Number(2.0))?;
                Ok((cols, uint(call, rows)?))
            })?)
        };
        let owner = call.object_pointer(Value::Table(actor)).unwrap() as u64;
        store.lock().unwrap().sprite_textures.push((
            owner,
            crate::SongLuaSpriteTexture {
                second,
                key: Arc::from(path.to_string_lossy().as_ref()),
                path: path.clone(),
                states: states.into(),
                animation_epoch: epoch,
                frame_sheet,
            },
        ));
    }
    if let Some(player) = optional_uint(call, actor, "__songlua_judgment_sprite_player")?
        && let Some(frame_size) = frame_size(call, actor)?
    {
        let (cols, rows) = sheet(call, actor)?.unwrap_or((1, 1));
        let second = effects::values(call, false)?[1];
        store.lock().unwrap().judgment_textures.push((
            player as usize,
            crate::SongLuaJudgmentTexture {
                second,
                key: Arc::from(path.to_string_lossy().as_ref()),
                path,
                frame_size,
                frame_cols: cols as usize,
                frame_rows: rows as usize,
            },
        ));
    }
    Ok(())
}
pub(super) fn load(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
    value: Value,
    preserve: bool,
) -> Result<(), Error> {
    action_capture::prepare(call, store, actor)?;
    let previous = assets::texture_path(call, actor)?;
    if preserve {
        let key = call.try_intern(b"__songlua_state_sprite_frame_sheet")?;
        let existing = call.raw_table_get(actor, key)?;
        if existing == Value::Nil {
            store_sheet(call, actor)?;
        } else {
            table(call, existing)?;
        }
    }
    set_texture(call, store, actor, value, !preserve)?;
    if !preserve {
        raw_set(
            call,
            actor,
            "__songlua_state_sprite_default_states",
            Value::Boolean(true),
        )?;
        store_sheet(call, actor)?;
    }
    let explicit = if preserve && let Value::Table(texture) = value {
        let value = field(call, texture, "__songlua_source_frame_size")?;
        vector::<2>(call, value)?
    } else {
        None
    };
    let dimensions = if explicit.is_some() {
        explicit
    } else {
        frame_size(call, actor)?
    };
    if let Some(dimensions) = dimensions {
        size(store, actor, dimensions);
        capture::write(
            call,
            store,
            actor,
            crate::SongLuaOverlayStateDelta {
                size: Some(dimensions),
                ..Default::default()
            },
            true,
        );
    }
    if preserve {
        let bound = default_texture(call, actor)?
            || assets::field(call, actor, b"__songlua_aft_capture_name")?.is_some()
            || assets::texture_path(call, actor)?.is_some();
        store
            .lock()
            .unwrap()
            .actors
            .get_mut(&actor)
            .unwrap()
            .state
            .render
            .sprite_texture = bound;
        set(
            call,
            actor,
            "__songlua_state_sprite_texture",
            Value::Boolean(bound),
        )?;
        capture::write(
            call,
            store,
            actor,
            crate::SongLuaOverlayStateDelta {
                sprite_texture: Some(bound),
                ..Default::default()
            },
            true,
        );
    } else if dimensions.is_some() {
        store
            .lock()
            .unwrap()
            .actors
            .get_mut(&actor)
            .unwrap()
            .state
            .render
            .sprite_texture = true;
        set(
            call,
            actor,
            "__songlua_state_sprite_texture",
            Value::Boolean(true),
        )?;
        capture::write(
            call,
            store,
            actor,
            crate::SongLuaOverlayStateDelta {
                sprite_texture: Some(true),
                ..Default::default()
            },
            true,
        );
    }
    capture(call, store, actor, previous)
}

pub(super) fn texture_parameter(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
    name: &str,
    value: Value,
) -> Result<(), Error> {
    if !store.lock().unwrap().actors.contains_key(&actor) {
        return Err(call.error(b"texture owner has been released"));
    }
    if name == "loop" {
        let value = boolish(call, value).unwrap_or(true);
        action_capture::prepare_write(call, store, actor)?;
        set(
            call,
            actor,
            "__songlua_state_sprite_loop",
            Value::Boolean(value),
        )?;
        store
            .lock()
            .unwrap()
            .actors
            .get_mut(&actor)
            .unwrap()
            .state
            .render
            .sprite_loop = value;
        song_media::record(call, store, actor, "sprite_loop");
    } else if let Some(value) = float_scalar(call, value, false) {
        action_capture::prepare_write(call, store, actor)?;
        set(
            call,
            actor,
            "__songlua_state_sprite_playback_rate",
            Value::Number(value as f64),
        )?;
        store
            .lock()
            .unwrap()
            .actors
            .get_mut(&actor)
            .unwrap()
            .state
            .render
            .sprite_playback_rate = value;
        song_media::record(call, store, actor, "sprite_playback_rate");
    }
    Ok(())
}
pub(super) fn method(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
    name: &str,
) -> Result<(), Error> {
    let value = arg(call, 1);
    match name {
        "Load" | "LoadFromCached" | "SetTexture" => {
            let value = if name == "LoadFromCached" {
                call.arguments().get(2).copied().unwrap_or(value)
            } else {
                value
            };
            load(call, store, actor, value, name == "SetTexture")?;
        }
        "LoadBanner" | "LoadBackground" => {
            let value = call
                .coerce_string(value)?
                .ok_or_else(|| call.error(b"string expected"))?;
            call.with_roots(&[Value::String(value)], |call| {
                load(call, store, actor, Value::String(value), false)
            })?;
            return call.push(call.arguments().last().copied().unwrap_or(Value::Nil));
        }
        "GetTexture" => return textures::get(call, store, actor),
        "GetNumStates" => {
            let count = crate::sprite_frame_count(sheet(call, actor)?);
            return call.push(Value::Number(count as f64));
        }
        "GetState" => {
            let state = optional_uint(call, actor, "__songlua_state_sprite_state_index")?;
            return call.push(Value::Number(
                crate::song_lua_valid_sprite_state_index(state).unwrap_or(0) as f64,
            ));
        }
        "setstate" => {
            if let Value::Number(n) = value
                && n.is_finite()
                && (0.0..=u32::MAX as f64).contains(&n)
                && n.fract() == 0.0
            {
                state(call, store, actor, n as u32, true)?;
            }
        }
        "animate" | "texturewrapping" => {
            let enabled = boolish(call, value).unwrap_or(true);
            action_capture::prepare_write(call, store, actor)?;
            let mut guard = store.lock().unwrap();
            let render = &mut guard.actors.get_mut(&actor).unwrap().state.render;
            if name == "animate" {
                render.sprite_animate = enabled;
            } else {
                render.texture_wrapping = enabled;
            }
            drop(guard);
            let key = if name == "animate" {
                "sprite_animate"
            } else {
                "texture_wrapping"
            };
            set(
                call,
                actor,
                &format!("__songlua_state_{key}"),
                Value::Boolean(enabled),
            )?;
            song_media::record(call, store, actor, key);
        }
        "loop" | "rate" => texture_parameter(call, store, actor, name, value)?,
        "SetDecodeMovie" => decode(call, store, actor, boolish(call, value).unwrap_or(true))?,
        "GetDecodeMovie" => {
            let value = field(call, actor, "__songlua_state_decode_movie")?;
            let fallback = is_video(call, actor)?;
            return call.push(Value::Boolean(if value == Value::Nil {
                fallback
            } else {
                value != Value::Boolean(false)
            }));
        }
        "SetEffectMode" => {
            if matches!(value, Value::String(_)) && assets::text(call, value).is_ok() {
                set(call, actor, "__songlua_sprite_effect_mode", value)?;
            }
        }
        "SetAllStateDelays" => {
            if let Some(delay) = float_scalar(call, value, false) {
                let delay = delay.max(0.0);
                action_capture::prepare_write(call, store, actor)?;
                set(
                    call,
                    actor,
                    "__songlua_state_sprite_state_delay",
                    Value::Number(delay as f64),
                )?;
                store
                    .lock()
                    .unwrap()
                    .actors
                    .get_mut(&actor)
                    .unwrap()
                    .state
                    .render
                    .sprite_state_delay = delay;
                song_media::record(call, store, actor, "sprite_state_delay");
            }
        }
        "GetAnimationLengthSeconds" => {
            let length = if let Some(length) =
                optional_number(call, actor, "__songlua_sprite_animation_length_seconds")?
            {
                length
            } else {
                optional_number(call, actor, "__songlua_state_sprite_state_delay")?
                    .unwrap_or(0.1)
                    .max(0.0)
                    * crate::sprite_frame_count(sheet(call, actor)?) as f32
            };
            return call.push(Value::Number(length as f64));
        }
        "SetSecondsIntoAnimation" => {
            if let Some(seconds) = float_scalar(call, value, false) {
                let delay = optional_number(call, actor, "__songlua_state_sprite_state_delay")?
                    .unwrap_or(0.1);
                let count = crate::sprite_frame_count(sheet(call, actor)?);
                state(
                    call,
                    store,
                    actor,
                    crate::sprite_animation_state_at(seconds, delay, count),
                    true,
                )?;
            }
        }
        "SetStateProperties" => {
            if let Value::Table(entries) = value
                && call.raw_table_len(entries)? > 0
            {
                let mut length = 0.0f32;
                let mut first = None;
                let mut first_delay = None;
                for i in 1usize.. {
                    let entry = call.raw_table_get(entries, Value::Number(i as f64))?;
                    if entry == Value::Nil {
                        break;
                    }
                    let entry_table = table(call, entry)?;
                    call.with_roots(&[entry], |call| {
                        if first.is_none() {
                            first = optional_uint(call, entry_table, "Frame")?;
                        }
                        let delay = optional_number(call, entry_table, "Delay")?
                            .unwrap_or(0.0)
                            .max(0.0);
                        first_delay.get_or_insert(delay);
                        length += delay;
                        Ok(())
                    })?;
                }
                set(
                    call,
                    actor,
                    "__songlua_sprite_animation_length_seconds",
                    Value::Number(length as f64),
                )?;
                if let Some(delay) = first_delay {
                    action_capture::prepare_write(call, store, actor)?;
                    set(
                        call,
                        actor,
                        "__songlua_state_sprite_state_delay",
                        Value::Number(delay as f64),
                    )?;
                    store
                        .lock()
                        .unwrap()
                        .actors
                        .get_mut(&actor)
                        .unwrap()
                        .state
                        .render
                        .sprite_state_delay = delay;
                    song_media::record(call, store, actor, "sprite_state_delay");
                }
                state(call, store, actor, first.unwrap_or(0), true)?;
            }
        }
        "customtexturerect" | "SetCustomImageRect" => {
            let values =
                [1, 2, 3, 4].map(|i| float_scalar(call, arg(call, i), false).unwrap_or(0.0));
            rect(call, store, actor, values)?;
        }
        "stretchtexcoords" | "addimagecoords" => {
            let upload = if name == "addimagecoords" {
                let Some(size) = actor_upload_size(call, actor)? else {
                    return call.push(Value::Table(actor));
                };
                Some(size)
            } else {
                None
            };
            let delta = [1, 2].map(|i| float_scalar(call, arg(call, i), false).unwrap_or(0.0));
            let current = current_rect(call, actor)?;
            let next = if let Some(size) = upload {
                crate::texture_pixel_offset_rect(current, size, delta)
            } else {
                Some(crate::offset_texture_rect(current, delta))
            };
            if let Some(next) = next {
                rect(call, store, actor, next)?;
            }
        }
        "texturetranslate" | "texcoordvelocity" => {
            let value = [1, 2].map(|i| float_scalar(call, arg(call, i), false).unwrap_or(0.0));
            action_capture::prepare_write(call, store, actor)?;
            let mut guard = store.lock().unwrap();
            let render = &mut guard.actors.get_mut(&actor).unwrap().state.render;
            if name == "texturetranslate" {
                render.texcoord_offset = Some(value);
            } else {
                render.texcoord_velocity = Some(value);
            }
            drop(guard);
            let key = if name == "texturetranslate" {
                "texcoord_offset"
            } else {
                "texcoord_velocity"
            };
            put_vector(call, actor, &format!("__songlua_state_{key}"), &value)?;
            song_media::record(call, store, actor, key);
        }
        "CropTo" | "scaletoclipped" => {
            let (Some(w), Some(h)) = (
                float_scalar(call, value, false),
                float_scalar(call, arg(call, 2), false),
            ) else {
                return call.push(Value::Table(actor));
            };
            let target = [w, h];
            if name == "scaletoclipped" {
                action_capture::prepare_write(call, store, actor)?;
                size(store, actor, target);
                capture::property(call, store, actor, "size");
            } else if target.iter().all(|n| n.is_finite() && *n > 0.0) {
                let source = match frame_size(call, actor)? {
                    Some(size) => Some(size),
                    None => movie_size(call, actor)?,
                };
                if let Some(source) = source
                    && let Some(crop) = crate::crop_texture_rect(source, target)
                {
                    action_capture::prepare_write(call, store, actor)?;
                    size(store, actor, target);
                    capture::property(call, store, actor, "size");
                    rect(call, store, actor, crop)?;
                    let mut guard = store.lock().unwrap();
                    let pose = guard.actors.get_mut(&actor).unwrap().destination_mut();
                    pose.zoom = 1.0;
                    pose.zoom_x = 1.0;
                    pose.zoom_y = 1.0;
                    pose.zoom_z = 1.0;
                    drop(guard);
                    capture::property(call, store, actor, "zoom");
                }
            }
        }
        _ => {
            return Err(call.error(
                format!("native actor method {name} has no semantic implementation").as_bytes(),
            ));
        }
    }
    call.push(Value::Table(actor))
}
