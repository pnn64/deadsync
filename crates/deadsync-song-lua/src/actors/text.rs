//! BitmapText bindings and immutable renderer payloads over the Rust host API.
use super::action_capture;
use super::{
    Host, arg, assets, boolish, float_scalar, get, get_static, helper, integer_scalar, render,
    song_media, sprites, table,
};
use deadlib_lua::{Error, ErrorKind, HostCall, TableId, Value};
use deadlib_present::actors::TextAttribute;
use sprites::{field, set, vector, vector_value};
use std::{path::PathBuf, sync::Arc};

#[derive(Clone, Debug)]
pub struct BitmapText {
    pub font: Option<(&'static str, PathBuf)>,
    pub text: Arc<str>,
    pub stroke_color: Option<[f32; 4]>,
    pub attributes: Arc<[TextAttribute]>,
}

pub(super) const METHODS: &[&str] = &[
    "wrapwidthpixels",
    "maxwidth",
    "maxheight",
    "max_dimension_use_zoom",
    "vertspacing",
    "settext",
    "rainbowscroll",
    "jitter",
    "distort",
    "undistort",
    "GetText",
    "AddAttribute",
    "ClearAttributes",
    "strokecolor",
    "getstrokecolor",
    "uppercase",
    "textglowmode",
    "get_mult_attrs_with_diffuse",
    "set_mult_attrs_with_diffuse",
];
const ATTRIBUTES: &str = "__songlua_text_attributes";

pub(super) fn text_value(call: &mut HostCall<'_>, value: Value) -> Result<String, Error> {
    match value {
        Value::String(_) => assets::text(call, value),
        Value::Number(n) => Ok(n.to_string()),
        Value::Boolean(b) => Ok(b.to_string()),
        _ => Ok(String::new()),
    }
}
pub(super) fn content(call: &mut HostCall<'_>, actor: TableId) -> Result<String, Error> {
    let value = field(call, actor, "Text")?;
    text_value(call, value)
}

pub(super) fn global_color(call: &mut HostCall<'_>) -> Result<(), Error> {
    let value = match render::color_at(call, 0)? {
        Some(color) => vector_value(call, &color)?,
        None => Value::Nil,
    };
    call.push(value)
}

fn color_value(call: &mut HostCall<'_>, value: Value) -> Result<Option<[f32; 4]>, Error> {
    match value {
        Value::String(_) => {
            Ok(render::text(call, value).map(|v| crate::parse_color_text(v).unwrap_or([1.; 4])))
        }
        Value::Table(t) => call.with_roots(&[value], |call| {
            let mut color = [1.; 4];
            for (i, component) in color.iter_mut().enumerate() {
                let value = call.raw_table_get(t, Value::Number((i + 1) as f64))?;
                if i == 3 && value == Value::Nil {
                    continue;
                }
                let Value::Number(n) = call.call(helper(call, 6)?, &[value])?[0] else {
                    return Ok(None);
                };
                *component = n as f32;
            }
            Ok(Some(color))
        }),
        _ => Ok(None),
    }
}

fn colors(
    call: &mut HostCall<'_>,
    value: Value,
    permissive: bool,
) -> Result<Option<[[f32; 4]; 4]>, Error> {
    let Value::Table(t) = value else {
        return Ok(None);
    };
    call.with_roots(&[value], |call| {
        let mut colors = [[1.; 4]; 4];
        let mut saw = false;
        for (i, color) in colors.iter_mut().enumerate() {
            let value = call.raw_table_get(t, Value::Number((i + 1) as f64))?;
            let read = if permissive {
                color_value(call, value)?
            } else if matches!(value, Value::Table(_)) {
                vector(call, value)?
            } else {
                None
            };
            if let Some(read) = read {
                *color = read;
                saw = true;
            } else if !permissive {
                return Ok(None);
            }
        }
        Ok(saw.then_some(colors))
    })
}

fn first(call: &mut HostCall<'_>, params: TableId, keys: &[&str]) -> Result<Value, Error> {
    for key in keys {
        let value = field(call, params, key)?;
        if value != Value::Nil {
            return Ok(value);
        }
    }
    Ok(Value::Nil)
}
fn optional_table(call: &HostCall<'_>, value: Value) -> Result<Option<TableId>, Error> {
    if value == Value::Nil {
        Ok(None)
    } else {
        table(call, value).map(Some)
    }
}
fn raw_color(
    call: &mut HostCall<'_>,
    entry: TableId,
    key: &[u8],
    strict: bool,
) -> Result<Option<[f32; 4]>, Error> {
    let value = get(call, entry, key)?;
    if strict || matches!(value, Value::Table(_)) {
        vector(call, value)
    } else {
        Ok(None)
    }
}
fn strict_integer(
    call: &mut HostCall<'_>,
    entry: TableId,
    key: &[u8],
    default: i32,
) -> Result<i32, Error> {
    let value = get(call, entry, key)?;
    if value == Value::Nil {
        return Ok(default);
    }
    let n = sprites::number(call, value)?.trunc();
    if n >= i32::MIN as f64 && n <= i32::MAX as f64 {
        Ok(n as i32)
    } else {
        Err(call.error(b"integer out of range"))
    }
}

fn add_attribute(call: &mut HostCall<'_>, actor: TableId) -> Result<(), Error> {
    let Some(start) = integer_scalar(call, arg(call, 1)).map(|n| n.max(0)) else {
        return Ok(());
    };
    let Value::Table(params) = arg(call, 2) else {
        return Ok(());
    };
    let value = first(call, params, &["Length", "length"])?;
    let length = integer_scalar(call, value).unwrap_or(1);
    if length == 0 {
        return Ok(());
    }
    let value = first(call, params, &["Diffuses", "diffuses"])?;
    let mut vertex_colors = colors(call, value, true)?.unwrap_or([[1.; 4]; 4]);
    let value = first(
        call,
        params,
        &[
            "Diffuse",
            "diffuse",
            "DiffuseColor",
            "diffusecolor",
            "Color",
            "color",
        ],
    )?;
    if let Some(color) = color_value(call, value)? {
        vertex_colors = [color; 4];
    }
    let color = vertex_colors[0];
    let value = first(call, params, &["Glow", "glow"])?;
    let glow = color_value(call, value)?;
    let attr = call.try_new_table()?;
    call.with_roots(&[Value::Table(attr)], |call| {
        set(call, attr, "start", Value::Number(start as f64))?;
        set(call, attr, "length", Value::Number(length as f64))?;
        let value = vector_value(call, &color)?;
        set(call, attr, "color", value)?;
        if vertex_colors != [color; 4] {
            let rows = call.try_new_table()?;
            set(call, attr, "vertex_colors", Value::Table(rows))?;
            for (i, color) in vertex_colors.iter().enumerate() {
                let value = vector_value(call, color)?;
                call.raw_table_set(rows, Value::Number((i + 1) as f64), value)?;
            }
        }
        if let Some(glow) = glow {
            let value = vector_value(call, &glow)?;
            set(call, attr, "glow", value)?;
        }
        let value = field(call, actor, ATTRIBUTES)?;
        let attrs = match optional_table(call, value)? {
            Some(t) => t,
            None => {
                let t = call.try_new_table()?;
                call.with_roots(&[Value::Table(t)], |call| {
                    set(call, actor, ATTRIBUTES, Value::Table(t))
                })?;
                t
            }
        };
        call.with_roots(&[Value::Table(attrs)], |call| {
            let mut i = 1;
            loop {
                let value = call.raw_table_get(attrs, Value::Number(i as f64))?;
                if value == Value::Nil {
                    break;
                }
                let old = table(call, value)?;
                if strict_integer(call, old, b"start", -1)? == start
                    && strict_integer(call, old, b"length", 0)? == length
                {
                    let old_color = raw_color(call, old, b"color", true)?.unwrap_or([1.; 4]);
                    let value = get_static(call, old, b"vertex_colors")?;
                    optional_table(call, value)?;
                    let old_colors = colors(call, value, false)?.unwrap_or([old_color; 4]);
                    let old_glow = raw_color(call, old, b"glow", true)?;
                    if old_color == color && old_colors == vertex_colors && old_glow == glow {
                        return Ok(());
                    }
                }
                i += 1;
            }
            let end = call.raw_table_len(attrs)? + 1;
            call.raw_table_set(attrs, Value::Number(end as f64), Value::Table(attr))
        })
    })
}

pub(super) fn attributes(
    call: &mut HostCall<'_>,
    actor: TableId,
) -> Result<Arc<[TextAttribute]>, Error> {
    let mut out = Vec::new();
    visit_attributes(call, actor, |attribute| out.push(attribute))?;
    Ok(out.into())
}

pub(super) fn visit_attributes(
    call: &mut HostCall<'_>,
    actor: TableId,
    mut visit: impl FnMut(TextAttribute),
) -> Result<(), Error> {
    let value = field(call, actor, ATTRIBUTES)?;
    let Some(attrs) = optional_table(call, value)? else {
        return Ok(());
    };
    call.with_roots(&[value], |call| {
        let mut i = 1;
        loop {
            let value = call.raw_table_get(attrs, Value::Number(i as f64))?;
            if value == Value::Nil {
                break;
            }
            let entry = table(call, value)?;
            let value = get_static(call, entry, b"start")?;
            let start = integer_scalar(call, value).unwrap_or(0).max(0) as usize;
            let value = get_static(call, entry, b"length")?;
            let length = integer_scalar(call, value).unwrap_or(1);
            if length != 0 {
                let color = raw_color(call, entry, b"color", false)?.unwrap_or([1.; 4]);
                let value = get_static(call, entry, b"vertex_colors")?;
                let vertex_colors = colors(call, value, false)?;
                let glow = raw_color(call, entry, b"glow", false)?;
                visit(TextAttribute {
                    start,
                    length: if length < 0 {
                        usize::MAX
                    } else {
                        length as usize
                    },
                    color,
                    vertex_colors,
                    glow,
                });
            }
            i += 1;
        }
        Ok(())
    })
}

pub(super) fn stroke(call: &mut HostCall<'_>, actor: TableId) -> Result<Option<[f32; 4]>, Error> {
    let value = field(call, actor, "__songlua_stroke_color")?;
    if let Some(color) = vector(call, value)? {
        return Ok(Some(color));
    }
    let fallback = field(call, actor, "StrokeColor").and_then(|v| vector(call, v));
    match fallback {
        Err(e) if e.kind() == ErrorKind::Runtime => Ok(None),
        result => result,
    }
}
pub(super) fn font(
    call: &mut HostCall<'_>,
    actor: TableId,
) -> Result<Option<(&'static str, PathBuf)>, Error> {
    let mut font = None;
    if let Some(name) = assets::field(call, actor, b"Font")? {
        let resolved = match assets::resolve(call, actor, &name) {
            Err(e) if e.kind() == ErrorKind::Runtime => None,
            result => result?,
        };
        font = if let Some(path) = resolved {
            Some((crate::bitmap_font::song_lua_font_name(&path), path))
        } else if crate::bitmap_font::song_lua_uses_builtin_theme_font(&name) {
            Some(("miso", PathBuf::from("assets/fonts/miso/_miso light.ini")))
        } else {
            None
        };
    }
    Ok(font)
}
pub(super) fn read(call: &mut HostCall<'_>, actor: TableId) -> Result<BitmapText, Error> {
    let font = font(call, actor)?;
    let kind = assets::field(call, actor, b"__songlua_actor_type")?.unwrap_or_default();
    if let Some(text) = crate::input_status_actor_text(&kind) {
        return Ok(BitmapText {
            font,
            text: text.into(),
            stroke_color: None,
            attributes: Arc::from([]),
        });
    }
    Ok(BitmapText {
        font,
        text: content(call, actor)?.into(),
        stroke_color: stroke(call, actor)?,
        attributes: attributes(call, actor)?,
    })
}

fn flag(call: &mut HostCall<'_>, actor: TableId, key: &str, value: bool) -> Result<(), Error> {
    action_capture::prepare_write(call, actor)?;
    {
        let guard = call.store();
        let state = &mut guard.actors.get_mut(&actor).unwrap().state.render;
        match key {
            "rainbow_scroll" => state.rainbow_scroll = value,
            "text_jitter" => state.text_jitter = value,
            "uppercase" => state.uppercase = value,
            "mult_attrs_with_diffuse" => state.mult_attrs_with_diffuse = value,
            "max_w_pre_zoom" => state.max_w_pre_zoom = value,
            "max_h_pre_zoom" => state.max_h_pre_zoom = value,
            "max_dimension_uses_zoom" => state.max_dimension_uses_zoom = value,
            _ => unreachable!(),
        }
    }
    set(
        call,
        actor,
        &format!("__songlua_state_{key}"),
        Value::Boolean(value),
    )?;
    song_media::record(call, actor, key);
    Ok(())
}
pub(super) fn pre_zoom(
    call: &mut HostCall<'_>,
    actor: TableId,
    width: bool,
    height: bool,
) -> Result<(), Error> {
    if !super::text_displays::is_text(call.store().actors[&actor].state.class) {
        return Ok(());
    }
    for (enabled, saw, key) in [
        (width, "__songlua_text_saw_max_width", "max_w_pre_zoom"),
        (height, "__songlua_text_saw_max_height", "max_h_pre_zoom"),
    ] {
        if enabled && !matches!(field(call, actor, saw)?, Value::Nil | Value::Boolean(false)) {
            flag(call, actor, key, true)?;
        }
    }
    Ok(())
}

pub(super) fn method(call: &mut HostCall<'_>, actor: TableId, name: &str) -> Result<(), Error> {
    match name {
        "settext" => {
            let text = text_value(call, arg(call, 1))?;
            let value = call.try_intern(text.as_bytes())?;
            set(call, actor, "Text", value)?;
        }
        "GetText" => {
            let text = content(call, actor)?;
            let value = call.try_intern(text.as_bytes())?;
            return call.push(value);
        }
        "AddAttribute" => add_attribute(call, actor)?,
        "ClearAttributes" => {
            let t = call.try_new_table()?;
            call.with_roots(&[Value::Table(t)], |call| {
                set(call, actor, ATTRIBUTES, Value::Table(t))
            })?;
        }
        "strokecolor" => {
            if let Some(color) = render::color(call)? {
                let value = vector_value(call, &color)?;
                call.with_roots(&[value], |call| {
                    set(call, actor, "__songlua_stroke_color", value)
                })?;
            }
        }
        "getstrokecolor" => {
            let color = stroke(call, actor)?.unwrap_or([0.; 4]);
            let value = vector_value(call, &color)?;
            return call.push(value);
        }
        "rainbowscroll" | "jitter" | "uppercase" => {
            let Value::Boolean(value) = arg(call, 1) else {
                return Err(call.error(format!("{name}: boolean expected").as_bytes()));
            };
            flag(
                call,
                actor,
                match name {
                    "rainbowscroll" => "rainbow_scroll",
                    "jitter" => "text_jitter",
                    _ => name,
                },
                value,
            )?;
        }
        "set_mult_attrs_with_diffuse" => flag(
            call,
            actor,
            "mult_attrs_with_diffuse",
            !matches!(arg(call, 1), Value::Nil | Value::Boolean(false)),
        )?,
        "get_mult_attrs_with_diffuse" => {
            let value = field(call, actor, "__songlua_state_mult_attrs_with_diffuse")?;
            return call.push(Value::Boolean(!matches!(
                value,
                Value::Nil | Value::Boolean(false)
            )));
        }
        "max_dimension_use_zoom" => flag(
            call,
            actor,
            "max_dimension_uses_zoom",
            boolish(call, arg(call, 1)).unwrap_or(true),
        )?,
        "distort" | "undistort" => {
            let n = if name == "undistort" {
                0.
            } else {
                float_scalar(call, arg(call, 1), false)
                    .unwrap_or(0.)
                    .max(0.)
            };
            action_capture::prepare_write(call, actor)?;
            call.store()
                .actors
                .get_mut(&actor)
                .unwrap()
                .state
                .render
                .text_distortion = n;
            set(
                call,
                actor,
                "__songlua_state_text_distortion",
                Value::Number(n as f64),
            )?;
            song_media::record(call, actor, "text_distortion");
        }
        "wrapwidthpixels" | "vertspacing" => {
            if let Some(n) = float_scalar(call, arg(call, 1), false) {
                let n = if name == "vertspacing" {
                    n.round() as i32
                } else {
                    n as i32
                };
                if name == "vertspacing" || n >= 0 {
                    action_capture::prepare_write(call, actor)?;
                    let key = if name == "vertspacing" {
                        "vert_spacing"
                    } else {
                        "wrap_width_pixels"
                    };
                    {
                        let guard = call.store();
                        let s = &mut guard.actors.get_mut(&actor).unwrap().state.render;
                        if name == "vertspacing" {
                            s.vert_spacing = Some(n)
                        } else {
                            s.wrap_width_pixels = Some(n)
                        }
                    }
                    set(
                        call,
                        actor,
                        &format!("__songlua_state_{key}"),
                        Value::Number(n as f64),
                    )?;
                    song_media::record(call, actor, key);
                }
            }
        }
        "maxwidth" | "maxheight" => {
            if let Some(n) = float_scalar(call, arg(call, 1), false) {
                let width = name == "maxwidth";
                action_capture::prepare_write(call, actor)?;
                {
                    let guard = call.store();
                    let s = &mut guard.actors.get_mut(&actor).unwrap().state.render;
                    if width {
                        s.max_width = Some(n)
                    } else {
                        s.max_height = Some(n)
                    }
                }
                set(
                    call,
                    actor,
                    if width {
                        "__songlua_state_max_width"
                    } else {
                        "__songlua_state_max_height"
                    },
                    Value::Number(n as f64),
                )?;
                song_media::record(call, actor, if width { "max_width" } else { "max_height" });
                flag(
                    call,
                    actor,
                    if width {
                        "max_w_pre_zoom"
                    } else {
                        "max_h_pre_zoom"
                    },
                    false,
                )?;
                set(
                    call,
                    actor,
                    if width {
                        "__songlua_text_saw_max_width"
                    } else {
                        "__songlua_text_saw_max_height"
                    },
                    Value::Boolean(true),
                )?;
            }
        }
        "textglowmode" => {
            if let Some(mode) =
                render::text(call, arg(call, 1)).and_then(crate::parse_overlay_text_glow_mode)
            {
                action_capture::prepare_write(call, actor)?;
                call.store()
                    .actors
                    .get_mut(&actor)
                    .unwrap()
                    .state
                    .render
                    .text_glow_mode = mode;
                let value = call.try_intern(crate::text_glow_mode_label(mode).as_bytes())?;
                set(call, actor, "__songlua_state_text_glow_mode", value)?;
                song_media::record(call, actor, "text_glow_mode");
            }
        }
        _ => unreachable!(),
    }
    call.push(Value::Table(actor))
}

// LoadFont creates a live, reusable template, including the current directory;
// declaration concatenation clones its native state without running Init.
pub(super) fn load_font(call: &mut HostCall<'_>) -> Result<(), Error> {
    let font = render::text(call, arg(call, 0))
        .unwrap_or_default()
        .to_owned();
    if call.store().offline_definitions {
        let actor = super::offline::dummy(call, "BitmapText")?;
        let font = call.try_intern(font.as_bytes())?;
        set(call, actor, "Font", font)?;
        return call.push(Value::Table(actor));
    }
    let declaration = call.try_new_table()?;
    call.with_roots(&[Value::Table(declaration)], |call| {
        let class = call.try_intern(b"BitmapText")?;
        set(call, declaration, "Class", class)?;
        let globals = table(call, helper(call, 5)?)?;
        for key in ["__songlua_script_dir", "__songlua_song_dir"] {
            if let Some(directory) = assets::field(call, globals, key.as_bytes())? {
                let value = call.try_intern(directory.as_bytes())?;
                set(call, declaration, key, value)?;
            }
        }
        let id = super::materialize(call, declaration, super::Materialization::Template)?;
        let actor = call.store().scenes[&id].root.expect("font template");
        let key = call.try_intern(b"Class")?;
        call.raw_table_set(actor, key, Value::Nil)?;
        let value = call.try_intern(font.as_bytes())?;
        set(call, actor, "Font", value)?;
        call.push(Value::Table(actor))
    })
}

// Before replay is installed, LoadFont template setters publish a preparation
// capture block. Production relative position setters consult that block even
// after the final instance owns replay, until playback starts. Preserve this
// existing boundary (including authored getters) rather than replacing it with
// the newer destination unconditionally.
pub(super) fn captured_position(
    call: &mut HostCall<'_>,
    actor: TableId,
    axis: &str,
) -> Result<Option<f32>, Error> {
    let value = field(call, actor, "__songlua_capture_block")?;
    let Some(block) = optional_table(call, value)? else {
        return Ok(None);
    };
    call.with_roots(&[value], |call| {
        let value = field(call, block, axis)?;
        if value == Value::Nil {
            Ok(None)
        } else {
            sprites::number(call, value).map(|n| Some(n as f32))
        }
    })
}
pub(super) fn capture_template_position(
    call: &mut HostCall<'_>,
    actor: TableId,
    axis: &str,
    value: f32,
) -> Result<(), Error> {
    let block = field(call, actor, "__songlua_capture_block")?;
    let block = match optional_table(call, block)? {
        Some(t) => t,
        None => {
            let t = call.try_new_table()?;
            call.with_roots(&[Value::Table(t)], |call| {
                set(call, actor, "__songlua_capture_block", Value::Table(t))
            })?;
            t
        }
    };
    call.with_roots(&[Value::Table(block)], |call| {
        set(call, block, axis, Value::Number(value as f64))
    })
}
