//! Callback failures retained by song preparation, separate from host notices.
use super::*;

#[derive(Clone, Copy)]
pub(super) enum Callback<'a> {
    Update,
    Queue,
    Command(&'a [u8]),
    Draw,
    Health,
    Gameplay(&'static str, &'static str),
}
impl Callback<'_> {
    fn flag(self) -> &'static str {
        match self {
            Self::Update => "__songlua_update_function_error_reported",
            Self::Queue => "__songlua_update_queue_error_reported",
            Self::Command(_) => "__songlua_update_command_error_reported",
            Self::Draw => "__songlua_draw_error_reported",
            Self::Health => "__songlua_health_error_reported",
            Self::Gameplay(flag, _) => flag,
        }
    }
}

/// Mirror the native report boundary: ordinary flag read/write, then the
/// fallible clock and actor label reads, without holding actor state across Lua.
pub(super) fn report(
    call: &mut HostCall<'_>,
    actor: TableId,
    callback: Callback<'_>,
    error: Error,
) -> Result<(), Error> {
    if error.kind() != ErrorKind::Runtime {
        return Err(error);
    }
    call.with_roots(&[Value::Table(actor)], |call| {
        let flag = sprites::field(call, actor, callback.flag())?;
        if !matches!(flag, Value::Nil | Value::Boolean(false)) {
            return Ok(());
        }
        sprites::set(call, actor, callback.flag(), Value::Boolean(true))?;
        let beat = effects::values(call, false).map_or(0., |values| values[0]);
        let kind = assets::field(call, actor, b"__songlua_actor_type")
            .ok()
            .flatten()
            .unwrap_or_else(|| "Actor".to_owned());
        let name = assets::field(call, actor, b"Name").ok().flatten();
        let label = match name {
            Some(name) if !name.trim().is_empty() => format!("{kind} '{name}'"),
            _ => kind,
        };
        let callback_name: std::borrow::Cow<'_, str> = match callback {
            Callback::Update => "update function".into(),
            Callback::Queue => "update queue".into(),
            Callback::Draw => "draw callback".into(),
            Callback::Health => "HealthStateChanged".into(),
            Callback::Gameplay(_, label) => label.into(),
            Callback::Command(name) => format!("{}Command", String::from_utf8_lossy(name)).into(),
        };
        let detail = format!("{callback_name} for {label} at beat {beat:.3}: {error}");
        // Preserve the existing standalone adapter diagnostic view. Only the
        // separate callback stream contributes to compiler unsupported counts.
        let notice = match callback {
            Callback::Queue => format!("update queue: {error}"),
            Callback::Command(name) => format!("{}Command: {error}", String::from_utf8_lossy(name)),
            Callback::Update | Callback::Draw | Callback::Health | Callback::Gameplay(..) => {
                error.to_string()
            }
        };
        let guard = call.store();
        if matches!(callback, Callback::Update)
            && let Some(actor) = guard.actors.get_mut(&actor)
        {
            actor.update_error_reported = true;
        }
        guard.diagnostics.push(notice);
        guard.callback_errors.push(detail);
        Ok(())
    })
}

impl Actors {
    pub(crate) fn take_callback_errors(&self, vm: &mut Vm) -> Vec<String> {
        std::mem::take(&mut vm.store().callback_errors)
    }
}
