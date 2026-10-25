use deadlib_lua::{Value, Vm, compile};
use lua_reference::{Lua, MultiValue, Value as Ref};
#[derive(Debug, PartialEq)]
enum Scalar {
    Nil,
    Boolean(bool),
    Number(u64),
    Bytes(Vec<u8>),
}

#[test]
fn named_local_ranges_include_parameters_shadowing_varargs_and_loops() {
    compare(
        br#"
        local function inspect(level)
            local out={}
            for i=1,40 do
                local name,value=debug.getlocal(level+1,i)
                if not name then break end
                out[#out+1]=name..':'..type(value)..':'..(type(value)=='number'and value or '')
            end
            return table.concat(out,'|')
        end
        local function f(a,b,...)
            local out={inspect(1)}
            local x,y=17,19
            out[#out+1]=inspect(1)
            do local x=23;out[#out+1]=inspect(1)end
            out[#out+1]=inspect(1)
            for i=1,1 do local z=29;out[#out+1]=inspect(1)end
            for k,v in ipairs({31})do out[#out+1]=inspect(1)end
            return table.concat(out,'\n')
        end
        local function g(a,...)return inspect(1),... end
        return f(3,5,7,11),g(2,13)
    "#,
    );
}

#[test]
fn local_mutation_updates_shared_cells_and_suspended_failed_threads() {
    compare(
        br#"
        local x=1
        local function change()
            assert(debug.setlocal(2,1,42)=='x')
            collectgarbage()
        end
        local function read()return x end
        change()
        local co=coroutine.create(function(a)
            local x=17
            local function f()return x end
            coroutine.yield(f)
            error(f)
        end)
        local ok,f=coroutine.resume(co,13)
        assert(debug.getlocal(co,0,1)==nil)
        assert(debug.setlocal(co,0,1,'temporary')=='(*temporary)')
        assert(debug.getlocal(co,0,1)==nil)
        local name,value=debug.getlocal(co,1,1)
        local name2,value2=debug.getlocal(co,1,2)
        local set=debug.setlocal(co,1,2,19)
        local before=f()
        local ok,e=coroutine.resume(co)
        local set2=debug.setlocal(co,1,2,23)
        collectgarbage()
        return x,read(),name,value,name2,value2,set,before,set2,f(),e==f
    "#,
    );
}

#[test]
fn native_argument_slots_bounds_and_validation_match_lua51() {
    compare(
        br#"
        local out={}
        for i=-1,6 do
            local name,value=debug.getlocal(0,i,'extra')
            out[#out+1]=tostring(name)..':'..tostring(value)
        end
        for i=0,6 do out[#out+1]=tostring(debug.setlocal(0,i,99,'dropped'))end
        local function check(fn,...)
            local ok,e=pcall(fn,...);out[#out+1]=tostring(ok)..':'..type(e)
            if not ok then out[#out+1]=string.match(e,'#(%d+)')or'?'end
        end
        check(debug.getlocal);check(debug.getlocal,10000,false);check(debug.getlocal,1,false)
        check(debug.setlocal,10000,false);check(debug.setlocal,1,false)
        check(debug.setlocal,1,false,nil)
        check(debug.getlocal,-1,1);check(debug.setlocal,-1,1,2)
        return table.concat(out,'|')
    "#,
    );
}

#[test]
fn temporary_slots_hide_synthetic_operand_copies_during_nested_calls() {
    compare(
        br#"
        local function inspect(level)
            local out={}
            for i=1,30 do
                local name,value=debug.getlocal(level+1,i)
                if not name then break end
                out[#out+1]=name..':'..type(value)..':'..(type(value)=='number'and value or '')
            end
            return table.concat(out,'|'),1
        end
        local function f(...)return select(2,...)end
        local a=17
        local out={}
        out[#out+1]=f(13,inspect(1))
        out[#out+1]=select(2,inspect(1))
        local n=1+select(2,inspect(1))
        local n=a+select(2,inspect(1))
        local n=true and select(2,inspect(1))
        local n=1*(a+select(2,inspect(1)))
        return table.concat(out,'\n')
    "#,
    );
}

#[test]
fn generated_expression_contexts_preserve_observable_temporary_slots() {
    let contexts = [
        "inspect(1)",
        "1+inspect(1)",
        "a+inspect(1)",
        "1*(a+inspect(1))",
        "a+1+inspect(1)",
        "'x'..inspect(1)",
        "a<inspect(1)",
        "a==inspect(1)",
        "f(13,inspect(1))",
        "f(13,17,inspect(1))",
        "f({a},inspect(1))",
        "true and inspect(1)",
        "false or inspect(1)",
        "a and inspect(1)",
        "(a==17)and inspect(1)or 3",
        "({x=inspect(1)}).x",
        "({1,inspect(1)})[2]",
        "t.x+inspect(1)",
        "t['x']+inspect(1)",
        "t[inspect(1)]",
        "t:m(1,inspect(1))",
        "t.m(1,inspect(1))",
        "-inspect(1)",
        "not inspect(1)",
        "inspect(1)+inspect(1)",
    ];
    for context in contexts {
        let source = format!(
            r#"
            local observations={{}}
            local function inspect(level)
                local out={{}}
                for i=1,40 do
                    local name,value=debug.getlocal(level+1,i)
                    if not name then break end
                    out[#out+1]=name..':'..type(value)..':'..(type(value)=='number'and value or '')
                end
                observations[#observations+1]=table.concat(out,'|')
                return 1
            end
            local a=17
            local function f(...)return select('#',...)end
            local t={{x=19,m=f}}
            local result={context}
            return table.concat(observations,'\n')
        "#
        );
        compare(source.as_bytes());
    }
}

#[test]
fn local_operand_reads_and_indexed_writes_follow_lua51_callback_order() {
    for source in [
        "local a=1;local function f()a=2;return 3 end;return a+f(),a",
        "local a=17;local function f()debug.setlocal(2,1,23);return 1 end;return a+f(),a",
        "local a=2;local function f()a=5;return 4 end;return a-f(),a*f(),a/f(),a%f(),a^f(),a<f(),a==f()",
        "local t={x=1};local old=t;local function f()t={x=2};return 3 end;t.x=f();return old.x,t.x",
        "local k=1;local t={};local function f()k=2;return 3 end;t[k]=f();return t[1],t[2]",
        "local k=1;local t={};local function f()k=2;return 3 end;t[k],k=f(),4;return t[1],t[2],k",
        "local a={};local old=a;local i=1;local function f()a={};i=2;return 3 end;a[i],a=f(),{};return old[1],old[2],a[1],a[2],i",
        "local k=1;local function f()k=2;return 3 end;local t={[k]=f()};return t[1],t[2]",
        "local a='old';local function f()a='new';return 'right'end;return a..f(),a",
        "local a=1;local function f()a=2;return 3 end;local function g(x,y)return x,y end;return g(a,f())",
    ] {
        compare(source.as_bytes());
    }
}

#[test]
fn callback_boundaries_keep_native_stack_arguments_and_reader_reserved_slots() {
    compare(br#"
        local out={}
        local function inspect(level,limit)
            local row={}
            for i=1,limit or 12 do
                local name,value=debug.getlocal(level+1,i)
                if not name then break end
                row[#row+1]=name..':'..type(value)..':'..(type(value)=='number'and value or '')
            end
            out[#out+1]=table.concat(row,'|')
        end
        local function visit()inspect(2)end
        pcall(visit);xpcall(visit,function()end)
        table.foreach({[2]=3},visit,'extra');table.foreachi({5},visit,'extra')
        tostring(setmetatable({},{__tostring=function()inspect(2);return'x'end}))
        local original=tostring;tostring=function(v)inspect(2);return original(v)end
        print(7,11);tostring=original
        -- PUC's later slots contain its private lexer table/prototype, which
        -- are not Lua language values shared by independent compilers.
        local n=0;load(function()inspect(2,3);n=n+1;if n==1 then return'return 42'end end,'named',13,'dropped')
        string.gsub('a','.',function()inspect(2);return'b'end)
        return table.concat(out,'\n')
    "#);
}

#[test]
fn generated_assignment_conflicts_preserve_late_reads_and_required_snapshots() {
    let targets = ["t[k]", "t[j]", "k", "j", "t", "u[k]", "u[j]", "u"];
    for a in targets {
        for b in targets {
            for c in targets {
                let source = format!(
                    r#"
            local k,j=1,2
            local t,u={{}},{{}}
            local oldt,oldu=t,u
            local nt,nu={{}},{{}}
            local function rhs()t,u=nt,nu;k,j=2,1;return 11,22,33 end
            {a},{b},{c}=rhs()
            return k,j,type(t),type(u),oldt[1],oldt[2],oldu[1],oldu[2],nt[1],nt[2],nu[1],nu[2]
        "#
                );
                compare(source.as_bytes());
            }
        }
    }
}

#[test]
fn mutated_local_and_temporary_values_remain_roots_during_collection() {
    compare(
        br#"
        local x=0
        local function read()return x end
        local function change()
            assert(debug.setlocal(2,1,{tag=42})=='x')
            collectgarbage()
        end
        change()
        local function take(a,b)return a.tag end
        local function change_argument()
            for i=1,40 do
                local n,v=debug.getlocal(2,i)
                if not n then break end
                if n=='(*temporary)'and v==13 then
                    debug.setlocal(2,i,{tag=47});collectgarbage();return 1
                end
            end
            error('missing temporary')
        end
        local result=take(13,change_argument())
        return read().tag,result
    "#,
    );
    let mut vm = Vm::new();
    let functions = vm
        .execute(&compile(b"return debug.getlocal,debug.setlocal").unwrap())
        .unwrap()
        .to_vec();
    let roots: Vec<_> = functions.into_iter().map(|v| vm.pin(v).unwrap()).collect();
    vm.set_global(b"debug", Value::Nil).unwrap();
    vm.collect_garbage().unwrap();
    for (i, root) in roots.into_iter().enumerate() {
        let function = vm.rooted(root).unwrap();
        vm.set_global(b"query", function).unwrap();
        let source = if i == 0 {
            b"local a=42;local n,v=query(1,1);return v".as_slice()
        } else {
            b"local a=0;query(1,1,42);return a"
        };
        assert_eq!(
            vm.execute(&compile(source).unwrap()).unwrap(),
            &[Value::Number(42.0)]
        );
        vm.unpin(root).unwrap();
    }
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
        for (a, b) in actual.iter().zip(&expected) {
            if let (Scalar::Bytes(a), Scalar::Bytes(b)) = (a, b)
                && a != b
            {
                let at = a
                    .iter()
                    .zip(b)
                    .position(|(a, b)| a != b)
                    .unwrap_or(a.len().min(b.len()));
                panic!(
                    "bytes differ at {at}: rust={:?}, reference={:?}: {}",
                    String::from_utf8_lossy(&a[at.saturating_sub(40)..(at + 80).min(a.len())]),
                    String::from_utf8_lossy(&b[at.saturating_sub(40)..(at + 80).min(b.len())]),
                    String::from_utf8_lossy(source)
                );
            }
        }
        assert_eq!(actual, expected, "{}", String::from_utf8_lossy(source));
    }
}
