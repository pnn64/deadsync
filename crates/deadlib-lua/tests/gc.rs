use deadlib_lua::{Value, Vm, compile};
use lua_reference::{Lua, MultiValue, Value as Ref};
#[derive(Debug, PartialEq)]
enum Scalar {
    Nil,
    Boolean(bool),
    Number(u64),
    Bytes(Vec<u8>),
}
fn compare(source: &[u8]) {
    let mut vm = Vm::new();
    vm.set_instruction_limit(Some(10_000_000));
    vm.set_output_capture(true);
    let program = compile(source).unwrap();
    let values = vm
        .execute(&program)
        .unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(source)))
        .to_vec();
    let actual: Vec<_> = values
        .into_iter()
        .map(|v| match v {
            Value::Nil => Scalar::Nil,
            Value::Boolean(b) => Scalar::Boolean(b),
            Value::Number(n) => Scalar::Number(n.to_bits()),
            Value::String(id) => Scalar::Bytes(vm.string_bytes(id).unwrap().to_vec()),
            v => panic!(
                "return scalar observations, got {v:?}: {}",
                String::from_utf8_lossy(source)
            ),
        })
        .collect();
    // SAFETY: fixed local programs need the development oracle's debug library.
    let lua = unsafe { Lua::unsafe_new() };
    let expected: Vec<_> = lua
        .load(source)
        .set_name("=(deadlib-lua)")
        .eval::<MultiValue>()
        .unwrap()
        .into_iter()
        .map(|v| match v {
            Ref::Nil => Scalar::Nil,
            Ref::Boolean(b) => Scalar::Boolean(b),
            Ref::Number(n) => Scalar::Number(n.to_bits()),
            Ref::Integer(n) => Scalar::Number((n as f64).to_bits()),
            Ref::String(s) => Scalar::Bytes(s.as_bytes().to_vec()),
            _ => panic!("return reference scalar observations"),
        })
        .collect();
    assert_eq!(actual, expected, "{}", String::from_utf8_lossy(source));
}

#[test]
fn weak_tables_preserve_lua51_strings_cycles_and_strong_values() {
    for source in [
        "collectgarbage('stop');local w=setmetatable({},{__mode='v'});do local a={};a.self=a;w[1]=a end;collectgarbage();return next(w)",
        "collectgarbage('stop');local w=setmetatable({},{__mode='k'});do local a={};w[a]=1 end;collectgarbage();return next(w)",
        "collectgarbage('stop');local w=setmetatable({},{__mode='k'});do local a={};w[a]=a end;collectgarbage();local k,v=next(w);return k==v,type(k)",
        "collectgarbage('stop');local w=setmetatable({},{__mode='k'});do local a={};w[a]=function()return a end end;collectgarbage();local k,v=next(w);return v()==k",
        "collectgarbage('stop');local w=setmetatable({},{__mode='kv'});do local a={};w[a]=a end;collectgarbage();return next(w)",
        "collectgarbage('stop');local w=setmetatable({},{__mode='kv'});for i=1,20 do local s=string.rep('@',i);w[s]=s..'#' end;collectgarbage();local n=0;for k,v in pairs(w)do assert(v==k..'#');n=n+1 end;return n",
        "local w=setmetatable({},{__mode='v'});w[1]=false;w[2]=17;w[3]='abc';collectgarbage();return w[1],w[2],w[3]",
        "collectgarbage('stop');local mt={__mode=17};local w=setmetatable({},mt);do local x={};w[1]=x end;collectgarbage();local a=type(w[1]);mt.__mode='v';collectgarbage();return a,w[1]",
        "collectgarbage('stop');local w=setmetatable({},{__mode='k\\0v'});do local t={};w[1]=t end;collectgarbage();return type(w[1])",
    ] {
        compare(source.as_bytes());
    }
}

#[test]
fn collected_threads_close_surviving_cells_and_preserve_identity() {
    for source in [
        "collectgarbage('stop');local w=setmetatable({},{__mode='v'});local f;do local co=coroutine.wrap(function()local n=10;coroutine.yield(function()n=n+1;return n end)end);f=co();w[1]=co end;collectgarbage();return w[1],f(),f()",
        "collectgarbage('stop');local w=setmetatable({},{__mode='v'});local f;do local co=coroutine.create(function()local t={n=10};f=function()t.n=t.n+1;return t end;error(7)end);coroutine.resume(co);w[1]=co end;collectgarbage();local a=f();return w[1],a==f(),a.n",
        "collectgarbage('stop');local co=coroutine.create(function()local x={n=7};coroutine.yield();return x.n end);coroutine.resume(co);collectgarbage();return coroutine.resume(co)",
        "local co=coroutine.create(function()local x={n=5};local child=coroutine.create(function()collectgarbage();return x.n end);return coroutine.resume(child)end);return coroutine.resume(co)",
        "local co=coroutine.create(function()local f=string.gmatch('ab','.');collectgarbage();coroutine.yield(f());collectgarbage();return f()end);local a,b=coroutine.resume(co);collectgarbage();local c,d=coroutine.resume(co);return a,b,c,d",
    ] {
        compare(source.as_bytes());
    }
}

#[test]
fn collection_inside_callbacks_preserves_arguments_and_continuations() {
    for source in [
        "local t={};return string.gsub('ab','.',function(s)collectgarbage();return s..s end)",
        "local t={7,2,9,1,4};table.sort(t,function(a,b)collectgarbage();return a<b end);return table.concat(t,',')",
        "local a={x=1,y=2};local n=0;table.foreach(a,function(k,v)a[k]=nil;collectgarbage();n=n+v end);return n,next(a)",
        "local t={};local a,b=xpcall(function()local x={};error(t)end,function(e)collectgarbage();return e end);return a,b==t",
        "local t=setmetatable({},{__add=function(a,b)collectgarbage();return a end});return (t+t)==t",
        "local function many(n)local t={};for i=1,n do t[i]={i}end;return unpack(t)end;local function f(...)collectgarbage();local a={...};return #a,a[1][1],a[#a][1]end;return f(many(500))",
    ] {
        compare(source.as_bytes());
    }
}

#[test]
fn collector_controls_and_memory_relationships() {
    compare(
        br#"
        collectgarbage('stop');collectgarbage()
        local old=collectgarbage('setpause',250)
        local mul=collectgarbage('setstepmul',200)
        local before=gcinfo()
        do local t={};for i=1,1000 do t[i]={} end end
        local high=gcinfo();collectgarbage();local after=gcinfo()
        local a,b=collectgarbage('stop'),collectgarbage('restart')
        return old,mul,high>before,after<high,a,b,type(collectgarbage('step',1000000)),collectgarbage('collect\0ignored')
    "#,
    );
}

const PRESSURE_HELPER: &str = r#"
  local weak=setmetatable({},{__mode='v'});local keep={}
  for i=1,1000 do local t={};weak[i]=t;keep[i]=t end
  function pressure()
    keep=nil
    for i=1,10000 do local t={}end
    return next(weak)==nil
  end
"#;
fn control_compare(body: &str) {
    compare(format!("collectgarbage('restart');collectgarbage('setpause',200);collectgarbage();collectgarbage('stop');{PRESSURE_HELPER}{body}").as_bytes());
}

#[test]
fn explicit_collections_and_steps_resume_automatic_collection() {
    for operation in [
        "",
        "collectgarbage()",
        "collectgarbage(nil)",
        "collectgarbage('collect\\0ignored')",
        "collectgarbage('step',0)",
        "collectgarbage('step',1)",
        "collectgarbage('step',1000000)",
        "collectgarbage('restart')",
        "collectgarbage('collect');collectgarbage('stop')",
        "collectgarbage('step',0);collectgarbage('stop')",
    ] {
        control_compare(&format!("{operation};return pressure()"));
    }
}

#[test]
fn pause_changes_wait_for_a_cycle_and_restart_discards_the_old_threshold() {
    for operation in [
        "collectgarbage();collectgarbage('setpause',2000000000)",
        "collectgarbage('setpause',2000000000);collectgarbage();collectgarbage('stop');collectgarbage('restart')",
        "collectgarbage('setpause',2000000000);collectgarbage('stop')",
    ] {
        control_compare(&format!("{operation};return pressure()"));
    }
}

#[test]
fn successful_finalizers_restore_controls_and_errors_preserve_the_last_change() {
    for operation in [
        "",
        "collectgarbage('stop')",
        "collectgarbage('restart')",
        "collectgarbage('stop');collectgarbage();collectgarbage('stop')",
        "collectgarbage('setpause',2000000000)",
        "local q=newproxy(true);getmetatable(q).__gc=function()collectgarbage('stop');error(tag,0)end;q=nil;local ok,e=pcall(collectgarbage);assert(not ok and e==tag)",
        "error(tag,0)",
        "collectgarbage('stop');error(tag,0)",
        "collectgarbage('restart');error(tag,0)",
        "collectgarbage('step',0);error(tag,0)",
    ] {
        control_compare(&format!(
            r#"
          local tag={{}};local hit=false;local p=newproxy(true)
          getmetatable(p).__gc=function()hit=true;{operation} end
          p=nil;local ok,e=pcall(collectgarbage)
          return hit,ok,e==tag,pressure()
        "#
        ));
    }
}

#[test]
fn successful_automatic_finalizers_do_not_leave_collection_stopped() {
    control_compare(
        r#"
      local hit=false;local p=newproxy(true)
      getmetatable(p).__gc=function()hit=true;collectgarbage('stop')end
      p=nil;collectgarbage('restart')
      for i=1,20000 do local t={};if hit then break end end
      return hit,pressure()
    "#,
    );
}

#[test]
fn finalizer_control_checkpoints_survive_child_coroutine_suspension() {
    control_compare(
        r#"
      local co;local p=newproxy(true)
      getmetatable(p).__gc=function()
        co=coroutine.create(function()
          collectgarbage('stop');coroutine.yield(7)
          collectgarbage('restart');return 11
        end)
        local ok,n=coroutine.resume(co);assert(ok and n==7)
      end
      p=nil;collectgarbage();local automatic=pressure()
      local ok,n=coroutine.resume(co)
      return automatic,ok,n,coroutine.status(co)
    "#,
    );
    control_compare(
        r#"
      local tag={};local p=newproxy(true)
      getmetatable(p).__gc=function()error(tag,0)end;p=nil
      local ok,e=xpcall(collectgarbage,function(e)collectgarbage('stop');return e end)
      return ok,e==tag,pressure()
    "#,
    );
}

#[test]
fn collection_hook_controls_run_before_and_after_the_native_cycle() {
    for event in ["call", "return"] {
        control_compare(&format!(
            r#"
          local hit=0
          debug.sethook(function(event)
            if event=='{event}' and debug.getinfo(2,'f').func==collectgarbage then
              hit=hit+1;collectgarbage('stop')
            end
          end,'cr')
          collectgarbage();debug.sethook()
          return hit,pressure()
        "#
        ));
    }
}

#[test]
fn generated_collector_control_sequences_match_reference_reachability() {
    let controls = [
        "collectgarbage('stop')",
        "collectgarbage('restart')",
        "collectgarbage('collect')",
        "collectgarbage('step',0)",
        "collectgarbage('step',1000000)",
        "collectgarbage('count')",
        "collectgarbage('setpause',100)",
        "collectgarbage('setpause',400)",
    ];
    let mut state = 527u32;
    for _ in 0..64 {
        let mut body = String::new();
        for _ in 0..5 {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            body.push_str(controls[(state >> 16) as usize % controls.len()]);
            body.push(';');
        }
        body.push_str("return pressure()");
        control_compare(&body);
    }
}

#[test]
fn invalid_collector_arguments_do_not_resume_a_stopped_collector() {
    for operation in [
        "false",
        "'bad'",
        "'collect',{}",
        "'restart',{}",
        "'step',{}",
        "'count',false",
    ] {
        control_compare(&format!(
            "local ok,e=pcall(collectgarbage,{operation});return ok,type(e),pressure()"
        ));
    }
}

#[test]
fn host_full_collection_uses_the_reference_restart_and_finalizer_controls() {
    for operation in [
        None,
        Some(""),
        Some("collectgarbage('stop')"),
        Some("error('tag',0)"),
        Some("collectgarbage('stop');error('tag',0)"),
    ] {
        let mut setup = "collectgarbage('stop');".to_owned() + PRESSURE_HELPER;
        if let Some(operation) = operation {
            setup.push_str(&format!(
                "local p=newproxy(true);getmetatable(p).__gc=function(){operation} end;p=nil"
            ));
        }
        let mut vm = Vm::new();
        vm.execute(&compile(setup.as_bytes()).unwrap()).unwrap();
        let actual_error = vm.collect_garbage().is_err();
        let lua = Lua::new();
        lua.load(setup.as_bytes()).exec().unwrap();
        let reference_error = lua.gc_collect().is_err();
        assert_eq!(actual_error, reference_error, "{setup}");
        let source = b"return pressure()";
        let expected: bool = lua.load(source.as_slice()).eval().unwrap();
        assert_eq!(
            vm.execute(&compile(source).unwrap()).unwrap(),
            &[Value::Boolean(expected)],
            "{setup}"
        );
    }
}

#[test]
fn roots_reject_release_and_foreign_vm_and_keep_values_alive() {
    let mut vm = Vm::new();
    let foreign = Vm::new();
    let value = vm.execute(&compile(b"return {n=7}").unwrap()).unwrap()[0];
    let root = vm.pin(value).unwrap();
    vm.execute(&compile(b"return nil").unwrap()).unwrap();
    vm.collect_garbage().unwrap();
    assert!(foreign.rooted(root).is_err());
    assert_eq!(vm.rooted(root).unwrap(), value);
    vm.set_global(b"t", value).unwrap();
    assert_eq!(
        vm.execute(&compile(b"return t.n").unwrap()).unwrap(),
        &[Value::Number(7.0)]
    );
    vm.set_global(b"t", Value::Nil).unwrap();
    vm.unpin(root).unwrap();
    assert!(vm.unpin(root).is_err());
    assert!(vm.rooted(root).is_err());
    vm.collect_garbage().unwrap();
    assert!(vm.set_global(b"stale", value).is_err());
    let next = vm.execute(&compile(b"return {}").unwrap()).unwrap()[0];
    assert_ne!(next, value);
    let text = vm.intern(b"unrooted temporary string");
    let Value::String(id) = text else {
        unreachable!()
    };
    vm.execute(&compile(b"return nil").unwrap()).unwrap();
    vm.collect_garbage().unwrap();
    assert_eq!(vm.string_bytes(id), None);
    let r2 = vm.pin(Value::Number(3.0)).unwrap();
    assert_ne!(root, r2);
    vm.unpin(r2).unwrap();
}

#[test]
fn repeated_collection_reclaims_programs_and_bounds_object_slots() {
    let mut vm = Vm::new();
    let program=compile(b"for i=1,1000 do local t={i};local s='item'..i;local f=loadstring('return '..i);assert(f()==i) end;return nil").unwrap();
    let mut maximum = 0;
    for _ in 0..12 {
        vm.execute(&program).unwrap();
        let stats = vm.collect_garbage().unwrap();
        maximum = maximum.max(stats.reserved_bytes);
        assert!(stats.tables < 20);
        assert!(stats.functions < 10);
        assert!(stats.programs < 10);
    }
    assert!(
        maximum < 2_000_000,
        "reusable storage grows with peak live data, not total churn: {maximum}"
    );
    assert!(vm.heap_stats().reclaimed_objects > 20_000);
}

#[test]
fn host_program_bindings_survive_collection_without_pinning_loadstring_caches() {
    let program=compile(b"if getfenv(1)==_G then setfenv(1,setmetatable({},{__index=_G})) end;n=(n or 0)+1;return n").unwrap();
    let mut a = Vm::new();
    let mut b = Vm::new();
    assert_eq!(a.execute(&program).unwrap(), &[Value::Number(1.0)]);
    assert_eq!(b.execute(&program).unwrap(), &[Value::Number(1.0)]);
    a.collect_garbage().unwrap();
    b.collect_garbage().unwrap();
    assert_eq!(a.execute(&program).unwrap(), &[Value::Number(2.0)]);
    assert_eq!(b.execute(&program).unwrap(), &[Value::Number(2.0)]);
    drop(program);
    a.execute(&compile(b"return nil").unwrap()).unwrap();
    b.execute(&compile(b"return nil").unwrap()).unwrap();
    assert_eq!(a.collect_garbage().unwrap().programs, 0);
    assert_eq!(b.collect_garbage().unwrap().programs, 0);
}

#[test]
fn automatic_collection_handles_opcode_only_churn_and_large_live_arrays() {
    let mut vm = Vm::new();
    let program = compile(b"local s='';for i=1,5000 do s=s..'x' end;return #s").unwrap();
    assert_eq!(vm.execute(&program).unwrap(), &[Value::Number(5000.0)]);
    let stats = vm.heap_stats();
    assert!(stats.collections > 10);
    assert!(stats.strings < 500);
    assert!(stats.reserved_bytes < 2_000_000);
    let program = compile(b"local t={};for i=1,10000 do t[i]={i} end;local n=0;for i=1,#t do n=n+t[i][1] end;return n").unwrap();
    assert_eq!(
        vm.execute(&program).unwrap(),
        &[Value::Number(50_005_000.0)]
    );
    assert!(vm.heap_stats().tables >= 10_000);
    assert!(vm.collect_garbage().unwrap().tables < 20);
}

#[test]
fn native_handles_cannot_restore_collected_environments() {
    let mut vm = Vm::new();
    let native = vm.global(b"assert");
    vm.execute(&compile(b"local f=assert;debug.setfenv(f,{});assert=nil;return nil").unwrap())
        .unwrap();
    vm.collect_garbage().unwrap();
    assert!(vm.pin(native).is_err());
    assert!(vm.set_global(b"stale", native).is_err());
}

#[test]
fn deep_graphs_and_high_register_roots_survive_collection() {
    compare(b"local t={};for i=1,20000 do t={next=t} end;collectgarbage();local n=0;while t do n=n+1;t=t.next end;return n");
    let mut source = String::new();
    for i in 1..=150 {
        source.push_str(&format!("local t{i}={{{i}}};"));
    }
    source.push_str("collectgarbage();local sum=0;");
    for i in 1..=150 {
        source.push_str(&format!("sum=sum+t{i}[1];"));
    }
    source.push_str("return sum");
    compare(source.as_bytes());
}
