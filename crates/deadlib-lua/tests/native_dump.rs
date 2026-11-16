use deadlib_lua::{Error, ErrorKind, HostCall, Value, Vm, compile};

fn run(vm: &mut Vm, source: &[u8]) -> Vec<Value> {
    vm.execute(&compile(source).unwrap()).unwrap().to_vec()
}
fn install(vm: &mut Vm, callback: fn(&mut HostCall<'_>) -> Result<(), Error>) {
    let value = vm.new_host_function(callback, &[]).unwrap();
    vm.set_global(b"host", value).unwrap();
}

// The 5.1 C API has no strip flag. Remove only line vectors from a valid C
// fixture, then let its own loader/writer establish the expected empty blocks.
fn without_line_info(bytes: &[u8]) -> Vec<u8> {
    struct Input<'a> {
        bytes: &'a [u8],
        at: usize,
        width: usize,
    }
    impl Input<'_> {
        fn copy(&mut self, out: &mut Vec<u8>, n: usize) {
            out.extend_from_slice(&self.bytes[self.at..self.at + n]);
            self.at += n;
        }
        fn count(&mut self) -> usize {
            let n = u32::from_ne_bytes(self.bytes[self.at..self.at + 4].try_into().unwrap());
            self.at += 4;
            n as usize
        }
        fn string(&mut self, out: &mut Vec<u8>) {
            let n = if self.width == 8 {
                u64::from_ne_bytes(self.bytes[self.at..self.at + 8].try_into().unwrap()) as usize
            } else {
                u32::from_ne_bytes(self.bytes[self.at..self.at + 4].try_into().unwrap()) as usize
            };
            self.copy(out, self.width + n);
        }
        fn function(&mut self, out: &mut Vec<u8>, depth: usize) {
            assert!(depth < 10);
            self.string(out);
            self.copy(out, 12);
            let n = self.count();
            out.extend_from_slice(&(n as u32).to_ne_bytes());
            self.copy(out, 4 * n);
            let n = self.count();
            out.extend_from_slice(&(n as u32).to_ne_bytes());
            for _ in 0..n {
                let tag = self.bytes[self.at];
                self.copy(out, 1);
                match tag {
                    0 => {}
                    1 => self.copy(out, 1),
                    3 => self.copy(out, 8),
                    4 => self.string(out),
                    _ => panic!("invalid fixture constant"),
                }
            }
            let n = self.count();
            out.extend_from_slice(&(n as u32).to_ne_bytes());
            for _ in 0..n {
                self.function(out, depth + 1);
            }
            let n = self.count();
            self.at += 4 * n;
            out.extend_from_slice(&0u32.to_ne_bytes());
            let n = self.count();
            out.extend_from_slice(&(n as u32).to_ne_bytes());
            for _ in 0..n {
                self.string(out);
                self.copy(out, 8);
            }
            let n = self.count();
            out.extend_from_slice(&(n as u32).to_ne_bytes());
            for _ in 0..n {
                self.string(out);
            }
        }
    }
    let mut input = Input {
        bytes,
        at: 12,
        width: bytes[8] as usize,
    };
    let mut out = bytes[..12].to_vec();
    input.function(&mut out, 0);
    assert_eq!(input.at, bytes.len());
    out
}

#[test]
fn imported_writer_blocks_match_the_official_c_writer_including_empty_vectors() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let expected = call.function_data::<Vec<Vec<u8>>>().unwrap().clone();
        let mut stack = call.native_stack()?;
        let function = stack.value(1);
        let top = stack.top();
        let mut index = 0;
        assert_eq!(
            stack.dump(|stack, bytes| {
                assert_eq!(stack.top(), top);
                assert_eq!(stack.value(1), function);
                assert_eq!(bytes, expected[index]);
                index += 1;
                Ok(0)
            })?,
            0
        );
        assert_eq!(index, expected.len());
        stack.finish(1)
    }

    for source in [
        "return",
        "local a,b=17,'a\\000z';return function(x,...)return a,b,x,... end",
        "local t={};for i=1,17 do t[i]=i end;return t",
    ] {
        for empty_lines in [false, true] {
            let mut binary = reference_test::native_chunk(
                "tests/native_dump.rs::imported_writer_blocks_match_the_official_c_writer_including_empty_vectors::chunk0",
                &(source),
            );
            if empty_lines {
                binary = without_line_info(&binary);
            }

            let expected = reference_test::native_writer_blocks(
                "tests/native_dump.rs::reference_blocks::blocks",
                &(source),
            );
            if empty_lines {
                assert!(expected.iter().any(Vec::is_empty));
            }
            let mut vm = Vm::new();
            let host = vm
                .new_host_function_with_data(callback, &[], expected)
                .unwrap();
            vm.set_global(b"host", host).unwrap();
            let input = vm.intern(&binary);
            vm.set_global(b"binary", input).unwrap();
            run(
                &mut vm,
                b"local f=assert(loadstring(binary,'=writer input'));assert(host(f)==f)",
            );
        }
    }
}

#[test]
fn every_writer_failure_boundary_returns_the_original_signed_status() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let expected = call.function_data::<Vec<Vec<u8>>>().unwrap().clone();
        let mut stack = call.native_stack()?;
        let stop = stack.to_integer(2).unwrap() as usize;
        let status = stack.to_integer(3).unwrap() as i32;
        stack.push(stack.value(1).unwrap())?;
        let mut count = 0;
        let result = stack.dump(|stack, bytes| {
            assert_eq!(stack.top(), 4);
            assert_eq!(bytes, expected[count]);
            count += 1;
            Ok(if count == stop { status } else { 0 })
        })?;
        assert_eq!(stack.top(), 4);
        assert_eq!(stack.value(1), stack.value(-1));
        stack.push_integer(result as isize)?;
        stack.push_integer(count as isize)?;
        stack.finish(2)
    }

    let binary = reference_test::native_chunk(
        "tests/native_dump.rs::every_writer_failure_boundary_returns_the_original_signed_status::chunk0",
        &("local a=17;return function()return a,'binary\\000text' end"),
    );
    let expected = reference_test::native_writer_blocks(
        "tests/native_dump.rs::reference_blocks::blocks",
        &("local a=17;return function()return a,'binary\\000text' end"),
    );
    let count = expected.len();
    let mut vm = Vm::new();
    let host = vm
        .new_host_function_with_data(callback, &[], expected)
        .unwrap();
    vm.set_global(b"host", host).unwrap();
    let input = vm.intern(&binary);
    vm.set_global(b"binary", input).unwrap();
    run(&mut vm,format!("local f=assert(loadstring(binary));for stop=1,{count} do local s,n=host(f,stop,17);assert(s==17 and n==stop)end;for _,s in ipairs{{-2147483648,-1,1,2147483647}}do local v,n=host(f,2,s);assert(v==s and n==2)end").as_bytes());
}

#[test]
fn writer_reentry_collection_nested_dumps_and_caller_mutation_preserve_bytes() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut stack = call.native_stack()?;
        let visit = stack.value(2).unwrap();
        stack.push(stack.value(1).unwrap())?;
        let mut bytes = Vec::new();
        let mut calls = 0;
        assert_eq!(
            stack.dump(|stack, block| {
                calls += 1;
                bytes.extend_from_slice(block);
                stack.push(visit)?;
                stack.push_integer(calls)?;
                stack.call_unprotected(1, Some(0))?;
                assert_eq!(stack.top(), 3);
                Ok(0)
            })?,
            0
        );
        let bytes = stack.intern(&bytes);
        stack.push(bytes)?;
        stack.finish(1)
    }
    let mut vm = Vm::new();
    install(&mut vm, callback);
    run(&mut vm,br#"
local f=function()return 42 end
local expected=string.dump(f)
local bytes=host(f,function(i)
 assert(debug.getinfo(2,'f').func==host)
 assert(debug.setlocal(2,1,91));collectgarbage()
 local co=coroutine.create(function()coroutine.yield(17)end)
 assert(coroutine.resume(co))
 if i==1 then local b=host(function()return 7 end,function()end);assert(assert(loadstring(b))()==7)end
end)
assert(bytes==expected and assert(loadstring(bytes))()==42)
"#);
}

#[test]
fn writer_failures_keep_fault_frames_and_cannot_be_suppressed() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut stack = call.native_stack()?;
        let visit = stack.value(2).unwrap();
        stack.push(stack.value(1).unwrap())?;
        let error = stack
            .dump(|stack, _| {
                stack.push(visit)?;
                let _ = stack.call_unprotected(0, Some(0));
                Ok(0)
            })
            .unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Runtime);
        assert_eq!(
            stack.load_buffer(b"return 42", None).unwrap_err().value(),
            error.value()
        );
        Ok(())
    }
    let mut vm = Vm::new();
    install(&mut vm, callback);
    run(&mut vm,br#"
local f=function()return 42 end;local marker={};local retained=false
local ok,e=xpcall(function()host(f,function()local captured=marker;error(captured)end)end,function(e)
 for level=2,12 do
  local info=debug.getinfo(level,'f');if not info then break end
  if info.func==host then retained=select(2,debug.getlocal(level,1))==f end
 end
 return e
end)
assert(not ok and e==marker and retained)
"#);
    assert_eq!(run(&mut vm, b"return 42"), [Value::Number(42.)]);
}

#[test]
fn native_writer_limits_escape_then_release_the_serialization_and_child_state() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut stack = call.native_stack()?;
        let visit = stack.value(2).unwrap();
        stack.push(stack.value(1).unwrap())?;
        stack.dump(|stack, _| {
            stack.push(visit)?;
            stack.call_unprotected(0, Some(0))?;
            Ok(0)
        })?;
        stack.finish(0)
    }
    let mut vm = Vm::new();
    install(&mut vm, callback);
    vm.set_instruction_limit(Some(1000));
    let error = vm
        .execute(
            &compile(b"host(function()return 42 end,function()while true do end end)").unwrap(),
        )
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Limit);
    vm.set_instruction_limit(None);
    assert_eq!(run(&mut vm, b"return 42"), [Value::Number(42.)]);
    vm.collect_garbage().unwrap();
}

#[test]
fn native_and_nonfunction_values_return_one_without_calling_the_writer() {
    fn callback(call: &mut HostCall<'_>) -> Result<(), Error> {
        let mut stack = call.native_stack()?;
        let before = stack.top();
        let original = stack.value(-1);
        assert_eq!(
            stack.dump(|_, _| panic!("writer called for a non-Lua value"))?,
            1
        );
        assert_eq!(stack.top(), before);
        assert_eq!(stack.value(-1), original);
        stack.finish(1)
    }
    let mut vm = Vm::new();
    install(&mut vm, callback);
    run(&mut vm,b"for _,v in ipairs{false,17,'bytes',{},print,newproxy(),coroutine.create(function()end)}do assert(host(v)==v)end;assert(host(nil)==nil)");
}

#[allow(dead_code)]
#[path = "../src/reference_test.rs"]
mod reference_test;
