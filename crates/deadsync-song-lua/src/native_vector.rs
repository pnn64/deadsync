//! Native storage for immediate actor vectors during live playback.
#[cfg(any(test, feature = "legacy-oracle"))]
use mlua::{AnyUserData, Error, FromLua, Lua, Table, Value};

#[derive(Clone, Copy)]
struct NativeVector {
    values: [f32; 5],
    len: usize,
}

impl NativeVector {
    fn new<const N: usize>(values: [f32; N]) -> Self {
        assert!((2..=5).contains(&N));
        let mut stored = Self {
            values: [0.0; 5],
            len: N,
        };
        stored.values[..N].copy_from_slice(&values);
        stored
    }
}

/// Decode either native live storage or a legacy Lua table. Table fallback
/// preserves coercions, malformed vectors, and the original type errors.
pub(crate) struct ActorVectorValue<const N: usize>(pub Option<[f32; N]>);

// Borrow once to check the private type and access its values. Scoped borrows
// avoid a second type lookup and a temporary rooted userdata reference. These
// closures only copy native values; none invokes Lua while a borrow is held.
#[cfg(any(test, feature = "legacy-oracle"))]
fn native_result<T>(result: mlua::Result<T>) -> mlua::Result<Option<T>> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(Error::UserDataTypeMismatch | Error::UserDataDestructed) => Ok(None),
        Err(error) => Err(error),
    }
}

#[cfg(any(test, feature = "legacy-oracle"))]
impl<const N: usize> FromLua for ActorVectorValue<N> {
    fn from_lua(value: Value, lua: &Lua) -> mlua::Result<Self> {
        if let Value::UserData(ref data) = value
            && let Some(vector) = native_result(data.borrow_scoped::<NativeVector, _>(|vector| {
                vector.values[..vector.len]
                    .get(..N)
                    .map(|values| values.try_into().expect("vector length checked"))
            }))?
        {
            return Ok(Self(vector));
        }
        let table = Table::from_lua(value, lua)?;
        Ok(Self(crate::lua_util::table_vector(&table)))
    }
}

#[cfg(any(test, feature = "legacy-oracle"))]
/// Update the actor's owned slot. Previously emitted draws contain value
/// copies, so changing this slot cannot mutate an earlier draw snapshot.
pub(crate) fn store<const N: usize>(
    lua: &Lua,
    actor: &Table,
    key: &str,
    value: [f32; N],
) -> mlua::Result<()> {
    let value = NativeVector::new(value);
    if let Value::UserData(data) = actor.raw_get::<Value>(key)?
        && native_result(data.borrow_mut_scoped::<NativeVector, _>(|slot| *slot = value))?.is_some()
    {
        return Ok(());
    }
    actor.set(key, lua.create_any_userdata(value)?)
}

#[cfg(any(test, feature = "legacy-oracle"))]
/// State snapshots and cloned definitions own independent values. Export the
/// ordinary table representation so restoring them also works in compilation.
pub(crate) fn snapshot(lua: &Lua, data: &AnyUserData) -> mlua::Result<Option<Table>> {
    let Some(value) = native_result(data.borrow_scoped::<NativeVector, _>(|vector| *vector))?
    else {
        return Ok(None);
    };
    lua.create_sequence_from(value.values[..value.len].iter().copied())
        .map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;

    

    

    
}
