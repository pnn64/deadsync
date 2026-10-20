//! Native storage for immediate actor vectors during live playback.
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
fn native_result<T>(result: mlua::Result<T>) -> mlua::Result<Option<T>> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(Error::UserDataTypeMismatch | Error::UserDataDestructed) => Ok(None),
        Err(error) => Err(error),
    }
}

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

    #[test]
    fn native_vectors_keep_borrow_errors_and_foreign_userdata_conversion() {
        let lua = Lua::new();
        let actor = lua.create_table().unwrap();
        store(&lua, &actor, "vector", [1.0, 2.0]).unwrap();
        let data = actor.get::<AnyUserData>("vector").unwrap();
        let guard = data.borrow_mut::<NativeVector>().unwrap();
        assert!(matches!(
            actor.get::<ActorVectorValue<2>>("vector"),
            Err(Error::UserDataBorrowError)
        ));
        assert!(matches!(
            store(&lua, &actor, "vector", [3.0, 4.0]),
            Err(Error::UserDataBorrowMutError)
        ));
        drop(guard);
        data.take::<NativeVector>().unwrap();
        for value in [
            Value::UserData(data),
            Value::UserData(lua.create_any_userdata(1_u32).unwrap()),
        ] {
            let expected = Table::from_lua(value.clone(), &lua)
                .err()
                .unwrap()
                .to_string();
            assert_eq!(
                ActorVectorValue::<2>::from_lua(value, &lua)
                    .err()
                    .unwrap()
                    .to_string(),
                expected
            );
        }
        store(&lua, &actor, "vector", [5.0, 6.0]).unwrap();
        assert_eq!(
            actor.get::<ActorVectorValue<2>>("vector").unwrap().0,
            Some([5.0, 6.0])
        );
    }

    #[test]
    fn native_vectors_preserve_table_coercions_and_errors() {
        let lua = Lua::new();
        for expression in [
            "{1,'2',3}",
            "{1,false,3}",
            "{1}",
            "{}",
            "setmetatable({}, {__index=function() error('must use raw reads') end})",
        ] {
            let table = lua.load(expression).eval::<Table>().unwrap();
            assert_eq!(
                ActorVectorValue::<3>::from_lua(Value::Table(table.clone()), &lua)
                    .unwrap()
                    .0,
                crate::lua_util::table_vector::<3>(&table)
            );
        }
        for expression in ["false", "12", "'bad'", "function() end"] {
            let value = lua.load(expression).eval::<Value>().unwrap();
            let expected = Table::from_lua(value.clone(), &lua)
                .err()
                .unwrap()
                .to_string();
            assert_eq!(
                ActorVectorValue::<3>::from_lua(value, &lua)
                    .err()
                    .unwrap()
                    .to_string(),
                expected
            );
        }
        let actor = lua.create_table().unwrap();
        let values = [-0.0, f32::NAN, f32::INFINITY, -f32::INFINITY, 0.25];
        store(&lua, &actor, "vector", values).unwrap();
        let actual = actor
            .get::<ActorVectorValue<5>>("vector")
            .unwrap()
            .0
            .unwrap();
        assert_eq!(values.map(f32::to_bits), actual.map(f32::to_bits));
        store(&lua, &actor, "vector", [1.0, 2.0]).unwrap();
        assert!(
            actor
                .get::<ActorVectorValue<3>>("vector")
                .unwrap()
                .0
                .is_none()
        );
    }

    #[test]
    fn native_vectors_reuse_slots_and_snapshot_independently() {
        let lua = Lua::new();
        let actor = lua.create_table().unwrap();
        let key = "__songlua_state_size";
        store(&lua, &actor, key, [12.0, 24.0]).unwrap();
        let original = actor.get::<AnyUserData>(key).unwrap();
        let state = crate::lua_util::snapshot_actor_mutable_state(&lua, &actor).unwrap();
        let cloned = crate::clone_lua_value(&lua, Value::Table(actor.clone())).unwrap();
        crate::draw_capture::perf::allocations::assert_no_churn(|| {
            for i in 0..100 {
                store(&lua, &actor, key, [i as f32, 8.0]).unwrap();
                assert_eq!(
                    actor.get::<ActorVectorValue<2>>(key).unwrap().0,
                    Some([i as f32, 8.0])
                );
            }
        });
        assert_eq!(original, actor.get::<AnyUserData>(key).unwrap());
        let cloned = Table::from_lua(cloned, &lua).unwrap();
        assert_eq!(
            cloned.get::<ActorVectorValue<2>>(key).unwrap().0,
            Some([12.0, 24.0])
        );
        crate::lua_util::restore_actor_mutable_state(&actor, state).unwrap();
        assert_eq!(
            actor.get::<ActorVectorValue<2>>(key).unwrap().0,
            Some([12.0, 24.0])
        );
        store(&lua, &actor, key, [3.0, 4.0]).unwrap();
        assert_eq!(
            cloned.get::<ActorVectorValue<2>>(key).unwrap().0,
            Some([12.0, 24.0])
        );
        let foreign = lua.create_any_userdata(42_u32).unwrap();
        assert!(
            matches!(crate::clone_lua_value(&lua, Value::UserData(foreign.clone())).unwrap(),
            Value::UserData(copy) if copy == foreign)
        );
    }
}
