//! Rooted PlayerOptions observations for static and chronological compilation.
use super::*;
use crate::perframe::ModState;
use rustc_hash::FxHashSet;

#[derive(Clone, Default)]
pub(crate) struct Snapshot(Box<[(Arc<str>, f32)]>);
impl ModState for Snapshot {
    fn entries(&self) -> impl Iterator<Item = (&str, &f32)> {
        self.0.iter().map(|(name, value)| (name.as_ref(), value))
    }
    fn get(&self, key: &str) -> Option<&f32> {
        self.0
            .binary_search_by(|(name, _)| name.as_ref().cmp(key))
            .ok()
            .map(|index| &self.0[index].1)
    }
    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}
#[derive(Default)]
struct Entries {
    names: FxHashSet<Arc<str>>,
    entries: Vec<(Arc<str>, f32)>,
}
impl Entries {
    fn name(&mut self, text: &str) -> Arc<str> {
        if let Some(name) = self.names.get(text) {
            return name.clone();
        }
        let name: Arc<str> = text.into();
        self.names.insert(name.clone());
        name
    }
    fn finish(&mut self) -> Snapshot {
        self.entries.sort_by(|a, b| a.0.cmp(&b.0));
        self.entries.dedup_by(|next, previous| {
            if next.0 == previous.0 {
                std::mem::swap(next, previous);
                true
            } else {
                false
            }
        });
        Snapshot(self.entries.as_slice().into())
    }
    fn override_value(&mut self, key: &str, value: f32) {
        if let Some((_, previous)) = self
            .entries
            .iter_mut()
            .rev()
            .find(|(name, _)| &**name == key)
        {
            *previous = value;
        } else {
            let name = self.name(key);
            self.entries.push((name, value));
        }
    }
}
type SharedEntries = Arc<Mutex<Option<Entries>>>;
struct Scratch {
    entries: SharedEntries,
    output: Option<[Snapshot; crate::LUA_PLAYERS]>,
}
type SharedScratch = Arc<Mutex<Option<Scratch>>>;

/// Both original option tables are retained even if authored getters replace
/// their globals. Only the enclosing compiler owns this load-lifetime root.
pub(crate) struct Options {
    owner: Option<Root>,
    scratch: SharedScratch,
}
impl Actors {
    pub(crate) fn update_options(&self, vm: &mut Vm) -> Result<Options, Error> {
        let owner = vm.try_new_table()?;
        let root = vm.pin(Value::Table(owner))?;
        let entries = Arc::new(Mutex::new(Some(Entries::default())));
        let scratch = Arc::new(Mutex::new(Some(Scratch {
            entries: entries.clone(),
            output: None,
        })));
        let result = (|| {
            let read = vm.new_host_function(
                |call| {
                    let owner = table(call, call.captures()[2])?;
                    let globals = helper(call, 5)?;
                    for (index, key) in crate::SONG_LUA_PLAYER_OPTIONS_KEYS.iter().enumerate() {
                        let value = super::super::field(call, globals, key)?;
                        table(call, value)?;
                        call.raw_table_set(owner, Value::Number((index + 1) as f64), value)?;
                    }
                    Ok(())
                },
                &[
                    Value::Table(self.registry),
                    Value::Table(self.helpers),
                    Value::Table(owner),
                ],
            )?;
            vm.call(read, &[])?;
            for speeds in [false, true] {
                let callback =
                    vm.new_host_function_with_data(pairs, &[], (entries.clone(), speeds))?;
                vm.raw_table_set(owner, Value::Number(if speeds { 6. } else { 5. }), callback)?;
                let callback = vm.new_host_function_with_data(
                    sample,
                    &[
                        Value::Table(self.registry),
                        Value::Table(self.helpers),
                        Value::Table(owner),
                    ],
                    (scratch.clone(), speeds),
                )?;
                vm.raw_table_set(owner, Value::Number(if speeds { 4. } else { 3. }), callback)?;
            }
            Ok(())
        })();
        if let Err(error) = result {
            vm.unpin(root)?;
            return Err(error);
        }
        Ok(Options {
            owner: Some(root),
            scratch,
        })
    }
}
impl Options {
    pub fn sample(
        &mut self,
        vm: &mut Vm,
        speeds: bool,
    ) -> Result<[Snapshot; crate::LUA_PLAYERS], Error> {
        let Value::Table(owner) = vm.rooted(self.owner.expect("active option capture"))? else {
            unreachable!()
        };
        let callback = vm.raw_table_get(owner, Value::Number(if speeds { 4. } else { 3. }))?;
        vm.call(callback, &[])?;
        Ok(self
            .scratch
            .lock()
            .unwrap()
            .as_mut()
            .unwrap()
            .output
            .take()
            .unwrap())
    }
    pub fn release(&mut self, vm: &mut Vm) -> Result<(), Error> {
        if let Some(owner) = self.owner {
            vm.unpin(owner)?;
            self.owner = None;
        }
        Ok(())
    }
}
fn sample(call: &mut HostCall<'_>) -> Result<(), Error> {
    let (shared, speeds) = call
        .function_data::<(SharedScratch, bool)>()
        .unwrap()
        .clone();
    let scratch = shared.lock().unwrap().take();
    let mut scratch =
        scratch.ok_or_else(|| call.error(b"PlayerOptions capture is already active"))?;
    scratch.output = None;
    let result = (|| {
        let owner = table(call, call.captures()[2])?;
        let pairs = call.raw_table_get(owner, Value::Number(if speeds { 6. } else { 5. }))?;
        let mut output = [Snapshot::default(), Snapshot::default()];
        for (index, output) in output.iter_mut().enumerate() {
            let value = call.raw_table_get(owner, Value::Number((index + 1) as f64))?;
            *output = read(call, table(call, value)?, speeds, &mut scratch, pairs)?;
        }
        scratch.output = Some(output);
        Ok(())
    })();
    *shared.lock().unwrap() = Some(scratch);
    result
}
fn raw(call: &mut HostCall<'_>, table: TableId, key: &str) -> Result<Value, Error> {
    let key = call.try_intern(key.as_bytes())?;
    call.raw_table_get(table, key)
}
fn read(
    call: &mut HostCall<'_>,
    owner: TableId,
    speeds: bool,
    scratch: &mut Scratch,
    pairs: Value,
) -> Result<Snapshot, Error> {
    scratch
        .entries
        .lock()
        .unwrap()
        .as_mut()
        .unwrap()
        .entries
        .clear();
    let state = raw(
        call,
        owner,
        if speeds {
            "__songlua_player_option_speeds"
        } else {
            "__songlua_player_option_state"
        },
    )?;
    if state != Value::Nil {
        let state = table(call, state)?;
        // Each traversal gets its own host invocation and stack lease. The
        // parent also performs ordinary getters and can call Lua between reads.
        call.call(pairs, &[Value::Table(state)])?;
    }
    if !speeds {
        let active = raw(call, owner, "__songlua_speedmod_active")?;
        let active = initial_state::value_text(call, active)?;
        for (key, field) in [
            ("xmod", "__songlua_speedmod_xmod"),
            ("cmod", "__songlua_speedmod_cmod"),
            ("mmod", "__songlua_speedmod_mmod"),
        ] {
            if active.as_deref().is_some_and(|active| active != key) {
                continue;
            }
            let value = sprites::field(call, owner, field)?;
            if value != Value::Nil {
                let value =
                    song_columns::numeric(call, value, song_columns::Numeric::Float)? as f32;
                scratch
                    .entries
                    .lock()
                    .unwrap()
                    .as_mut()
                    .unwrap()
                    .override_value(key, value);
            }
        }
    }
    Ok(scratch.entries.lock().unwrap().as_mut().unwrap().finish())
}
fn pairs(call: &mut HostCall<'_>) -> Result<(), Error> {
    let (shared, speeds) = call
        .function_data::<(SharedEntries, bool)>()
        .unwrap()
        .clone();
    let entries = shared.lock().unwrap().take();
    let mut entries =
        entries.ok_or_else(|| call.error(b"PlayerOptions traversal is already active"))?;
    let result = read_pairs(call, speeds, &mut entries);
    *shared.lock().unwrap() = Some(entries);
    result
}
fn read_pairs(call: &mut HostCall<'_>, speeds: bool, scratch: &mut Entries) -> Result<(), Error> {
    let mut stack = call.native_stack()?;
    let index = 1;
    stack.push(Value::Nil)?;
    while stack.next_table(index)? {
        // Convert a copy: numeric key coercion must not replace the traversal key.
        stack.push(stack.value(-2).unwrap())?;
        let name = match stack.to_bytes(-1)? {
            Some(bytes) => match std::str::from_utf8(bytes) {
                Ok(text) => scratch.name(text),
                Err(error) => return Err(stack.error(error.to_string().as_bytes())),
            },
            None => return Err(stack.error(b"PlayerOptions name: string expected")),
        };
        stack.set_top(stack.top() - 1)?;
        let value = if speeds {
            Some(
                stack
                    .to_number(-1)
                    .ok_or_else(|| stack.error(b"PlayerOptions speed: number expected"))?
                    as f32,
            )
        } else {
            match stack.value(-1).unwrap() {
                Value::Boolean(value) => Some(f32::from(value)),
                Value::Number(value) => (value as f32).is_finite().then_some(value as f32),
                Value::String(_) => std::str::from_utf8(stack.to_bytes(-1)?.unwrap())
                    .ok()
                    .and_then(|s| s.trim().parse::<f32>().ok()),
                _ => None,
            }
        };
        if let Some(value) = value {
            scratch.entries.push((name, value));
        }
        stack.set_top(stack.top() - 1)?;
    }
    stack.finish(0)
}
