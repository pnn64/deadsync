//! Publish live content in native field order, retaining unchanged immutable arrays.
use super::*;
use crate::{SongLuaOverlayKind as Kind, SongLuaOverlayMeshVertex, SongLuaSpriteState};
use deadlib_present::actors::TextAttribute;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

enum Content {
    Mesh {
        vertices: Arc<[SongLuaOverlayMeshVertex]>,
        path: Option<PathBuf>,
        key: Option<Arc<str>>,
    },
    Text {
        text: Arc<str>,
        attributes: Arc<[TextAttribute]>,
        stroke: Option<[f32; 4]>,
    },
    Sprite {
        path: PathBuf,
        key: Arc<str>,
        states: Arc<[SongLuaSpriteState]>,
    },
}

struct Publication {
    content: Mutex<Option<Content>>,
    available: AtomicBool,
}

impl Actors {
    #[cfg(feature = "test-support")]
    pub(crate) fn visual_path(&self, vm: &mut Vm, actor: Value) -> Result<String, Error> {
        let function = vm.new_host_function(path_callback, &[actor])?;
        let value = vm.call(function, &[])?[0];
        let Value::String(id) = value else {
            unreachable!()
        };
        Ok(String::from_utf8(vm.string_bytes(id).unwrap().to_vec()).unwrap())
    }

    pub(crate) fn publish_visual_content<N, M>(
        &self,
        vm: &mut Vm,
        actor: Value,
        kind: &mut Kind<N, M, TextAttribute>,
    ) -> Result<(), String> {
        let content = match kind {
            Kind::ActorMultiVertex {
                vertices,
                texture_path,
                texture_key,
                ..
            } => Content::Mesh {
                vertices: vertices.clone(),
                path: texture_path.clone(),
                key: texture_key.clone(),
            },
            Kind::BitmapText {
                text,
                attributes,
                stroke_color,
                ..
            } => Content::Text {
                text: text.clone(),
                attributes: attributes.clone(),
                stroke: *stroke_color,
            },
            Kind::Sprite {
                texture_path,
                texture_key,
                states,
                ..
            } => Content::Sprite {
                path: texture_path.clone(),
                key: texture_key.clone(),
                states: states.clone(),
            },
            _ => return Ok(()),
        };
        // Each read owns its request and partial result. No store or output mutex
        // is held across authored getters, GC or nested actor calls.
        let output = Arc::new(Publication {
            content: Mutex::new(Some(content)),
            available: AtomicBool::new(true),
        });
        let function = vm
            .new_host_function_with_data(
                publish,
                &[
                    Value::Table(self.registry),
                    Value::Table(self.helpers),
                    actor,
                ],
                output.clone(),
            )
            .map_err(|e| e.to_string())?;
        let result = vm.call(function, &[]);
        let content = output
            .content
            .lock()
            .unwrap()
            .take()
            .expect("live content result");
        match (content, kind) {
            (
                Content::Mesh {
                    vertices: v,
                    path,
                    key,
                },
                Kind::ActorMultiVertex {
                    vertices,
                    texture_path,
                    texture_key,
                    ..
                },
            ) => {
                *vertices = v;
                *texture_path = path;
                *texture_key = key;
            }
            (
                Content::Text {
                    text: t,
                    attributes: a,
                    stroke,
                },
                Kind::BitmapText {
                    text,
                    attributes,
                    stroke_color,
                    ..
                },
            ) => {
                *text = t;
                *attributes = a;
                *stroke_color = stroke;
            }
            (
                Content::Sprite {
                    path,
                    key,
                    states: s,
                },
                Kind::Sprite {
                    texture_path,
                    texture_key,
                    states,
                    ..
                },
            ) => {
                *texture_path = path;
                *texture_key = key;
                *states = s;
            }
            _ => unreachable!(),
        }
        // Native publication keeps earlier fields when a later read fails.
        result.map(|_| ()).map_err(|e| e.to_string())
    }
}

fn publish(call: &mut HostCall<'_>) -> Result<(), Error> {
    let output = call.function_data::<Arc<Publication>>().unwrap().clone();
    // Debug inspection can expose a host caller to Lua. A saved or recursive
    // call must not consume another request or enter with a missing payload.
    if !output.available.swap(false, Ordering::Relaxed) {
        return Err(call.error(b"live content publication is no longer active"));
    }
    let mut content = output
        .content
        .lock()
        .unwrap()
        .take()
        .expect("live content request");
    let result = (|| {
        let actor = table(call, call.captures()[2])?;
        match &mut content {
            Content::Mesh {
                vertices,
                path,
                key,
            } => {
                *vertices = meshes::vertices(call, actor)?.unwrap_or_else(|| Arc::from([]));
                let next = meshes::texture_path(call, actor)?;
                if *path != next {
                    *key = next
                        .as_ref()
                        .map(|p| Arc::from(p.to_string_lossy().as_ref()));
                    *path = next;
                }
            }
            Content::Text {
                text: current,
                attributes,
                stroke,
            } => {
                sync_text(call, actor, current)?;
                sync_attributes(call, actor, attributes)?;
                let value = sprites::field(call, actor, "__songlua_stroke_color")?;
                *stroke = sprites::vector(call, value)?;
            }
            Content::Sprite { path, key, states } => {
                if let Some(next) = assets::texture_path(call, actor)?
                    && *path != next
                {
                    *key = Arc::from(next.to_string_lossy().as_ref());
                    *path = next;
                }
                sync_array(
                    states,
                    |visit| sprites::visit_states(call, actor, visit),
                    |a, b| a.frame == b.frame && a.delay.to_bits() == b.delay.to_bits(),
                )?;
            }
        }
        Ok(())
    })();
    *output.content.lock().unwrap() = Some(content);
    result
}

pub(super) fn sync_text(
    call: &mut HostCall<'_>,
    actor: TableId,
    current: &mut Arc<str>,
) -> Result<(), Error> {
    let next = text::content(call, actor)?;
    if current.as_ref() != next {
        *current = next.into();
    }
    Ok(())
}
pub(super) fn sync_attributes(
    call: &mut HostCall<'_>,
    actor: TableId,
    current: &mut Arc<[TextAttribute]>,
) -> Result<(), Error> {
    sync_array(
        current,
        |visit| text::visit_attributes(call, actor, visit),
        same_attribute,
    )
}

fn sync_array<T: Clone>(
    current: &mut Arc<[T]>,
    read: impl FnOnce(&mut dyn FnMut(T)) -> Result<(), Error>,
    same: impl Fn(&T, &T) -> bool,
) -> Result<(), Error> {
    let mut replacement: Option<Vec<T>> = None;
    let mut index = 0;
    read(&mut |value| {
        if let Some(values) = &mut replacement {
            values.push(value);
        } else if current.get(index).is_none_or(|old| !same(old, &value)) {
            let mut values = Vec::with_capacity(current.len().max(index + 1));
            values.extend_from_slice(&current[..index]);
            values.push(value);
            replacement = Some(values);
        }
        index += 1;
    })?;
    if let Some(values) = replacement {
        *current = values.into();
    } else if index != current.len() {
        *current = Arc::from(&current[..index]);
    }
    Ok(())
}

fn same_attribute(a: &TextAttribute, b: &TextAttribute) -> bool {
    a.start == b.start
        && a.length == b.length
        && a.color.map(f32::to_bits) == b.color.map(f32::to_bits)
        && a.vertex_colors.map(|c| c.map(|c| c.map(f32::to_bits)))
            == b.vertex_colors.map(|c| c.map(|c| c.map(f32::to_bits)))
        && a.glow.map(|c| c.map(f32::to_bits)) == b.glow.map(|c| c.map(f32::to_bits))
}

#[cfg(feature = "test-support")]
fn path_callback(call: &mut HostCall<'_>) -> Result<(), Error> {
    let actor = table(call, call.captures()[0])?;
    let path = actor_path(call, actor)?;
    let value = call.try_intern(path.as_bytes())?;
    call.push(value)
}

#[cfg(feature = "test-support")]
pub(super) fn actor_path(call: &mut HostCall<'_>, actor: TableId) -> Result<String, Error> {
    fn string(
        call: &mut HostCall<'_>,
        actor: TableId,
        key: &[u8],
    ) -> Result<Option<String>, Error> {
        let value = get(call, actor, key)?;
        if value == Value::Nil {
            return Ok(None);
        }
        let id = call
            .coerce_string(value)?
            .ok_or_else(|| call.error(b"string expected"))?;
        let bytes = call.string_bytes(id).unwrap();
        String::from_utf8(bytes.to_vec())
            .map(Some)
            .map_err(|e| call.error(e.to_string().as_bytes()))
    }
    let mut current = actor;
    let mut parts = Vec::new();
    let mut seen = HashSet::new();
    loop {
        if !seen.insert(current) {
            return Err(call.error(b"cycle in subscriber ownership"));
        }
        if let Some(path) = string(call, current, b"__songlua_message_path")? {
            parts.push(path);
            break;
        }
        if string(call, current, b"__songlua_actor_type")?.as_deref() == Some("TopScreen") {
            parts.push("ScreenGameplay".into());
            break;
        }
        let parent = get_static(call, current, b"__songlua_parent")?;
        if parent == Value::Nil {
            return Err(call.error(b"subscriber has no captured actor path"));
        }
        let parent = table(call, parent)?;
        let mut component = None;
        let index_key = call.try_intern(b"__songlua_message_authored_index")?;
        let authored = call.raw_table_get(current, index_key)?;
        if authored != Value::Nil {
            let Value::Number(index) = authored else {
                return Err(call.error(b"invalid native subscriber child index"));
            };
            if !index.is_finite() || index < 1. || index.fract() != 0. || index > usize::MAX as f64
            {
                return Err(call.error(b"invalid native subscriber child index"));
            }
            component = Some((index as usize).to_string());
        }
        let wrappers = get_static(call, parent, b"__songlua_wrappers")?;
        if component.is_none() && wrappers != Value::Nil {
            let wrappers = table(call, wrappers)?;
            for index in 1usize.. {
                let wrapper = call.raw_table_get(wrappers, Value::Number(index as f64))?;
                if wrapper == Value::Nil {
                    break;
                }
                if table(call, wrapper)? == current {
                    component = Some(format!("WrapperState{index}"));
                    break;
                }
            }
        }
        if component.is_none() {
            for index in 1usize.. {
                let child = call.raw_table_get(parent, Value::Number(index as f64))?;
                if child == Value::Nil {
                    break;
                }
                if child == Value::Table(current) {
                    component = Some(index.to_string());
                    break;
                }
            }
        }
        let name = component.or(string(call, current, b"Name")?);
        let Some(name) = name.filter(|name| !name.is_empty()) else {
            return Err(call.error(b"subscriber has no captured child identity"));
        };
        parts.push(name);
        current = parent;
    }
    parts.reverse();
    Ok(parts.join("/"))
}

#[cfg(feature = "test-support")]
impl Actors {
    pub(crate) fn fixture_sync_sprite_states(
        &self,
        vm: &mut Vm,
        actor: Value,
        states: &mut Arc<[crate::SongLuaSpriteState]>,
    ) -> Result<(), Error> {
        let output = Arc::new(Mutex::new(Some(states.clone())));
        let callback = vm.new_host_function_with_data(
            |call| {
                let output = call
                    .function_data::<Arc<Mutex<Option<Arc<[crate::SongLuaSpriteState]>>>>>()
                    .unwrap()
                    .clone();
                let actor = table(call, call.captures()[2])?;
                let mut states = output.lock().unwrap().take().unwrap();
                let result = sync_array(
                    &mut states,
                    |visit| sprites::visit_states(call, actor, visit),
                    |a, b| a.frame == b.frame && a.delay.to_bits() == b.delay.to_bits(),
                );
                *output.lock().unwrap() = Some(states);
                result
            },
            &[
                Value::Table(self.registry),
                Value::Table(self.helpers),
                actor,
            ],
            output.clone(),
        )?;
        let result = vm.call(callback, &[]).map(|_| ());
        *states = output.lock().unwrap().take().unwrap();
        result
    }
}
