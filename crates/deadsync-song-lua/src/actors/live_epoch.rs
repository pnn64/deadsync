//! Getters on saved actors after their previous native playback owner is gone.
use super::*;
fn field(call: &mut HostCall<'_>, actor: TableId, key: &str) -> Result<Option<f32>, Error> {
    let value = sprites::field(call, actor, key)?;
    if value == Value::Nil {
        Ok(None)
    } else {
        sprites::number(call, value).map(|n| Some(n as f32))
    }
}
pub(super) fn destination(
    call: &mut HostCall<'_>,
    actor: TableId,
    axis: &str,
) -> Result<f32, Error> {
    Ok(field(call, actor, &format!("__songlua_state_{axis}"))?.unwrap_or(0.))
}
fn zoom(call: &mut HostCall<'_>, actor: TableId, axis: &str, base: bool) -> Result<f32, Error> {
    let prefix = if base { "basezoom" } else { "zoom" };
    Ok(
        field(call, actor, &format!("__songlua_state_{prefix}_{axis}"))?
            .or(field(call, actor, &format!("__songlua_state_{prefix}"))?)
            .unwrap_or(1.),
    )
}
pub(super) fn getter(call: &mut HostCall<'_>, actor: TableId, name: &str) -> Result<bool, Error> {
    let value = match name {
        "GetX" | "GetY" | "GetZ" => {
            let axis = name[3..].to_ascii_lowercase();
            field(call, actor, &format!("__songlua_current_{axis}"))?
                .unwrap_or(destination(call, actor, &axis)?)
        }
        "GetDestX" | "GetDestY" | "GetDestZ" => {
            destination(call, actor, &name[7..].to_ascii_lowercase())?
        }
        "GetZoom" | "GetZoomX" => zoom(call, actor, "x", false)?,
        "GetZoomY" => zoom(call, actor, "y", false)?,
        "GetZoomZ" => zoom(call, actor, "z", false)?,
        "GetBaseZoomX" => zoom(call, actor, "x", true)?,
        "GetBaseZoomY" => zoom(call, actor, "y", true)?,
        "GetBaseZoomZ" => zoom(call, actor, "z", true)?,
        "GetHAlign" => field(call, actor, "__songlua_state_halign")?.unwrap_or(0.5),
        "GetVAlign" => field(call, actor, "__songlua_state_valign")?.unwrap_or(0.5),
        "GetZoomedWidth" | "GetZoomedHeight" => {
            sprites::refresh_size(call, actor)?;
            let axis = if name == "GetZoomedWidth" { 0 } else { 1 };
            let size = call.store().actors[&actor].state.size[axis].abs();
            let axis = if axis == 0 { "x" } else { "y" };
            size * (zoom(call, actor, axis, true)? * zoom(call, actor, axis, false)?)
        }
        "GetRotationX" | "GetRotationY" | "GetRotationZ" => field(
            call,
            actor,
            &format!(
                "__songlua_state_rot_{}_deg",
                name[11..].to_ascii_lowercase()
            ),
        )?
        .unwrap_or(0.),
        "getaux" => field(call, actor, "__songlua_aux")?.unwrap_or(0.),
        "getrotation" => {
            for axis in ["x", "y", "z"] {
                let v =
                    field(call, actor, &format!("__songlua_state_rot_{axis}_deg"))?.unwrap_or(0.);
                call.push(Value::Number(v as f64))?;
            }
            return Ok(true);
        }
        "GetVisible" => {
            let value = sprites::field(call, actor, "__songlua_visible")?;
            call.push(Value::Boolean(value == Value::Nil || value.is_truthy()))?;
            return Ok(true);
        }
        _ => return Ok(false),
    };
    call.push(Value::Number(value as f64))?;
    Ok(true)
}
