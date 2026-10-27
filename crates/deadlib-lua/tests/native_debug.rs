use deadlib_lua::{Error, GcOperation, HostCall, NativeDebugFrame, Value, Vm, compile};

fn run(callback: deadlib_lua::HostFunction, source: &[u8]) -> Vm {
    let mut vm = Vm::new();
    let host = vm.new_host_function(callback, &[]).unwrap();
    vm.set_global(b"host", host).unwrap();
    vm.execute(&compile(source).unwrap()).unwrap();
    vm
}

#[test]
fn options_preserve_unrequested_fields_and_apply_pushes_even_when_invalid() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut s = call.native_stack()?;
        let thread = s.thread_id();
        let current = s.get_stack(thread, 0)?.unwrap();
        let empty = s.get_info(current, b"")?;
        assert!(empty.valid && empty.source.is_none() && empty.name.is_none());
        assert!(empty.current_line.is_none() && empty.upvalues.is_none());
        assert_eq!(s.top(), 1);
        let original = s.value(1).unwrap();
        s.push(original)?;
        let arbitrary = NativeDebugFrame::from_index(thread, -999);
        let info = s.get_info(arbitrary, b">?LLffuSn")?;
        assert!(!info.valid);
        assert_eq!(info.upvalues, Some(1));
        assert_eq!(info.what, Some(b"Lua\0".as_slice()));
        assert!(info.name.unwrap().is_none());
        assert_eq!(s.value(2), Some(original));
        assert!(matches!(s.value(3), Some(Value::Table(_))));
        assert_eq!(s.top(), 3);
        s.set_top(1)?;
        let info = s.get_info(current, b"u\0?fL")?;
        assert!(info.valid && info.upvalues == Some(0));
        assert_eq!(s.top(), 1);
        assert!(s.get_info(arbitrary, b"S").is_err());
        assert_eq!(s.top(), 1);
        assert!(s.get_stack(thread, i32::MAX)?.is_none());
        let tail = s.get_stack(thread, -1)?.unwrap();
        let info = s.get_info(tail, b"?Lf")?;
        assert!(info.valid);
        assert_eq!(info.what, Some(b"tail\0".as_slice()));
        assert_eq!(info.current_line, Some(-1));
        assert_eq!(s.value(2), Some(Value::Nil));
        assert_eq!(s.value(3), Some(Value::Nil));
        s.finish(0)
    }
    run(callback, b"local up=17;host(function()return up end)");
}

#[test]
fn native_locals_observe_pushes_aliases_and_unconditional_setter_pop() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut s = call.native_stack()?;
        let current = s.get_stack(s.thread_id(), 0)?.unwrap();
        assert!(s.get_local(current, 0)?.is_none());
        assert!(s.get_local(current, 3)?.is_none());
        let name = s.get_local(current, 1)?.unwrap();
        assert_eq!(s.debug_bytes(name), Some(b"(*temporary)\0".as_slice()));
        assert_eq!(s.value(-1), Some(Value::Number(17.)));
        s.push_integer(29)?;
        assert!(s.set_local(current, 1)?.is_some());
        assert_eq!(s.value(1), Some(Value::Number(29.)));
        assert_eq!(s.top(), 3);
        assert!(s.set_local(current, 3)?.is_some());
        assert_eq!(s.top(), 2);
        s.push_integer(99)?;
        assert!(s.set_local(current, -1)?.is_none());
        assert_eq!(s.top(), 2);
        s.set_top(8000)?;
        assert!(s.get_local(current, 1).is_err());
        assert_eq!(s.top(), 8000);
        s.set_top(0)?;
        assert!(s.set_local(current, 1).is_err());
        assert!(s.get_info(current, b">S").is_err());
        s.finish(0)
    }
    run(callback, b"host(17,false)");
}

#[test]
fn fresh_thread_base_locals_and_tail_info_use_the_selected_stack() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut s = call.native_stack()?;
        let co = s.new_thread()?;
        assert!(s.get_stack(co, 0)?.is_none());
        let base = s.get_stack(co, -1)?.unwrap();
        assert!(s.get_local(base, 1)?.is_none());
        s.thread_stack(co)?.push(Value::Number(17.))?;
        assert!(s.get_local(base, 1)?.is_some());
        assert_eq!(s.thread_stack(co)?.value(2), Some(Value::Number(17.)));
        s.thread_stack(co)?.push(Value::Number(29.))?;
        assert!(s.set_local(base, 1)?.is_some());
        assert_eq!(s.thread_stack(co)?.value(1), Some(Value::Number(29.)));
        let info = s.get_info(base, b"?fL")?;
        assert!(info.valid && info.upvalues == Some(0));
        assert_eq!(s.thread_stack(co)?.top(), 4);
        assert_eq!(s.top(), 1);
        s.set_top(0)?;
        s.gc(GcOperation::Collect)?;
        assert!(s.get_stack(co, 0).is_err());
        assert!(s.get_info(base, b"S").is_err());
        s.finish(0)
    }
    run(callback, b"host()");
}

#[test]
fn debug_strings_share_interned_storage_survive_collection_and_release_with_program() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut s = call.native_stack()?;
        let f = s.value(1).unwrap();
        s.push(f)?;
        let record = NativeDebugFrame::from_index(s.thread_id(), 0);
        let info = s.get_info(record, b">S")?;
        let name = info.source.unwrap();
        let bytes = s.debug_bytes(name).unwrap().to_vec();
        let pointer = s.debug_bytes(name).unwrap().as_ptr();
        let value = s.intern(&bytes[..bytes.len() - 1]);
        s.push(value)?;
        assert_eq!(s.to_bytes(-1)?.unwrap().as_ptr(), pointer);
        s.gc(GcOperation::Collect)?;
        assert_eq!(s.debug_bytes(name).unwrap().as_ptr(), pointer);
        assert_eq!(s.debug_bytes(name).unwrap(), bytes);
        s.finish(0)
    }
    let mut vm = run(
        callback,
        br#"
for i=1,100 do
 local f=assert(loadstring('return '..i,'=native-debug-'..i));host(f)
end
"#,
    );
    vm.collect_garbage().unwrap();
    vm.collect_garbage().unwrap();
    assert_eq!(vm.heap_stats().programs, 0);
}
