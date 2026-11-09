//! Banner's existing Luna methods and the shared Sprite media helpers.
use super::*;
use sprites::{field, set};

pub(super) const METHODS: &[&str] = &[
    "scaletoclipped",
    "ScaleToClipped",
    "LoadFromSong",
    "LoadFromCourse",
    "LoadFromCachedBanner",
    "LoadIconFromCharacter",
    "LoadCardFromCharacter",
    "LoadBannerFromUnlockEntry",
    "LoadBackgroundFromUnlockEntry",
    "LoadFromSongGroup",
    "LoadFromSortOrder",
    "GetScrolling",
    "SetScrolling",
    "GetPercentScrolling",
];
pub(super) const SPRITE_METHODS: &[&str] = &["LoadFromSongBackground", "position", "loop", "rate"];

pub(super) const CURRENT_BACKGROUND: &[u8] = br#"
return function() return function(self)
    local song = GAMESTATE:GetCurrentSong()
    if not song then
        local trail = GAMESTATE:GetCurrentTrail(GAMESTATE:GetMasterPlayerNumber())
        local entries = trail:GetTrailEntries()
        if #entries > 0 then song = entries[1]:GetSong() end
    end
    if not song then return self end
    self:LoadFromSongBackground(song)
    return self
end end
"#;

fn truthy(value: Value) -> bool {
    !matches!(value, Value::Nil | Value::Boolean(false))
}

fn query(call: &mut HostCall<'_>, object: TableId, name: &str) -> Result<Value, Error> {
    let function = field(call, object, name)?;
    if function == Value::Nil {
        return Ok(Value::Nil);
    }
    if !matches!(function, Value::Function(_) | Value::Native(_)) {
        return Err(call.error(b"media path method must be a function"));
    }
    Ok(call
        .call(function, &[Value::Table(object)])?
        .first()
        .copied()
        .unwrap_or(Value::Nil))
}

fn set_path(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
    path: &str,
) -> Result<bool, Error> {
    let path = path.trim();
    if path.is_empty() {
        return Ok(false);
    }
    let value = call.try_intern(path.as_bytes())?;
    set(call, actor, "Texture", value)?;
    // Preserve publication order even when an authored setter reenters Lua.
    let key = call.try_intern(b"__songlua_amv_texture_handle")?;
    call.raw_table_set(actor, key, Value::Nil)?;
    let key = call.try_intern(b"__songlua_default_texture")?;
    call.raw_table_set(actor, key, Value::Boolean(false))?;
    set(call, actor, "__songlua_aft_capture_name", Value::Nil)?;
    let video = sprites::is_video(call, actor)?;
    sprites::decode(call, store, actor, video)?;
    Ok(true)
}

fn from_methods(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
    source: Value,
    names: &[&str],
) -> Result<bool, Error> {
    let Value::Table(source) = source else {
        return Ok(false);
    };
    for name in names {
        let value = query(call, source, name)?;
        if let Value::String(_) = value {
            let path = assets::text(call, value)?;
            if set_path(call, store, actor, &path)? {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

fn reset_scrolling(call: &mut HostCall<'_>, actor: TableId) -> Result<(), Error> {
    set(
        call,
        actor,
        "__songlua_state_banner_scrolling",
        Value::Boolean(false),
    )?;
    set(
        call,
        actor,
        "__songlua_state_banner_scroll_percent",
        Value::Number(0.0),
    )
}

pub(super) fn method(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
    name: &str,
) -> Result<(), Error> {
    let value = arg(call, 1);
    match name {
        "scaletoclipped" | "ScaleToClipped" => {
            return sprites::method(call, store, actor, "scaletoclipped");
        }
        "LoadFromCachedBanner" => sprites::set_texture(call, store, actor, value, true)?,
        "LoadFromSong" => {
            let mut path = None;
            if let Value::Table(song) = value
                && truthy(query(call, song, "HasBanner")?)
                && let Value::String(text) = query(call, song, "GetBannerPath")?
            {
                path = Some(String::from_utf8_lossy(call.string_bytes(text).unwrap()).into_owned());
            }
            let path = match path.filter(|p| !p.is_empty()) {
                Some(path) => path,
                None => crate::blank_texture::load_native_blank_texture()
                    .map_err(|e| call.error(e.as_bytes()))?
                    .to_owned(),
            };
            let value = call.try_intern(path.as_bytes())?;
            call.with_roots(&[value], |call| {
                sprites::load(call, store, actor, value, false)
            })?;
            reset_scrolling(call, actor)?;
        }
        "LoadFromCourse" | "LoadFromSongBackground" => {
            let methods = if name == "LoadFromCourse" {
                &["GetBannerPath", "GetBackgroundPath"][..]
            } else {
                &["GetBackgroundPath"][..]
            };
            from_methods(call, store, actor, value, methods)?;
        }
        "LoadFromSongGroup" => {
            let group = render::text(call, value).unwrap_or("").to_owned();
            let globals = table(call, helper(call, 5)?)?;
            let manager = field(call, globals, "SONGMAN")?;
            if manager == Value::Nil {
                return call.push(Value::Table(actor));
            }
            call.with_roots(&[manager], |call| {
                let manager = table(call, manager)?;
                let function = field(call, manager, "GetSongGroupBannerPath")?;
                if function == Value::Nil {
                    return Ok(());
                }
                if !matches!(function, Value::Function(_) | Value::Native(_)) {
                    return Err(call.error(b"GetSongGroupBannerPath must be a function"));
                }
                let result = call.with_roots(&[function], |call| {
                    let group = call.try_intern(group.as_bytes())?;
                    Ok(call
                        .call(function, &[Value::Table(manager), group])?
                        .first()
                        .copied()
                        .unwrap_or(Value::Nil))
                })?;
                let loaded = if matches!(result, Value::String(_)) {
                    let path = assets::text(call, result)?;
                    set_path(call, store, actor, &path)?
                } else {
                    false
                };
                if !loaded {
                    set_path(
                        call,
                        store,
                        actor,
                        &crate::theme_path("G", "Banner", "group fallback"),
                    )?;
                }
                reset_scrolling(call, actor)
            })?;
        }
        "LoadFromSortOrder" => {
            let sort = render::text(call, value).unwrap_or("SortOrder_Invalid");
            if let Some(path) = crate::banner_sort_order_path(sort) {
                set_path(call, store, actor, &path)?;
            }
            reset_scrolling(call, actor)?;
        }
        "LoadIconFromCharacter"
        | "LoadCardFromCharacter"
        | "LoadBannerFromUnlockEntry"
        | "LoadBackgroundFromUnlockEntry" => {
            let method = match name {
                "LoadIconFromCharacter" => "GetIconPath",
                "LoadCardFromCharacter" => "GetCardPath",
                "LoadBannerFromUnlockEntry" => "GetBannerFile",
                _ => "GetBackgroundFile",
            };
            if !from_methods(call, store, actor, value, &[method])? {
                set_path(
                    call,
                    store,
                    actor,
                    &crate::theme_path("G", "Common", "fallback banner"),
                )?;
            }
            reset_scrolling(call, actor)?;
        }
        "SetScrolling" => {
            let percent = float_scalar(call, arg(call, 2), false).unwrap_or(0.0);
            set(
                call,
                actor,
                "__songlua_state_banner_scrolling",
                Value::Boolean(truthy(value)),
            )?;
            set(
                call,
                actor,
                "__songlua_state_banner_scroll_percent",
                Value::Number(percent as f64),
            )?;
        }
        "GetScrolling" => {
            let value = field(call, actor, "__songlua_state_banner_scrolling")?;
            return call.push(Value::Boolean(truthy(value)));
        }
        "GetPercentScrolling" => {
            let value = field(call, actor, "__songlua_state_banner_scroll_percent")?;
            let percent = if value == Value::Nil {
                0.0
            } else {
                sprites::number(call, value)? as f32
            };
            return call.push(Value::Number(percent as f64));
        }
        "position" => return sprites::method(call, store, actor, "SetSecondsIntoAnimation"),
        "loop" | "rate" => sprites::texture_parameter(call, store, actor, name, value)?,
        _ => unreachable!(),
    }
    call.push(Value::Table(actor))
}
