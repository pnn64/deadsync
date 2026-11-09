//! Command closures first reached inside an action probe, including nested scopes.
use super::*;
use crate::rust_engine::function_snapshot::Snapshot;

#[derive(Default)]
pub(in crate::rust_engine) struct Scope {
    functions: HashSet<deadlib_lua::FunctionId>,
    snapshots: Vec<Snapshot>,
}
impl Actors {
    pub(in crate::rust_engine) fn begin_command_locals(&self) -> Option<Scope> {
        self.store
            .lock()
            .unwrap()
            .command_locals
            .replace(Scope::default())
    }
    pub(in crate::rust_engine) fn restore_command_locals(
        &self,
        vm: &mut Vm,
        previous: Option<Scope>,
    ) -> Result<(), Error> {
        let current = self.store.lock().unwrap().command_locals.take().unwrap();
        if let Some(mut previous) = previous {
            // Nested stability probes let locals advance across attempts. The
            // outermost scope undoes snapshots backwards so the earliest wins.
            previous.functions.extend(current.functions);
            previous.snapshots.extend(current.snapshots);
            self.store.lock().unwrap().command_locals = Some(previous);
        } else {
            let mut snapshots = current.snapshots;
            while let Some(saved) = snapshots.pop() {
                if let Err(error) = saved.restore(vm) {
                    // The native scope drops all remaining saved handles on an
                    // early restoration error. Detach our traced roots as well.
                    for saved in snapshots.into_iter().rev() {
                        saved.discard(vm)?;
                    }
                    return Err(error);
                }
            }
        }
        Ok(())
    }
}
pub(super) fn capture(
    call: &mut HostCall<'_>,
    store: &Shared,
    function: Value,
) -> Result<(), Error> {
    let Value::Function(id) = function else {
        return Ok(());
    };
    let preserve = store
        .lock()
        .unwrap()
        .command_locals
        .as_mut()
        .is_some_and(|scope| scope.functions.insert(id));
    if preserve {
        let owner = table(call, call.captures()[0])?;
        let saved = Snapshot::capture_locals(call, owner, function)?;
        store
            .lock()
            .unwrap()
            .command_locals
            .as_mut()
            .expect("command scope remains active during snapshot")
            .snapshots
            .push(saved);
    }
    Ok(())
}
