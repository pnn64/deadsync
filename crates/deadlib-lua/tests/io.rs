use deadlib_lua::{Value, Vm, compile};
use lua_reference::{Lua, MultiValue, Value as Ref};

#[derive(Debug, PartialEq)]
enum Scalar {
    Nil,
    Boolean(bool),
    Number(u64),
    Bytes(Vec<u8>),
}

fn compare_file(source: &str) {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/lua-io-fixtures");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!(
        "{}-{}.dat",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let source = source.replace("@FILE@", &format!("{:?}", path.to_string_lossy()));
    compare(source.as_bytes());
    if path.exists() {
        std::fs::remove_file(path).unwrap();
    }
}

#[test]
fn named_files_preserve_modes_append_and_automatic_iterator_close() {
    compare_file(
        r#"
        local path=@FILE@;local out={}
        local f=assert(io.open(path,'w+b'));f:write('a\nb');f:close()
        f=assert(io.open(path,'r+b'));out[#out+1]=f:read(1);f:seek('cur',0);f:write('Z');f:seek('set',0);out[#out+1]=f:read('*a');f:close()
        f=assert(io.open(path,'a+b'));out[#out+1]=f:seek();f:write('tail');f:seek('set',0);out[#out+1]=f:read('*a');f:close()
        local iter=io.lines(path);out[#out+1]=iter();out[#out+1]=select('#',iter());out[#out+1]=tostring((pcall(iter)))
        f=assert(io.open(path..'\0ignored','rb\0ignored'));out[#out+1]=f:read('*a');f:close()
        return table.concat(out,'|')
    "#,
    );
}

#[test]
fn windows_text_translation_control_z_and_seek_positions_match_reference() {
    compare_file(
        r#"
        local path=@FILE@;local out={}
        local f=assert(io.open(path,'wb'));f:write('a\r\nb\nc\rd\026tail\n');f:close()
        f=assert(io.open(path,'r'));out[#out+1]=f:read(1);out[#out+1]=f:seek();out[#out+1]=f:read('*l');out[#out+1]=f:seek();out[#out+1]=f:read('*a');out[#out+1]=f:seek();out[#out+1]=f:read('*a');f:close()
        f=assert(io.open(path,'wt'));f:write('x\ny\r\n');out[#out+1]=f:seek();f:close()
        f=assert(io.open(path,'rb'));out[#out+1]=f:read('*a');f:close()
        f=assert(io.open(path,'wb'));f:write('end\026');f:close()
        f=assert(io.open(path,'r+t'));out[#out+1]=f:seek('end');f:close()
        return table.concat(out,'|')
    "#,
    );
}

#[test]
fn access_errors_and_open_failures_preserve_result_tuples() {
    compare_file(
        r#"
        local path=@FILE@;local out={}
        local function save(...)out[#out+1]=select('#',...);for i=1,select('#',...)do out[#out+1]=tostring(select(i,...))end end
        local f=assert(io.open(path,'wb'));local a,b,c=f:read('*a');save(a==nil,type(b),type(c));f:close()
        f=assert(io.open(path,'wb'));f:write('abc');f:close()
        f=assert(io.open(path,'rb'));save(f:write(''));save(f:write('x'));f:seek('set',0);save(f:read('*a'));save(f:seek('set',-1));f:close()
        local f,why,code=io.open(path..'.missing','r')
        return table.concat(out,'|'),f==nil,code,string.sub(why,-25)
    "#,
    );
}

#[test]
fn pipes_read_lines_and_preserve_lua51_pclose_status_behavior() {
    let command = if cfg!(windows) {
        "echo alpha&echo beta&exit /b 3"
    } else {
        "printf 'alpha\\nbeta\\n'; exit 3"
    };
    let source = format!(
        r#"local f=assert(io.popen({command:?},'r'));local a,b=f:read('*l','*l');local n=select('#',f:read('*a'));local ok=f:close();local f=assert(io.popen({zero:?},'r'));f:read('*a');local a0,b0,c0=f:close();return a,b,n,ok,a0==nil,type(b0),type(c0)"#,
        zero = if cfg!(windows) { "exit /b 0" } else { "exit 0" }
    );
    compare(source.as_bytes());
}

#[test]
fn pipe_writes_flush_and_signal_eof_before_waiting_for_the_child() {
    let command = if cfg!(windows) { "more" } else { "cat" };
    compare_file(&format!(
        r#"local path=@FILE@;local f=assert(io.popen('{command} > "'..path..'"','w'));assert(f:write('first\nsecond\n'));assert(f:close());local f=assert(io.open(path,'rb'));local s=f:read('*a');f:close();return s"#
    ));
}

#[test]
fn cli_stdin_and_mixed_stdout_keep_text_translation_and_write_order() {
    use std::io::Write;
    let mut child=std::process::Command::new(env!("CARGO_BIN_EXE_deadlua")).args(["-e",r#"local a,b=io.read('*n','*n');print(a,b,io.read(),io.read());io.write('x');print('y\nz');io.flush()"#]).stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).spawn().unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"1\r\n2\r\nlast\r\n")
        .unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        result.stdout,
        if cfg!(windows) {
            b"1\t2\t\tlast\r\nxy\r\nz\r\n".as_slice()
        } else {
            b"1\t2\t\r\tlast\r\nxy\nz\n".as_slice()
        }
    );
}

#[test]
fn file_finalization_clears_weak_values_and_closes_once() {
    compare(br#"
        local f=assert(io.tmpfile());local meta=getmetatable(f);local original=meta.__gc
        local called=0;local before,after
        meta.__gc=function(file)
            if file~=io.stdin and file~=io.stdout and file~=io.stderr then
                called=called+1;before=io.type(file);original(file);after=io.type(file);collectgarbage()
            end
        end
        local weak=setmetatable({f},{__mode='v'});f=nil;collectgarbage();meta.__gc=original;collectgarbage()
        return called,before,after,weak[1]==nil
    "#);
}

#[test]
fn file_records_and_buffers_are_reclaimed_and_reused() {
    let mut vm = Vm::new();
    let source=compile(br#"
        for i=1,100 do local f=assert(io.tmpfile());f:write(string.rep('x',1000));if i%2==0 then f:close()end end
        return 42
    "#).unwrap();
    for _ in 0..10 {
        assert_eq!(vm.execute(&source).unwrap(), &[Value::Number(42.0)]);
        vm.collect_garbage().unwrap();
    }
    vm.collect_garbage().unwrap();
    let heap = vm.heap_stats();
    assert_eq!(heap.files, 3);
    assert_eq!(heap.userdata, 3);
    assert!(heap.reserved_bytes < 2_000_000, "{heap:?}");
}

#[test]
fn stdout_capture_keeps_print_and_io_write_order_and_binary_bytes() {
    let mut vm = Vm::new();
    vm.set_output_capture(true);
    vm.execute(
        &compile(br#"io.write('a');print('b\nc');io.stdout:write('\0z');io.flush()"#).unwrap(),
    )
    .unwrap();
    assert_eq!(vm.output(), Some(b"ab\nc\n\0z".as_slice()));
}

#[test]
fn regular_file_buffering_controls_visibility_to_other_handles() {
    compare_file(
        r#"
        local path=@FILE@;local out={}
        for _,mode in ipairs({'no','full','line'})do
            local writer=assert(io.open(path,'wb'));assert(writer:setvbuf(mode,512));writer:write('body\n')
            local reader=assert(io.open(path,'rb'));out[#out+1]=reader:read('*a');reader:close()
            writer:flush();reader=assert(io.open(path,'rb'));out[#out+1]=reader:read('*a');reader:close();writer:close()
        end
        return table.concat(out,'|')
    "#,
    );
}

#[test]
fn line_iterators_observe_appended_data_after_eof_and_preserve_seek_peeks() {
    compare_file(
        r#"
        local path=@FILE@;local out={}
        local f=assert(io.open(path,'wb'));f:write('one\n');f:close()
        f=assert(io.open(path,'r'));local iter=f:lines();out[#out+1]=iter();out[#out+1]=select('#',iter())
        local writer=assert(io.open(path,'ab'));writer:write('two\n');writer:close()
        out[#out+1]=select('#',iter());out[#out+1]=f:read('*l');out[#out+1]=select('#',iter())
        f:seek('set',0);out[#out+1]=f:read(0);out[#out+1]=iter();f:close()
        return table.concat(out,'|')
    "#,
    );
}

#[test]
fn temporary_files_keep_userdata_identity_metatables_and_environments() {
    compare(br#"
        local f=assert(io.tmpfile())
        local m=getmetatable(f)
        local e=debug.getfenv(f)
        local a,b=io.stdout:close()
        local iter=f:lines()
        local out={type(f),io.type(f),tostring(m.__index==m),tostring(e==debug.getfenv(io.open)),tostring(io.input()==io.stdin),tostring(io.output()==io.stdout),tostring(m.close==io.close),tostring(debug.getfenv(io.stdin)~=e),tostring(debug.getinfo(iter,'u').nups),tostring(a),b}
        assert(f:close())
        out[#out+1]=io.type(f);out[#out+1]=tostring(f)
        return table.concat(out,'|'),io.type({}),io.type(nil)
    "#);
}

#[test]
fn debug_registry_is_shared_rooted_and_controls_the_file_type_tag() {
    compare(
        br#"
        local r=debug.getregistry();assert(r==debug.getregistry())
        assert(r._LOADED._G==_G and r._LOADED.io==io and r._LOADED.math==math)
        local mt=getmetatable(io.stdout);assert(r['FILE*']==mt)
        r['FILE*']=nil;local absent=io.type(io.stdout)==nil;local lines=pcall(io.lines);r['FILE*']=mt
        local value={tag=42};local weak=setmetatable({value},{__mode='v'});r.saved=value;value=nil
        collectgarbage();local alive=weak[1] and weak[1].tag==42;r.saved=nil;collectgarbage()
        return absent,alive,weak[1]==nil,io.type(io.stdout),lines
    "#,
    );
}

#[test]
fn byte_writes_reads_seeks_and_lua51_boolean_results_match_reference() {
    compare(
        br#"
        local f=assert(io.tmpfile());local out={}
        out[#out+1]=type(f:write('hello',string.char(0),'world\r\n',12.5,-0))
        out[#out+1]=f:seek();assert(f:seek('set',0)==0)
        out[#out+1]=f:read(5);out[#out+1]=f:read(1);out[#out+1]=f:read(5)
        out[#out+1]=f:seek('cur',-5);out[#out+1]=f:read('*a')
        out[#out+1]=f:seek('end',-1);out[#out+1]=f:read('*a')
        assert(f:close());return table.concat(out,'|')
    "#,
    );
}

#[test]
fn line_reads_preserve_lua51_fgets_and_embedded_nul_behavior() {
    compare(br#"
        local out={}
        for _,data in ipairs({'','\n','a\n\nb','a\r\nb\rc\n','a\0b\nc\n','\0\n',string.rep('x',510)..'\0tail\nnext\n',string.rep('y',1025)..'\nlast'})do
            local f=assert(io.tmpfile());assert(f:write(data));f:seek('set',0)
            while true do local s=f:read();if not s then break end;out[#out+1]='['..s..']'end
            out[#out+1]='EOF';f:close()
        end
        return table.concat(out,'|')
    "#);
}

#[test]
fn zero_length_all_and_multiple_reads_keep_result_arity_and_stop_order() {
    compare(br#"
        local out={}
        local function save(...)out[#out+1]=select('#',...);for i=1,select('#',...)do out[#out+1]=tostring(select(i,...))end end
        local f=assert(io.tmpfile());f:write('abc\n12\n');f:seek('set',0)
        save(f:read(0));save(f:read(2,'*l','*n','*l',4,'invalid'))
        save(f:read(0));save(f:read('*a'));save(f:read('*a','*a',0))
        f:seek('set',0);save(f:read(-1));f:seek('set',0);save(f:read(2.9))
        f:close();return table.concat(out,'|')
    "#);
}

#[test]
fn numeric_reads_match_consumption_and_decimal_values() {
    compare(br#"
        local out={}
        for _,data in ipairs({'1 rest',' -12.5e+2x','.5!','1.','1e','1e+','+','-x','.x','abc','  1 2\n3','0x20tail','inf rest','nan rest','1e9999rest','1e-9999rest'})do
            local f=assert(io.tmpfile());f:write(data);f:seek('set',0)
            local n=f:read('*n')
            out[#out+1]=type(n)..':'..tostring(n)..':'..f:read('*a')
            f:close()
        end
        return table.concat(out,'|')
    "#);
}

#[test]
fn generated_numeric_streams_match_value_bits_and_unconsumed_suffixes() {
    let mut tokens: Vec<String> = [
        "-0",
        "0x0p0",
        "-0x0.0p-9999",
        "0x1p-1074",
        "0x1.fffffffffffffp-1023",
        "0x1.00000000000008p0",
        "0x1.fffffffffffff8p1023",
        "INF",
        "-Infinity",
        "NaN",
        "-nan(ind)",
        "nan(snan)",
        "infi",
        "nan(x)",
        "0x",
        "0x.",
        "0x1p",
        "1e--2",
        ".",
        "-.",
        "nan()",
        "nan(indx)",
        "nan(snake)",
        "nan(_bar)",
        "NaN(foo_bar_42)",
        "nan(",
        "nan(x y)",
        "infinityX",
        "infinite!",
        "IN",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    let mut random = 0x4d59_5df4_d0f3_3173u64;
    for _ in 0..300 {
        random = random.wrapping_mul(6364136223846793005).wrapping_add(1);
        let exponent = (random % 2200) as i32 - 1100;
        tokens.push(format!(
            "{}0x{:x}.{:013x}p{exponent:+}",
            if random & 1 == 0 { "" } else { "-" },
            (random >> 60) | 1,
            random & 0x000f_ffff_ffff_ffff
        ));
    }
    let literals = tokens
        .iter()
        .map(|s| format!("{s:?}"))
        .collect::<Vec<_>>()
        .join(",");
    let source = format!(
        r#"local out={{}};local f=assert(io.tmpfile());for _,text in ipairs({{{literals}}})do f:seek('set',0);f:write(text..'!');f:flush();f:seek('set',0);local n=f:read('*n');out[#out+1]=n==nil and 'nil' or n~=n and 'nan' or n;out[#out+1]=f:read(#text+1)or'' end;f:close();return unpack(out)"#
    );
    compare(source.as_bytes());
}

#[test]
fn default_files_share_private_function_environments_and_restore_cleanly() {
    compare(
        br#"
        local oldin,oldout=io.input(),io.output()
        local f=assert(io.tmpfile());assert(io.output(f)==f)
        assert(io.write('one\ntwo\n')==true);assert(io.flush()==true)
        f:seek('set',0);assert(io.input(f)==f)
        local iter=io.lines();local a,b=iter(),io.read()
        assert(io.input(oldin)==oldin);assert(io.output(oldout)==oldout)
        assert(f:close());local ok=pcall(f.read,f)
        return a,b,ok,io.input()==oldin,io.output()==oldout,(pcall(iter))
    "#,
    );
}

#[test]
fn line_iterators_retain_files_and_have_two_native_upvalues() {
    compare(br#"
        local f=assert(io.tmpfile());f:write('one\ntwo\nlast');f:seek('set',0)
        local weak=setmetatable({f},{__mode='v'});local iter=f:lines();f=nil
        collectgarbage();local out={tostring(weak[1]~=nil),iter()}
        collectgarbage();out[#out+1]=iter();out[#out+1]=iter();out[#out+1]=select('#',iter());out[#out+1]=select('#',iter())
        assert(debug.getinfo(iter,'u').nups==2)
        iter=nil;collectgarbage();collectgarbage();out[#out+1]=tostring(weak[1]==nil)
        return table.concat(out,'|')
    "#);
}

#[test]
fn explicit_close_and_standard_streams_keep_errors_and_arity() {
    compare(br#"
        local f=assert(io.tmpfile());local out={};f:close()
        for _,name in ipairs({'read','write','flush','lines','seek','setvbuf','close'})do local ok,value=pcall(f[name],f);out[#out+1]=tostring(ok)..':'..type(value)end
        for _,f in ipairs({io.stdin,io.stdout,io.stderr})do
            local a,b,c=f:close();out[#out+1]=tostring(a)..':'..b..':'..tostring(c)..':'..io.type(f)
        end
        return table.concat(out,'|')
    "#);
}

#[test]
fn buffering_modes_and_partial_read_validation_preserve_file_position() {
    compare(
        br#"
        local out={};local f=assert(io.tmpfile())
        for _,mode in ipairs({'no','full','line'})do
            assert(f:setvbuf(mode,512));f:write(mode,'\n');assert(f:flush())
        end
        f:seek('set',0);local ok,value=pcall(f.read,f,2,'bad')
        out[#out+1]=tostring(ok);out[#out+1]=f:seek();out[#out+1]=f:read('*l')
        f:seek('end');assert(f:write()==true);assert(f:write('')==true)
        f:close();return table.concat(out,'|')
    "#,
    );
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
