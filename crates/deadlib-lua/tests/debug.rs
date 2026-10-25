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
fn function_sources_definitions_and_option_fields_match_lua51() {
    compare(br#"
        local out={}
        local function info(f,options)
            local d=debug.getinfo(f,options)
            local keys={'source','short_src','what','linedefined','lastlinedefined','currentline','nups','name','namewhat'}
            for _,key in ipairs(keys)do out[#out+1]=key..'='..tostring(d[key])end
            out[#out+1]=tostring(d.func==f)
            local lines={};for l in pairs(d.activelines or {})do lines[#lines+1]=l end
            table.sort(lines);out[#out+1]=table.concat(lines,',')
        end
        info(print,'SlnufL');info(debug.getinfo,'SlnufL')
        local x,y=1,2
        local function f(a,...)
            local z=x+y
            return z,a,...
        end
        info(f,'SlnufL');info(f,'');info(f);info(f,'S\000invalid')
        for _,name in ipairs({'','@path/file.lua','=literal','plain source','\nfirst line',string.rep('x',200),'@'..string.rep('x',200)..'end','='..string.rep('x',200),'@x\000ignored'})do
            local chunk=assert(loadstring('\nreturn function(a,...)\n\n return a\nend\n',name))
            info(chunk,'SlnufL');info(chunk(),'SlnufL')
        end
        return table.concat(out,'|')
    "#);
}

#[test]
fn caller_names_cover_locals_upvalues_globals_fields_methods_and_callbacks() {
    compare(
        br#"
        local out={}
        local function note()
            local d=debug.getinfo(1,'n');out[#out+1]=tostring(d.name)..':'..d.namewhat
        end
        note();(note)();global_note=note;global_note()
        local t={field=note};t.field();t:field();t['field']();local key='field';t[key]()
        local function outer()note();return 1 end;outer()
        pcall(note);xpcall(note,note)
        tostring(setmetatable({},{__tostring=function()note();return 'x'end}))
        local mt={__index=note,__call=note};local v=setmetatable({},mt);local r=v.x;v()
        local f=function()note();return 2 end;local h=function()return f()end;h()
        local function it()note();return nil end;for k in it do end
        local other=note;local yes=true
        (note or other)();(yes and note or other)();(false and note or other)()
        local function factory()return note end;factory()()
        t['fi'..'eld']();(t.field)();(t[key])()
        return table.concat(out,'|')
    "#,
    );
}

#[test]
fn debug_argument_checks_preserve_order_coercions_and_result_counts() {
    compare(
        br#"
        local out={}
        local co=coroutine.create(function()end)
        local function check(f,...)
            local ok,value=pcall(f,...)
            out[#out+1]=tostring(ok)..':'..type(value)
            if not ok then out[#out+1]=string.match(value,'#(%d+)') or '?' end
        end
        check(debug.getinfo);check(debug.getinfo,{},true);check(debug.getinfo,{},'S')
        check(debug.getinfo,10000,'invalid');check(debug.getinfo,10000,true)
        check(debug.getinfo,co,0,'invalid');check(debug.getinfo,co,0,true)
        check(debug.getinfo,print,'invalid');check(debug.getinfo,0,17)
        for _,level in ipairs({'0','1','1.9','-1','-1.9','10000'})do
            local d=debug.getinfo(level,'S');out[#out+1]=d and d.what or 'nil'
        end
        check(debug.getupvalue);check(debug.getupvalue,{},false)
        check(debug.setupvalue);check(debug.setupvalue,{},false,nil)
        local x=2;local function f()return x end
        for _,index in ipairs({'1','1.9','0','-1','10000'})do
            out[#out+1]=select('#',debug.getupvalue(f,index))
            out[#out+1]=select('#',debug.setupvalue(f,index,3))
        end
        return table.concat(out,'|')
    "#,
    );
}

#[test]
fn thirty_thousand_tail_calls_are_inspected_without_recursive_stack_walks() {
    compare(
        br#"
        local function f(n)
            if n>0 then return f(n-1)end
            local count=0
            for i=2,30001 do if debug.getinfo(i,'S').what=='tail'then count=count+1 end end
            return count,debug.getinfo(30002,'S').what,debug.getinfo(30003)==nil
        end
        local n,what,last=f(30000);return n,what,last
    "#,
    );
}

#[test]
fn retained_inspection_results_and_native_functions_survive_collection() {
    let mut vm = Vm::new();
    let program = compile(
        br#"
        return debug.getinfo,debug.getupvalue,debug.setupvalue
    "#,
    )
    .unwrap();
    let functions = vm.execute(&program).unwrap().to_vec();
    let roots: Vec<_> = functions.into_iter().map(|v| vm.pin(v).unwrap()).collect();
    vm.set_global(b"debug", Value::Nil).unwrap();
    vm.collect_garbage().unwrap();
    for root in roots {
        assert!(matches!(vm.rooted(root).unwrap(), Value::Native(_)));
        vm.unpin(root).unwrap();
    }
    compare(br#"
        local list={}
        for i=1,4000 do
            local f=assert(loadstring('local x=42;return function()return x end','@retained.lua'))()
            if i%1000==0 then list[#list+1]=debug.getinfo(f,'SfLu')end
        end
        collectgarbage();local out={}
        for _,d in ipairs(list)do out[#out+1]=d.func()+d.nups;out[#out+1]=d.source;out[#out+1]=next(d.activelines)end
        return table.concat(out,'|')
    "#);
}

#[test]
fn current_lines_track_multiline_calls_and_nested_function_boundaries() {
    compare(br#"
        local out={}
        local function line()
            out[#out+1]=debug.getinfo(2,'l').currentline
        end
        line()
        line(
            'argument',
            2
        )
        line {
            1,2,3
        }
        line [[long
string]]
        local function
        split_name
        ()
            line()
        end
        split_name()
        local d=debug.getinfo(split_name,'S');out[#out+1]=d.linedefined;out[#out+1]=d.lastlinedefined
        return table.concat(out,',')
    "#);
}

#[test]
fn generated_active_line_sets_cover_statements_expressions_and_empty_chunks() {
    let corpus = [
        "",
        "\n\n-- comment\n",
        "--[[\ncomment\n]]",
        "return 7\n\n",
        "local a=\n1;\nreturn a\n",
        "local a=\n(1+\n2)\nreturn a",
        "local a=1\nlocal b=2\nreturn a+\nb",
        "local a=true\nreturn a and\n3 or\n4",
        "local t={\n1,\n2,\nx=\n3,\n}\nreturn t",
        "local t={}\nt.x=\n4\nreturn t.x",
        "function f(a)\nlocal b=a\nreturn b\nend\nreturn f",
        "local function f(a)\nreturn a\nend\nreturn f",
        "return function()\n\nend\n",
        "if\na\nthen\nb=1\nelse\nb=2\nend\n",
        "if a then\nb=1\nelseif b then\nc=2\nend\nreturn 1",
        "while a do\na=a-1\nend\nreturn a",
        "repeat\na=a-1\nuntil a<1\nreturn a",
        "for i=\n1,\n10 do\nf(i)\nend\nreturn 1",
        "for k,v in\npairs({}) do\nf(k,v)\nend\nreturn 1",
        "do\nlocal x=1\nf(x)\nend\nreturn 1",
        "local a=function()\nreturn 2\nend\nf(a)\n",
    ];
    for source in corpus {
        let script = format!(
            r#"
            local f=assert(loadstring([==[{source}]==],'@active.lua'))
            local d=debug.getinfo(f,'L');local a={{}}
            for line in pairs(d.activelines)do a[#a+1]=line end
            table.sort(a);return table.concat(a,',')
        "#
        );
        compare(script.as_bytes());
    }
}

#[test]
fn generated_line_breaks_preserve_active_lines_across_the_grammar() {
    let corpus = [
        "local a = 1 ; local b = a ; return a + b",
        "local a = true ; return a and 3 or 4",
        "local a = 1 ; return - a , not a , # 'hello'",
        "local a = 1 ; return a == 2 , a < 3 , a ~= 4",
        "local a = 'x' ; return a .. 'y' .. a",
        "local t = { a = 1 , [ 'b' ] = 2 , 3 , 4 } ; return t . a",
        "local t = { } ; t [ 'x' ] = 5 ; return t [ 'x' ]",
        "local a = 1 ; if a then a = 2 else a = 3 end ; return a",
        "local a = 0 ; while a < 3 do a = a + 1 end ; return a",
        "local a = 0 ; repeat a = a + 1 until a == 3 ; return a",
        "for i = 1 , 10 , 2 do f(i) end ; return 1",
        "for k , v in pairs({}) do f(k,v) end ; return 1",
        "function f(a) return a end ; return f",
        "local function f(a) return a end ; return f",
        "local a = function() return 3 end ; return a",
        "local t = { } ; function t : f(a) return self , a end ; return t",
        "local a = 1 ; return a == 1 and 2 or 3",
        "local a = 1 ; if a == 1 then a = 2 end ; return a",
        "local a = 1 ; return true and a , false or a , nil or a",
        "local a = 1 ; return ( a ) , ( 1 + 2 ) , ( 'x' )",
        "local a = 1 ; return global + a , a + global",
        "local a = 1 ; return t [ 'x' ] + a",
        "local t = { } ; do local x = 1 ; t . f = function() return x end end ; return t",
        "local a = 1 ; if a then return a elseif b then return b else return 3 end",
        "local a = 1 ; return a > 2 or a <= 3",
        "local a = 1 ; return 1 + 2 + a , a ^ - 2",
        "return f(1,2) , f{1,2} , f 'hello'",
        "local a = 1 ; return not ( a == 2 )",
        "local a , b = 1 , 2 ; a , b = b , a ; return a , b",
        "local a = false ; if not a then f(a) end ; return a",
        "local t = { x = 1 } ; return t . x , t [ 1 ]",
        "local a = 1 ; do local b = a end ; return a",
    ];
    let mut random = 0x53d9_248bu32;
    for case in 0..512 {
        let mut source = String::new();
        for token in corpus[case % corpus.len()].split(' ') {
            if !source.is_empty() {
                random ^= random << 13;
                random ^= random >> 17;
                random ^= random << 5;
                source.push_str(match random % 4 {
                    0 => "\n",
                    1 => "\r\n",
                    2 => " -- comment\n",
                    _ => " ",
                });
            }
            source.push_str(token);
        }
        let script = format!(
            r#"
            local source=[==[{source}]==]
            local out={{}}
            for mode=1,2 do
                local f
                if mode==1 then f=assert(loadstring(source,'@generated.lua'))
                else f=assert(loadstring('return function(...) '..source..'\nend','@generated.lua'))()end
                local d=debug.getinfo(f,'SL');local lines={{}}
                for line in pairs(d.activelines)do lines[#lines+1]=line end
                table.sort(lines);out[#out+1]=table.concat(lines,',')
                out[#out+1]=d.linedefined..':'..d.lastlinedefined
            end
            return table.concat(out,'|')
        "#
        );
        compare(script.as_bytes());
    }
}

#[test]
fn native_upvalue_counts_and_lua_upvalue_mutations_preserve_identity() {
    compare(br#"
        local out={}
        for _,f in ipairs({print,pairs,ipairs,newproxy,string.gmatch('x','.'),coroutine.wrap(function()end),debug.getinfo})do
            out[#out+1]=debug.getinfo(f,'u').nups
            out[#out+1]=select('#',debug.getupvalue(f,1))
            out[#out+1]=select('#',debug.setupvalue(f,1,99))
        end
        local x,y={tag=1},4
        local function f()return x,y end
        local function g()return y,x end
        for i=-1,4 do
            local n,v=debug.getupvalue(f,i);out[#out+1]=tostring(n)..':'..type(v)
        end
        local replacement={tag=9}
        out[#out+1]=debug.setupvalue(f,1,replacement)
        out[#out+1]=tostring(select(2,g())==replacement)
        out[#out+1]=debug.setupvalue(g,1,42)
        out[#out+1]=select(2,f())
        local co=coroutine.create(function()local a=2;local function h()return a end;coroutine.yield(h);return a end)
        local ok,h=coroutine.resume(co);out[#out+1]=debug.setupvalue(h,1,17)
        local ok,value=coroutine.resume(co);out[#out+1]=value
        co=nil;collectgarbage();out[#out+1]=h()
        return table.concat(out,'|')
    "#);
}

#[test]
fn stack_levels_include_tail_records_and_suspended_failed_coroutines() {
    compare(br#"
        local out={}
        local function stack(co)
            for level=0,12 do
                local d;if co then d=debug.getinfo(co,level,'Slnu')else d=debug.getinfo(level,'Slnu')end
                if not d then out[#out+1]='nil';break end
                out[#out+1]=table.concat({d.what,d.currentline,d.linedefined,d.lastlinedefined,d.nups,tostring(d.name),d.namewhat},':')
            end
        end
        local function tail(n)if n==0 then stack();return end;return tail(n-1)end
        tail(5)
        local co=coroutine.create(function()local function f()coroutine.yield();error('failure')end;return f()end)
        stack(co);coroutine.resume(co);stack(co);coroutine.resume(co);stack(co)
        co=coroutine.create(function()return 1 end);coroutine.resume(co);stack(co)
        local function nested(n)if n==0 then stack(coroutine.running());return end;nested(n-1)end
        co=coroutine.create(function()nested(3)end);coroutine.resume(co)
        local d=debug.getinfo(-1,'nSuLf');out[#out+1]=table.concat({d.what,d.source,d.short_src,d.name,d.namewhat,d.nups,tostring(d.func),tostring(d.activelines)},':')
        out[#out+1]=tostring(debug.getinfo(10000,'invalid'))
        out[#out+1]=debug.getinfo(-1,'invalid').what or 'no fields'
        return table.concat(out,'|')
    "#);
}

#[test]
fn protected_handlers_inspect_faulting_frames_and_gc_keeps_debug_results() {
    compare(br#"
        local out={}
        local function failing()local x=2;error({x=x})end
        local ok,err=xpcall(failing,function(e)
            local d=debug.getinfo(2,'fSlnu');out[#out+1]=d.what..':'..tostring(d.func==error)..':'..tostring(d.name)
            d=debug.getinfo(3,'fSlnu');out[#out+1]=d.what..':'..tostring(d.func==failing)..':'..tostring(d.name)
            collectgarbage();return e
        end)
        out[#out+1]=tostring(ok)..':'..err.x
        local code=assert(loadstring('return function()return 42 end','@retained.lua'))()
        local d=debug.getinfo(code,'SfL');code=nil;collectgarbage();out[#out+1]=d.func();out[#out+1]=d.source
        out[#out+1]=next(d.activelines)
        return table.concat(out,'|')
    "#);
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
        .unwrap_or_else(|e| panic!("reference: {e}: {}", String::from_utf8_lossy(source)))
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
