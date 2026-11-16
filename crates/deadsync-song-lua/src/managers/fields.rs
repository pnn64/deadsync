//! Small value constructors for the song managers' initial tables.
use super::*;

pub(super) trait Field {
    fn value(self, call: &mut HostCall<'_>) -> Result<Value, Error>;
}
impl Field for Value {
    fn value(self, _: &mut HostCall<'_>) -> Result<Value, Error> {
        Ok(self)
    }
}
impl Field for TableId {
    fn value(self, _: &mut HostCall<'_>) -> Result<Value, Error> {
        Ok(Value::Table(self))
    }
}
impl Field for bool {
    fn value(self, _: &mut HostCall<'_>) -> Result<Value, Error> {
        Ok(Value::Boolean(self))
    }
}
macro_rules! numbers {
    ($($ty:ty),*) => {$(impl Field for $ty {
        fn value(self, _: &mut HostCall<'_>) -> Result<Value, Error> { Ok(Value::Number(self as f64)) }
    })*};
}
numbers!(u8, i32, i64, usize, f32, f64);
impl Field for &str {
    fn value(self, call: &mut HostCall<'_>) -> Result<Value, Error> {
        call.try_intern(self.as_bytes())
    }
}
impl Field for String {
    fn value(self, call: &mut HostCall<'_>) -> Result<Value, Error> {
        self.as_str().value(call)
    }
}

pub(super) fn set(
    table: TableId,
    key: impl Field,
    value: impl Field,
    call: &mut HostCall<'_>,
) -> Result<(), Error> {
    let key = key.value(call)?;
    let value = value.value(call)?;
    call.raw_table_set(table, key, value)
}

pub(super) fn object(
    call: &mut HostCall<'_>,
    build: impl FnOnce(&mut HostCall<'_>, TableId) -> Result<(), Error>,
) -> Result<TableId, Error> {
    let table = call.try_new_table()?;
    call.with_roots(&[Value::Table(table)], |call| {
        build(call, table)?;
        Ok(table)
    })
}

pub(super) fn array<T: Field>(
    call: &mut HostCall<'_>,
    values: impl IntoIterator<Item = T>,
) -> Result<TableId, Error> {
    object(call, |call, table| {
        for (i, value) in values.into_iter().enumerate() {
            set(table, i + 1, value, call)?;
        }
        Ok(())
    })
}
