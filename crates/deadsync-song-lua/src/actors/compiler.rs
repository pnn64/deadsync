//! Lua observations at the existing song compiler's assembly boundaries.
use super::*;
use std::path::PathBuf;

#[derive(Clone, Copy)]
pub(crate) enum Query {
    State,
    LiveState,
    Layer(usize),
    MarkLayers,
    MarkScreen(usize),
    SoundPaths,
    Spin,
    Late(u8),
    Draw,
    End,
}
pub(crate) enum Kind {
    Sprite(PathBuf, Vec<crate::SongLuaSpriteState>),
    Aft(FrameTexture),
    AftSprite(String),
    Proxy(crate::SongLuaProxyTarget, Option<usize>),
}
#[derive(Default)]
pub(crate) struct Output {
    pub state: Option<crate::SongLuaOverlayState>,
    pub layer: usize,
    pub spin: bool,
    pub paths: Vec<PathBuf>,
    pub kind: Option<Kind>,
}
#[derive(Clone)]
struct Read {
    query: Query,
    output: Arc<Mutex<Option<Output>>>,
}
impl Actors {
    pub(crate) fn compiler_read(
        &self,
        vm: &mut Vm,
        actor: Value,
        query: Query,
    ) -> Result<Output, Error> {
        let output = Arc::new(Mutex::new(None));
        let read = vm.new_host_function_with_data(
            read,
            &[
                Value::Table(self.registry),
                Value::Table(self.helpers),
                actor,
            ],
            Read {
                query,
                output: output.clone(),
            },
        )?;
        vm.call(read, &[])?;
        Ok(output.lock().unwrap().take().unwrap())
    }
}
fn read(call: &mut HostCall<'_>) -> Result<(), Error> {
    let read = call.function_data::<Read>().unwrap().clone();
    let value = call.captures()[2];
    let mut out = Output::default();
    match read.query {
        Query::State => out.state = Some(initial_state::read(call, table(call, value)?)?),
        Query::LiveState => out.state = Some(initial_state::read_live(call, table(call, value)?)?),
        Query::Layer(default) => {
            let layer = super::super::field(call, value, "__songlua_compile_layer")?;
            out.layer = if layer == Value::Nil {
                default
            } else {
                let convert = call.new_host_function(
                    |call| {
                        let value = call.captures()[0];
                        let mut stack = call.native_stack()?;
                        stack.push(value)?;
                        let integer = stack
                            .to_integer(-1)
                            .ok_or_else(|| stack.error(b"integer expected"))?;
                        let integer = usize::try_from(integer)
                            .map_err(|_| stack.error(b"layer index out of range"))?;
                        stack.push(Value::Number(integer as f64))?;
                        stack.finish(1)
                    },
                    &[layer],
                )?;
                let Value::Number(layer) = call.call(convert, &[])?[0] else {
                    unreachable!()
                };
                layer as usize
            };
        }
        Query::MarkLayers => {
            if let Value::Table(root) = value {
                for i in 1usize.. {
                    let actor = call.raw_table_get(root, Value::Number(i as f64))?;
                    if actor == Value::Nil {
                        break;
                    }
                    mark(call, actor, i - 1)?;
                }
            }
        }
        Query::MarkScreen(layer) => super::super::set(
            call,
            value,
            "__songlua_compile_layer",
            Value::Number(layer as f64),
        )?,
        Query::SoundPaths => {
            let globals = helper(call, 5)?;
            let paths = super::super::field(call, globals, crate::SONG_LUA_SOUND_PATHS_KEY)?;
            if paths != Value::Nil {
                call.with_roots(&[paths], |call| {
                    let paths = table(call, paths)?;
                    out.paths.reserve(call.raw_table_len(paths)?);
                    for i in 1usize.. {
                        let path = call.raw_table_get(paths, Value::Number(i as f64))?;
                        if path == Value::Nil {
                            break;
                        }
                        out.paths.push(PathBuf::from(assets::text(call, path)?));
                    }
                    Ok(())
                })?;
            }
        }
        Query::Spin => out.spin = chronological::render_spin(call, table(call, value)?)?.is_some(),
        Query::Late(tag) => {
            let actor = table(call, value)?;
            if tag == 0
                && assets::field(call, actor, b"__songlua_actor_type")?
                    .is_some_and(|kind| kind.eq_ignore_ascii_case("Sprite"))
            {
                if let Some(name) = framebuffers::capture_name(call, actor)? {
                    out.kind = Some(Kind::AftSprite(name));
                } else if let Some(texture) = assets::field(call, actor, b"Texture")? {
                    let path = assets::resolve(call, actor, &texture)?.ok_or_else(|| {
                        call.error(
                            format!("actor asset '{texture}' could not be resolved").as_bytes(),
                        )
                    })?;
                    out.kind = Some(Kind::Sprite(path, sprites::states(call, actor)?));
                }
            } else if tag == 1 {
                if let Some(target) = proxies::read(call, actor)? {
                    let mut hidden = None;
                    if let crate::SongLuaProxyTarget::Player { player_index } = target {
                        let target = sprites::field(call, actor, "__songlua_proxy_target_actor")?;
                        if target != Value::Nil {
                            let id = table(call, target)?;
                            let value = call.with_roots(&[target], |call| {
                                sprites::field(call, id, "__songlua_visible")
                            });
                            if matches!(value, Ok(Value::Boolean(false))) {
                                hidden = Some(player_index);
                            }
                        }
                    }
                    out.kind = Some(Kind::Proxy(target, hidden));
                }
            } else if tag == 2 {
                out.kind = Some(Kind::Aft(framebuffers::read(call, actor)?));
            } else if tag == 3
                && assets::field(call, actor, b"__songlua_actor_type")?
                    .is_some_and(|kind| kind.eq_ignore_ascii_case("Sprite"))
            {
                let name = get(call, actor, b"__songlua_aft_capture_name")?;
                if let Some(name) = initial_state::value_text(call, name)? {
                    out.kind = Some(Kind::AftSprite(name));
                }
            }
        }
        Query::Draw => {
            if let Value::Table(root) = value
                && let Err(error) = draw::scene(call, root)
            {
                diagnostics::report(call, root, diagnostics::Callback::Draw, error)?;
            }
        }
        Query::End => {
            if let Value::Table(root) = value {
                call.store().startup_queues = Some(Vec::new());
                let globals = table(call, helper(call, 5)?)?;
                let key = call.try_intern(b"__songlua_top_screen")?;
                call.raw_table_set(globals, key, Value::Nil)?;
                let result = end_children(call, root);
                call.store().startup_queues = None;
                result?;
            }
        }
    }
    *read.output.lock().unwrap() = Some(out);
    Ok(())
}
fn mark(call: &mut HostCall<'_>, actor: Value, index: usize) -> Result<(), Error> {
    let Value::Table(id) = actor else {
        return Ok(());
    };
    call.with_roots(&[actor], |call| {
        super::super::set(
            call,
            actor,
            "__songlua_compile_layer",
            Value::Number(index as f64),
        )?;
        let wrappers = sprites::field(call, id, "__songlua_wrappers")?;
        if wrappers != Value::Nil {
            call.with_roots(&[wrappers], |call| {
                let array = table(call, wrappers)?;
                for i in 1usize.. {
                    let wrapper = call.raw_table_get(array, Value::Number(i as f64))?;
                    if wrapper == Value::Nil {
                        break;
                    }
                    mark(call, wrapper, index)?;
                }
                Ok(())
            })?;
        } else {
            let wrappers = call
                .store()
                .actors
                .get(&id)
                .map(|a| a.wrappers.clone())
                .unwrap_or_default();
            for wrapper in wrappers {
                mark(call, Value::Table(wrapper), index)?;
            }
        }
        for i in 1usize.. {
            let child = call.raw_table_get(id, Value::Number(i as f64))?;
            if child == Value::Nil {
                break;
            }
            mark(call, child, index)?;
        }
        Ok(())
    })
}

fn end_children(call: &mut HostCall<'_>, root: TableId) -> Result<(), Error> {
    native_children::with_children(call, root, |call, child| {
        native_children::run_command_with_drain(call, child, b"End", None, true)?;
        end_children(call, child)
    })
}
