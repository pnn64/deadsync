//! ActorProxy target ownership and the existing renderer-facing classification.
use super::*;
use crate::SongLuaProxyTarget;
use sprites::{field, set};

pub(super) fn install(call: &mut HostCall<'_>, actor: TableId, proxy: bool) -> Result<(), Error> {
    let methods = get_static(call, actor, b"__songlua_bound_methods")?;
    let methods = if methods == Value::Nil {
        let methods = call.try_new_table()?;
        let key = call.try_intern(b"__songlua_bound_methods")?;
        call.raw_table_set(actor, key, Value::Table(methods))?;
        methods
    } else {
        table(call, methods)?
    };
    // Native SetTarget is bound on every actor, but only ActorProxy publishes
    // it through the class table. Captures keep the owner and its target alive.
    let captures = [Value::Table(actor), call.captures()[1]];
    let setter = call.new_host_function(set_target, &captures)?;
    let key = call.try_intern(b"SetTarget")?;
    call.raw_table_set(methods, key, setter)?;
    if proxy {
        let getter = call.new_host_function(get_target, &captures)?;
        let key = call.try_intern(b"GetTarget")?;
        call.raw_table_set(methods, key, getter)?;
    }
    Ok(())
}

fn optional_integer(
    call: &mut HostCall<'_>,
    actor: TableId,
    key: &str,
) -> Result<Option<i64>, Error> {
    let value = field(call, actor, key)?;
    if value == Value::Nil {
        return Ok(None);
    }
    let n = sprites::number(call, value)?;
    if !(i64::MIN as f64..9223372036854775808.0).contains(&n) {
        return Err(call.error(b"proxy player index out of range"));
    }
    Ok(Some(n as i64))
}

fn set_kind(call: &mut HostCall<'_>, actor: TableId, kind: &str) -> Result<(), Error> {
    let kind = call.try_intern(kind.as_bytes())?;
    set(call, actor, "__songlua_proxy_target_kind", kind)
}

fn set_target(call: &mut HostCall<'_>) -> Result<(), Error> {
    let actor = table(call, call.captures()[0])?;
    let Value::Table(target) = arg(call, 1) else {
        return Err(call.error(b"ActorProxy.SetTarget requires an Actor"));
    };
    let kind = get_static(call, target, b"__songlua_actor_type")?;
    if kind == Value::Nil {
        return Err(call.error(b"ActorProxy.SetTarget requires an Actor"));
    }
    assets::text(call, kind)?;
    set(call, actor, "__songlua_proxy_target_kind", Value::Nil)?;
    set(call, actor, "__songlua_proxy_player_index", Value::Nil)?;
    set(
        call,
        actor,
        "__songlua_proxy_target_actor",
        Value::Table(target),
    )?;
    if let Some(name) = assets::field(call, target, b"__songlua_top_screen_child_name")? {
        if let Some(index) = crate::underlay_score_index(&name) {
            set_kind(call, actor, "score")?;
            set(
                call,
                actor,
                "__songlua_proxy_player_index",
                Value::Number(index as f64),
            )?;
        } else if name.eq_ignore_ascii_case("Underlay") {
            set_kind(call, actor, "underlay")?;
        } else if name.eq_ignore_ascii_case("Overlay") {
            set_kind(call, actor, "overlay")?;
        }
    } else if let Some(index) = optional_integer(call, target, "__songlua_player_index")? {
        set(
            call,
            actor,
            "__songlua_proxy_player_index",
            Value::Number(index as f64),
        )?;
        let child = assets::field(call, target, b"__songlua_player_child_name")?;
        let kind = match child.as_deref() {
            Some(name) if name.eq_ignore_ascii_case("NoteField") => "notefield",
            Some(name) if name.eq_ignore_ascii_case("Judgment") => "judgment",
            Some(name) if name.eq_ignore_ascii_case("Combo") => "combo",
            _ => "player",
        };
        set_kind(call, actor, kind)?;
    } else if assets::field(call, target, b"__songlua_actor_type")?.is_some() {
        set_kind(call, actor, "actor")?;
    }
    call.push(Value::Table(actor))
}

fn get_target(call: &mut HostCall<'_>) -> Result<(), Error> {
    let actor = table(call, call.captures()[0])?;
    let target = get_static(call, actor, b"__songlua_proxy_target_actor")?;
    call.push(target)
}

pub(super) fn read(
    call: &mut HostCall<'_>,
    actor: TableId,
) -> Result<Option<SongLuaProxyTarget>, Error> {
    let Some(kind) = assets::field(call, actor, b"__songlua_proxy_target_kind")? else {
        return Ok(None);
    };
    let player_index = optional_integer(call, actor, "__songlua_proxy_player_index")?
        .and_then(|index| usize::try_from(index).ok());
    let target = field(call, actor, "__songlua_proxy_target_actor")?;
    let target_id = if target == Value::Nil {
        None
    } else {
        Some(table(call, target)?)
    };
    call.with_roots(&[target], |call| {
        let hidden = if let Some(target) = target_id {
            match field(call, target, "__songlua_visible") {
                Ok(value) => value == Value::Boolean(false),
                Err(error) if error.kind() == ErrorKind::Runtime => false,
                Err(error) => return Err(error),
            }
        } else {
            false
        };
        Ok(match kind.as_str() {
            "player" => {
                player_index.map(|player_index| SongLuaProxyTarget::Player { player_index })
            }
            "notefield" => {
                player_index.map(|player_index| SongLuaProxyTarget::NoteField { player_index })
            }
            "judgment" => {
                player_index.map(|player_index| SongLuaProxyTarget::Judgment { player_index })
            }
            "combo" => player_index.map(|player_index| SongLuaProxyTarget::Combo { player_index }),
            "score" => player_index.map(|player_index| SongLuaProxyTarget::Score { player_index }),
            "underlay" => Some(SongLuaProxyTarget::Underlay { hidden }),
            "overlay" => Some(SongLuaProxyTarget::Overlay { hidden }),
            "actor" => target_id.map(|id| SongLuaProxyTarget::Actor {
                overlay_index: call
                    .object_pointer(Value::Table(id))
                    .expect("rooted proxy target"),
            }),
            _ => None,
        })
    })
}
