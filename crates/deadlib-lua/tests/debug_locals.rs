use deadlib_lua::{ErrorKind, Value, Vm, compile};
use lua_reference::{Lua, MultiValue, Value as Ref};
#[derive(Debug, PartialEq)]
enum Scalar {
    Nil,
    Boolean(bool),
    Number(u64),
    Bytes(Vec<u8>),
}

#[test]
fn native_string_coercions_update_observable_argument_slots() {
    for (name, arguments, count) in [
        ("string.sub", "12345,2", 2),
        ("string.format", "'%s:%q',17,19", 3),
        ("tonumber", "1234,16", 2),
        ("loadstring", "1234,17", 2),
        ("tostring", "1234", 1),
        ("os.getenv", "123456789", 1),
    ] {
        compare(format!("local seen={{}};local fn={name};debug.sethook(function(e)if e=='return' and debug.getinfo(2,'f').func==fn then for i=1,{count} do local n,v=debug.getlocal(2,i);seen[i]=type(v)..':'..tostring(v)end end end,'r');fn({arguments});debug.sethook();return table.concat(seen,'|')").as_bytes());
    }
}

#[test]
fn gsub_reloads_the_replacement_after_each_callback() {
    compare(br#"local out={};local function second(s)out[#out+1]='new:'..s;return 'N'end;local result,n=string.gsub('abc','.',function(s)out[#out+1]='old:'..s;debug.setlocal(2,3,second);return 'O'end);return result,n,table.concat(out,'|')"#);
}

#[test]
fn gsub_coerces_numeric_replacements_only_when_used() {
    for (pattern, limit) in [("'a'", "1"), ("'z'", "1"), ("'a'", "0")] {
        compare(format!("local kind;local fn=string.gsub;debug.sethook(function(e)if e=='return' and debug.getinfo(2,'f').func==fn then local n,v=debug.getlocal(2,3);kind=type(v)end end,'r');local s,n=fn('abc',{pattern},17,{limit});debug.sethook();return s,n,kind").as_bytes());
    }
}

#[test]
fn gsub_buffer_pieces_are_visible_and_rooted_between_callbacks() {
    for width in [0, 1, 255, 256, 511, 512, 513, 1024, 8191, 8192, 8193, 40000] {
        compare(format!("local out={{}};local n=0;local result,count=string.gsub(string.rep('a',12),'.',function(s)n=n+1;local row={{}};for i=1,20 do local name,v=debug.getlocal(2,i);if not name then break end;row[#row+1]=type(v)..':'..(type(v)=='string' and #v or '')end;out[#out+1]=table.concat(row,',');collectgarbage();return string.rep('x',{width})end);return #result,count,table.concat(out,'|')").as_bytes());
    }
}

#[test]
fn gsub_live_replacements_follow_native_stack_and_type_dispatch() {
    for replacement in [
        "'R'",
        "17",
        "{a='A',b='B',c='C'}",
        "nil",
        "false",
        "true",
        "newproxy()",
        "coroutine.create(function()end)",
    ] {
        for extra in ["", ",3", ",3,'extra'"] {
            compare(format!("local n=0;local ok,s,c=pcall(function()return string.gsub('abc','.',function(x)n=n+1;debug.setlocal(2,3,{replacement});return 'O'end{extra})end);return ok,s,c,n").as_bytes());
        }
    }
}

#[test]
fn gsub_releases_replaced_functions_and_tables() {
    compare(br#"local weak=setmetatable({},{__mode='v'});local gone;local function second(s)collectgarbage();gone=weak[1]==nil;return 'N'end;local function first()local f=function(s)debug.setlocal(2,3,second);return 'O'end;weak[1]=f;return f end;local s,n=string.gsub('abc','.',first());return s,n,gone"#);
    compare(br#"local weak=setmetatable({},{__mode='v'});local gone;local function second(s)collectgarbage();gone=weak[1]==nil;return 'N'end;local function first()local t=setmetatable({},{__index=function(t,s)debug.setlocal(2,3,second);return 'O'end});weak[1]=t;return t end;local s,n=string.gsub('abc','.',first());return s,n,gone"#);
}

#[test]
fn native_multiple_results_preserve_arguments_for_return_hooks() {
    for (name, arguments, count) in [
        ("string.byte", "12345,2,4", 3),
        ("math.modf", "1.25", 1),
        ("math.frexp", "17", 1),
        ("string.find", "12345,23", 2),
        ("unpack", "{1,2,3,4,5,6}", 1),
    ] {
        compare(format!("local seen={{}};local fn={name};debug.sethook(function(e)if e=='return' and debug.getinfo(2,'f').func==fn then for i=1,{count} do local n,v=debug.getlocal(2,i);seen[#seen+1]=type(v)..':'..(type(v)=='number' and v or type(v)=='string' and v or '')end end end,'r');fn({arguments});debug.sethook();return table.concat(seen,'|')").as_bytes());
    }
}

#[test]
fn gsub_buffer_concatenations_resume_lua_metamethods() {
    for width in [513, 1024, 8193, 20000] {
        compare(format!("local out={{}};debug.setmetatable(false,{{__concat=function(a,b)out[#out+1]=type(a)..':'..type(b);collectgarbage();return 'C'end}});local n=0;local ok,s,c=pcall(function()return string.gsub('abc','.',function()n=n+1;debug.setlocal(2,3,false);return string.rep('O',{width})end)end);return ok,s,c,n,table.concat(out,'|')").as_bytes());
    }
}

#[test]
fn native_partial_errors_preserve_coerced_arguments_for_handlers() {
    compare(br#"local seen;local ok,e=xpcall(function()return string.find(123,'(')end,function(e)local n,v=debug.getlocal(2,1);seen=type(v)..':'..tostring(v);return e end);return ok,e,seen"#);
    compare(br#"local f=assert(io.tmpfile());f:write('a\nb\n');f:seek('set');local seen;local ok,e=xpcall(function()return f:read('*l',false)end,function(e)local n,v=debug.getlocal(2,1);seen=type(v);return e end);f:close();return ok,e,seen"#);
}

#[test]
fn generated_gsub_buffer_argument_changes_match_native_stack_effects() {
    for width in [0, 511, 512, 513, 1024, 4096, 8192, 10000] {
        for replacement in ["nil", "false", "true", "17"] {
            for extra in ["", ",3", ",3,'extra'"] {
                compare(format!("local n=0;local ok,s,c=pcall(function()return string.gsub('abc','.',function(x)n=n+1;debug.setlocal(2,3,{replacement});return string.rep('O',{width})end{extra})end);return ok,s,c,n").as_bytes());
            }
        }
    }
}

#[test]
fn gsub_pending_buffer_calls_keep_errors_yield_rules_and_nested_jobs() {
    for body in [
        "error('stop',0)",
        "return coroutine.yield('x')",
        "return string.gsub('aaa','a','b')",
        "debug.setlocal(2,3,17);return 'C'",
    ] {
        compare(format!("debug.setmetatable(false,{{__concat=function(a,b){body} end}});local n=0;local ok,s,c=pcall(function()return string.gsub('abc','.',function()n=n+1;debug.setlocal(2,3,false);return string.rep('O',513)end)end);return ok,s,c,n").as_bytes());
    }
}

#[test]
fn aborting_a_pending_gsub_buffer_call_does_not_retain_its_job() {
    let source=compile(b"debug.setmetatable(false,{__concat=function()started=true;local n=0;for i=1,1000000 do n=n+i end;return 'C'end});return string.gsub('abc','.',function()debug.setlocal(2,3,false);return string.rep('O',513)end)").unwrap();
    let mut vm = Vm::new();
    vm.set_instruction_limit(Some(1000));
    assert_eq!(vm.execute(&source).unwrap_err().kind(), ErrorKind::Limit);
    assert_eq!(vm.global(b"started"), Value::Boolean(true));
    vm.set_instruction_limit(None);
    vm.collect_garbage().unwrap();
    let recovery = compile(b"local s,n=string.gsub('aa','a','b');return s=='bb' and n==2").unwrap();
    assert_eq!(vm.execute(&recovery).unwrap(), &[Value::Boolean(true)]);
}

#[test]
fn sort_return_hook_sees_the_normalized_optional_comparator_slot() {
    compare(br#"local name,kind;local sort=table.sort;debug.sethook(function(e)if e=='return' and debug.getinfo(2,'f').func==sort then local n,v=debug.getlocal(2,2);name=n;kind=type(v)end end,'r');sort({});debug.sethook();return name,kind"#);
}

#[test]
fn sort_reloads_comparator_and_table_arguments_after_callbacks() {
    compare(br#"local out={};local first=true;local a={3,1,2};local b={7,6,5};local function second(x,y)out[#out+1]='new:'..x..':'..y;return x<y end;table.sort(a,function(x,y)out[#out+1]='old:'..x..':'..y;if first then first=false;debug.setlocal(2,1,b);debug.setlocal(2,2,second)end;return x<y end);return table.concat(out,'|'),table.concat(a,','),table.concat(b,',')"#);
}

#[test]
fn sort_releases_replaced_native_argument_roots() {
    compare(br#"local weak=setmetatable({},{__mode='v'});local gone=false;local function second(a,b)collectgarbage();gone=weak[1]==nil;return a<b end;local function make()local function first(a,b)debug.setlocal(2,2,second);return a<b end;weak[1]=first;return first end;local t={3,2,1};table.sort(t,make());return gone,table.concat(t,',')"#);
    compare(br#"local weak=setmetatable({},{__mode='v'});local other={7,6,5};local gone=false;local first=true;local function make()local t={3,2,1};weak[1]=t;return t end;table.sort(make(),function(a,b)if first then first=false;debug.setlocal(2,1,other);collectgarbage();gone=weak[1]==nil end;return a<b end);return gone,table.concat(other,',')"#);
}

#[test]
fn native_traversal_and_completed_sort_return_before_rechecking_mutated_tables() {
    compare(br#"local a=table.foreach({7},function()debug.setlocal(2,1,false);return 42 end);local b=table.foreachi({7},function()debug.setlocal(2,1,false);return 43 end);local ok,e=pcall(function()table.sort({1,2},function()debug.setlocal(2,1,false);return false end)end);return a,b,ok,e"#);
    compare(br#"local n=0;local ok,e=pcall(function()table.sort({1,2,3},function(a,b)n=n+1;if n==3 then debug.setlocal(2,1,false)end;return a<b end)end);return ok,e,n"#);
}

#[test]
fn sort_accepts_a_callable_replacement_after_initial_function_validation() {
    compare(br#"local out={};local replacement=setmetatable({},{__call=function(t,a,b)out[#out+1]='new';return a<b end});local first=true;local t={3,2,1};table.sort(t,function(a,b)out[#out+1]='old';if first then first=false;debug.setlocal(2,2,replacement)end;return a<b end);return table.concat(out,','),table.concat(t,',')"#);
}

#[test]
fn generated_sort_argument_changes_preserve_pending_values_and_comparison_order() {
    for count in 2..=6 {
        for change in 1..=6 {
            compare(format!("local a,b={{}},{{}};for i=1,{count} do a[i]={count}-i+1;b[i]=i+10 end;local n=0;local out={{}};local function second(x,y)n=n+1;out[#out+1]='new:'..tostring(x)..':'..tostring(y);return x<y end;local ok,e=pcall(function()table.sort(a,function(x,y)n=n+1;out[#out+1]='old:'..tostring(x)..':'..tostring(y);if n=={change} then debug.setlocal(2,1,b);debug.setlocal(2,2,second)end;return x<y end)end);return ok,e,n,table.concat(a,','),table.concat(b,','),table.concat(out,'|')").as_bytes());
        }
    }
}

#[test]
fn replaced_sort_comparators_use_default_and_dynamic_call_semantics() {
    for replacement in ["nil", "false", "17"] {
        compare(format!("local t={{3,2,1}};local n=0;local ok,e=pcall(function()table.sort(t,function(a,b)n=n+1;debug.setlocal(2,2,{replacement});return a<b end)end);return ok,e,n,table.concat(t,',')").as_bytes());
    }
    compare(br#"local out={};debug.setmetatable(0,{__call=function(self,a,b)out[#out+1]=self;return a<b end});local t={3,2,1};table.sort(t,function(a,b)debug.setlocal(2,2,17);return a<b end);return table.concat(t,','),table.concat(out,',')"#);
}

#[test]
fn traversal_reloads_table_and_callable_callback_slots_between_iterations() {
    for function in ["foreach", "foreachi"] {
        compare(format!("local out={{}};local replacement=setmetatable({{}},{{__call=function(self,k,v)out[#out+1]='new:'..k..':'..v end}});table.{function}({{4,5,6}},function(k,v)out[#out+1]='old:'..k..':'..v;debug.setlocal(2,1,{{7,8,9}});debug.setlocal(2,2,replacement)end);return table.concat(out,'|')").as_bytes());
    }
}

#[test]
fn sort_observes_comparator_replacement_from_default_order_metamethods() {
    compare(br#"local out={};local function second(a,b)out[#out+1]='new';return a.n<b.n end;local mt={__lt=function(a,b)out[#out+1]='meta';debug.setlocal(2,2,second);return a.n<b.n end};local t={};for i=1,3 do t[i]=setmetatable({n=4-i},mt)end;table.sort(t);return t[1].n,t[2].n,t[3].n,table.concat(out,',')"#);
}

#[test]
fn traversal_state_survives_native_callbacks_with_discarded_results() {
    compare(br#"local key={1,2,3,4,5,6};local count=select('#',table.foreach({[key]=0},unpack));return count"#);
}

#[test]
fn failed_calls_keep_native_and_lua_error_locations_distinct() {
    for body in [
        "return pcall(nil)",
        "return pcall(false)",
        "return pcall(17)",
        "return pcall(tostring,setmetatable({},{__tostring=false}))",
        "return pcall(function()local f=false;return f()end)",
        "return pcall(function()local t=setmetatable({},{__call=false});return t()end)",
        "return pcall(function()local t=setmetatable({},{__add=false});return t+1 end)",
        "return pcall(function()local t=setmetatable({},{__lt=false});return t<t end)",
        "return pcall(function()return next({},17)end)",
        "return pcall(function()return table.sort({true,false})end)",
        "return pcall(function()return true<false end)",
        "return pcall(function()return {}+true end)",
        "return pcall(function()table.foreach({7},function()debug.setlocal(2,3,17)end)end)",
        "return pcall(function()local t={};setmetatable(t,{__index=t});return t.x end)",
        "return pcall(function()local t={};setmetatable(t,{__newindex=t});t.x=1 end)",
        "return pcall(function()local t={};setmetatable(t,{__index=t});return string.gsub('a','.',t)end)",
        "return pcall(function()return rawset({},nil,1)end)",
        "return pcall(function()return rawset({},0/0,1)end)",
        "return pcall(function()local t={};t[nil]=1 end)",
        "return pcall(function()local t={};t[0/0]=1 end)",
    ] {
        compare(body.as_bytes());
    }
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
