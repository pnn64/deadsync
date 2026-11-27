use super::*;
use crate::compile;

#[test]
fn raw_string_absence_survives_overwrites_and_expires_on_reinsertion() {
    fn version(call: &mut HostCall<'_>) -> Result<(), Error> {
        let [Value::Table(table)] = *call.arguments() else {
            unreachable!()
        };
        let value = f64::from(call.raw_string_keys_added(table)?);
        call.push(Value::Number(value))
    }

    let program = compile(
        br#"
        local actor = {}
        local empty = version(actor)
        rawset(actor, 'mode', nil)
        assert(version(actor) == empty)
        rawset(actor, 'mode', 'bob')
        local added = version(actor)
        assert(added > empty)
        rawset(actor, 'mode', false)
        for i = 1, 64 do rawset(actor, i, i) end
        assert(version(actor) == added)
        assert(rawget(actor, 'mode') == false)
        rawset(actor, 'mode', nil)
        collectgarbage('collect')
        assert(version(actor) == added and rawget(actor, 'mode') == nil)
        rawset(actor, 'mode', 'spin')
        assert(version(actor) > added)
    "#,
    )
    .unwrap();

    for native in [false, true] {
        let mut vm = if native {
            Vm::try_new_with_lua51_table_order().unwrap()
        } else {
            Vm::new()
        };
        let callback = vm.new_host_function(version, &[]).unwrap();
        vm.set_global(b"version", callback).unwrap();
        vm.execute(&program).unwrap();
    }
}

#[test]
fn bound_number_fields_preserve_raw_writes_across_layouts_and_vms() {
    use std::sync::{Arc, Mutex};

    const NAMES: [&[u8]; 3] = [b"first", b"second", b"third"];

    fn publish(call: &mut HostCall<'_>) -> Result<(), Error> {
        let [Value::Table(table), Value::Number(number)] = *call.arguments() else {
            unreachable!()
        };
        let fields = call
            .function_data::<Arc<Mutex<NumberFields<3>>>>()
            .unwrap()
            .clone();
        let mut fields = fields.lock().unwrap();
        call.raw_table_write_number_fields(table, &mut fields, &[number, -0.0, f64::NAN])
    }

    let fields = Arc::new(Mutex::new(NumberFields::new(&NAMES)));
    let script = compile(
        br#"
        local actor={}
        local function verify(expected)
            assert(rawget(actor,'first')==expected)
            assert(1/rawget(actor,'second')==-math.huge)
            assert(rawget(actor,'third')~=rawget(actor,'third'))
        end
        local function keys()
            local out={}
            for k in next,actor do out[#out+1]=k end
            return table.concat(out,',')
        end
        publish(actor,4)
        verify(4)
        local first=keys()
        publish(actor,6)
        verify(6)
        assert(keys()==first)
        for i=1,127 do actor['other'..i]=i end
        actor.second=nil
        setmetatable(actor,{__newindex=function()error('raw write expected')end})
        collectgarbage()
        publish(actor,7)
        verify(7)
        assert(actor.other127==127)
        return actor
    "#,
    )
    .unwrap();

    for different_order in [false, true] {
        let mut vm = Vm::try_new_with_lua51_table_order().unwrap();
        if different_order {
            vm.intern_static(b"third").unwrap();
            vm.intern_static(b"unrelated").unwrap();
        }
        let function = vm
            .new_host_function_with_data(publish, &[], fields.clone())
            .unwrap();
        vm.set_global(b"publish", function).unwrap();
        let Value::Table(table) = vm.execute(&script).unwrap()[0] else {
            unreachable!()
        };
        let root = vm.pin(Value::Table(table)).unwrap();
        let before = vm.raw_fields_version(table).unwrap();
        vm.call(function, &[Value::Table(table), Value::Number(8.)])
            .unwrap();
        assert_eq!(vm.raw_fields_version(table).unwrap(), before.wrapping_add(3));
        vm.unpin(root).unwrap();
        let binding = fields.lock().unwrap();
        assert_eq!(binding.binding.as_ref().unwrap().owner, vm.identity);
    }
}

#[test]
fn shared_number_arrays_keep_identity_and_copy_on_write() {
    let mut vm = Vm::try_new_with_lua51_table_order().unwrap();
    let from_host = vm
        .new_host_function(
            |call| {
                let table = call.try_new_number_array(&[1., 1., 1., 0.])?;
                call.push(Value::Table(table))
            },
            &[],
        )
        .unwrap();
    let from_stack = vm
        .new_host_function(
            |call| {
                let mut stack = call.native_stack()?;
                stack.create_number_array(&[0., 0., 0., 1.])?;
                stack.finish(1)
            },
            &[],
        )
        .unwrap();
    vm.set_global(b"from_host", from_host).unwrap();
    vm.set_global(b"from_stack", from_stack).unwrap();
    vm.execute(&compile(b"a,b=from_host(),from_host(); c,d=from_stack(),from_stack()").unwrap())
        .unwrap();
    let data = |vm: &mut Vm, name| {
        let Value::Table(table) = vm.get_global(name).unwrap() else {
            panic!("expected a vector table");
        };
        vm.tables[slot_index(table.0)].array_values().as_ptr()
    };
    assert_eq!(data(&mut vm, b"a"), data(&mut vm, b"b"));
    assert_eq!(data(&mut vm, b"c"), data(&mut vm, b"d"));
    vm.execute(
        &compile(
            br#"
            assert(a~=b and c~=d and #a==4 and #c==4)
            table.sort(a)
            assert(a[1]==0 and a[4]==1 and b[1]==1 and b[4]==0)
            c[1] = -1/math.huge
            assert(1/c[1]==-math.huge and 1/d[1]==math.huge)
            c[5] = 2
            assert(#c==5 and #d==4 and d[5]==nil)
            "#,
        )
        .unwrap(),
    )
    .unwrap();
    assert_ne!(data(&mut vm, b"a"), data(&mut vm, b"b"));
    assert_ne!(data(&mut vm, b"c"), data(&mut vm, b"d"));
}

#[test]
fn triplet_writes_reuse_rows_until_the_field_is_read() {
    let mut vm = Vm::try_new_with_lua51_table_order().unwrap();
    let run = vm
        .new_host_function(
            |call| {
                let owner = call.try_new_table()?;
                let key = call.intern_static(b"points")?;
                call.set_empty_triplet_array(owner, b"points")?;
                for index in 1..32 {
                    let name = call.try_intern(format!("field{index}").as_bytes())?;
                    call.raw_table_set(owner, name, Value::Number(f64::from(index)))?;
                }
                assert!(call.set_unpublished_triplet(owner, key, 1, [1., 2., 3.])?);
                let root = call
                    .unpublished_number_rows(owner, key, NumberRows::Triplets)
                    .unwrap();
                let row = call.vm.tables[slot_index(root.0)].get_integer(1);
                assert!(call.set_unpublished_triplet(owner, key, 1, [4., 5., 6.])?);
                assert_eq!(call.vm.tables[slot_index(root.0)].get_integer(1), row);
                assert_eq!(
                    call.unpublished_triplets(owner, key).unwrap()(1),
                    [4., 5., 6.]
                );

                assert_eq!(call.raw_table_get(owner, key)?, Value::Table(root));
                for index in 32..128 {
                    let name = call.try_intern(format!("field{index}").as_bytes())?;
                    call.raw_table_set(owner, name, Value::Number(f64::from(index)))?;
                }
                assert!(!call.set_unpublished_triplet(owner, key, 1, [7., 8., 9.])?);
                call.raw_table_set_number_array(root, 1, &[7., 8., 9.])?;
                assert_ne!(call.vm.tables[slot_index(root.0)].get_integer(1), row);
                Ok(())
            },
            &[],
        )
        .unwrap();
    vm.call(run, &[]).unwrap();
}

#[test]
fn ordinary_field_results_are_rooted_until_the_scratch_register_is_reused() {
    fn read(call: &mut HostCall<'_>) -> Result<(), Error> {
        let target = call.arguments()[0];
        let key = call.arguments()[1];
        let Value::Table(table) = target else {
            unreachable!()
        };
        let value = call.table_get(target, key)?;
        call.raw_table_set(table, key, Value::Nil)?;
        // Collect directly inside the VM: opening a native stack or reentering
        // Lua would consume the scratch result before collection.
        call.vm.gc_control(GcOperation::Collect)?;
        call.push(value)
    }
    let mut vm = Vm::new();
    let function = vm.new_host_function(read, &[]).unwrap();
    vm.set_global(b"read", function).unwrap();
    assert_eq!(
        vm.execute(
            &compile(
                br#"
            local weak = setmetatable({}, {__mode = 'v'})
            local t = {x = {answer = 42}}
            weak[1] = t.x
            local value = read(t, 'x')
            assert(value.answer == 42 and value == weak[1] and t.x == nil)
            value = nil
            collectgarbage(); collectgarbage()
            return weak[1] == nil
        "#
            )
            .unwrap()
        )
        .unwrap(),
        &[Value::Boolean(true)]
    );
}
