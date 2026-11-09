//! Song actor render bindings. Tween destinations and immediate render settings
//! stay separate so queued commands cannot delay native immediate properties.
use super::action_capture;
use super::{
    Shared, arg, capture, children, float_scalar, helper, song_immediate, song_media,
    song_properties, song_replay,
};
use crate::base_transform::BaseField;
use crate::{SongLuaOverlayUpdateTarget as Target, SongLuaOverlayUpdateValue as Update};
use deadlib_lua::{Error, HostCall, TableId, Value};
use deadlib_render_core::CullMode;

pub(super) const ACTOR_METHODS: &[&str] = &[
    "cropleft",
    "cropright",
    "croptop",
    "cropbottom",
    "fadeleft",
    "faderight",
    "fadetop",
    "fadebottom",
    "skewx",
    "skewy",
    "diffuse",
    "diffusecolor",
    "diffusealpha",
    "glow",
    "diffuseupperleft",
    "diffuseupperright",
    "diffuselowerleft",
    "diffuselowerright",
    "diffuseleftedge",
    "diffuserightedge",
    "diffusetopedge",
    "diffusebottomedge",
    "GetDiffuse",
    "GetDiffuseAlpha",
    "GetGlow",
    "getrotation",
    "basealpha",
    "basezoom",
    "basezoomx",
    "basezoomy",
    "basezoomz",
    "GetBaseZoomX",
    "GetBaseZoomY",
    "GetBaseZoomZ",
    "baserotationx",
    "baserotationy",
    "baserotationz",
    "heading",
    "pitch",
    "roll",
    "align",
    "halign",
    "valign",
    "horizalign",
    "vertalign",
    "GetHAlign",
    "GetVAlign",
    "shadowlength",
    "shadowlengthx",
    "shadowlengthy",
    "shadowcolor",
    "zoomto",
    "zoomtowidth",
    "zoomtoheight",
    "stretchto",
    "FullScreen",
    "Center",
    "CenterX",
    "CenterY",
    "scaletofit",
    "scaletocover",
    "blend",
    "backfacecull",
    "cullmode",
    "zbuffer",
    "ztest",
    "zwrite",
    "ztestmode",
    "zbias",
    "clearzbuffer",
    "MaskSource",
    "MaskDest",
    "SetTextureFiltering",
];
pub(super) const FRAME_METHODS: &[&str] = &["fov", "vanishpoint", "SetDrawByZPosition"];

pub(super) fn initialize(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
) -> Result<(), Error> {
    let key = call.try_intern(b"FOV")?;
    let value = call.raw_table_get(actor, key)?;
    if value != Value::Nil {
        let number = super::sprites::number(call, value)? as f32;
        action_capture::prepare_write(call, store, actor)?;
        scalar(call, store, actor, "fov", number);
    }
    Ok(())
}

pub(super) fn text<'a>(call: &'a HostCall<'_>, value: Value) -> Option<&'a str> {
    let Value::String(id) = value else {
        return None;
    };
    std::str::from_utf8(call.string_bytes(id)?).ok()
}

fn alignment(call: &HostCall<'_>, value: Value, vertical: bool) -> Option<f32> {
    if let Some(number) = float_scalar(call, value, false) {
        return Some(number);
    }
    let token = crate::borrowed_alignment_token(text(call, value)?);
    if token.eq_ignore_ascii_case(if vertical { "top" } else { "left" }) {
        Some(0.0)
    } else if token.eq_ignore_ascii_case("center") || token.eq_ignore_ascii_case("middle") {
        Some(0.5)
    } else if token.eq_ignore_ascii_case(if vertical { "bottom" } else { "right" }) {
        Some(1.0)
    } else {
        None
    }
}

pub(super) fn color(call: &mut HostCall<'_>) -> Result<Option<[f32; 4]>, Error> {
    color_at(call, 1)
}
pub(super) fn color_at(call: &mut HostCall<'_>, offset: usize) -> Result<Option<[f32; 4]>, Error> {
    match arg(call, offset) {
        Value::Table(table) => {
            let mut out = [1.0; 4];
            for (i, component) in out.iter_mut().enumerate() {
                let value = call.raw_table_get(table, Value::Number((i + 1) as f64))?;
                if i == 3 && value == Value::Nil {
                    continue;
                }
                // The production table reader uses Lua numeric coercion, which
                // accepts hex strings and nonfinite numbers. It ignores __index.
                let converted = call.call(helper(call, 6)?, &[value])?[0];
                let Value::Number(number) = converted else {
                    return Ok(None);
                };
                *component = number as f32;
            }
            Ok(Some(out))
        }
        Value::String(_) => Ok(text(call, arg(call, offset))
            .map(|text| crate::parse_color_text(text).unwrap_or([1.0; 4]))),
        first => {
            let Some(r) = float_scalar(call, first, false) else {
                return Ok(None);
            };
            let Some(g) = float_scalar(call, arg(call, offset + 1), false) else {
                return Ok(None);
            };
            let Some(b) = float_scalar(call, arg(call, offset + 2), false) else {
                return Ok(None);
            };
            Ok(Some([
                r,
                g,
                b,
                float_scalar(call, arg(call, offset + 3), false).unwrap_or(1.0),
            ]))
        }
    }
}

fn push_color(call: &mut HostCall<'_>, color: [f32; 4]) -> Result<(), Error> {
    let table = call.try_new_table()?;
    call.with_roots(&[Value::Table(table)], |call| {
        for (i, component) in color.into_iter().enumerate() {
            call.raw_table_set(
                table,
                Value::Number((i + 1) as f64),
                Value::Number(component as f64),
            )?;
        }
        call.push(Value::Table(table))
    })
}

fn strict_boolean(call: &HostCall<'_>, name: &str) -> Result<bool, Error> {
    match arg(call, 1) {
        Value::Boolean(value) => Ok(value),
        Value::Number(value) => Ok(value.trunc() != 0.0),
        _ => Err(call.error(format!("{name} expects a boolean or number").as_bytes())),
    }
}

fn cull_mode(call: &HostCall<'_>) -> Result<CullMode, Error> {
    match arg(call, 1) {
        Value::Number(0.0) => Ok(CullMode::Back),
        Value::Number(1.0) => Ok(CullMode::Front),
        Value::Number(2.0) => Ok(CullMode::None),
        value => match text(call, value) {
            Some("CullMode_Back") => Ok(CullMode::Back),
            Some("CullMode_Front") => Ok(CullMode::Front),
            Some("CullMode_None") => Ok(CullMode::None),
            Some(value) if value.eq_ignore_ascii_case("back") => Ok(CullMode::Back),
            Some(value) if value.eq_ignore_ascii_case("front") => Ok(CullMode::Front),
            Some(value) if value.eq_ignore_ascii_case("none") => Ok(CullMode::None),
            _ => Err(call.error(b"invalid CullMode")),
        },
    }
}

fn depth(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
    test: Option<(bool, bool)>,
    write: Option<bool>,
) -> Result<(), Error> {
    if let Some((enabled, on_fail)) = test {
        depth_flag(call, store, actor, Target::DepthTest, enabled)?;
        depth_flag(call, store, actor, Target::DepthTestOnFail, on_fail)?;
    }
    if let Some(enabled) = write {
        depth_flag(call, store, actor, Target::DepthWrite, enabled)?;
    }
    Ok(())
}

fn depth_flag(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
    target: Target,
    enabled: bool,
) -> Result<(), Error> {
    action_capture::prepare_write(call, store, actor)?;
    {
        let mut store = store.lock().unwrap();
        let render = &mut store.actors.get_mut(&actor).unwrap().state.render;
        match target {
            Target::DepthTest => render.depth_test = enabled,
            Target::DepthTestOnFail => render.depth_test_on_fail = enabled,
            Target::DepthWrite => render.depth_write = enabled,
            _ => unreachable!(),
        }
    }
    super::host_compat::raw_flag(
        call,
        store,
        actor,
        match target {
            Target::DepthTest => "__songlua_state_depth_test",
            Target::DepthTestOnFail => "__songlua_state_depth_test_on_fail",
            Target::DepthWrite => "__songlua_state_depth_write",
            _ => unreachable!(),
        },
        enabled,
    )?;
    song_properties::record(call, store, actor, target, Update::Bool(enabled));
    for child in children(call, store, actor)? {
        depth_flag(call, store, child, target, enabled)?;
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
    if name == "align" {
        for (index, vertical) in [(1, false), (2, true)] {
            if let Some(value) = alignment(call, arg(call, index), vertical) {
                action_capture::prepare_write(call, store, actor)?;
                let mut delta = crate::SongLuaOverlayStateDelta::default();
                {
                    let mut guard = store.lock().unwrap();
                    let actor = guard.actors.get_mut(&actor).unwrap();
                    actor.base_transform.set(
                        if vertical {
                            BaseField::VAlign
                        } else {
                            BaseField::HAlign
                        },
                        value,
                    );
                    let render = &mut actor.state.render;
                    if vertical {
                        render.valign = value;
                        delta.valign = Some(value);
                    } else {
                        render.halign = value;
                        delta.halign = Some(value);
                    }
                }
                capture::write(call, store, actor, delta, true);
            }
        }
        return call.push(Value::Table(actor));
    }
    if matches!(name, "FullScreen" | "Center" | "CenterX" | "CenterY") {
        screen_geometry(call, store, actor, name)?;
        return call.push(Value::Table(actor));
    }
    // Parse values before taking the metadata lock. Lua numeric conversion and
    // return-table allocation may reenter the VM or collect traced captures.
    if matches!(name, "GetDiffuse" | "GetGlow") {
        let color = {
            let store = store.lock().unwrap();
            let pose = store.actors[&actor].getter_destination();
            if name == "GetDiffuse" {
                pose.diffuse
            } else {
                pose.glow
            }
        };
        return push_color(call, color);
    }
    if name == "getrotation" {
        let rotation = store.lock().unwrap().actors[&actor].state.rotation;
        for value in rotation {
            call.push(Value::Number(value as f64))?;
        }
        return Ok(());
    }
    if name.starts_with("Get") {
        let value = {
            let store = store.lock().unwrap();
            let actor = &store.actors[&actor];
            let render = &actor.state.render;
            match name {
                "GetDiffuseAlpha" => actor.getter_destination().diffuse[3],
                "GetBaseZoomX" => render.basezoom_x,
                "GetBaseZoomY" => render.basezoom_y,
                "GetBaseZoomZ" => render.basezoom_z,
                "GetHAlign" => render.halign,
                "GetVAlign" => render.valign,
                _ => unreachable!(),
            }
        };
        return call.push(Value::Number(value as f64));
    }
    if matches!(name, "diffuse" | "diffusecolor" | "glow" | "shadowcolor")
        || corner_mask(name).is_some()
    {
        if let Some(mut color) = color(call)? {
            action_capture::prepare_write(call, store, actor)?;
            let shared = store;
            let id = actor;
            let mut store = store.lock().unwrap();
            let sync = !store.playback && matches!(name, "diffuse" | "diffusecolor");
            let actor = store.actors.get_mut(&actor).unwrap();
            let alpha = actor.getter_destination().diffuse[3];
            if name == "shadowcolor" {
                actor.state.render.shadow_color = color;
            } else {
                let pose = actor.destination_mut();
                if let Some(mask) = corner_mask(name) {
                    for (i, vertex) in pose
                        .vertex_colors
                        .get_or_insert([[1.0; 4]; 4])
                        .iter_mut()
                        .enumerate()
                    {
                        if mask & (1 << i) != 0 {
                            *vertex = color;
                        }
                    }
                } else if name == "glow" {
                    pose.glow = color;
                } else {
                    if name == "diffusecolor" {
                        color[3] = alpha;
                    }
                    pose.diffuse = color;
                }
            }
            if sync {
                actor.sync_rotation_getters();
            }
            drop(store);
            capture::property(
                call,
                shared,
                id,
                if corner_mask(name).is_some() {
                    "vertex_colors"
                } else {
                    name
                },
            );
            if matches!(name, "diffuse" | "diffusecolor") {
                song_replay::sync_getters(call, shared, id)?;
            }
        }
    } else if matches!(name, "halign" | "valign" | "horizalign" | "vertalign") {
        let align = alignment(call, value, matches!(name, "valign" | "vertalign"));
        let text_align = (name == "horizalign")
            .then(|| text(call, value).and_then(crate::parse_overlay_text_align))
            .flatten();
        if align.is_some() || text_align.is_some() {
            action_capture::prepare_write(call, store, actor)?;
        }
        let shared = store;
        let mut guard = store.lock().unwrap();
        let actor_state = guard.actors.get_mut(&actor).unwrap();
        if let Some(value) = align {
            actor_state.base_transform.set(
                if matches!(name, "valign" | "vertalign") {
                    BaseField::VAlign
                } else {
                    BaseField::HAlign
                },
                value,
            );
        }
        let render = &mut actor_state.state.render;
        if let Some(value) = align {
            if matches!(name, "valign" | "vertalign") {
                render.valign = value;
            } else {
                render.halign = value;
            }
        }
        if let Some(value) = text_align {
            render.text_align = value;
        }
        drop(guard);
        if let Some(value) = align {
            song_immediate::record(
                call,
                shared,
                actor,
                if matches!(name, "valign" | "vertalign") {
                    crate::SongLuaOverlayUpdateTarget::VAlign
                } else {
                    crate::SongLuaOverlayUpdateTarget::HAlign
                },
                crate::SongLuaOverlayUpdateValue::F32(value),
            );
        }
        if let Some(value) = text_align {
            song_immediate::record(
                call,
                shared,
                actor,
                crate::SongLuaOverlayUpdateTarget::TextAlign,
                crate::SongLuaOverlayUpdateValue::TextAlign(value),
            );
        }
    } else if name == "backfacecull" || name == "cullmode" {
        let mode = if name == "backfacecull" {
            if strict_boolean(call, name)? {
                CullMode::Back
            } else {
                CullMode::None
            }
        } else {
            cull_mode(call)?
        };
        action_capture::prepare_write(call, store, actor)?;
        store
            .lock()
            .unwrap()
            .actors
            .get_mut(&actor)
            .unwrap()
            .state
            .render
            .cull_mode = mode;
        song_immediate::record(
            call,
            store,
            actor,
            crate::SongLuaOverlayUpdateTarget::CullMode,
            crate::SongLuaOverlayUpdateValue::CullMode(mode),
        );
    } else if matches!(name, "MaskSource" | "MaskDest") {
        action_capture::prepare_write(call, store, actor)?;
        let key = if name == "MaskSource" {
            "mask_source"
        } else {
            "mask_dest"
        };
        {
            let mut guard = store.lock().unwrap();
            let render = &mut guard.actors.get_mut(&actor).unwrap().state.render;
            if name == "MaskSource" {
                render.mask_source = true;
            } else {
                render.mask_dest = true;
            }
        }
        super::sprites::set(
            call,
            actor,
            &format!("__songlua_state_{key}"),
            Value::Boolean(true),
        )?;
        song_media::record(call, store, actor, key);
    } else if matches!(name, "zbuffer" | "ztest" | "zwrite") {
        let enabled = strict_boolean(call, name)?;
        depth(
            call,
            store,
            actor,
            (name != "zwrite").then_some((enabled, false)),
            (name != "ztest").then_some(enabled),
        )?;
    } else if name == "ztestmode" {
        let mode = text(call, value).ok_or_else(|| call.error(b"ztestmode expects a ZTestMode"))?;
        let flags = match mode
            .trim()
            .trim_start_matches("ZTestMode_")
            .to_ascii_lowercase()
            .as_str()
        {
            "off" => (false, false),
            "writeonpass" => (true, false),
            "writeonfail" => (true, true),
            _ => return Err(call.error(format!("invalid ZTestMode: {mode}").as_bytes())),
        };
        depth(call, store, actor, Some(flags), None)?;
    } else if matches!(name, "SetTextureFiltering" | "SetDrawByZPosition") {
        let enabled = call
            .arguments()
            .get(1)
            .is_none_or(|v| !matches!(v, Value::Nil | Value::Boolean(false)));
        action_capture::prepare_write(call, store, actor)?;
        let mut guard = store.lock().unwrap();
        let render = &mut guard.actors.get_mut(&actor).unwrap().state.render;
        if name == "SetTextureFiltering" {
            render.texture_filtering = enabled;
        } else {
            render.draw_by_z_position = enabled;
        }
        drop(guard);
        super::host_compat::raw_flag(
            call,
            store,
            actor,
            if name == "SetTextureFiltering" {
                "__songlua_state_texture_filtering"
            } else {
                "__songlua_state_draw_by_z_position"
            },
            enabled,
        )?;
        song_properties::record(
            call,
            store,
            actor,
            if name == "SetTextureFiltering" {
                Target::TextureFiltering
            } else {
                Target::DrawByZPosition
            },
            Update::Bool(enabled),
        );
    } else if name == "blend" {
        if let Some(blend) = text(call, value).and_then(crate::parse_overlay_blend_mode) {
            action_capture::prepare_write(call, store, actor)?;
            store
                .lock()
                .unwrap()
                .actors
                .get_mut(&actor)
                .unwrap()
                .state
                .render
                .blend = blend;
            song_properties::record(call, store, actor, Target::Blend, Update::Blend(blend));
        }
    } else if matches!(
        name,
        "stretchto"
            | "scaletofit"
            | "scaletocover"
            | "zoomto"
            | "zoomtowidth"
            | "zoomtoheight"
            | "vanishpoint"
    ) {
        geometry(call, store, actor, name)?;
    } else if name != "clearzbuffer" {
        let nonfinite = matches!(
            name,
            "croptop"
                | "cropbottom"
                | "fadeleft"
                | "faderight"
                | "fadetop"
                | "fadebottom"
                | "basealpha"
                | "heading"
                | "pitch"
                | "roll"
        ) || (name == "diffusealpha"
            && matches!(value, Value::Number(v) if v.is_infinite()));
        if let Some(number) = float_scalar(call, value, nonfinite) {
            action_capture::prepare_write(call, store, actor)?;
            scalar(call, store, actor, name, number);
            if name == "diffusealpha" {
                song_replay::sync_getters(call, store, actor)?;
            }
        }
    }
    call.push(Value::Table(actor))
}

fn corner_mask(name: &str) -> Option<u8> {
    Some(match name {
        "diffuseupperleft" => 1,
        "diffuseupperright" => 2,
        "diffuselowerleft" => 4,
        "diffuselowerright" => 8,
        "diffuseleftedge" => 5,
        "diffuserightedge" => 10,
        "diffusetopedge" => 3,
        "diffusebottomedge" => 12,
        _ => return None,
    })
}

fn scalar(call: &mut HostCall<'_>, store: &Shared, actor: TableId, name: &str, value: f32) {
    let shared = store;
    let id = actor;
    let mut store = store.lock().unwrap();
    let sync = name == "diffusealpha" && !store.playback;
    let actor = store.actors.get_mut(&actor).unwrap();
    let diffuse = actor.getter_destination().diffuse;
    match name {
        "basezoom" => {
            for field in [
                BaseField::Zoom,
                BaseField::ZoomX,
                BaseField::ZoomY,
                BaseField::ZoomZ,
            ] {
                actor.base_transform.set(field, value);
            }
        }
        "basezoomx" => actor.base_transform.set(BaseField::ZoomX, value),
        "basezoomy" => actor.base_transform.set(BaseField::ZoomY, value),
        "basezoomz" => actor.base_transform.set(BaseField::ZoomZ, value),
        _ => (),
    }
    let render = &mut actor.state.render;
    match name {
        "basealpha" => render.base_alpha = value,
        "basezoom" => {
            render.basezoom = value;
            render.basezoom_x = value;
            render.basezoom_y = value;
            render.basezoom_z = value;
        }
        "basezoomx" => render.basezoom_x = value,
        "basezoomy" => render.basezoom_y = value,
        "basezoomz" => render.basezoom_z = value,
        "baserotationx" => render.base_rotation[0] = value,
        "baserotationy" => render.base_rotation[1] = value,
        "baserotationz" => render.base_rotation[2] = value,
        "shadowlength" => render.shadow_len = [value, -value],
        "shadowlengthx" => render.shadow_len[0] = value,
        "shadowlengthy" => render.shadow_len[1] = -value,
        "zbias" => render.z_bias = value,
        "fov" => render.fov = (value != -1.0).then_some(value),
        _ => {
            let pose = actor.destination_mut();
            match name {
                "cropleft" => pose.cropleft = value,
                "cropright" => pose.cropright = value,
                "croptop" => pose.croptop = value,
                "cropbottom" => pose.cropbottom = value,
                "fadeleft" => pose.fadeleft = value,
                "faderight" => pose.faderight = value,
                "fadetop" => pose.fadetop = value,
                "fadebottom" => pose.fadebottom = value,
                "skewx" => pose.skew_x = value,
                "skewy" => pose.skew_y = value,
                "diffusealpha" => {
                    pose.diffuse = diffuse;
                    pose.diffuse[3] = value;
                }
                "heading" | "pitch" | "roll" => {
                    let axis = match name {
                        "heading" => 1,
                        "pitch" => 0,
                        _ => 2,
                    };
                    pose.quaternion = crate::actor_quaternion::rotate(pose.quaternion, axis, value);
                }
                _ => unreachable!(),
            }
        }
    }
    if sync {
        actor.sync_rotation_getters();
    }
    drop(store);
    capture::property(call, shared, id, name);
    if matches!(name, "zbias" | "fov") {
        song_properties::record(
            call,
            shared,
            id,
            if name == "zbias" {
                Target::ZBias
            } else {
                Target::Fov
            },
            Update::F32(value),
        );
    }
}

fn geometry(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
    name: &str,
) -> Result<(), Error> {
    let count = match name {
        "zoomtowidth" | "zoomtoheight" => 1,
        "zoomto" | "vanishpoint" => 2,
        _ => 4,
    };
    let mut values = [0.0; 4];
    for (i, value) in values.iter_mut().enumerate().take(count) {
        match float_scalar(call, arg(call, i + 1), false) {
            Some(number) => *value = number,
            None if name == "stretchto" => {}
            None => return Ok(()),
        }
    }
    geometry_values(call, store, actor, name, values)
}

fn screen_size(call: &mut HostCall<'_>) -> Result<[f32; 2], Error> {
    let globals = super::table(call, helper(call, 5)?)?;
    let mut size = [640., 480.];
    for (index, key) in ["SCREEN_WIDTH", "SCREEN_HEIGHT"].into_iter().enumerate() {
        let value = super::sprites::field(call, globals, key)?;
        if value != Value::Nil {
            let number = super::sprites::number(call, value)?.trunc();
            if !(i32::MIN as f64..=i32::MAX as f64).contains(&number) {
                return Err(call.error(b"screen size out of range"));
            }
            size[index] = number as i32 as f32;
        }
    }
    Ok(size)
}

fn screen_geometry(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
    name: &str,
) -> Result<(), Error> {
    if name == "FullScreen" {
        let [width, height] = screen_size(call)?;
        return geometry_values(call, store, actor, "stretchto", [0., 0., width, height]);
    }
    let globals = super::table(call, helper(call, 5)?)?;
    let mut center = [0.; 2];
    for (index, key) in ["SCREEN_CENTER_X", "SCREEN_CENTER_Y"]
        .into_iter()
        .enumerate()
    {
        let value = super::sprites::field(call, globals, key)?;
        let value = if value == Value::Nil {
            None
        } else {
            Some(super::sprites::number(call, value)? as f32)
        };
        // The original helper eagerly evaluates its size fallback, even with
        // an explicit center; invalid dimensions must still report an error.
        center[index] = value.unwrap_or(screen_size(call)?[index] * 0.5);
    }
    action_capture::prepare_write(call, store, actor)?;
    let mut guard = store.lock().unwrap();
    let pose = guard.actors.get_mut(&actor).unwrap().destination_mut();
    if name != "CenterY" {
        pose.x = center[0];
    }
    if name != "CenterX" {
        pose.y = center[1];
    }
    drop(guard);
    capture::property(call, store, actor, name);
    Ok(())
}

fn geometry_values(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
    name: &str,
    values: [f32; 4],
) -> Result<(), Error> {
    if name != "vanishpoint" {
        super::sprites::refresh_size(call, store, actor)?;
    }
    let bitmap = store.lock().unwrap().actors[&actor].state.class == "BitmapText";
    let text_height =
        bitmap && name == "zoomtoheight" && !super::text::content(call, actor)?.is_empty();
    let scale_plan = if matches!(name, "scaletofit" | "scaletocover") {
        let guard = store.lock().unwrap();
        let instance = &guard.actors[&actor];
        let plan = crate::scale_to_rect_plan(
            values,
            instance.state.size.map(f32::abs),
            [instance.state.render.halign, instance.state.render.valign],
            name == "scaletocover",
        );
        let Some(plan) = plan else { return Ok(()) };
        Some(plan)
    } else {
        None
    };
    action_capture::prepare_write(call, store, actor)?;
    let id = actor;
    let mut guard = store.lock().unwrap();
    let actor = guard.actors.get_mut(&id).unwrap();
    if bitmap && (matches!(name, "zoomto" | "zoomtowidth") || text_height) {
        let [width, height] = actor.state.size.map(f32::abs);
        let size = match name {
            "zoomtowidth" => [values[0], height],
            "zoomtoheight" => [width, values[0]],
            _ => [values[0], values[1]],
        };
        actor.state.size = size;
        actor.state.render.size = Some(size);
        drop(guard);
        capture::property(call, store, id, "size");
        return Ok(());
    }
    let mut scaled = false;
    let [width, height] = actor.state.size.map(f32::abs);
    let render = &mut actor.state.render;
    if name == "vanishpoint" {
        render.vanishpoint = Some([values[0], values[1]]);
        drop(guard);
        song_properties::record(
            call,
            store,
            id,
            Target::Vanishpoint,
            Update::Vec2([values[0], values[1]]),
        );
        return Ok(());
    }
    let pose = actor.destination_mut();
    match name {
        "zoomtowidth" => pose.zoom_x = values[0] / width,
        "zoomtoheight" => pose.zoom_y = values[0] / height,
        "zoomto" => {
            pose.zoom_x = values[0] / width;
            pose.zoom_y = values[1] / height;
        }
        "stretchto" => {
            let w = values[2] - values[0];
            let h = values[3] - values[1];
            pose.x = values[0] + w / 2.0;
            pose.y = values[1] + h / 2.0;
            pose.zoom_x = w / width;
            pose.zoom_y = h / height;
        }
        "scaletofit" | "scaletocover" => {
            let plan = scale_plan.unwrap();
            scaled = true;
            pose.x = plan.pos[0];
            pose.y = plan.pos[1];
            pose.zoom = plan.zoom;
            pose.zoom_x = plan.zoom;
            pose.zoom_y = plan.zoom;
            pose.zoom_z = plan.zoom;
            if plan.flip_x {
                pose.rot_y_deg = 180.0;
            }
            if plan.flip_y {
                pose.rot_x_deg = 180.0;
            }
            if plan.flip_x {
                actor.state.rotation[1] = 180.0;
            }
            if plan.flip_y {
                actor.state.rotation[0] = 180.0;
            }
        }
        _ => unreachable!(),
    }
    drop(guard);
    capture::property(call, store, id, name);
    if let Some(plan) = scale_plan {
        if plan.flip_x {
            capture::property(call, store, id, "rotationy");
        }
        if plan.flip_y {
            capture::property(call, store, id, "rotationx");
        }
    }
    if scaled {
        super::text::pre_zoom(call, store, id, true, true)?;
    }
    Ok(())
}
