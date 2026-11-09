//! Source capture history and scalar globals around one compiler command probe.
use super::*;

pub(super) struct Snapshot {
    actor: TableId,
    raw: TableId,
    saved: function_capture::Saved,
    scalars: Vec<(Value, Value)>,
    names: HashSet<String>,
}
fn scalar(value: Value) -> bool {
    matches!(
        value,
        Value::Boolean(_) | Value::Number(_) | Value::String(_)
    )
}
impl Snapshot {
    pub(super) fn capture(
        call: &mut HostCall<'_>,
        store: &Shared,
        actor: TableId,
        retained: TableId,
    ) -> Result<Self, Error> {
        let raw = state_snapshot::raw_copy_mutable(call, actor)?;
        call.raw_table_set(retained, Value::Number(1.), Value::Table(raw))?;
        let saved = function_capture::Saved::read(store, actor, raw)
            .ok_or_else(|| call.error(b"capture actor is not owned by this runtime"))?;
        let roots = call.try_new_table()?;
        call.raw_table_set(retained, Value::Number(2.), Value::Table(roots))?;
        let output = Arc::new(Mutex::new(None::<ScalarRows>));
        let scan = call.new_host_function_with_data(
            capture_scalars,
            &[helper(call, 5)?, Value::Table(roots)],
            output.clone(),
        )?;
        call.call(scan, &[])?;
        let (scalars, names) = output.lock().unwrap().take().unwrap();
        Ok(Self {
            actor,
            raw,
            saved,
            scalars,
            names,
        })
    }
    pub(super) fn restore(
        self,
        call: &mut HostCall<'_>,
        store: &Shared,
        raw: bool,
    ) -> Result<(), Error> {
        let globals = helper(call, 5)?;
        let output: Removals = Arc::new(Mutex::new(None));
        let scan = call.new_host_function_with_data(
            scan_scalar_removals,
            &[globals],
            (self.names, output.clone()),
        )?;
        call.call(scan, &[])?;
        let keys = output.lock().unwrap().take().unwrap();
        // Validate the complete global-key scan before making any changes.
        for (key, value) in keys
            .into_iter()
            .map(|key| (key, Value::Nil))
            .chain(self.scalars)
        {
            if raw {
                call.raw_table_set(table(call, globals)?, key, value)?;
            } else {
                call.table_set(globals, key, value)?;
            }
        }
        state_snapshot::restore_mutable(call, self.actor, self.raw, raw)?;
        self.saved.restore_mutable(store);
        lifecycle::invalidate_plans(call, store)
    }
}
type ScalarRows = (Vec<(Value, Value)>, HashSet<String>);
fn capture_scalars(call: &mut HostCall<'_>) -> Result<(), Error> {
    let output = call
        .function_data::<Arc<Mutex<Option<ScalarRows>>>>()
        .unwrap()
        .clone();
    let globals = call.captures()[0];
    let roots = call.captures()[1];
    let result = {
        let mut stack = call.native_stack()?;
        stack.push(roots)?;
        let retained = stack.top_offset();
        stack.push(globals)?;
        let globals = stack.top_offset();
        stack.push(Value::Nil)?;
        let mut rows = Vec::new();
        let mut names = HashSet::new();
        while stack.next_table(globals)? {
            let key = stack.value(-2).unwrap();
            let value = stack.value(-1).unwrap();
            if matches!(key, Value::String(_)) && scalar(value) {
                let name = std::str::from_utf8(stack.to_bytes(-2)?.unwrap()).map(str::to_owned);
                let name = name.map_err(|e| stack.error(e.to_string().as_bytes()))?;
                names.insert(name);
                let index = rows.len() * 2 + 1;
                for (slot, item) in [(index, key), (index + 1, value)] {
                    stack.push(Value::Number(slot as f64))?;
                    stack.push(item)?;
                    stack.set_table(retained, true)?;
                }
                rows.push((key, value));
            }
            stack.set_top(stack.top() - 1)?;
        }
        (rows, names)
    };
    *output.lock().unwrap() = Some(result);
    Ok(())
}
type Removals = Arc<Mutex<Option<Vec<Value>>>>;
fn scan_scalar_removals(call: &mut HostCall<'_>) -> Result<(), Error> {
    let (names, output) = call
        .function_data::<(HashSet<String>, Removals)>()
        .unwrap()
        .clone();
    let globals = call.captures()[0];
    let result = {
        let mut stack = call.native_stack()?;
        stack.push(globals)?;
        let globals = stack.top_offset();
        stack.push(Value::Nil)?;
        let mut keys = Vec::new();
        while stack.next_table(globals)? {
            if matches!(stack.value(-2), Some(Value::String(_))) {
                let name = std::str::from_utf8(stack.to_bytes(-2)?.unwrap()).map(str::to_owned);
                let name = name.map_err(|e| stack.error(e.to_string().as_bytes()))?;
                if scalar(stack.value(-1).unwrap()) && !names.contains(&name) {
                    keys.push(stack.value(-2).unwrap());
                }
            }
            stack.set_top(stack.top() - 1)?;
        }
        keys
    };
    *output.lock().unwrap() = Some(result);
    Ok(())
}
