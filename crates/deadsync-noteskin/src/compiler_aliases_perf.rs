use super::*;

// Frozen normalize_table_aliases from a85295a5e; only the name changed.
fn before(noteskin: &Table, table_key: &str, canonical_keys: &[String]) -> mlua::Result<()> {
    let Some(table) = noteskin.get::<Option<Table>>(table_key)? else {
        return Ok(());
    };
    let mut existing = SmallVec::<[(u64, mlua::LuaString, Value); 8]>::new();
    for pair in table.pairs::<Value, Value>() {
        let (key, value) = pair?;
        let Value::String(text) = key else {
            continue;
        };
        let Ok(text_ref) = text.to_str() else {
            continue;
        };
        let fingerprint = ascii_case_hash(&text_ref);
        existing.push((fingerprint, text, value));
    }
    for want in canonical_keys {
        if table.contains_key(want.as_str())? {
            continue;
        }
        let fingerprint = ascii_case_hash(want);
        if let Some((_, _, value)) = existing.iter().find(|(hash, have, _)| {
            *hash == fingerprint
                && have
                    .to_str()
                    .is_ok_and(|have| have.eq_ignore_ascii_case(want))
        }) {
            table.set(want.as_str(), value.clone())?;
        }
    }
    Ok(())
}

#[test]
fn aliases_preserve_identity_raw_order_and_metamethod_effects() {
    let lua = Lua::new();
    let values = lua.create_table().unwrap();
    let shared = lua.create_table().unwrap();
    values.set("left", shared.clone()).unwrap();
    values.set("LEFT", lua.create_table().unwrap()).unwrap();
    values.set("down", false).unwrap();
    values.set(7, 99).unwrap();
    values.set(lua.create_string([255]).unwrap(), 17).unwrap();
    let names = ["Left", "Down", "Missing", "Unseen"].map(String::from);
    let mut results = Vec::new();
    for current in [false, true] {
        let table = lua.create_table().unwrap();
        values
            .for_each::<Value, Value>(|k, v| table.raw_set(k, v))
            .unwrap();
        let calls = lua.create_table().unwrap();
        let factory: Function = lua.load(r#"return function(t, calls)
            return setmetatable(t, {
                __index=function(_, k) calls[#calls+1]='get:'..k; if k=='Missing' then return false end end,
                __newindex=function(t,k,v) calls[#calls+1]='set:'..k; rawset(t,k,v) end
            })
        end"#).eval().unwrap();
        factory
            .call::<Table>((table.clone(), calls.clone()))
            .unwrap();
        let noteskin = lua.create_table().unwrap();
        noteskin.set("Rotate", table.clone()).unwrap();
        if current {
            normalize_table_aliases(&noteskin, "Rotate", &names)
        } else {
            before(&noteskin, "Rotate", &names)
        }
        .unwrap();
        let selected = names
            .iter()
            .map(|k| table.raw_get::<Value>(k.as_str()).unwrap())
            .collect::<Vec<_>>();
        let calls = calls
            .sequence_values::<String>()
            .collect::<mlua::Result<Vec<_>>>()
            .unwrap();
        results.push((selected, calls));
    }
    assert_eq!(results[0], results[1]);
    assert_eq!(results[0].0[1], Value::Boolean(false));
    assert_eq!(
        results[0].1,
        [
            "get:Left",
            "set:Left",
            "get:Down",
            "set:Down",
            "get:Missing",
            "get:Unseen"
        ]
    );
}

#[cfg(windows)]
#[test]
#[ignore = "paired release benchmark; run alone"]
fn benchmark_noteskin_aliases() {
    super::callbacks_perf::pin();
    let lua = Lua::new();
    for count in [0, 8, 64, 512] {
        let table = lua.create_table().unwrap();
        for i in 0..count {
            table.set(format!("key{i}"), i).unwrap();
        }
        let noteskin = lua.create_table().unwrap();
        noteskin.set("Rotate", table).unwrap();
        let names: Vec<_> = (0..8).map(|i| format!("Missing{i}")).collect();
        super::callbacks_perf::compare(&format!("noteskin_aliases_{count}"), 256, |current| {
            if current {
                normalize_table_aliases(&noteskin, "Rotate", &names)
            } else {
                before(&noteskin, "Rotate", &names)
            }
            .unwrap();
        });
    }
}
