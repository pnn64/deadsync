use deadlib_lua::{Value, Vm, compile};

#[test]
fn equal_binary_strings_keep_identity_across_word_boundaries_and_collection() {
    let mut vm = Vm::new();
    for len in [
        0usize, 1, 3, 4, 7, 8, 15, 16, 17, 31, 32, 33, 63, 64, 65, 511, 512, 513, 4095, 4096, 4097,
        65536,
    ] {
        let bytes: Vec<_> = (0..len).map(|i| ((i * 131 + i / 7) % 256) as u8).collect();
        let value = vm.intern(&bytes);
        let root = vm.pin(value).unwrap();
        assert_eq!(vm.intern(&bytes.clone()), value);
        let mut variants = Vec::new();
        for pos in [
            0,
            1,
            3,
            7,
            8,
            15,
            16,
            17,
            31,
            32,
            len / 2,
            len.saturating_sub(1),
        ] {
            if pos >= len {
                continue;
            }
            let mut changed = bytes.clone();
            changed[pos] ^= 0x80;
            let other = vm.intern(&changed);
            assert_ne!(other, value);
            variants.push((vm.pin(other).unwrap(), changed));
        }
        vm.collect_garbage().unwrap();
        assert_eq!(vm.intern(&bytes), vm.rooted(root).unwrap());
        for (other, changed) in variants {
            let value = vm.rooted(other).unwrap();
            assert_eq!(vm.intern(&changed), value);
            let Value::String(id) = value else {
                panic!("interned value is a string")
            };
            assert_eq!(vm.string_bytes(id), Some(changed.as_slice()));
            vm.unpin(other).unwrap();
        }
        vm.unpin(root).unwrap();
    }
}

#[test]
fn unaligned_input_slices_match_an_independent_content_map() {
    let bytes: Vec<_> = (0..512).map(|i| ((i * 17 + i / 11) % 256) as u8).collect();
    let mut vm = Vm::new();
    let mut expected = std::collections::HashMap::new();
    for offset in 0..16 {
        for len in 0..=257 {
            let slice = &bytes[offset..offset + len];
            let value = vm.intern(slice);
            assert_eq!(*expected.entry(slice.to_vec()).or_insert(value), value);
            let Value::String(id) = value else {
                panic!("interned value is a string")
            };
            assert_eq!(vm.string_bytes(id), Some(slice));
        }
    }
}

#[test]
fn string_keys_survive_growth_sweeping_and_slot_reuse() {
    let mut vm = Vm::new();
    let table = vm.new_table();
    let root = vm.pin(Value::Table(table)).unwrap();
    let get = compile(b"local t,k=...;return t[k]").unwrap();
    let clear = compile(b"return nil").unwrap();
    let mut keys = Vec::new();
    for i in 0..2048u64 {
        let mut bytes = vec![b'x'; 136];
        bytes[64..72].copy_from_slice(&i.to_le_bytes());
        let key = vm.intern(&bytes);
        vm.raw_table_set(table, key, Value::Number(i as f64))
            .unwrap();
        keys.push((key, bytes));
    }
    for (i, (key, _)) in keys.iter().enumerate().step_by(3) {
        vm.raw_table_set(table, *key, Value::Nil).unwrap();
        assert_eq!(
            vm.execute_with_args(&get, &[Value::Table(table), *key])
                .unwrap(),
            &[Value::Nil],
            "{i}"
        );
    }
    vm.execute(&clear).unwrap();
    vm.collect_garbage().unwrap();
    let Value::String(victim) = keys[0].0 else {
        panic!("key is a string")
    };
    assert!(vm.string_bytes(victim).is_none());
    for (i, (key, bytes)) in keys.iter().enumerate() {
        let current = vm.intern(bytes);
        if i % 3 == 0 {
            assert_ne!(current, *key);
            vm.raw_table_set(table, current, Value::Number(i as f64))
                .unwrap();
        } else {
            assert_eq!(current, *key);
        }
        assert_eq!(
            vm.execute_with_args(&get, &[Value::Table(table), current])
                .unwrap(),
            &[Value::Number(i as f64)]
        );
    }
    vm.collect_garbage().unwrap();
    vm.unpin(root).unwrap();
}

#[test]
fn lua_binary_string_keys_and_generated_middle_changes_match_the_reference() {
    let source=br#"local t={};for i=0,255 do local s=string.rep('x',64)..string.char(i)..string.rep('\0',96);t[s]=i end;collectgarbage();local out={};local n=0;for i=0,255 do local s=string.rep('x',64)..string.char(i)..string.rep('\0',96);n=n+t[s];out[#out+1]=string.char(t[s])end;return n,table.concat(out)"#;
    let mut vm = Vm::new();
    let values = vm.execute(&compile(source).unwrap()).unwrap().to_vec();
    let [Value::Number(n), Value::String(id)] = values.as_slice() else {
        panic!("scalar observations")
    };
    assert_eq!(*n, f64::from((0..=255).sum::<u32>()));
    assert_eq!(
        vm.string_bytes(*id).unwrap(),
        (0..=255).collect::<Vec<u8>>()
    );
}

static NAMES: &[u8] = b"positionx";

#[test]
fn static_slices_are_found_by_address_and_agree_with_ordinary_interning() {
    let mut vm = Vm::new();
    let whole = vm.intern_static(NAMES).unwrap();
    // Slices of one literal differ in address or length; each is its own name.
    let prefix = vm.intern_static(&NAMES[..8]).unwrap();
    let suffix = vm.intern_static(&NAMES[8..]).unwrap();
    let empty = vm.intern_static(&NAMES[..0]).unwrap();
    assert_eq!(vm.intern(b"positionx"), whole);
    assert_eq!(vm.intern(b"position"), prefix);
    assert_eq!(vm.intern(b"x"), suffix);
    assert_eq!(vm.intern(b""), empty);
    assert_eq!(vm.intern_static(b"").unwrap(), empty);
    for _ in 0..3 {
        assert_eq!(vm.intern_static(NAMES).unwrap(), whole);
        assert_eq!(vm.intern_static(&NAMES[..8]).unwrap(), prefix);
        assert_eq!(vm.intern_static(&NAMES[8..]).unwrap(), suffix);
    }
    // Equal bytes at another address name the same string.
    let copy: &'static [u8] = Vec::from(NAMES).leak();
    assert_ne!(copy.as_ptr(), NAMES.as_ptr());
    assert_eq!(vm.intern_static(copy).unwrap(), whole);
}

#[test]
fn static_strings_survive_collection_without_a_root_and_index_growth() {
    let mut vm = Vm::new();
    let names: Vec<&'static [u8]> = (0..700)
        .map(|index| &*format!("static name {index}").into_bytes().leak())
        .collect();
    let mut values = Vec::new();
    for (index, name) in names.iter().enumerate() {
        values.push(vm.intern_static(name).unwrap());
        if index % 97 == 0 {
            // Nothing else refers to these strings.
            vm.collect_garbage().unwrap();
        }
    }
    vm.collect_garbage().unwrap();
    vm.execute(&compile(b"local t = {} for i = 1, 2000 do t[i] = 'churn ' .. i end").unwrap())
        .unwrap();
    vm.collect_garbage().unwrap();
    for (name, value) in names.iter().zip(&values) {
        let Value::String(id) = *value else {
            panic!("interned value is a string")
        };
        assert_eq!(vm.string_bytes(id), Some(*name));
        assert_eq!(vm.intern_static(name).unwrap(), *value);
        assert_eq!(vm.intern(name), *value);
    }
}

#[test]
fn a_static_string_first_interned_during_a_collection_cycle_is_kept() {
    let mut vm = Vm::new();
    let mut values = Vec::new();
    let names: Vec<&'static [u8]> = (0..64)
        .map(|index| &*format!("cycle name {index}").into_bytes().leak())
        .collect();
    // Step the incremental collector between interning calls, so some strings
    // are first seen while a cycle is marking or sweeping.
    let churn =
        compile(b"local t = {} for i = 1, 200 do t[i] = {} end collectgarbage('step', 1)").unwrap();
    for name in &names {
        vm.execute(&churn).unwrap();
        values.push(vm.intern_static(name).unwrap());
    }
    for _ in 0..3 {
        vm.collect_garbage().unwrap();
    }
    for (name, value) in names.iter().zip(&values) {
        assert_eq!(vm.intern_static(name).unwrap(), *value);
        assert_eq!(vm.intern(name), *value);
    }
}

#[test]
fn host_callbacks_read_fields_named_by_static_strings() {
    use deadlib_lua::{Error, HostCall};
    fn read(call: &mut HostCall<'_>) -> Result<(), Error> {
        let Value::Table(target) = call.arguments()[0] else {
            return Err(call.error(b"table expected"));
        };
        let key = call.intern_static(b"static field")?;
        let value = call.raw_table_get(target, key)?;
        call.push(value)
    }
    let mut vm = Vm::new();
    let function = vm.new_host_function(read, &[]).unwrap();
    vm.set_global(b"read", function).unwrap();
    let program = compile(
        br#"
        local total = 0
        for i = 1, 100 do
            total = total + read({['static field'] = i})
            if i % 10 == 0 then collectgarbage() end
        end
        return total, read({})
        "#,
    )
    .unwrap();
    assert_eq!(
        vm.execute(&program).unwrap(),
        &[Value::Number(5050.0), Value::Nil]
    );
}
