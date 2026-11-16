//! Traced batches for temporary callbacks over arbitrary-sized actor scenes.
use super::*;

pub(super) fn values(
    vm: &mut Vm,
    values: impl IntoIterator<Item = Value>,
) -> Result<(TableId, Root), Error> {
    let table = vm.try_new_table()?;
    let pin = vm.pin(Value::Table(table))?;
    let result = values
        .into_iter()
        .enumerate()
        .try_for_each(|(i, value)| vm.raw_table_set(table, Value::Number((i + 1) as f64), value));
    if let Err(error) = result {
        vm.unpin(pin)?;
        return Err(error);
    }
    Ok((table, pin))
}

pub(super) fn attach_values(
    vm: &mut Vm,
    owner: TableId,
    inputs: impl IntoIterator<Item = Value>,
) -> Result<(), Error> {
    let (batch, pin) = values(vm, inputs)?;
    // A table key keeps the batch separate from the owner's indexed snapshots.
    let result = vm.raw_table_set(owner, Value::Table(batch), Value::Boolean(true));
    vm.unpin(pin)?;
    result
}
