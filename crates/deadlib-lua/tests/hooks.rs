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
fn native_dump_hooks_expose_the_live_registry_hook_table() {
    compare(br#"
      local seen,key;local hits=0;local function f()return 1 end
      local function first(e)
        if e=='return' and debug.getinfo(2,'f').func==string.dump then
          local n,t=debug.getlocal(2,3);seen=t
          for k,v in pairs(t)do if v==first then key=k end end
        end
      end
      debug.sethook(first,'r');string.dump(f)
      local function second(e)if e=='return' and debug.getinfo(2,'f').func==math.abs then hits=hits+1;collectgarbage()end end
      seen[key]=second;local same=debug.gethook()==second;math.abs(-3)
      debug.sethook();local count=0;for k in pairs(seen)do count=count+1 end
      local registered=false;for k,v in pairs(debug.getregistry())do if v==seen and type(k)=='userdata' then registered=true end end
      return hits,same,registered,type(key),count
    "#);
}

#[test]
fn controls_keep_function_identity_masks_counts_and_thread_independence() {
    compare(
        br#"
        local out={}
        local function observe(thread, expected)
            local f,m,c
            if thread then f,m,c=debug.gethook(thread) else f,m,c=debug.gethook() end
            out[#out+1]=tostring(f==expected)..':'..m..':'..c
        end
        local function hook()end
        observe(nil,nil)
        for _,mask in ipairs({'','xyz','rrccll','l\0cr','cr'})do
            debug.sethook(hook,mask,0);observe(nil,hook)
        end
        for _,count in ipairs({-1,1,4,2^24-1,2^32+1,1/0,0/0})do
            debug.sethook(hook,'',count);observe(nil,hook)
        end
        debug.sethook(hook,'crl',17)
        local co=coroutine.create(function()return 1 end)
        observe(co,nil)
        debug.sethook(co,hook,'c',5);observe(co,hook);observe(nil,hook)
        debug.sethook(co);observe(co,nil)
        debug.sethook(nil,'ignored',20);observe(nil,nil)
        return table.concat(out,'|')
    "#,
    );
}

#[test]
fn call_return_and_tail_return_events_preserve_visible_frames() {
    compare(
        br#"
        local out={}
        local function h(e,l)
            local d=debug.getinfo(2,'nSfl')
            out[#out+1]=e..':'..tostring(l)..':'..d.what..':'..tostring(d.name)..':'..d.currentline
            collectgarbage()
        end
        local function f(x)return x+1 end
        local function g(x)return f(x)end
        local function p(x)local y=g(x);return y end
        debug.sethook(h,'cr')
        local x=p(3);local y=math.abs(-2);local z=pcall(f,7)
        debug.sethook()
        return x,y,z,table.concat(out,'|')
    "#,
    );
}

#[test]
fn entry_callbacks_can_change_parameters_before_tail_collapse() {
    compare(
        br#"
        local out={}
        local function f(a,b,...)return a,b,arg.n,arg[1]end
        local function g(x)return f(x,2,3,4)end
        debug.sethook(function(e)
            local d=debug.getinfo(2,'f')
            if d.func==f then
                out[#out+1]=select(2,debug.getlocal(2,1))
                out[#out+1]=select(2,debug.getlocal(3,1))
                assert(debug.setlocal(2,1,11)=='a')
            end
        end,'c')
        local a,b,n,x=g(7)
        debug.sethook()
        return a,b,n,x,table.concat(out,',')
    "#,
    );
}

#[test]
fn line_events_follow_loops_branches_calls_and_returns() {
    for body in [
        "a=1\nrepeat\n a=a+1\nuntil a==3\n",
        "local a\na=1\nwhile a<=3 do\n a=a+1\nend\n",
        "for i=1,3 do\n a=i\nend\n",
        "for i=1,4 do a=1 end",
        "if\nmath.sin(1)\nthen\n a=1\nelse\n a=2\nend\n",
        "--\nif nil then\n a=1\nelse\n a=2\nend\n",
        "for i,v in ipairs{'a','b'} do\n a=i..v\nend\n",
        "local function f(x)\n return x+1\nend\nf(3)\nf(4)\n",
    ] {
        let source = format!(
            r#"local out={{}}; local f=assert(loadstring({body:?}));debug.sethook(function(e,l)out[#out+1]=l end,'l');f();debug.sethook();return table.concat(out,',')"#
        );
        compare(source.as_bytes());
    }
}

#[test]
fn hooks_do_not_reenter_and_recover_after_callback_errors() {
    compare(br#"local lines=0;local f=assert(loadstring("local sum=0\nfor i=1,100 do\nsum=sum+i\nend\nreturn sum\n"));debug.sethook(function(e,l)lines=lines+l end,"l");local sum=f();debug.sethook();return sum+lines"#);
    compare(
        br#"
        local seen=0;local tag={};local function f()return 42 end
        debug.sethook(function(e)
            seen=seen+1
            assert(not pcall(function()error(tag,0)end))
            if seen==2 then error(tag,0)end
        end,'c')
        local ok,value=pcall(f)
        debug.sethook()
        local calls=0
        debug.sethook(function()calls=calls+1 end,'c')
        f();debug.sethook()
        return ok,value==tag,seen,calls
    "#,
    );
}

#[test]
fn count_hooks_cover_empty_numeric_loops_without_recursive_callbacks() {
    compare(
        br#"
        local results={}
        for _,step in ipairs({1,4,4000})do
            local a=0
            debug.sethook(function(e)assert(e=='count');a=a+1 end,'',step)
            a=0;for i=1,1000 do end
            debug.sethook()
            results[#results+1]=a
        end
        return unpack(results)
    "#,
    );
}

#[test]
fn coroutine_hooks_inherit_controls_without_inheriting_the_lua_callback() {
    compare(
        br#"
        local out={}
        local function h(e,l)
            local d=debug.getinfo(2,'nSl')
            out[#out+1]=e..':'..tostring(l)..':'..d.what..':'..d.currentline
            collectgarbage()
        end
        debug.sethook(h,'crl',100)
        local co=coroutine.create(function(x)
            local f,m,c=debug.gethook()
            assert(f==nil and m=='crl' and c==100)
            coroutine.yield(x)
            coroutine.yield(x+1)
            return x+2
        end)
        debug.sethook()
        local f,m,c=debug.gethook(co)
        assert(f==nil and m=='crl' and c==100)
        assert(coroutine.resume(co,10))
        debug.sethook(co,h,'crl')
        assert(coroutine.resume(co));assert(coroutine.resume(co))
        debug.sethook(co)
        return table.concat(out,'|')
    "#,
    );
}

#[test]
fn callbacks_can_resume_children_and_collect_but_cannot_yield_directly() {
    compare(
        br#"
        local out={};local once=false
        local child=coroutine.create(function()coroutine.yield(12);return 13 end)
        local co=coroutine.create(function()
            debug.sethook(function(e)
                collectgarbage()
                if not once then
                    once=true
                    local ok,v=coroutine.resume(child);out[#out+1]=v
                    ok,v=coroutine.resume(child);out[#out+1]=v
                    local ok,why=pcall(coroutine.yield,99)
                    out[#out+1]=ok and 'yielded' or 'blocked'
                end
            end,'cl')
            local a=1;a=a+1
            debug.sethook()
            return a
        end)
        local ok,v=coroutine.resume(co)
        return ok,v,coroutine.status(co),table.concat(out,',')
    "#,
    );
}

#[test]
fn changing_masks_and_handlers_inside_a_callback_takes_effect_after_it_returns() {
    compare(
        br#"
        local out={}
        local function f(a)
            local x=a+1
            return x
        end
        debug.sethook(function(e,l)
            out[#out+1]=e..':'..tostring(l)
            debug.sethook(function(e,l)
                out[#out+1]=e..':'..tostring(l)
                debug.sethook()
                assert(f(10)==11)
            end,'l')
        end,'c')
        local x=f(2)
        return x,table.concat(out,'|')
    "#,
    );
}

#[test]
fn return_callbacks_see_closed_upvalues_and_mutate_returned_registers() {
    compare(
        br#"
        local out={};local saved
        local function f()
            local x=1
            saved=function()return x end
            return x
        end
        debug.sethook(function(e)
            if debug.getinfo(2,'f').func==f then
                local n=debug.setlocal(2,1,42)
                out[#out+1]=tostring(n)..':'..saved()
                collectgarbage()
            end
        end,'r')
        local value=f()
        debug.sethook()
        return value,saved(),table.concat(out,'|')
    "#,
    );
}

#[test]
fn protected_hook_errors_keep_payload_identity_and_faulting_stacks() {
    compare(
        br#"
        local tag={};local once=false;local out={}
        local function target()return 9 end
        local function install()
            debug.sethook(function(e)
                if debug.getinfo(2,'f').func==target then error(tag,0)end
            end,'c')
        end
        local ok,value=xpcall(function()install();target()end,function(e)
            out[#out+1]=e==tag
            out[#out+1]=debug.getinfo(3,'f').func==target
            collectgarbage();return e
        end)
        debug.sethook()
        local n=0;debug.sethook(function()n=n+1 end,'c');target();debug.sethook()
        return ok,value==tag,out[1],out[2],n
    "#,
    );
}

#[test]
fn hook_function_storage_is_strong_without_rooting_its_thread_key() {
    compare(
        br#"
        local weak=setmetatable({},{__mode='v'})
        local co=coroutine.create(function()return 1 end)
        local hook=function()end
        weak[1]=hook;weak[2]=co
        debug.sethook(co,hook,'')
        hook=nil;collectgarbage();local first=weak[1]~=nil
        co=nil;collectgarbage();collectgarbage()
        return first,weak[1]~=nil,weak[2]==nil
    "#,
    );
}

#[test]
fn hook_quota_abort_restores_main_and_allows_future_hook_delivery() {
    let mut vm = Vm::new();
    vm.set_instruction_limit(Some(1000));
    let source = compile(
        br#"
        co=coroutine.create(function()
            debug.sethook(function()while true do end end,'l')
            local a=1
        end)
        coroutine.resume(co)
    "#,
    )
    .unwrap();
    assert_eq!(
        vm.execute(&source).unwrap_err().kind(),
        deadlib_lua::ErrorKind::Limit
    );
    vm.set_instruction_limit(Some(10000));
    let source = compile(
        br#"
        local n=0;debug.sethook(function()n=n+1 end,'c')
        local function f()return 1 end;f();debug.sethook()
        collectgarbage();return n,coroutine.running(),coroutine.status(co)=='dead'
    "#,
    )
    .unwrap();
    assert_eq!(
        vm.execute(&source).unwrap(),
        &[Value::Number(2.0), Value::Nil, Value::Boolean(true)]
    );
}

#[test]
fn count_event_locations_follow_generated_expression_and_control_flow_layouts() {
    for body in [
        "local a=1\na=a+2\nreturn a\n",
        "local a\na=1\nreturn a\n",
        "local a,b=1,2\nreturn a+b,a<b\n",
        "local a=1\nif a<3 then\na=2\nelse\na=3\nend\nreturn a\n",
        "local a=0\nfor i=1,3 do\na=a+i\nend\nreturn a\n",
        "local a=0\nwhile a<3 do\na=a+1\nend\nreturn a\n",
        "local a=0\nrepeat\na=a+1\nuntil a==3\nreturn a\n",
        "local t={1,2,3}\nt[1]=t[2]+1\nreturn t[1]\n",
        "local function f(x)return x+1 end\nlocal a=f(1)\nreturn a\n",
    ] {
        for step in [1, 2, 4] {
            let source = format!(
                r#"local out={{}};local f=assert(loadstring({body:?}));debug.sethook(function(e)out[#out+1]=debug.getinfo(2,'l').currentline end,'',{step});f();debug.sethook();return table.concat(out,',')"#
            );
            compare(source.as_bytes());
        }
    }
}

#[test]
fn comparison_materialization_callbacks_run_after_the_comparison_has_finished() {
    compare(
        br#"
        local f=assert(loadstring('local a=1\nlocal result=\n a==1\nreturn result,a'))
        local visits=0
        debug.sethook(function(e)
            local d=debug.getinfo(2,'fl')
            if d.func==f and d.currentline==3 then
                visits=visits+1
                if visits==2 then assert(debug.setlocal(2,1,2)=='a')end
                collectgarbage()
            end
        end,'',1)
        local a,b=f()
        debug.sethook()
        return a,b,visits
    "#,
    );
}

#[test]
fn compound_expressions_preserve_reference_instruction_events() {
    for expression in [
        "a",
        "a+b",
        "a+b*c",
        "a+b+c",
        "a*(b+c)",
        "a^b^c",
        "a+(b+c)",
        "a-b-c",
        "a-(b-c)",
        "-a",
        "not a",
        "not not a",
        "not nil",
        "not true",
        "not false",
        "not 17",
        "not 'bytes'",
        "not not 'bytes'",
        "not not not false",
        "a<b",
        "a<=b",
        "a==b",
        "a~=b",
        "a>b",
        "a>=b",
        "(a<b)==(b<c)",
        "not(a<b)",
        "not not(a<b)",
        "not(a and b)",
        "not(a or b)",
        "not((a<b)and c)",
        "not((a>b)or c)",
        "not(a and b or c)",
        "not not(a and b)",
        "not not(a or b)",
        "not((a<b)and(b<c))",
        "not((a>b)or(b<c))",
        "not(false and a)",
        "not(true and a)",
        "not(false or a)",
        "not(true or a)",
        "a and b",
        "a or b",
        "false and a",
        "true and a",
        "false or a",
        "true or a",
        "(a<b)and a or b",
        "(a<b)and(b<c)",
        "(a>b)or(b<c)",
        "a..b",
        "a..b..c",
        "(a..b)..c",
        "a..(b..c)",
        "({a,b,c})[2]",
        "({x=a+b}).x",
        "#{a,b,c}",
        "math.abs(a-b)",
        "(function(x)return x+1 end)(a)",
    ] {
        let source = format!(
            r#"local function f(a,b,c)
local value={expression}
return value
end
local out={{}};debug.sethook(function(event,line)local d=debug.getinfo(2,'fl');if d.func==f then out[#out+1]=event..':'..tostring(line)..':'..d.currentline end end,'l',1)
local value=f(2,3,4);debug.sethook();return value,table.concat(out,'|')"#
        );
        compare(source.as_bytes());
    }
}

#[test]
fn unary_instructions_read_local_mutations_at_their_own_hook_boundary() {
    for (expression, initial, replacement) in [
        ("-value", "2", "17"),
        ("not value", "false", "true"),
        ("#value", "'old'", "'replacement'"),
        ("-(value)", "2", "17"),
        ("not(value)", "false", "true"),
        ("#(value)", "'old'", "'replacement'"),
    ] {
        let source = format!(
            r#"local function f(value)
local result={expression}
return result,value
end
local visits=0;debug.sethook(function(event)local d=debug.getinfo(2,'f');if d.func==f then visits=visits+1;if visits==1 then assert(debug.setlocal(2,1,{replacement})=='value');collectgarbage()end end end,'',1)
local result,value=f({initial});debug.sethook();return result,value,visits"#
        );
        compare(source.as_bytes());
    }
}

#[test]
fn negated_short_circuits_keep_both_paths_and_nested_boolean_events() {
    for expression in [
        "a and b",
        "a or b",
        "(a and b)or c",
        "(a or b)and c",
        "a and(b or c)",
        "a or(b and c)",
        "(a and b)or(c and a)",
        "(a or b)and(c or a)",
        "not a and b",
        "not a or b",
        "a and not b",
        "a or not b",
        "false and a",
        "true or a",
        "true and a",
        "false or a",
        "(a and a)or a",
        "(a or a)and a",
        "'bytes' or a",
        "0 or a",
        "nil and a",
        "not nil and a",
        "a and false",
        "a or true",
    ] {
        for arguments in [
            "false,false,false",
            "true,true,true",
            "nil,3,false",
            "3,nil,true",
            "false,'',0",
            "'',false,nil",
        ] {
            for negations in 1..=4 {
                let prefix = "not ".repeat(negations);
                let source = format!(
                    r#"local function f(a,b,c)
local value={prefix}({expression})
return value
end
local out={{}};debug.sethook(function(event,line)local d=debug.getinfo(2,'fl');if d.func==f then out[#out+1]=event..':'..tostring(line)..':'..d.currentline end end,'l',1)
local value=f({arguments});debug.sethook();return value,table.concat(out,'|')"#
                );
                compare(source.as_bytes());
            }
        }
    }
}

#[test]
fn boolean_conditions_read_hook_mutations_and_preserve_named_destinations() {
    for expression in [
        "a or b",
        "a and b",
        "(a and b)or c",
        "(a or b)and c",
        "a and(b or c)",
        "a or(b and c)",
    ] {
        for visit in 1..=5 {
            for index in 1..=3 {
                let source = format!(
                    r#"local function f(a,b,c)
local result='old'
result=not not({expression})
return result,a,b,c
end
local out={{}};local visits=0;debug.sethook(function(event)local d=debug.getinfo(2,'fl');if d.func==f and d.currentline==3 then visits=visits+1;local name,value=debug.getlocal(2,4);out[#out+1]=name..':'..tostring(value);if visits=={visit} then assert(debug.setlocal(2,{index},false));collectgarbage()end end end,'',1)
local result,a,b,c=f(true,true,true);debug.sethook();return result,a,b,c,visits,table.concat(out,'|')"#
                );
                compare(source.as_bytes());
            }
        }
    }
}

#[test]
fn short_circuit_boolean_lines_and_callback_order_follow_the_reference() {
    for expression in [
        "not(\n a or b\n)",
        "not not(\n a\n or b)",
        "not(a\n and(b\n or c))",
        "not((a\n <b)\n or c)",
        "not(a\n or(b\n <c))",
    ] {
        for arguments in ["2,3,4", "3,2,1"] {
            let source = format!(
                r#"local function f(a,b,c)
return {expression}
end
local out={{}};debug.sethook(function(event,line)local d=debug.getinfo(2,'fl');if d.func==f then out[#out+1]=event..':'..tostring(line)..':'..d.currentline end end,'l',1)
local value=f({arguments});debug.sethook();return value,table.concat(out,'|')"#
            );
            compare(source.as_bytes());
        }
    }
    compare(br#"local old={};local result=old;local trace={};local a,b=false,true;local function right()trace[#trace+1]=result==old;collectgarbage();b=false;return b end;result=not(a or right()or b);return result,trace[1],#trace"#);
    compare(br#"local trace={};local b=true;local env=setmetatable({},{__index=function(_,k)trace[#trace+1]=k;b=false;collectgarbage();return false end});local function f()setfenv(1,env);return not not(left or b)end;return f(),table.concat(trace,'|')"#);
    compare(br#"local weak=setmetatable({},{__mode='v'});local function f(a,b)a=not not(a or b);b=nil;collectgarbage();return a,weak[1]==nil end;local p={};weak[1]=p;p=nil;return f(false,weak[1])"#);
    for expression in ["(a<b)or c", "c or(a<b)", "(a<b)and c", "c and(a<b)"] {
        let source = format!(
            r#"local trace={{}};local mt={{__lt=function(a,b)trace[#trace+1]=a.n..':'..b.n;collectgarbage();return {{}}end}};local a,b=setmetatable({{n=2}},mt),setmetatable({{n=3}},mt);local function f(a,b,c)return not not({expression})end;local out={{}};debug.sethook(function(event)if debug.getinfo(2,'f').func==f then out[#out+1]=event end end,'',1);local x=f(a,b,false);local y=f(a,b,true);debug.sethook();return x,y,table.concat(trace,'|'),table.concat(out,'|')"#
        );
        compare(source.as_bytes());
    }
}

#[test]
fn long_boolean_chains_compile_flat_and_patch_both_outcomes() {
    for count in [64, 1024, 8192] {
        let expression = vec!["a"; count].join(" or ");
        let source = format!(
            r#"local function f(a)return not not({expression})end;local hits=0;debug.sethook(function()if debug.getinfo(2,'f').func==f then hits=hits+1 end end,'',1);local a=f(true);local b=f(false);debug.sethook();return a,b,hits"#
        );
        compare(source.as_bytes());
    }
}

#[test]
fn negated_comparisons_keep_metamethods_nan_rules_and_completed_comparison_results() {
    for operator in ["<", "<=", ">", ">=", "==", "~="] {
        for negations in 1..=5 {
            for (a, b) in [
                ("2", "3"),
                ("0/0", "3"),
                ("3", "0/0"),
                ("1/0", "1/0"),
                ("-0", "0"),
            ] {
                let prefix = "not ".repeat(negations);
                let source = format!(
                    r#"local function f(a,b)
local result={prefix}(a{operator}b)
return result
end
local out={{}};debug.sethook(function(event,line)local d=debug.getinfo(2,'fl');if d.func==f then out[#out+1]=event..':'..tostring(line)..':'..d.currentline end end,'l',1)
local result=f({a},{b});debug.sethook();return result,table.concat(out,'|')"#
                );
                compare(source.as_bytes());
            }
            let prefix = "not ".repeat(negations);
            let source = format!(
                r#"local trace={{}};local mt={{__lt=function(a,b)trace[#trace+1]='lt:'..a.n..':'..b.n;collectgarbage();return {{}}end,__le=function(a,b)trace[#trace+1]='le:'..a.n..':'..b.n;return false end,__eq=function(a,b)trace[#trace+1]='eq:'..a.n..':'..b.n;return true end}};local a=setmetatable({{n=2}},mt);local b=setmetatable({{n=3}},mt);return {prefix}(a{operator}b),table.concat(trace,'|')"#
            );
            compare(source.as_bytes());
        }
    }
    compare(br#"local function f(a,b)
local result=not(a<b)
return result,a
end
local visits=0;debug.sethook(function()if debug.getinfo(2,'f').func==f then visits=visits+1;if visits==2 then assert(debug.setlocal(2,1,99)=='a');collectgarbage()end end end,'',1)
local result,a=f(2,3);debug.sethook();return result,a,visits"#);
}

#[test]
fn multiline_unary_operands_and_boolean_literals_keep_line_events_and_constant_signs() {
    for expression in [
        "-\n a",
        "#\n 'bytes'",
        "not\n a",
        "not\n(a<b)",
        "not(\n a<b\n)",
        "not\n not(\n a<b\n)",
        "not\n false",
        "not(\n 0\n)",
        "not not(\n 'x'\n)",
    ] {
        let source = format!(
            r#"local function f(a,b)
local result={expression}
return result
end
local out={{}};debug.sethook(function(event,line)local d=debug.getinfo(2,'fl');if d.func==f then out[#out+1]=event..':'..tostring(line)..':'..d.currentline end end,'l',1)
local result=f(2,3);debug.sethook();return result,table.concat(out,'|')"#
        );
        compare(source.as_bytes());
    }
    compare(b"local a=not -0;local b=not not 0;return a,b,1/0,1/-0");
    compare(b"local a=not 0;local b=not not -0;return a,b,1/-0,1/0");
}

#[test]
fn call_hooks_and_initial_nil_stores_keep_parameter_and_local_lifetimes() {
    for body in [
        "local a\nreturn a",
        "local a\na=nil\nreturn a",
        "local a\nlocal b\nreturn a,b",
        "local n=0\nrepeat\nlocal a\nn=n+1\na=3\nuntil n==2\nreturn n",
    ] {
        let source = format!(
            r#"local f=assert(loadstring({body:?}));local out={{}};debug.sethook(function(e,l)if debug.getinfo(2,'f').func==f then out[#out+1]=e..':'..tostring(l)..':'..debug.getinfo(2,'l').currentline;if e=='call' then local n=debug.setlocal(2,1,42);out[#out+1]=tostring(n)end end end,'crl');local a,b=f();debug.sethook();return a,b,table.concat(out,'|')"#
        );
        compare(source.as_bytes());
    }
}

#[test]
fn call_hook_temporary_writes_remain_rooted_during_collection() {
    compare(
        br#"
        local weak=setmetatable({},{__mode='v'});local alive=false
        local function f()local n=0;return n end
        debug.sethook(function(e)
            if debug.getinfo(2,'f').func==f then
                local t={tag=42};weak[1]=t
                assert(debug.setlocal(2,1,t)=='(*temporary)')
                t=nil;collectgarbage()
                alive=weak[1] and weak[1].tag==42
            end
        end,'c')
        local result=f();debug.sethook();collectgarbage()
        return alive,result,weak[1]==nil
    "#,
    );
}

#[test]
fn finalizer_callbacks_suppress_hooks_without_losing_pending_function_entries() {
    compare(
        br#"
        local entered,finalized=0,0;local visible=false
        local function finish()
            finalized=finalized+1
            local function nested()return 1 end;nested()
        end
        local function target(...)return arg.n end
        debug.sethook(function(e)
            local f=debug.getinfo(2,'f').func
            if f==target then entered=entered+1 end
            if f==finish then visible=true end
        end,'c')
        collectgarbage('setpause',0)
        for i=1,300 do
            local p=newproxy(true);getmetatable(p).__gc=finish;p=nil
            target()
        end
        collectgarbage();debug.sethook()
        return entered,finalized,visible
    "#,
    );
}
#[test]
fn source_concatenation_ranges_keep_reference_line_and_count_hook_events() {
    for body in [
        "return a\n ..b\n ..c",
        "return a..(b\n ..c)",
        "return (a\n ..b)..c",
        "return a..(false and b..c or b)..c",
    ] {
        let function = format!("return function(a,b,c)\n{body}\nend");
        let source = format!(
            r#"
          local f=assert(loadstring({function:?}))();local trace={{}}
          debug.sethook(function(event,line)
            local d=debug.getinfo(2,'fl')
            if d.func==f then trace[#trace+1]=event..':'..tostring(line)..':'..d.currentline end
          end,'l',1)
          local result=f('a',1,2);debug.sethook()
          return result,table.concat(trace,'|')
        "#
        );
        compare(source.as_bytes());
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
