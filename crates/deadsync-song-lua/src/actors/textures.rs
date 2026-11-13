//! Retained texture metadata and owner-directed movie parameters.
use super::*;
use sprites::{field, number};
use std::path::Path;

pub(super) struct Info {
    pub(super) path: String,
    pub(super) source: [f32; 2],
    pub(super) image: [f32; 2],
    pub(super) allocation: [f32; 2],
    pub(super) frames: u32,
}
#[derive(Clone)]
struct Binding {
    info: Arc<Info>,
    name: &'static str,
}

pub(super) fn install_getter(call: &mut HostCall<'_>, actor: TableId) -> Result<(), Error> {
    let methods = super::get(call, actor, b"__songlua_bound_methods")?;
    let methods = table(call, methods)?;
    let captures = [call.captures()[0], call.captures()[1], Value::Table(actor)];
    // Native instances own this getter even when their public class hides it.
    // Internal screen kinds expose it through the existing bound-method fallback.
    let getter = call.new_host_function(bound_getter, &captures)?;
    let key = call.try_intern(b"GetTexture")?;
    call.raw_table_set(methods, key, getter)
}

fn bound_getter(call: &mut HostCall<'_>) -> Result<(), Error> {
    // The callback requires a table receiver but reads its captured owner.
    table(call, arg(call, 0))?;
    let actor = table(call, call.captures()[2])?;
    if !call.store().actors.contains_key(&actor) {
        return Err(call.error(b"actor instance is not owned by this runtime or has been released"));
    }
    if assets::field(call, actor, b"__songlua_actor_type")?
        .is_some_and(|kind| kind.eq_ignore_ascii_case("ActorFrameTexture"))
    {
        framebuffers::method(call, actor, "GetTexture")
    } else {
        get(call, actor)
    }
}

// RageTextureID collapses only slash-terminated components, retaining leading
// ./ and trailing .. just like the existing song resource adapter.
pub(super) fn collapse(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for part in name.split_inclusive('/') {
        if (part == "/" || part == "./") && !out.is_empty() {
            continue;
        }
        if part == "../" && !out.is_empty() && out != "/" {
            let start = out[..out.len() - 1].rfind('/').map_or(0, |i| i + 1);
            if &out[start..] != "../" {
                out.truncate(start);
                continue;
            }
        }
        out.push_str(part);
    }
    out
}
pub(super) fn get(call: &mut HostCall<'_>, actor: TableId) -> Result<(), Error> {
    let key = call.try_intern(b"__songlua_amv_texture_handle")?;
    let retained = call.raw_table_get(actor, key)?;
    if retained != Value::Nil {
        table(call, retained)?;
        return call.push(retained);
    }
    let default = sprites::default_texture(call, actor)?;
    if !default && field(call, actor, "Texture")? == Value::Nil {
        return call.push(Value::Nil);
    }
    // Each ordinary GetTexture creates a fresh metadata handle. Retaining one
    // must not make its getters observe later loads or authored field changes.
    let texture = call.try_new_table()?;
    call.with_roots(&[Value::Table(texture)], |call| {
        let kind = call.try_intern(b"RageTexture")?;
        sprites::set(call, texture, "__songlua_native_type", kind)?;
        let (info, sheet) = if default {
            sprites::set(
                call,
                texture,
                "__songlua_default_texture",
                Value::Boolean(true),
            )?;
            (
                Info {
                    path: String::new(),
                    source: [1.0; 2],
                    image: [1.0; 2],
                    allocation: [1.0; 2],
                    frames: 1,
                },
                (1, 1),
            )
        } else {
            let frames = crate::sprite_frame_count(sprites::sheet(call, actor)?);
            let raw = assets::field(call, actor, b"Texture")?.unwrap_or_default();
            let resolved = assets::texture_path(call, actor)?;
            let path = resolved
                .as_deref()
                .map(crate::file_path_string)
                .unwrap_or_else(|| raw.clone());
            let source_path = resolved.as_deref().unwrap_or_else(|| Path::new(&raw));
            let source = if let Some(size) = sprites::image_size(source_path, true) {
                size
            } else if crate::is_song_lua_video_path(source_path) {
                if let Some(size) = sprites::movie_size(call, actor)? {
                    size
                } else {
                    let globals = table(call, helper(call, 5)?)?;
                    let mut size = [640.0, 480.0];
                    for (i, name) in ["SCREEN_WIDTH", "SCREEN_HEIGHT"].into_iter().enumerate() {
                        let value = field(call, globals, name)?;
                        if value != Value::Nil {
                            let value = number(call, value)?;
                            if !value.is_finite()
                                || value < i32::MIN as f64
                                || value >= i32::MAX as f64 + 1.0
                            {
                                return Err(call.error(b"screen dimension out of range"));
                            }
                            size[i] = (value as i32) as f32;
                        }
                    }
                    size
                }
            } else {
                sprites::refresh_size(call, actor)?;
                call.store().actors[&actor].state.size.map(f32::abs)
            };
            let upload = sprites::actor_upload_size(call, actor)?;
            let (image, allocation) = upload.map_or((source, source), |image| {
                let allocation = image.map(|n| {
                    (n as u32)
                        .checked_next_power_of_two()
                        .unwrap_or(u32::MAX)
                        .max(8) as f32
                });
                (image, allocation)
            });
            let sheet = sprites::sheet(call, actor)?.unwrap_or((1, 1));
            (
                Info {
                    path: collapse(&path),
                    source,
                    image,
                    allocation,
                    frames,
                },
                sheet,
            )
        };
        install(call, actor, texture, info, sheet)?;
        call.push(Value::Table(texture))
    })
}
pub(super) fn install(
    call: &mut HostCall<'_>,
    actor: TableId,
    texture: TableId,
    info: Info,
    sheet: (u32, u32),
) -> Result<(), Error> {
    let path = call.try_intern(info.path.as_bytes())?;
    sprites::set(call, texture, "__songlua_texture_path", path)?;
    let frame = [
        (info.source[0] / sheet.0.max(1) as f32).floor(),
        (info.source[1] / sheet.1.max(1) as f32).floor(),
    ];
    let frame = sprites::vector_value(call, &frame)?;
    sprites::set(call, texture, "__songlua_source_frame_size", frame)?;
    let info = Arc::new(info);
    let captures = [
        call.captures()[0],
        call.captures()[1],
        Value::Table(actor),
        Value::Table(texture),
    ];
    for name in [
        "GetPath",
        "GetSourceWidth",
        "GetSourceHeight",
        "GetTextureWidth",
        "GetTextureHeight",
        "GetImageWidth",
        "GetImageHeight",
        "GetNumFrames",
        "loop",
        "rate",
    ] {
        let function = call.new_host_function_with_data(
            callback,
            &captures,
            Binding {
                info: info.clone(),
                name,
            },
        )?;
        sprites::set(call, texture, name, function)?;
    }
    Ok(())
}
fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
    let Binding { info, name } = call.function_data::<Binding>().unwrap().clone();
    let value = match name {
        "GetPath" => {
            return call
                .try_intern(info.path.as_bytes())
                .and_then(|value| call.push(value));
        }
        "GetSourceWidth" => info.source[0],
        "GetSourceHeight" => info.source[1],
        "GetTextureWidth" => info.allocation[0],
        "GetTextureHeight" => info.allocation[1],
        "GetImageWidth" => info.image[0],
        "GetImageHeight" => info.image[1],
        "GetNumFrames" => return call.push(Value::Number(info.frames as f64)),
        "loop" | "rate" => {
            let actor = table(call, call.captures()[2])?;
            let offset = usize::from(matches!(arg(call, 0), Value::Table(_)));
            sprites::texture_parameter(call, actor, name, arg(call, offset))?;
            return call.push(call.captures()[3]);
        }
        _ => unreachable!(),
    };
    call.push(Value::Number(value as f64))
}
