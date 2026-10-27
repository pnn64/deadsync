use deadlib_lua::{Error, HostCall, Value, Vm, compile};
fn run(vm: &mut Vm, source: &[u8]) -> Vec<Value> {
    vm.execute(&compile(source).unwrap()).unwrap().to_vec()
}
fn object_set(vm: &mut Vm) -> Vec<Value> {
    run(vm,b"objects={{},function()end,print,coroutine.create(function()end),newproxy()};return unpack(objects)")
}

#[test]
fn object_addresses_are_unique_across_live_vms_and_stable_through_moves_and_gc() {
    let mut a = Vm::new();
    let av = object_set(&mut a);
    let ap: Vec<_> = av.iter().map(|&v| a.object_pointer(v).unwrap()).collect();
    let mut b = Vm::new();
    let bv = object_set(&mut b);
    let bp: Vec<_> = bv.iter().map(|&v| b.object_pointer(v).unwrap()).collect();
    let unique: std::collections::HashSet<_> = ap.iter().chain(&bp).copied().collect();
    assert_eq!(unique.len(), ap.len() + bp.len());
    let mut a = Box::new(a);
    run(
        &mut a,
        b"local t={};for i=1,2000 do t[i]={i}end;collectgarbage()",
    );
    for (value, pointer) in av.into_iter().zip(ap) {
        assert_eq!(a.object_pointer(value), Some(pointer));
    }
    for (value, pointer) in bv.into_iter().zip(bp) {
        assert_eq!(b.object_pointer(value), Some(pointer));
    }
}

#[test]
fn indexed_and_captured_pointer_queries_preserve_the_stack_and_reject_scalars() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut stack = call.native_stack()?;
        let before = stack.top();
        let p = stack.to_pointer(1).unwrap();
        assert_eq!(stack.to_pointer(-1), Some(p));
        assert_eq!(stack.to_pointer(-10003), Some(p));
        for i in [0, 2, -2, -10004] {
            assert_eq!(stack.to_pointer(i), None);
        }
        assert_eq!(stack.to_pointer(-10001), stack.to_pointer(-10002));
        assert_ne!(stack.to_pointer(-10000), Some(p));
        assert_eq!(stack.top(), before);
        stack.finish(1)
    }
    let mut vm = Vm::new();
    let table = Value::Table(vm.new_table());
    let f = vm.new_host_function(callback, &[table]).unwrap();
    vm.set_global(b"host", f).unwrap();
    vm.set_global(b"t", table).unwrap();
    run(&mut vm, b"assert(host(t)==t)");
    let text = vm.intern(b"scalar");
    for value in [Value::Nil, Value::Boolean(false), Value::Number(17.), text] {
        assert_eq!(vm.object_pointer(value), None);
    }
    assert_eq!(vm.object_pointer(Value::LightUserdata(0)), Some(0));
}

#[test]
fn opaque_pointer_bookkeeping_does_not_keep_objects_or_native_bindings_alive() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering::Relaxed},
    };
    struct Resource(Arc<AtomicUsize>);
    impl Drop for Resource {
        fn drop(&mut self) {
            self.0.fetch_add(1, Relaxed);
        }
    }
    fn unused(_: &mut HostCall<'_>) -> Result<(), Error> {
        Ok(())
    }
    let mut vm = Vm::new();
    let values=run(&mut vm,b"local t={{},function()end,coroutine.create(function()end),newproxy()};weak=setmetatable(t,{__mode='v'});return unpack(t)");
    for &value in &values {
        assert!(vm.object_pointer(value).is_some());
    }
    let drops = Arc::new(AtomicUsize::new(0));
    let native = vm
        .new_host_function_with_data(unused, &[], Resource(drops.clone()))
        .unwrap();
    assert!(vm.object_pointer(native).is_some());
    run(&mut vm, b"return 42");
    vm.collect_garbage().unwrap();
    for value in values {
        assert_eq!(vm.object_pointer(value), None);
    }
    assert_eq!(vm.object_pointer(native), None);
    assert_eq!(drops.load(Relaxed), 1);
    run(&mut vm, b"for i=1,4 do assert(weak[i]==nil)end");
}

#[test]
fn default_object_addresses_use_the_same_native_pointer_format_as_official_lua() {
    let reference = lua_reference::Lua::new();
    let mut vm = Vm::new();
    let values = object_set(&mut vm);
    for value in values {
        let pointer = vm.object_pointer(value).unwrap();
        reference
            .globals()
            .set(
                "pointer",
                lua_reference::LightUserData(pointer as *mut std::ffi::c_void),
            )
            .unwrap();
        let expected: String = reference.load("return tostring(pointer)").eval().unwrap();
        vm.set_global(b"value", value).unwrap();
        let actual = run(&mut vm, b"return tostring(value)");
        let Value::String(id) = actual[0] else {
            panic!("string expected")
        };
        let suffix = expected.strip_prefix("userdata: ").unwrap();
        let expected = format!("{}: {suffix}", value.type_name());
        assert_eq!(vm.string_bytes(id).unwrap(), expected.as_bytes());
    }
    let values = run(&mut vm, b"return newproxy()");
    let value = values[0];
    let pointer = vm.object_pointer(value).unwrap();
    let Value::Userdata(id) = value else {
        unreachable!()
    };
    assert_eq!(vm.userdata_bytes(id).unwrap().as_ptr() as usize, pointer);
}

#[test]
fn userdata_pointer_identity_survives_finalization_resurrection_and_nested_collection() {
    fn pointer(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut stack = call.native_stack()?;
        let p = stack.to_pointer(1).unwrap();
        stack.push(Value::LightUserdata(p as u64))?;
        stack.finish(1)
    }
    let mut vm = Vm::new();
    let host = vm.new_host_function(pointer, &[]).unwrap();
    vm.set_global(b"pointer", host).unwrap();
    run(
        &mut vm,
        br#"
local p=newproxy(true);local address=pointer(p)
getmetatable(p).__gc=function(value)
 assert(pointer(value)==address);revived=value;collectgarbage();assert(pointer(value)==address)
end
p=nil;collectgarbage();assert(revived and pointer(revived)==address)
assert(tostring(revived)==tostring(address))
revived=nil;collectgarbage()
"#,
    );
}
