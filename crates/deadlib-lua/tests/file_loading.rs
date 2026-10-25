use deadlib_lua::{Value, Vm, compile};
use lua_reference::{Lua, MultiValue, Value as Ref};

#[derive(Debug, PartialEq)]
enum Scalar {
    Nil,
    Boolean(bool),
    Number(u64),
    Bytes(Vec<u8>),
}

struct Fixture(std::path::PathBuf);
impl Fixture {
    fn new(bytes: &[u8]) -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/lua-load-fixtures");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!(
            "{}-{}.lua",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::write(&path, bytes).unwrap();
        Self(path)
    }
    fn script(&self, source: &str) -> Vec<u8> {
        source
            .replace("@FILE@", &format!("{:?}", self.0.to_string_lossy()))
            .into_bytes()
    }
    fn compare(&self, source: &str) {
        compare(&self.script(source));
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[test]
fn named_loads_preserve_results_varargs_and_debug_source() {
    let file = Fixture::new(b"return select('#',...),nil,17,false,'a\\000z'");
    file.compare(
        r#"
        local path=@FILE@;local f,e=loadfile(path,{},false)
        local info=debug.getinfo(f,'Su');local a,b,c,d,s=f()
        local p,q,r,t,u=dofile(path,99,nil,'ignored')
        return type(f),e,info.source=='@'..path,info.what,info.linedefined,info.lastlinedefined,
            getfenv(f)==_G,a,b,c,d,s,p,q,r,t,u,select('#',dofile(path)),select('#',f(1,nil,3))
    "#,
    );
    for source in [
        b"".as_slice(),
        b"return",
        b"return nil",
        b"return nil,nil",
        b"return false,1,nil,3",
    ] {
        Fixture::new(source).compare(
            "local f=assert(loadfile(@FILE@));return select('#',f()),select('#',dofile(@FILE@))",
        );
    }
}

#[test]
fn initial_hash_lines_and_text_translation_keep_source_lines() {
    for source in [
        b"# no newline".as_slice(),
        b"#!/usr/bin/lua\nreturn 17,debug.getinfo(1,'Sl').currentline",
        b"# any initial hash line\r\nreturn 'a',debug.getinfo(1,'Sl').currentline",
        b"# CR does not end a header\rreturn 123",
        b"\nreturn 'a\r\nb'", // Invalid short string in either engine.
        b"return [[a\r\nb\rc\nd]]",
        b"\xef\xbb\xbfreturn 1", // Lua 5.1 does not skip a UTF-8 BOM.
        b" \n# second-line hash",
    ] {
        Fixture::new(source).compare(
            r#"
            local f,e=loadfile(@FILE@)
            if f then return true,select('#',f()),f() end
            return false,type(e),string.match(e,':(%d+):'),string.sub(e,1,#@FILE@)==@FILE@
        "#,
        );
    }
    #[cfg(windows)]
    Fixture::new(b"return 'ok'\r\n\x1a ) bad syntax").compare("return dofile(@FILE@)");
}

#[test]
fn missing_files_and_bad_arguments_keep_errors_and_handler_order() {
    let file = Fixture::new(b"return 17");
    file.compare(r#"
        local missing=@FILE@..'.absent';local f,e,n=loadfile(missing)
        local count=0;local ok,raised=xpcall(function()return dofile(missing)end,function(v)count=count+1;return v end)
        local converted=0;local t=setmetatable({},{__tostring=function()converted=converted+1;return @FILE@ end})
        return f,e,n,select('#',loadfile(missing)),ok,raised==e,count,
            pcall(loadfile,false),pcall(dofile,t),converted
    "#);
    file.compare(r#"
        local path=@FILE@;local f,e=loadfile(path..'\0ignored')
        local n=0;local ok,v=xpcall(function()return dofile(path..'.missing')end,function(e)n=n+1;return 'handled' end)
        local x,y=loadfile(1/0);return f(),e,n,ok,v,x,type(y),string.find(y,'cannot open',1,true)~=nil
    "#);
}

#[test]
fn loading_uses_thread_globals_and_leaves_function_environments_independent() {
    let file = Fixture::new(b"return x,_G,select('#',...)");
    file.compare(r#"
        local original=_G;x=11;debug.setfenv(loadfile,{x=99});debug.setfenv(dofile,{x=100})
        local f=assert(loadfile(@FILE@));local a,b,c=f();setfenv(f,setmetatable({x=23},{__index=original}))
        local d,e,n=f();local p,q,r=dofile(@FILE@)
        local changed=setmetatable({x=37},{__index=original});setfenv(0,changed)
        local g=assert(loadfile(@FILE@));local z,w,k=dofile(@FILE@)
        return a,b==original,c,d,e==original,n,p,q==original,r,getfenv(g)==changed,g(),z,w==original,k
    "#);
}

#[test]
fn dofile_keeps_original_argument_slots_and_roots_during_execution() {
    let file=Fixture::new(br#"
        collectgarbage()
        local a,p=debug.getlocal(2,1);local b,t=debug.getlocal(2,2);local c,z=debug.getlocal(2,3)
        local absent=debug.getlocal(2,4)
        local changed=debug.setlocal(2,2,'changed')
        local name,value=debug.getlocal(2,2)
        return a,p==path,b,t==tag,c,z==nil,absent,changed,name,value,debug.getinfo(2,'f').func==dofile
    "#);
    file.compare(r#"path=@FILE@;tag={};return dofile(path,tag,nil)"#);
}

#[test]
fn dofile_propagates_arbitrary_runtime_objects_without_losing_faulting_frames() {
    for body in [
        b"error(tag,0)".as_slice(),
        b"return (function()error(tag,0)end)()",
    ] {
        let file = Fixture::new(body);
        file.compare(
            r#"
            tag={};local count=0
            local ok,value=xpcall(function()return dofile(@FILE@)end,function(e)
                count=count+1;collectgarbage();assert(debug.getinfo(2,'S').what=='C');return e
            end)
            return ok,value==tag,count
        "#,
        );
    }
    Fixture::new(b"\nerror('failure')").compare(
        r#"
        local ok,e=pcall(dofile,@FILE@)
        return ok,string.find(e,@FILE@..':2: failure',1,true)~=nil
    "#,
    );
}

#[test]
fn files_can_be_reloaded_rewritten_and_outlive_their_source_data() {
    let file = Fixture::new(b"return 1");
    file.compare(r#"
        local path=@FILE@;local o=assert(io.open(path,'wb'));o:write('return 1');o:close()
        local f=assert(loadfile(path));o=assert(io.open(path,'wb'));o:write('local x=7;return function()x=x+1;collectgarbage();return x end');o:close()
        local g=dofile(path);o=assert(io.open(path,'wb'));o:write('return 23');o:close()
        collectgarbage();return f(),g(),g(),dofile(path)
    "#);
}

#[test]
fn nested_files_use_flat_scheduling_and_native_call_boundaries() {
    let file =
        Fixture::new(b"depth=depth+1;if depth<limit then return dofile(path)end;return depth");
    file.compare(
        r#"
        path=@FILE@;limit=50;depth=0;local a=dofile(path)
        depth=0;limit=300;local ok,e=pcall(dofile,path)
        return a,ok,type(e),e,depth
    "#,
    );
    Fixture::new(b"return coroutine.yield(17)").compare(r#"
        local co=coroutine.create(function()return dofile(@FILE@)end)
        local ok,e=coroutine.resume(co)
        return ok,type(e),string.find(e,'attempt to yield across metamethod/C-call boundary',1,true)~=nil,coroutine.status(co)
    "#);
}

#[test]
fn file_calls_preserve_hooks_and_allow_entry_argument_changes() {
    let one = Fixture::new(b"return 17");
    let two = Fixture::new(b"return 37");
    let source = String::from_utf8(one.script(
        r#"
        local events={};local path=@FILE@;local replacement=@SECOND@
        debug.sethook(function(event)
            local f=debug.getinfo(2,'f').func
            if f==loadfile or f==dofile then
                events[#events+1]=(f==loadfile and 'l' or 'd')..event
                if event=='call' then debug.setlocal(2,1,replacement)end
            end
        end,'cr')
        local f=loadfile(path);local a=dofile(path);debug.sethook()
        return f(),a,table.concat(events,',')
    "#,
    ))
    .unwrap()
    .replace("@SECOND@", &format!("{:?}", two.0.to_string_lossy()));
    compare(source.as_bytes());
}

#[test]
fn chunk_boundaries_and_long_headers_retain_file_roots_during_collection() {
    for size in [510, 511, 512, 513, 4095, 4096, 8191] {
        let mut source = vec![b'#'];
        source.extend(vec![b'x'; size]);
        source.extend_from_slice(b"\n--[=[");
        source.extend(vec![b'y'; size]);
        source.extend_from_slice(b"]=]\nreturn 'a\\000b',debug.getinfo(1,'l').currentline");
        Fixture::new(&source).compare("collectgarbage('setpause',0);return dofile(@FILE@)");
    }
    let source = format!(
        "--[=[{}]=]\nlocal t={{}};{}return #t",
        "x".repeat(100_000),
        "t[#t+1]=function()return 7 end;".repeat(500)
    );
    Fixture::new(source.as_bytes()).compare("collectgarbage('setpause',0);return dofile(@FILE@)");
}

#[test]
fn file_reader_quota_abort_closes_handles_and_discards_parser_storage() {
    let file = Fixture::new(format!("--[=[{}]=]\nreturn 42", "x".repeat(100_000)).as_bytes());
    let program = compile(&file.script("return dofile(@FILE@)")).unwrap();
    let mut vm = Vm::new();
    for _ in 0..30 {
        vm.set_instruction_limit(Some(12));
        assert_eq!(
            vm.execute(&program).unwrap_err().kind(),
            deadlib_lua::ErrorKind::Limit
        );
        assert_eq!(vm.heap_stats().files, 3);
    }
    let coroutine = compile(&file.script(
        "local co=coroutine.create(function()return dofile(@FILE@)end);return coroutine.resume(co)",
    ))
    .unwrap();
    vm.set_instruction_limit(Some(18));
    assert_eq!(
        vm.execute(&coroutine).unwrap_err().kind(),
        deadlib_lua::ErrorKind::Limit
    );
    assert_eq!(vm.heap_stats().files, 3);
    // On Windows an unclosed reader denies deletion even though it allows writes.
    std::fs::remove_file(&file.0).unwrap();
    vm.set_instruction_limit(None);
    assert_eq!(
        vm.execute(&compile(b"return coroutine.running(),42").unwrap())
            .unwrap(),
        &[Value::Nil, Value::Number(42.)]
    );
    let stats = vm.collect_garbage().unwrap();
    assert!(stats.reserved_bytes < 2_000_000, "{}", stats.reserved_bytes);
}

#[test]
fn repeated_file_loads_reclaim_readers_and_escaped_code_stays_live() {
    let file = Fixture::new(b"local x=7;return function()x=x+1;return x end");
    let source=file.script("local retained=dofile(@FILE@);for i=1,250 do local f=assert(loadfile(@FILE@));assert(f()()==8)end;collectgarbage();return retained()");
    let program = compile(&source).unwrap();
    let mut vm = Vm::new();
    for _ in 0..4 {
        assert_eq!(vm.execute(&program).unwrap(), &[Value::Number(8.)]);
        let stats = vm.collect_garbage().unwrap();
        assert_eq!(stats.files, 3);
        assert!(stats.programs < 10);
        assert!(stats.reserved_bytes < 2_000_000, "{}", stats.reserved_bytes);
    }
}

#[test]
fn syntax_source_names_use_the_lexer_limit_for_each_loader() {
    compare(br#"
        local rows={}
        for n=42,90 do
            local name='@'..string.rep('x',n)
            local a,e=loadstring('return )',name)
            local once=false;local b,f=load(function()if not once then once=true;return 'return )'end end,name)
            rows[#rows+1]=string.match(e,'^(.-):%d+:')..'/'..string.match(f,'^(.-):%d+:')
        end
        return table.concat(rows,';')
    "#);
}

#[test]
fn host_file_errors_describe_the_raised_string() {
    let file = Fixture::new(b"return 17");
    let program = compile(&file.script("return dofile(@FILE@..'.missing')")).unwrap();
    let mut vm = Vm::new();
    let error = vm.execute(&program).unwrap_err();
    assert!(error.message().starts_with("cannot open "), "{error}");
    let Value::String(id) = error.value().expect("raised file error") else {
        panic!("file error must be a string")
    };
    assert_eq!(error.message().as_bytes(), vm.string_bytes(id).unwrap());
}

#[test]
fn standard_input_loading_shares_buffers_and_ignores_io_input_redirection() {
    let cases = [
        (
            "local x=io.read(1);local f=assert(loadfile());local s=debug.getinfo(f,'S').source;return x,f(),s",
            b"x#!header\r\nreturn 17".to_vec(),
        ),
        (
            "return dofile(nil,1,2)",
            b"return 17,select('#',...),nil".to_vec(),
        ),
        (
            "local old=io.read(1);local file=io.tmpfile();file:write('return 99');file:seek('set');io.input(file);io.stdin=nil;collectgarbage();collectgarbage();local f=assert(loadfile());return old,f(),io.read('*a')",
            b"xreturn 37".to_vec(),
        ),
        (
            "local f,e=loadfile();return f,type(e),io.read('*a')",
            format!(
                "){}tail",
                " ".repeat(if cfg!(windows) { 511 } else { 8191 })
            )
            .into_bytes(),
        ),
        (
            "local f=assert(loadfile());return f()",
            format!("#{}\nreturn 23", "x".repeat(1500)).into_bytes(),
        ),
        (
            "return select('#',dofile()),select('#',loadfile()())",
            vec![],
        ),
    ];
    if let Ok(index) = std::env::var("DEADLIB_LUA_STDIN_CASE") {
        let index: usize = index.parse().unwrap();
        let reference = std::env::var("DEADLIB_LUA_STDIN_REFERENCE").unwrap() == "1";
        println!("OBS\t{:?}", observe(cases[index].0.as_bytes(), reference));
        return;
    }
    use std::io::Write;
    for (index, (_, input)) in cases.iter().enumerate() {
        let mut observations = Vec::new();
        for reference in ["0", "1"] {
            let mut child = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "standard_input_loading_shares_buffers_and_ignores_io_input_redirection",
                    "--nocapture",
                    "--test-threads=1",
                ])
                .env("DEADLIB_LUA_STDIN_CASE", index.to_string())
                .env("DEADLIB_LUA_STDIN_REFERENCE", reference)
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .unwrap();
            child.stdin.take().unwrap().write_all(input).unwrap();
            let output = child.wait_with_output().unwrap();
            assert!(
                output.status.success(),
                "case {index}, engine {reference}: {} {}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            let stdout = String::from_utf8(output.stdout).unwrap();
            let at = stdout.find("OBS\t").expect("worker observation");
            observations.push(stdout[at..].lines().next().unwrap().to_owned());
        }
        assert_eq!(observations[0], observations[1], "stdin case {index}");
    }
}

fn observe(source: &[u8], reference: bool) -> Vec<Scalar> {
    if !reference {
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
        return actual;
    }
    // SAFETY: fixed local programs need the development oracle's debug library.
    let lua = unsafe { Lua::unsafe_new() };
    // Use a plain protected entry with no host error-handler value below it.
    // The normal mlua eval wrapper leaves that C function on the base stack,
    // which Lua 5.1 can expose through negative/lost-tail local queries.
    let reference: MultiValue = lua.exec_raw_lua(|raw| unsafe {
        use lua_reference::{FromLuaMulti, ffi};
        let state = raw.state();
        let start = ffi::lua_gettop(state);
        let mut status = ffi::luaL_loadbuffer(
            state,
            source.as_ptr().cast(),
            source.len(),
            c"=(deadlib-lua)".as_ptr(),
        );
        if status == 0 {
            status = ffi::lua_pcall(state, 0, ffi::LUA_MULTRET, 0);
        }
        if status != 0 {
            let mut len = 0;
            let text = ffi::lua_tolstring(state, -1, &mut len);
            let message = if text.is_null() {
                "non-string error".into()
            } else {
                String::from_utf8_lossy(std::slice::from_raw_parts(text.cast(), len)).into_owned()
            };
            ffi::lua_settop(state, start);
            panic!("reference: {message}: {}", String::from_utf8_lossy(source));
        }
        assert_ne!(
            ffi::lua_checkstack(state, 32),
            0,
            "oracle result conversion stack"
        );
        let values = MultiValue::from_stack_multi(ffi::lua_gettop(state) - start, raw).unwrap();
        ffi::lua_settop(state, start);
        values
    });
    let expected: Vec<_> = reference
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
    expected
}

fn compare(source: &[u8]) {
    let actual = observe(source, false);
    let expected = observe(source, true);
    if actual != expected {
        for (index, (a, b)) in actual.iter().zip(&expected).enumerate() {
            if a != b && !matches!((a, b), (Scalar::Bytes(_), Scalar::Bytes(_))) {
                panic!("observation {index}: rust={a:?}, reference={b:?}");
            }
            if let (Scalar::Bytes(a), Scalar::Bytes(b)) = (a, b)
                && a != b
            {
                let at = a
                    .iter()
                    .zip(b)
                    .position(|(a, b)| a != b)
                    .unwrap_or(a.len().min(b.len()));
                panic!(
                    "observation {index}, bytes differ at {at}: rust={:?}, reference={:?}: {}",
                    String::from_utf8_lossy(&a[at.saturating_sub(40)..(at + 80).min(a.len())]),
                    String::from_utf8_lossy(&b[at.saturating_sub(40)..(at + 80).min(b.len())]),
                    String::from_utf8_lossy(source)
                );
            }
        }
        assert_eq!(actual, expected, "{}", String::from_utf8_lossy(source));
    }
}
