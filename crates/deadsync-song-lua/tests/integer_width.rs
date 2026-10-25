use deadsync_song_lua::{create_range_table, json_to_lua_value, song_lua_runtime_number};
use mlua::{Lua, MultiValue, Value};

#[test]
fn json_integers_preserve_values_beyond_i32() {
    let lua = Lua::new();
    for value in [
        i64::MIN,
        i64::from(i32::MIN) - 1,
        i64::from(i32::MIN),
        0,
        i64::from(i32::MAX),
        i64::from(i32::MAX) + 1,
        i64::MAX,
    ] {
        let actual = json_to_lua_value(&lua, serde_json::json!(value)).unwrap();
        assert_eq!(lua.coerce_number(actual).unwrap(), Some(value as f64));
    }
}

#[test]
fn runtime_numbers_preserve_values_beyond_native_integer_range() {
    let lua = Lua::new();
    for value in boundary_values() {
        let actual = song_lua_runtime_number(value).unwrap();
        assert_eq!(lua.coerce_number(actual).unwrap(), Some(f64::from(value)));
    }
}

#[test]
fn range_numbers_preserve_values_beyond_native_integer_range() {
    let lua = Lua::new();
    for value in boundary_values() {
        let args = MultiValue::from_vec(vec![
            Value::Number(f64::from(value)),
            Value::Number(f64::from(value)),
            Value::Number(f64::from(value.abs().max(1.0))),
        ]);
        let Value::Table(table) = create_range_table(&lua, &args).unwrap() else {
            panic!("range should return a table");
        };
        assert_eq!(table.raw_len(), 1);
        assert_eq!(table.raw_get::<f64>(1).unwrap(), f64::from(value));
    }
}

fn boundary_values() -> [f32; 11] {
    [
        0.0,
        -1.0,
        1.0,
        0.5,
        -2_147_483_648.0,
        -2_147_483_904.0,
        2_147_483_648.0,
        -9_223_372_036_854_775_808.0,
        9_223_372_036_854_775_808.0,
        -18_446_744_073_709_551_616.0,
        f32::MAX,
    ]
}
