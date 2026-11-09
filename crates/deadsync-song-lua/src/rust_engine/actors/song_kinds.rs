//! Existing compiler renderer kinds. Command probing and tree policy are separate.
use super::*;
use crate::{
    SongLuaCompileContext, SongLuaOverlayKind as Kind, SongLuaOverlayState, SongLuaProxyTarget,
};
use deadlib_present::actors::TextAttribute;
use std::path::PathBuf;

type PlainKind = Kind<(), (), TextAttribute>;
pub(in crate::rust_engine) enum Output {
    Ready(Option<Box<PlainKind>>),
    Model(Option<crate::lua_util::SongLuaNoteskinTapActorModel>),
}
impl Output {
    fn ready(kind: Option<PlainKind>) -> Self {
        Self::Ready(kind.map(Box::new))
    }
}
#[derive(Clone)]
struct Read {
    store: Shared,
    class: String,
    initial: SongLuaOverlayState,
    context: SongLuaCompileContext,
    aft_names: HashSet<String>,
    output: Arc<Mutex<Option<Output>>>,
}
impl Actors {
    pub(in crate::rust_engine) fn read_song_overlay_kind(
        &self,
        vm: &mut Vm,
        actor: Value,
        inputs: &super::super::SongOverlayKindInputs<'_>,
    ) -> Result<Output, Error> {
        let output = Arc::new(Mutex::new(None));
        let function = vm.new_host_function_with_data(
            read,
            &[
                Value::Table(self.registry),
                Value::Table(self.helpers),
                actor,
            ],
            Read {
                store: self.store.clone(),
                class: inputs.actor_type.to_owned(),
                initial: inputs.initial_state,
                context: inputs.context.clone(),
                aft_names: inputs.aft_capture_names.clone(),
                output: output.clone(),
            },
        )?;
        vm.call(function, &[])?;
        Ok(output.lock().unwrap().take().unwrap())
    }
}
fn resolve(call: &mut HostCall<'_>, actor: TableId, path: &str) -> Result<Option<PathBuf>, Error> {
    match assets::resolve(call, actor, path) {
        Err(error) if error.kind() == ErrorKind::Runtime => Ok(None),
        result => result,
    }
}
fn read(call: &mut HostCall<'_>) -> Result<(), Error> {
    let data = call.function_data::<Read>().unwrap().clone();
    let actor = table(call, call.captures()[2])?;
    let result = kind(call, &data, actor)?;
    *data.output.lock().unwrap() = Some(result);
    Ok(())
}
fn kind(call: &mut HostCall<'_>, data: &Read, actor: TableId) -> Result<Output, Error> {
    let class = data.class.as_str();
    let kind = if class.eq_ignore_ascii_case("Actor") {
        Kind::Actor
    } else if class.eq_ignore_ascii_case("WrapperState") {
        Kind::WrapperState
    } else if class.eq_ignore_ascii_case("ActorFrame")
        || class.eq_ignore_ascii_case("CourseContentsList")
    {
        Kind::ActorFrame
    } else if class.eq_ignore_ascii_case("ActorFrameTexture") {
        let frame = framebuffers::read(call, actor)?;
        Kind::ActorFrameTexture {
            capture_name: frame.capture_name,
            capture_size: frame.capture_size,
            alpha_buffer: frame.alpha_buffer,
            depth_buffer: frame.depth_buffer,
            float_buffer: frame.float_buffer,
        }
    } else if class.eq_ignore_ascii_case("ActorProxy") {
        Kind::ActorProxy {
            target: proxies::read(call, actor)?.unwrap_or(SongLuaProxyTarget::Actor {
                overlay_index: usize::MAX,
            }),
        }
    } else if class.eq_ignore_ascii_case("Sprite") || class.eq_ignore_ascii_case("Banner") {
        if let Some(capture_name) = assets::field(call, actor, b"__songlua_aft_capture_name")? {
            Kind::AftSprite { capture_name }
        } else {
            let binding = get(call, actor, b"__songlua_startup_sprite")?;
            let binding = if binding == Value::Nil {
                actor
            } else {
                table(call, binding)?
            };
            let kind = call.with_roots(&[Value::Table(binding)], |call| {
                let Some(texture) = assets::field(call, binding, b"Texture")? else {
                    return Ok(Some(Kind::Actor));
                };
                let capture_name = textures::collapse(&texture);
                if data.aft_names.contains(&capture_name) {
                    return Ok(Some(Kind::AftSprite { capture_name }));
                }
                let Some(texture_path) = resolve(call, actor, &texture)? else {
                    return Ok(None);
                };
                let texture_key = crate::lua_util::texture_path_key(&texture_path);
                Ok(Some(Kind::Sprite {
                    texture_path,
                    texture_key,
                    states: sprites::states(call, binding)?.into(),
                    textures: Arc::from([]),
                }))
            })?;
            return Ok(Output::ready(kind));
        }
    } else if class.eq_ignore_ascii_case("Sound") {
        let Some(file) = assets::field(call, actor, b"File")? else {
            return Ok(Output::ready(None));
        };
        let Some(sound_path) = resolve(call, actor, &file)? else {
            return Ok(Output::ready(None));
        };
        Kind::Sound { sound_path }
    } else if ["BitmapText", "RollingNumbers", "DeviceList", "InputList"]
        .iter()
        .any(|name| class.eq_ignore_ascii_case(name))
    {
        let Some((font_name, font_path)) = text::font(call, actor)? else {
            return Ok(Output::ready(None));
        };
        if class.eq_ignore_ascii_case("DeviceList") || class.eq_ignore_ascii_case("InputList") {
            Kind::BitmapText {
                font_name,
                font_path,
                text: crate::input_status_actor_text(class)
                    .unwrap_or_default()
                    .into(),
                stroke_color: None,
                attributes: Arc::from([]),
                text_changes: Arc::from([]),
                text_changes_in_seconds: false,
            }
        } else {
            let startup = get(call, actor, b"__songlua_startup_text")?;
            let startup = initial_state::value_text(call, startup)?;
            // Native Option::or evaluates the authored Text lookup even when
            // the earlier startup value is present.
            let authored = assets::field(call, actor, b"Text")?;
            Kind::BitmapText {
                font_name,
                font_path,
                text: startup.or(authored).unwrap_or_default().into(),
                stroke_color: text::stroke(call, actor)?,
                attributes: text::attributes(call, actor)?,
                text_changes: Arc::from([]),
                text_changes_in_seconds: false,
            }
        }
    } else if class.eq_ignore_ascii_case("ActorMultiVertex") {
        let vertices = match meshes::vertices(call, actor)? {
            Some(vertices) => vertices,
            None if data.context.live => Arc::from([]),
            None => return Ok(Output::ready(None)),
        };
        let texture_path = if data.context.live {
            meshes::texture_path(call, actor)?
        } else if assets::field(call, actor, b"__songlua_aft_capture_name")?.is_some() {
            None
        } else if let Some(texture) = assets::field(call, actor, b"Texture")? {
            if data.aft_names.contains(&texture) {
                None
            } else {
                resolve(call, actor, &texture)?
            }
        } else {
            None
        };
        let texture_key = texture_path
            .as_ref()
            .map(|path| crate::lua_util::texture_path_key(path));
        let capture_name = framebuffers::capture_name(call, actor)?;
        let texture =
            assets::field(call, actor, b"Texture")?.filter(|name| data.aft_names.contains(name));
        Kind::ActorMultiVertex {
            vertices,
            texture_path,
            texture_key,
            capture_name: capture_name.or(texture),
        }
    } else if class.eq_ignore_ascii_case("Model") {
        return Ok(Output::Model(models::tap_model(call, actor)?));
    } else if class.eq_ignore_ascii_case("SongMeterDisplay") {
        let Some(meter) = song_meters::read(call, &data.store, actor)? else {
            return Ok(Output::ready(None));
        };
        Kind::SongMeterDisplay {
            stream_width: meter.stream_width,
            stream_state: meter.stream_state,
            music_length_seconds: data.context.music_length_seconds.max(0.),
        }
    } else if class.eq_ignore_ascii_case("GraphDisplay") {
        let graph = graphs::read(call, &data.store, actor)?;
        Kind::GraphDisplay {
            size: crate::lua_util::read_graph_display_size(data.initial, &data.context),
            body_values: graph.body_values,
            body_state: graph.body_state,
            line_state: Box::new(graph.line_state),
        }
    } else if class.eq_ignore_ascii_case("Quad") {
        Kind::Quad
    } else {
        return Ok(Output::ready(None));
    };
    Ok(Output::ready(Some(kind)))
}

pub(in crate::rust_engine) fn typed<N, M>(kind: PlainKind) -> Kind<N, M, TextAttribute> {
    match kind {
        Kind::Actor => Kind::Actor,
        Kind::ActorFrame => Kind::ActorFrame,
        Kind::WrapperState => Kind::WrapperState,
        Kind::UpdateTracks {
            tracks,
            draw_frames,
        } => Kind::UpdateTracks {
            tracks,
            draw_frames,
        },
        Kind::ActorFrameTexture {
            capture_name,
            capture_size,
            alpha_buffer,
            depth_buffer,
            float_buffer,
        } => Kind::ActorFrameTexture {
            capture_name,
            capture_size,
            alpha_buffer,
            depth_buffer,
            float_buffer,
        },
        Kind::ActorProxy { target } => Kind::ActorProxy { target },
        Kind::AftSprite { capture_name } => Kind::AftSprite { capture_name },
        Kind::Sprite {
            texture_path,
            texture_key,
            states,
            textures,
        } => Kind::Sprite {
            texture_path,
            texture_key,
            states,
            textures,
        },
        Kind::Sound { sound_path } => Kind::Sound { sound_path },
        Kind::BitmapText {
            font_name,
            font_path,
            text,
            text_changes,
            text_changes_in_seconds,
            stroke_color,
            attributes,
        } => Kind::BitmapText {
            font_name,
            font_path,
            text,
            text_changes,
            text_changes_in_seconds,
            stroke_color,
            attributes,
        },
        Kind::ActorMultiVertex {
            vertices,
            texture_path,
            texture_key,
            capture_name,
        } => Kind::ActorMultiVertex {
            vertices,
            texture_path,
            texture_key,
            capture_name,
        },
        Kind::SongMeterDisplay {
            stream_width,
            stream_state,
            music_length_seconds,
        } => Kind::SongMeterDisplay {
            stream_width,
            stream_state,
            music_length_seconds,
        },
        Kind::GraphDisplay {
            size,
            body_values,
            body_state,
            line_state,
        } => Kind::GraphDisplay {
            size,
            body_values,
            body_state,
            line_state,
        },
        Kind::Quad => Kind::Quad,
        Kind::Model { .. } | Kind::NoteskinActor { .. } => {
            unreachable!("model resources are loaded by the runtime host")
        }
    }
}
