use deadlib_lua::{Value, Vm, compile};
use lua_reference::{Lua, MultiValue, Value as RefValue};

#[derive(Debug, PartialEq)]
enum Observed {
    Nil,
    Boolean(bool),
    Number(u64),
    NaN,
    Bytes(Vec<u8>),
}
fn number(value: f64) -> Observed {
    if value.is_nan() {
        Observed::NaN
    } else {
        Observed::Number(value.to_bits())
    }
}
fn check(source: &[u8]) {
    let program = compile(source).unwrap();
    let mut vm = Vm::new();
    let values = vm.execute(&program).unwrap().to_vec();
    let got: Vec<_> = values
        .into_iter()
        .map(|v| match v {
            Value::Nil => Observed::Nil,
            Value::Boolean(v) => Observed::Boolean(v),
            Value::Number(v) => number(v),
            Value::String(id) => Observed::Bytes(vm.string_bytes(id).unwrap().to_vec()),
            _ => panic!("expose identity through Lua predicates"),
        })
        .collect();
    let lua = Lua::new();
    let expected: Vec<_> = lua
        .load(source)
        .set_name("=(deadlib-lua)")
        .eval::<MultiValue>()
        .unwrap()
        .into_iter()
        .map(|v| match v {
            RefValue::Nil => Observed::Nil,
            RefValue::Boolean(v) => Observed::Boolean(v),
            RefValue::Number(v) => number(v),
            RefValue::Integer(v) => number(v as f64),
            RefValue::String(v) => Observed::Bytes(v.as_bytes().to_vec()),
            _ => panic!("unexpected reference result"),
        })
        .collect();
    assert_eq!(got, expected, "{}", String::from_utf8_lossy(source));
}
fn quoted(bytes: &[u8]) -> String {
    let mut text = String::from("'");
    for byte in bytes {
        text.push_str(&format!("\\{byte:03}"));
    }
    text.push('\'');
    text
}

#[test]
fn growing_pools_preserve_first_zero_sign_numeric_equality_and_string_keys() {
    for n in [
        0, 1, 15, 16, 31, 32, 33, 47, 48, 49, 95, 96, 97, 255, 256, 257, 1024,
    ] {
        for zero in ["0", "-0"] {
            let mut source = format!("local first={zero};local t={{zero=0,[0]=17,['0']=19}};");
            for i in 1..=n {
                source.push_str(&format!("t.key{i}={i};"));
            }
            source.push_str(&format!(
                "return 1/first,1/t.zero,t.key1,t.key{n},t[0],t['0'],t['zero']==first"
            ));
            check(source.as_bytes());
        }
    }
}

#[test]
fn binary_literals_with_shared_prefix_suffix_and_middle_changes_stay_distinct() {
    let mut source = String::from("local t={};local score=0;");
    let entries: Vec<_> = (0..200)
        .map(|i| {
            let mut key = vec![255; 96];
            key[0] = 0;
            key[48] = i as u8;
            let mut value = key.clone();
            value[95] = 17;
            (quoted(&key), quoted(&value))
        })
        .collect();
    for (key, value) in &entries {
        source.push_str(&format!("t[{key}]={value};"));
    }
    for (key, value) in &entries {
        source.push_str(&format!("assert(t[{key}]=={value});score=score+#t[{key}];"));
    }
    source.push_str(&format!(
        "return score,t[{}],t[{}]",
        entries[0].0, entries[199].0
    ));
    check(source.as_bytes());
}

#[test]
fn nested_function_pools_keep_captures_methods_and_local_constants_independent() {
    let mut source =
        String::from("local base={};function base:add(v)return self.n+v end;local funcs={};");
    for i in 1..=64 {
        source.push_str(&format!("funcs[{i}]=function(x)local t={{"));
        for j in 1..=40 {
            source.push_str(&format!("k{j}={j},"));
        }
        source
            .push_str("};local o=setmetatable({n=x},{__index=base});return o:add(t.k1+t.k40)end;");
    }
    source.push_str("local n=0;for i=1,64 do n=n+funcs[i](i)end;return n");
    check(source.as_bytes());
}
