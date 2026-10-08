// Native public method tables and instance lookup. Inventories come from the
// bidirectional compiled ITGmania controls in actor-class-provenance.json.
use mlua::{Lua, MultiValue, Table, Value};

pub(crate) const METHODS_KEY: &str = "__songlua_bound_methods";

const ACTOR: &[&str] = &[
    "name",
    "sleep",
    "linear",
    "accelerate",
    "decelerate",
    "spring",
    "tween",
    "stoptweening",
    "finishtweening",
    "hurrytweening",
    "GetTweenTimeLeft",
    "x",
    "y",
    "z",
    "xy",
    "addx",
    "addy",
    "addz",
    "zoom",
    "zoomx",
    "zoomy",
    "zoomz",
    "zoomto",
    "zoomtowidth",
    "zoomtoheight",
    "setsize",
    "SetWidth",
    "SetHeight",
    "basealpha",
    "basezoom",
    "basezoomx",
    "basezoomy",
    "basezoomz",
    "stretchto",
    "cropleft",
    "croptop",
    "cropright",
    "cropbottom",
    "fadeleft",
    "fadetop",
    "faderight",
    "fadebottom",
    "diffuse",
    "diffuseupperleft",
    "diffuseupperright",
    "diffuselowerleft",
    "diffuselowerright",
    "diffuseleftedge",
    "diffuserightedge",
    "diffusetopedge",
    "diffusebottomedge",
    "diffusealpha",
    "diffusecolor",
    "glow",
    "aux",
    "getaux",
    "rotationx",
    "rotationy",
    "rotationz",
    "addrotationx",
    "addrotationy",
    "addrotationz",
    "getrotation",
    "baserotationx",
    "baserotationy",
    "baserotationz",
    "skewx",
    "skewy",
    "heading",
    "pitch",
    "roll",
    "shadowlength",
    "shadowlengthx",
    "shadowlengthy",
    "shadowcolor",
    "horizalign",
    "vertalign",
    "halign",
    "valign",
    "diffuseblink",
    "diffuseshift",
    "diffuseramp",
    "glowblink",
    "glowshift",
    "glowramp",
    "rainbow",
    "wag",
    "bounce",
    "bob",
    "pulse",
    "spin",
    "vibrate",
    "stopeffect",
    "effectcolor1",
    "effectcolor2",
    "effectperiod",
    "effecttiming",
    "effect_hold_at_full",
    "effectoffset",
    "effectclock",
    "effectmagnitude",
    "geteffectmagnitude",
    "scaletocover",
    "scaletofit",
    "animate",
    "play",
    "pause",
    "setstate",
    "GetNumStates",
    "texturetranslate",
    "texturewrapping",
    "SetTextureFiltering",
    "blend",
    "zbuffer",
    "ztest",
    "ztestmode",
    "zwrite",
    "zbias",
    "clearzbuffer",
    "backfacecull",
    "cullmode",
    "visible",
    "hibernate",
    "draworder",
    "playcommand",
    "queuecommand",
    "queuemessage",
    "addcommand",
    "GetCommand",
    "RunCommandsRecursively",
    "GetX",
    "GetY",
    "GetZ",
    "GetDestX",
    "GetDestY",
    "GetDestZ",
    "GetWidth",
    "GetHeight",
    "GetZoomedWidth",
    "GetZoomedHeight",
    "GetZoom",
    "GetZoomX",
    "GetZoomY",
    "GetZoomZ",
    "GetRotationX",
    "GetRotationY",
    "GetRotationZ",
    "GetBaseZoomX",
    "GetBaseZoomY",
    "GetBaseZoomZ",
    "GetSecsIntoEffect",
    "GetEffectDelta",
    "GetDiffuse",
    "GetDiffuseAlpha",
    "GetGlow",
    "GetVisible",
    "GetHAlign",
    "GetVAlign",
    "GetName",
    "GetParent",
    "GetFakeParent",
    "SetFakeParent",
    "AddWrapperState",
    "RemoveWrapperState",
    "GetNumWrapperStates",
    "GetWrapperState",
    "SetRateScalingEnabled",
    "GetRateScalingEnabled",
    "Draw",
    "get_tween_uses_effect_delta",
    "set_tween_uses_effect_delta",
];

const FRAME: &[&str] = &[
    "playcommandonchildren",
    "playcommandonleaves",
    "runcommandsonleaves",
    "RunCommandsOnChildren",
    "propagate",
    "fov",
    "SetUpdateRate",
    "GetUpdateRate",
    "SetFOV",
    "vanishpoint",
    "GetChild",
    "GetChildren",
    "GetNumChildren",
    "SetDrawByZPosition",
    "SetDrawFunction",
    "GetDrawFunction",
    "SetUpdateFunction",
    "SortByDrawOrder",
    "SetAmbientLightColor",
    "SetDiffuseLightColor",
    "SetSpecularLightColor",
    "SetLightDirection",
    "AddChildFromPath",
    "RemoveChild",
    "RemoveAllChildren",
];

const SPRITE: &[&str] = &[
    "Load",
    "LoadBanner",
    "LoadBackground",
    "LoadFromCached",
    "customtexturerect",
    "SetCustomImageRect",
    "SetCustomPosCoords",
    "StopUsingCustomPosCoords",
    "texcoordvelocity",
    "get_use_effect_clock_for_texcoords",
    "set_use_effect_clock_for_texcoords",
    "scaletoclipped",
    "CropTo",
    "stretchtexcoords",
    "addimagecoords",
    "setstate",
    "GetState",
    "SetStateProperties",
    "GetAnimationLengthSeconds",
    "SetSecondsIntoAnimation",
    "SetTexture",
    "GetTexture",
    "SetEffectMode",
    "GetNumStates",
    "SetAllStateDelays",
    "GetDecodeMovie",
    "SetDecodeMovie",
];

const AFT: &[&str] = &[
    "Create",
    "EnableDepthBuffer",
    "EnableAlphaBuffer",
    "EnableFloat",
    "EnablePreserveTexture",
    "SetTextureName",
    "GetTexture",
];

const AMV: &[&str] = &[
    "SetVertex",
    "SetVertices",
    "SetEffectMode",
    "SetTextureMode",
    "SetLineWidth",
    "SetDrawState",
    "SetNumVertices",
    "GetNumVertices",
    "GetDestDrawMode",
    "GetDestFirstToDraw",
    "GetDestNumToDraw",
    "GetCurrDrawMode",
    "GetCurrFirstToDraw",
    "GetCurrNumToDraw",
    "GetSpline",
    "SetVertsFromSplines",
    "GetUseAnimationState",
    "SetUseAnimationState",
    "GetNumStates",
    "GetNumQuadStates",
    "AddState",
    "RemoveState",
    "GetState",
    "SetState",
    "GetStateData",
    "SetStateData",
    "SetStateProperties",
    "SetAllStateDelays",
    "SetSecondsIntoAnimation",
    "AddQuadState",
    "RemoveQuadState",
    "GetQuadState",
    "SetQuadState",
    "ForceStateUpdate",
    "GetDecodeMovie",
    "SetDecodeMovie",
    "SetTexture",
    "GetTexture",
    "LoadTexture",
];

pub(crate) fn install(lua: &Lua) -> mlua::Result<()> {
    install_native(lua)?;
    let globals = lua.globals();
    // Banner is not linked into the compiled actor oracle. Its own methods and
    // base class are declared by the checked-out src/Banner.cpp Luna binding.
    let banner = lua.create_table()?;
    for method in [
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
    ] {
        forward(lua, &banner, method)?;
    }
    let mt = lua.create_table()?;
    mt.set("__index", globals.get::<Table>("Sprite")?)?;
    banner.set_metatable(Some(mt))?;
    globals.set("Banner", banner)?;
    let class = globals.get::<Table>("Actor")?;
    for method in [
        "ease",
        "bouncebegin",
        "bounceend",
        "smooth",
        "drop",
        "compound",
        "hide_if",
        "player",
        "align",
        "FullScreen",
        "scale_or_crop_background_no_move",
        "scale_or_crop_background",
        "CenterX",
        "CenterY",
        "Center",
        "bezier",
        "Real",
        "RealInverse",
        "MaskSource",
        "MaskDest",
        "thump",
        "heartbeat",
        "LyricCommand",
        "SetSize",
        "hidden",
    ] {
        forward(lua, &class, method)?;
    }
    let class = globals.get::<Table>("ActorFrame")?;
    for method in ["propagatecommand"] {
        forward(lua, &class, method)?;
    }
    let class = globals.get::<Table>("Sprite")?;
    for method in [
        "LoadFromSongBanner",
        "LoadFromSongBackground",
        "LoadFromCurrentSongBackground",
        "position",
        "loop",
        "rate",
        "cropto",
    ] {
        forward(lua, &class, method)?;
    }
    Ok(())
}

pub(crate) fn install_native(lua: &Lua) -> mlua::Result<()> {
    let globals = lua.globals();
    for (name, base, methods) in [
        ("Actor", None, ACTOR),
        ("ActorFrame", Some("Actor"), FRAME),
        ("Sprite", Some("Actor"), SPRITE),
        ("ActorFrameTexture", Some("ActorFrame"), AFT),
        ("ActorMultiVertex", Some("Actor"), AMV),
    ] {
        let class = lua.create_table()?;
        for method in methods {
            forward(lua, &class, method)?;
        }
        if let Some(base) = base {
            let mt = lua.create_table()?;
            mt.set("__index", globals.get::<Table>(base)?)?;
            class.set_metatable(Some(mt))?;
        }
        globals.set(name, class)?;
    }
    Ok(())
}

pub(crate) fn forward(lua: &Lua, class: &Table, name: &'static str) -> mlua::Result<()> {
    class.set(
        name,
        lua.create_function(move |_, args: MultiValue| {
            let Some(Value::Table(actor)) = args.front() else {
                return Err(mlua::Error::RuntimeError(format!(
                    "actor method {name} requires an actor"
                )));
            };
            let bound = actor.raw_get::<Option<Table>>(METHODS_KEY)?;
            let method = match bound {
                Some(bound) => bound.raw_get::<Value>(name)?,
                None => actor.raw_get::<Value>(name)?,
            };
            let Value::Function(method) = method else {
                return Err(mlua::Error::RuntimeError(format!(
                    "native actor method {name} has no semantic implementation"
                )));
            };
            method.call::<MultiValue>(args)
        })?,
    )
}

fn class_name(kind: &str) -> &'static str {
    match kind {
        "Sprite" | "Quad" => "Sprite",
        "Banner" => "Banner",
        "ActorFrameTexture" => "ActorFrameTexture",
        "ActorMultiVertex" => "ActorMultiVertex",
        "Player" => "Player",
        "NoteField" => "NoteField",
        kind if is_frame(kind) => "ActorFrame",
        _ => "Actor",
    }
}

fn is_frame(kind: &str) -> bool {
    matches!(
        kind,
        "ActorFrame"
            | "ActorFrameTexture"
            | "TopScreen"
            | "Player"
            | "NoteField"
            | "GraphDisplay"
            | "SongMeterDisplay"
            | "MeterDisplay"
            | "CourseContentsList"
            | "StepsDisplay"
            | "WrapperState"
    )
}

fn is_core(kind: &str) -> bool {
    matches!(
        kind,
        "Actor"
            | "ActorFrame"
            | "Sprite"
            | "Quad"
            | "Banner"
            | "ActorFrameTexture"
            | "ActorMultiVertex"
            | "WrapperState"
    )
}

pub(crate) fn bind(lua: &Lua, actor: &Table) -> mlua::Result<()> {
    let bound = match actor.raw_get::<Option<Table>>(METHODS_KEY)? {
        Some(bound) => bound,
        None => lua.create_table()?,
    };
    let mut methods = Vec::new();
    actor.for_each::<Value, Value>(|key, value| {
        if let (Value::String(name), Value::Function(function)) = (&key, &value) {
            let name = name.to_str()?;
            if !name.starts_with("__songlua_")
                && !name.ends_with("Command")
                && function.info().what == "C"
            {
                methods.push((key, value));
            }
        }
        Ok(())
    })?;
    for (key, value) in methods {
        bound.raw_set(key.clone(), value)?;
        actor.raw_set(key, Value::Nil)?;
    }
    actor.raw_set(METHODS_KEY, bound)
}

pub(crate) fn lookup(lua: &Lua, (actor, key): (Table, Value)) -> mlua::Result<Value> {
    let Value::String(name) = &key else {
        return Ok(Value::Nil);
    };
    let name = name.to_str()?;
    let kind = actor
        .raw_get::<Option<String>>("__songlua_actor_type")?
        .unwrap_or_else(|| "Actor".into());
    if let Some(class) = lua.globals().get::<Option<Table>>(class_name(&kind))? {
        let method = class.get::<Value>(key.clone())?;
        if !method.is_nil() {
            return Ok(method);
        }
        if is_core(&kind) {
            return Ok(Value::Nil);
        }
    }
    // Unlinked classes retain their existing own adapters until their native
    // inventories are audited. Frame callbacks are never inherited by Actor.
    if !is_frame(&kind)
        && matches!(
            name.as_ref(),
            "SetUpdateFunction" | "SetDrawFunction" | "GetDrawFunction"
        )
    {
        return Ok(Value::Nil);
    }
    match actor.raw_get::<Option<Table>>(METHODS_KEY)? {
        Some(methods) => methods.raw_get(key),
        None => Ok(Value::Nil),
    }
}

pub(crate) fn assign(_: &Lua, (actor, key, value): (Table, Value, Value)) -> mlua::Result<()> {
    if let (Value::String(name), Value::Function(function)) = (&key, &value) {
        let name = name.to_str()?;
        if !name.starts_with("__songlua_")
            && !name.ends_with("Command")
            && function.info().what == "C"
            && let Some(methods) = actor.raw_get::<Option<Table>>(METHODS_KEY)?
        {
            return methods.raw_set(key, value);
        }
    }
    actor.raw_set(key, value)
}
