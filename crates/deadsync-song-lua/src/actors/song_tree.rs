//! Lua observations for the existing overlay draw-tree collector.
use super::*;
use crate::SongLuaOverlayState;

#[derive(Clone)]
pub(crate) enum Query {
    AftNames,
    References,
    Wrappers,
    Index(usize, bool),
    Header,
    Name,
    Draw,
    Wrapper,
}
pub(crate) enum Answer {
    Names(HashSet<String>),
    References(HashSet<TableId>),
    Wrappers(Option<(Value, usize)>),
    Value(Value),
    Header(Option<Box<(String, SongLuaOverlayState, usize)>>),
    Name(Option<String>),
    Flag(bool),
}
pub(crate) struct Observation {
    pub answer: Answer,
    pin: Option<Root>,
}
impl Observation {
    pub fn release(self, vm: &mut Vm) -> Result<(), Error> {
        if let Some(pin) = self.pin {
            vm.unpin(pin)?;
        }
        Ok(())
    }
}
type Output = Arc<Mutex<Option<Answer>>>;
#[derive(Clone)]
struct Read {
    store: Shared,
    query: Query,
    output: Output,
}
pub(crate) struct Tree {
    registry: TableId,
    helpers: TableId,
    store: Shared,
}
impl Actors {
    pub(crate) fn overlay_tree(&self) -> Tree {
        Tree {
            registry: self.registry,
            helpers: self.helpers,
            store: self.store.clone(),
        }
    }
}
impl Tree {
    pub fn query(&self, vm: &mut Vm, actor: Value, query: Query) -> Result<Observation, Error> {
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
                query,
                output: output.clone(),
            },
        )?;
        let value = vm.call(function, &[])?[0];
        let answer = output.lock().unwrap().take().unwrap();
        // The callback returns a cursor handle on the VM stack. Pin it before
        // any subsequent Lua operation, then release at the native cursor scope.
        let pin = if matches!(answer, Answer::Value(_) | Answer::Wrappers(Some(_))) {
            Some(vm.pin(value)?)
        } else {
            None
        };
        Ok(Observation { answer, pin })
    }
    pub fn deferred(&self, actor: Value) -> bool {
        self.store
            .lock()
            .unwrap()
            .deferred_messages
            .iter()
            .any(|r| Value::Table(r.actor) == actor)
    }
}
fn text(call: &mut HostCall<'_>, actor: TableId, key: &str) -> Result<Option<String>, Error> {
    let value = sprites::field(call, actor, key)?;
    initial_state::value_text(call, value)
}
fn aft_names(
    call: &mut HostCall<'_>,
    actor: TableId,
    out: &mut HashSet<String>,
) -> Result<(), Error> {
    if text(call, actor, "__songlua_actor_type")?
        .is_some_and(|c| c.eq_ignore_ascii_case("ActorFrameTexture"))
    {
        let allocation = get(call, actor, b"__songlua_aft_allocation")?;
        if allocation != Value::Nil {
            table(call, allocation)?;
            if let Some(name) = framebuffers::capture_name(call, actor)? {
                out.insert(name);
            }
        }
    }
    let mut i = 1;
    loop {
        let value = call.raw_table_get(actor, Value::Number(i as f64))?;
        if value == Value::Nil {
            break;
        }
        if let Value::Table(child) = value {
            call.with_roots(&[value], |call| aft_names(call, child, out))?;
        }
        i += 1;
    }
    Ok(())
}
fn references(call: &mut HostCall<'_>, data: &Read) -> Result<HashSet<TableId>, Error> {
    let globals = helper(call, 5)?;
    let public_registry = sprites::field(call, table(call, globals)?, "__songlua_actor_registry")?;
    let known = if public_registry == Value::Nil {
        data.store
            .lock()
            .unwrap()
            .actor_order
            .iter()
            .copied()
            .collect::<HashSet<_>>()
    } else {
        let registry = table(call, public_registry)?;
        let mut known = HashSet::new();
        let mut index = 1;
        loop {
            let value = call.raw_table_get(registry, Value::Number(index as f64))?;
            if value == Value::Nil {
                break;
            }
            known.insert(table(call, value)?);
            index += 1;
        }
        known
    };
    let mut stack = call.native_stack()?;
    stack.push(globals)?;
    let outer = stack.top_offset();
    stack.push(Value::Nil)?;
    let mut referenced = HashSet::new();
    while stack.next_table(outer)? {
        if let Some(value @ Value::Table(id)) = stack.value(-1) {
            if known.contains(&id) {
                referenced.insert(id);
            } else if value != globals && value != public_registry {
                stack.push(value)?;
                let inner = stack.top_offset();
                stack.push(Value::Nil)?;
                while stack.next_table(inner)? {
                    if let Some(Value::Table(id)) = stack.value(-1)
                        && known.contains(&id)
                    {
                        referenced.insert(id);
                    }
                    stack.set_top(stack.top() - 1)?;
                }
                stack.set_top(stack.top() - 1)?;
            }
        }
        stack.set_top(stack.top() - 1)?;
    }
    Ok(referenced)
}
fn read(call: &mut HostCall<'_>) -> Result<(), Error> {
    let data = call.function_data::<Read>().unwrap().clone();
    let actor = table(call, call.captures()[2])?;
    let answer = match data.query {
        Query::AftNames => {
            let mut names = HashSet::new();
            aft_names(call, actor, &mut names)?;
            Answer::Names(names)
        }
        Query::References => Answer::References(references(call, &data)?),
        Query::Wrappers => {
            let raw = get(call, actor, b"__songlua_wrappers")?;
            let owned = if raw == Value::Nil {
                data.store
                    .lock()
                    .unwrap()
                    .actors
                    .get(&actor)
                    .map(|a| a.wrappers.clone())
                    .unwrap_or_default()
            } else {
                Vec::new()
            };
            let value = if !owned.is_empty() {
                // The Rust binding keeps the native default wrapper list in
                // actor metadata. An authored Lua list still takes precedence.
                let array = call.try_new_table()?;
                call.with_roots(&[Value::Table(array)], |call| {
                    for (i, wrapper) in owned.into_iter().enumerate() {
                        call.raw_table_set(
                            array,
                            Value::Number((i + 1) as f64),
                            Value::Table(wrapper),
                        )?;
                    }
                    Ok(())
                })?;
                Value::Table(array)
            } else {
                sprites::field(call, actor, "__songlua_wrappers")?
            };
            let value = if value == Value::Nil {
                None
            } else {
                let wrappers = table(call, value)?;
                Some((value, call.raw_table_len(wrappers)?))
            };
            Answer::Wrappers(value)
        }
        Query::Index(i, required) => {
            let value = call.raw_table_get(actor, Value::Number(i as f64))?;
            if required {
                table(call, value)?;
            }
            Answer::Value(value)
        }
        Query::Header => {
            let class = text(call, actor, "__songlua_actor_type")?;
            Answer::Header(if let Some(class) = class {
                let initial = initial_state::read(call, &data.store, actor)?;
                let initial = crate::overlay_state_after_blocks(initial, &[], 0.);
                let identity = call.object_pointer(Value::Table(actor)).unwrap();
                Some(Box::new((class, initial, identity)))
            } else {
                None
            })
        }
        Query::Name => Answer::Name(text(call, actor, "Name")?),
        Query::Draw => {
            let value = sprites::field(call, actor, "__songlua_draw_function")?;
            if !matches!(value, Value::Nil | Value::Function(_) | Value::Native(_)) {
                return Err(call.error(b"draw function expected"));
            }
            Answer::Flag(value != Value::Nil)
        }
        Query::Wrapper => Answer::Flag(
            text(call, actor, "__songlua_actor_type")?
                .is_some_and(|c| c.eq_ignore_ascii_case("WrapperState")),
        ),
    };
    let returned = match &answer {
        Answer::Value(value) => *value,
        Answer::Wrappers(Some((value, _))) => *value,
        _ => Value::Nil,
    };
    *data.output.lock().unwrap() = Some(answer);
    call.push(returned)
}
