use deadlib_lua::{Value, Vm, compile_named};
use lua_reference::Lua;

#[derive(Debug, PartialEq)]
enum Scalar {
    Nil,
    Boolean(bool),
    Number(u64),
    Bytes(Vec<u8>),
}
struct Oracle {
    vm: Vm,
    lua: Lua,
}
impl Oracle {
    fn new() -> Self {
        // SAFETY: fixed developer programs exercise the official protected VM.
        let mut vm = Vm::new();
        vm.set_instruction_limit(Some(5_000_000));
        Self {
            vm,
            lua: unsafe { Lua::unsafe_new() },
        }
    }
    fn check(&mut self, source: &str) {
        let program = compile_named(source.as_bytes(), b"=call-limits").unwrap();
        let values = self.vm.execute(&program).unwrap().to_vec();
        let actual: Vec<_> = values
            .into_iter()
            .map(|v| match v {
                Value::Nil => Scalar::Nil,
                Value::Boolean(b) => Scalar::Boolean(b),
                Value::Number(n) => Scalar::Number(n.to_bits()),
                Value::String(id) => Scalar::Bytes(self.vm.string_bytes(id).unwrap().to_vec()),
                other => panic!("unexpected {other:?}"),
            })
            .collect();
        let values: lua_reference::MultiValue = self
            .lua
            .load(source.as_bytes())
            .set_name("=call-limits")
            .eval()
            .unwrap();
        let expected: Vec<_> = values
            .into_iter()
            .map(|v| match v {
                lua_reference::Value::Nil => Scalar::Nil,
                lua_reference::Value::Boolean(b) => Scalar::Boolean(b),
                lua_reference::Value::Number(n) => Scalar::Number(n.to_bits()),
                lua_reference::Value::Integer(n) => Scalar::Number((n as f64).to_bits()),
                lua_reference::Value::String(s) => Scalar::Bytes(s.as_bytes().to_vec()),
                other => panic!("unexpected reference {other:?}"),
            })
            .collect();
        assert_eq!(actual, expected, "{source}");
    }
}

#[test]
fn first_and_repeated_overflows_preserve_depth_messages_and_recovery() {
    Oracle::new().check("collectgarbage('stop');local n=0;local function f()n=n+1;f()end;local out={};for i=1,3 do n=0;local ok,e=pcall(f);out[#out+1]=ok;out[#out+1]=n;out[#out+1]=e end;return unpack(out)");
}

#[test]
fn error_handlers_use_the_overflow_reserve_and_catch_nested_exhaustion() {
    Oracle::new().check("collectgarbage('stop');local n,m=0,0;local function f()n=n+1;f()end;local function handler(e)local function g()m=m+1;g()end;local ok,inner=pcall(g);return e..'|'..inner end;local ok,e=xpcall(f,handler);return ok,n,m,e");
}

#[test]
fn overflow_while_handling_overflow_returns_the_reference_error_object() {
    Oracle::new().check("collectgarbage('stop');local n,m=0,0;local function f()n=n+1;f()end;local function handler(e)m=m+1;handler(e)end;local ok,e=xpcall(f,handler);return ok,n,m,e");
}

#[test]
fn deep_error_tracebacks_run_before_the_faulting_frames_are_unwound() {
    Oracle::new().check("collectgarbage('stop');local n=0;local function f()n=n+1;f()end;local ok,e=xpcall(f,debug.traceback);return ok,n,e");
}

#[test]
fn explicit_collection_shrinks_logical_call_storage_like_lua51() {
    let mut oracle = Oracle::new();
    for collections in [0, 1, 2, 3, 4, 8] {
        oracle.check(&format!("collectgarbage('stop');local n=0;local function f()n=n+1;f()end;pcall(f);for i=1,{collections} do collectgarbage('collect');collectgarbage('stop')end;n=0;local ok,e=pcall(f);return ok,n,e"));
    }
}

#[test]
fn coroutine_overflow_and_yielded_recovery_keep_independent_capacities() {
    let mut oracle = Oracle::new();
    oracle.check("collectgarbage('stop');local n=0;local co=coroutine.create(function()local function f()n=n+1;f()end;f()end);local ok,e=coroutine.resume(co);return ok,n,e,coroutine.status(co),debug.getinfo(co,0,'S').what");
    oracle.check("collectgarbage('stop');local co=coroutine.create(function()local n=0;local function f()n=n+1;f()end;for i=1,2 do n=0;local ok,e=pcall(f);coroutine.yield(ok,n,e)end end);local a,b,c,d=coroutine.resume(co);local e,f,g,h=coroutine.resume(co);return a,b,c,d,e,f,g,h");
}

#[test]
fn unwinding_closes_surviving_captured_locals_before_collection() {
    Oracle::new().check("collectgarbage('stop');local a,b;local function f(n)local x=n;if n==100 then a=function()return x end elseif n==16000 then b=function()return x end end;f(n+1)end;local ok,e=pcall(f,1);collectgarbage('collect');return ok,e,a(),b()");
}

#[test]
fn tail_recursion_reuses_frames_without_exhausting_the_call_reserve() {
    Oracle::new().check("collectgarbage('stop');local function f(n)if n==0 then return 37 end;return f(n-1)end;return f(50000)");
}

#[test]
fn near_capacity_calls_preserve_precedence_and_native_error_locations() {
    for limit in [16379, 16380, 16381, 16382] {
        for action in [
            "math.sin(0)",
            "pcall(function()return 1 end)",
            "error('boom')",
            "(function()return 8 end)()",
            "bad()",
        ] {
            Oracle::new().check(&format!("collectgarbage('stop');local bad=false;local function f(n)if n=={limit} then return {action} end;local a,b,c=f(n+1);return a,b,c end;return pcall(f,1)"));
        }
    }
}

#[test]
fn handlers_can_reenter_after_a_handler_error_or_failed_handler_entry() {
    let mut oracle = Oracle::new();
    oracle.check("collectgarbage('stop');local n=0;local ok,e=xpcall(function()error('first',0)end,function(e)n=n+1;if n==1 then error('second',0)end;return e end);return ok,n,e");
    Oracle::new().check("collectgarbage('stop');local n=0;local bad=false;local function f()n=n+1;if n==16381 then bad()end;f()end;local ok,e=xpcall(f,function(e)return 'H|'..e end);return ok,n,e");
}

#[test]
fn recursive_runtime_error_handlers_reach_the_c_call_emergency_limit() {
    Oracle::new().check("collectgarbage('stop');local n=0;local ok,e=xpcall(function()error('first',0)end,function(e)n=n+1;error('again',0)end);return ok,n,e");
}

#[test]
fn nested_protected_calls_obey_the_reference_c_boundary_depth() {
    Oracle::new().check("collectgarbage('stop');local n,last=0;local function f()n=n+1;local ok,e=pcall(f);if not ok then last=e end end;f();return n,last");
    Oracle::new().check("collectgarbage('stop');local n,last=0;local function f()n=n+1;local ok,e=xpcall(f,function(e)return e end);if not ok then last=e end end;f();return n,last");
}

#[test]
fn emergency_error_handlers_can_still_call_lua_and_native_callbacks() {
    for action in [
        "setmetatable({},{__index=function()return 'handled'end}).x",
        "tostring(setmetatable({},{__tostring=function()return 'handled'end}))",
        "require('limit-provider')",
        "(function()local v;table.foreach({1},function()v='handled'end);return v end)()",
    ] {
        Oracle::new().check(&format!("collectgarbage('stop');package.preload['limit-provider']=function()return 'handled'end;local n=0;local ok,e=xpcall(function()error('first',0)end,function(e)n=n+1;if n<200 then error('again',0)end;return {action} end);return ok,n,e"));
    }
}

#[test]
fn host_overflow_is_runtime_and_later_executions_keep_the_recovered_limit() {
    let mut oracle = Oracle::new();
    let source = "collectgarbage('stop');local function f()f()end;f()";
    let program = compile_named(source.as_bytes(), b"=call-limits").unwrap();
    let error = oracle.vm.execute(&program).unwrap_err();
    assert_eq!(error.kind(), deadlib_lua::ErrorKind::Runtime);
    assert_eq!(error.message(), "stack overflow");
    oracle
        .lua
        .load("collectgarbage('stop');local function f()f()end;pcall(f)")
        .exec()
        .unwrap();
    oracle.check("collectgarbage('stop');local n=0;local function f()n=n+1;f()end;local ok,e=pcall(f);return ok,n,e");
}

#[test]
fn instruction_abort_in_an_overflow_handler_releases_the_reserved_context() {
    let mut oracle = Oracle::new();
    oracle.vm.set_instruction_limit(Some(200_000));
    let source =
        "collectgarbage('stop');local function f()f()end;xpcall(f,function()while true do end end)";
    let error = oracle
        .vm
        .execute(&compile_named(source.as_bytes(), b"=call-limits").unwrap())
        .unwrap_err();
    assert_eq!(error.kind(), deadlib_lua::ErrorKind::Limit);
    assert_eq!(error.message(), "instruction limit exceeded");
    oracle.vm.set_instruction_limit(Some(5_000_000));
    oracle
        .lua
        .load("collectgarbage('stop');local function f()f()end;pcall(f)")
        .exec()
        .unwrap();
    oracle.check("collectgarbage('stop');local n=0;local function f()n=n+1;f()end;local ok,e=pcall(f);return ok,n,e");
}

#[test]
fn call_hooks_count_real_entries_and_preserve_tail_frame_reuse() {
    Oracle::new().check("collectgarbage('stop');local n,m=0,0;debug.sethook(function()m=m+1 end,'c');local function f()n=n+1;f()end;local ok,e=pcall(f);debug.sethook();return ok,n,m,e");
    Oracle::new().check("collectgarbage('stop');local m=0;debug.sethook(function()m=m+1 end,'c');local function f(n)if n==0 then return 37 end;return f(n-1)end;local result=f(50000);debug.sethook();return result,m");
}
