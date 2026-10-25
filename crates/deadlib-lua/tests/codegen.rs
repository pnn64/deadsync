pub fn nested_functions(depth: usize) -> String {
    let mut source = String::from("local root=7\n");
    for index in 1..=depth {
        source.push_str(&format!("local function f{index}()\n"));
    }
    source.push_str("root=root+1;return root\n");
    for index in (1..=depth).rev() {
        source.push_str("end\n");
        if index > 1 {
            source.push_str(&format!("return f{index}()\n"));
        }
    }
    source.push_str("return f1(),root");
    source
}
pub fn nested_blocks(depth: usize) -> String {
    format!(
        "local x=0;{}x=x+1;{}return x",
        "do ".repeat(depth),
        "end;".repeat(depth)
    )
}
pub fn scoped_captures(depth: usize) -> String {
    let mut source = String::from("local out={}\n");
    for index in 1..=depth {
        source.push_str(&format!(
            "do local x={index};out[{index}]=function()return x end\n"
        ));
    }
    source.push_str(&"end\n".repeat(depth));
    source.push_str("local sum=0;for i=1,#out do sum=sum+out[i]()end;return sum,#out");
    source
}
pub fn logical_chain(count: usize, and: bool) -> String {
    let init = if and {
        "return n".to_owned()
    } else {
        format!("if n=={count} then return 7 end")
    };
    format!(
        "local n=0;local function f()n=n+1;{init} end;local value={};return value,n",
        vec!["f()"; count].join(if and { " and " } else { " or " })
    )
}
pub fn arithmetic_chain(count: usize, op: &str) -> String {
    format!("local x=1.25;return {}", vec!["x"; count].join(op))
}
pub fn index_chain(count: usize) -> String {
    format!("local t={{}};t.a=t;return t{}==t", ".a".repeat(count))
}
pub fn nested_constructor(depth: usize, kind: u8) -> String {
    let (open, close, step) = match kind {
        0 => ("{", "}", "t=t[1]"),
        1 => ("{key=", "}", "t=t.key"),
        2 => ("{[1]=", "}", "t=t[1]"),
        3 => ("{[false]=", "}", "t=t[false]"),
        _ => ("{[", "]=1}", "t=next(t)"),
    };
    format!(
        "local t={}7{};local n=0;while type(t)=='table' do n=n+1;{step} end;return n,t",
        open.repeat(depth),
        close.repeat(depth)
    )
}
pub fn call_chain(count: usize, kind: u8) -> String {
    if kind == 2 {
        format!(
            "local n=0;local obj={{}};function obj:again(x)assert(self==obj);n=n+(x or 1);return self end;local result=obj{};return result==obj,n",
            ":again(1)".repeat(count)
        )
    } else {
        let args = if kind == 0 { "()" } else { "(1,2)" };
        format!(
            "local n=0;local function f(a,b)n=n+(a or 1)+(b or 0);return f end;local result=f{};return result==f,n",
            args.repeat(count)
        )
    }
}
pub fn indexed_assignment(count: usize) -> String {
    let targets = (1..=count)
        .map(|i| format!("t[{i}]"))
        .collect::<Vec<_>>()
        .join(",");
    let values = (1..=count)
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join(",");
    format!("local t={{}};{targets}={values};return t[1],t[{count}]")
}
pub fn generated() -> Vec<String> {
    fn expr(seed: &mut u32, depth: u8) -> String {
        *seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let choice = *seed;
        if depth == 0 {
            return format!("mark({})", choice % 7 + 1);
        }
        let left = expr(seed, depth - 1);
        match choice % 7 {
            0 => format!("({left}+{})", expr(seed, depth - 1)),
            1 => format!("({{[1]={left},[2]=3}})[1]"),
            2 => format!("(function(z)return z end)({left})"),
            3 => format!("obj:m({left})"),
            4 => format!("factory(3)({left})"),
            5 => format!("(-{left})"),
            _ => format!(
                "({} and {left} or {})",
                if choice & 0x100 != 0 { "true" } else { "false" },
                expr(seed, depth - 1)
            ),
        }
    }
    let mut seed = 0xc0_de_51_u32;
    (0..256).map(|_| {
        let expression = expr(&mut seed, 4);
        format!("local sum=0;local function mark(x)sum=sum+1;return x end;local obj={{}};function obj:m(x)sum=sum+1;return x+2 end;local function factory(n)return function(v)sum=sum+1;return n+v end end;local result={expression};local a={{}};local i=1;a[i],i=result,2;return a[1],i,sum")
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadlib_lua::{Value, Vm, compile_named};
    use lua_reference::Lua;

    #[derive(Debug, PartialEq)]
    enum Scalar {
        Nil,
        Boolean(bool),
        Number(u64),
        Bytes(Vec<u8>),
    }
    fn check(sources: Vec<String>) {
        check_bound(sources, None);
    }
    fn check_bound(sources: Vec<String>, max_registers: Option<u16>) {
        // SAFETY: fixed developer programs run under the official protected VM.
        let lua = unsafe { Lua::unsafe_new() };
        let expected: Vec<Vec<Scalar>> = sources
            .iter()
            .map(|source| {
                let values: lua_reference::MultiValue = lua
                    .load(source.as_bytes())
                    .set_name("=codegen")
                    .eval()
                    .unwrap();
                values
                    .into_iter()
                    .map(|v| match v {
                        lua_reference::Value::Nil => Scalar::Nil,
                        lua_reference::Value::Boolean(v) => Scalar::Boolean(v),
                        lua_reference::Value::Integer(v) => Scalar::Number((v as f64).to_bits()),
                        lua_reference::Value::Number(v) => Scalar::Number(v.to_bits()),
                        lua_reference::Value::String(v) => Scalar::Bytes(v.as_bytes().to_vec()),
                        other => panic!("unexpected reference value {other:?}"),
                    })
                    .collect()
            })
            .collect();
        std::thread::Builder::new()
            .name("flat-codegen".into())
            .stack_size(512 * 1024)
            .spawn(move || {
                let mut vm = Vm::new();
                vm.set_instruction_limit(Some(5_000_000));
                for (source, expected) in sources.iter().zip(expected) {
                    let program = compile_named(source.as_bytes(), b"=codegen").unwrap();
                    if let Some(max) = max_registers {
                        assert!(program.register_count() <= max);
                    }
                    let values = vm.execute(&program).unwrap().to_vec();
                    let actual: Vec<_> = values
                        .into_iter()
                        .map(|v| match v {
                            Value::Nil => Scalar::Nil,
                            Value::Boolean(v) => Scalar::Boolean(v),
                            Value::Number(v) => Scalar::Number(v.to_bits()),
                            Value::String(id) => {
                                Scalar::Bytes(vm.string_bytes(id).unwrap().to_vec())
                            }
                            other => panic!("unexpected Rust value {other:?}"),
                        })
                        .collect();
                    assert_eq!(actual, expected, "{source}");
                }
            })
            .unwrap()
            .join()
            .unwrap();
    }

    #[test]
    fn nested_functions_and_iterative_ancestor_capture_fit_a_small_host_stack() {
        check(
            [12, 59, 99, 150, 190]
                .into_iter()
                .map(nested_functions)
                .collect(),
        );
    }
    #[test]
    fn nested_statement_scopes_fit_a_small_host_stack() {
        check([50, 120, 190].into_iter().map(nested_blocks).collect());
    }
    #[test]
    fn deeply_shadowed_captures_close_in_scope_order() {
        check([20, 120, 190].into_iter().map(scoped_captures).collect());
    }
    #[test]
    fn long_logical_expression_trees_preserve_short_circuit_calls() {
        check(
            [128, 1000, 20_000]
                .into_iter()
                .flat_map(|count| [logical_chain(count, false), logical_chain(count, true)])
                .collect(),
        );
    }
    #[test]
    fn generated_expression_contexts_preserve_values_effects_and_conflicting_assignments() {
        check(generated());
    }
    #[test]
    fn long_arithmetic_trees_use_bounded_registers_without_reassociation() {
        check_bound(
            [128, 200, 1000, 10_000]
                .into_iter()
                .flat_map(|count| {
                    ["+", "-", "*", "/", "%"]
                        .into_iter()
                        .map(move |op| arithmetic_chain(count, op))
                })
                .collect(),
            Some(4),
        );
        check(
            [
                "local a,b,c=1e16,-1e16,1;return a+b+c,a+(b+c),a-b-c,a-(b-c)",
                "local x,z=-0.0,0.0;return x+z,x-z,x*z,x/1,z/(-1)",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect(),
        );
    }
    #[test]
    fn long_index_chains_reuse_table_and_key_temporaries() {
        check_bound(
            [128, 200, 1000, 10_000]
                .into_iter()
                .map(index_chain)
                .collect(),
            Some(6),
        );
    }
    #[test]
    fn right_associated_concatenation_fits_the_source_syntax_budget() {
        check(
            [100, 150, 190]
                .into_iter()
                .map(|count| format!("local x='a';return {}", vec!["x"; count].join("..")))
                .collect(),
        );
    }
    #[test]
    fn named_destinations_remain_unchanged_until_callbacks_and_metamethods_finish() {
        check([
            "local x=10;local function f()local old=x;x=20;return old end;x=(x+1)+f();return x",
            "local x=10;local function f()x=20;return 1 end;x=x+f();return x",
            "local x=10;local function f()x=20;return 1 end;x=f()+x;return x",
            "local x=10;local function f()x=20;return 1 end;x=(x)+f();return x",
            "x=10;local function f()x=20;return 1 end;return x+f()",
            "local x=10;local function f()x=20;return 1 end;local function g()return x+f()end;return g()",
            "local x=10;local obj=setmetatable({},{__unm=function()return x end});local function f()return obj end;x=-f();return x",
            "local x=10;local seen;local function f()return {7}end;local function k()seen=x;return 1 end;x=f()[k()];return seen,x",
            "local x=10;local obj=setmetatable({},{__index=function()return x end});local function f()return obj end;x=f().a;return x",
            "local x='a';local function f()x='b';return 'c' end;x=x..f();return x",
        ].into_iter().map(str::to_owned).collect());
    }
    #[test]
    fn reused_operands_remain_roots_during_collection_callbacks() {
        check([
            "local weak=setmetatable({},{__mode='v'});local mt={__add=function(a,b)collectgarbage('collect');assert(weak[1]==a);return a end};local a=setmetatable({},mt);weak[1]=a;local r=(function()return a end)()+(function()a=nil;collectgarbage('collect');return 1 end)();collectgarbage('collect');return r==weak[1]",
            "local weak=setmetatable({},{__mode='v'});local target={[1]=7};weak[1]=target;local function get()local t=target;target=nil;return t end;local function key()collectgarbage('collect');assert(weak[1]~=nil);return 1 end;return get()[key()],weak[1]~=nil",
            "local weak=setmetatable({},{__mode='v'});local x=10;local obj=newproxy(true);getmetatable(obj).__unm=function(a)collectgarbage('collect');assert(weak[1]==a);return x end;weak[1]=obj;local function f()local a=obj;obj=nil;return a end;x=-f();return x",
        ].into_iter().map(str::to_owned).collect());
    }
    #[test]
    fn generated_mutating_operands_preserve_late_local_reads_and_captured_intermediates() {
        let mut sources = Vec::new();
        for op in ["+", "-", "*", "/", "%"] {
            for (left, right) in [
                ("a", "f(1)"),
                ("(a+1)", "f(1)"),
                ("f(1)", "a"),
                ("a", "(a+f(1))"),
                ("(a+f(1))", "(a+f(2))"),
                ("(a)", "f(2)"),
                ("factory(a)(f(1))", "a"),
                ("({a=f(1)}).a", "(a+f(2))"),
            ] {
                sources.push(format!("local a=3;local trace={{}};local function f(n)trace[#trace+1]=a;a=a+n;return n end;local function factory(x)return function(v)return x+v end end;a={left}{op}{right};return a,table.concat(trace,',')"));
            }
        }
        check(sources);
    }
    #[test]
    fn nested_list_record_and_key_constructors_fit_source_depth_limits() {
        check(
            [60, 125, 190]
                .into_iter()
                .flat_map(|depth| (0..5).map(move |kind| nested_constructor(depth, kind)))
                .collect(),
        );
    }
    #[test]
    fn chained_calls_and_methods_reuse_one_working_area() {
        check_bound(
            [300, 1000, 10_000]
                .into_iter()
                .flat_map(|count| (0..3).map(move |kind| call_chain(count, kind)))
                .collect(),
            Some(12),
        );
    }
    #[test]
    fn constructors_and_calls_preserve_preallocated_values_and_named_destinations() {
        let mut sources = vec![
            "local x={old=true};local function f()assert(x.old);return 7 end;x={key=f()};return x.key".into(),
            "local f;f=function()return f end;local before=f;f=f();return f==before".into(),
            "local f=function(x)return 'old'..x end;local function arg()f=function()return 'new'end;return 7 end;return f(arg())".into(),
            "local obj={id=1};function obj:m(x)return self.id,x end;local function arg()obj={id=2};return 7 end;return obj:m(arg())".into(),
            "local log={};local obj=setmetatable({},{__index=function()log[#log+1]='lookup';return function(self,x)log[#log+1]='call';return x end end});local function arg()log[#log+1]='arg';return 7 end;local n=obj:m(arg());return n,table.concat(log,',')".into(),
            "local function many()return 1,nil,3 end;local function pack(...)return select('#',...),...end;return pack(many())".into(),
            "local function none()end;local function pack(...)return select('#',...),...end;return pack(none())".into(),
            "local obj={};function obj:pack(...)return self==obj,select('#',...),...end;local function many()return 1,nil,3 end;return obj:pack(many())".into(),
            "local function many()return 1,nil,3 end;local t={many()};return t[1],t[2],t[3]".into(),
        ];
        let expression = format!("{}7{}", "{".repeat(190), "}".repeat(190));
        sources.push(format!("local a,b,c={expression},{expression},{expression};local total=0;for _,t in ipairs({{a,b,c}})do while type(t)=='table' do total=total+1;t=t[1]end end;return total"));
        check(sources);
    }
    #[test]
    fn literal_keys_preserve_pool_order_errors_and_large_pool_fallbacks() {
        let mut sources=vec![
            "local t={[0]=-0.0};return 1/t[0]".into(),
            "local t={[-0.0]=0.0};return 1/t[0]".into(),
            "local n=0;local function f()n=n+1;return 7 end;local ok,e=pcall(function()return {[nil]=f()}end);return n,ok,e".into(),
            "local n=0;local function f()n=n+1;return 7 end;local t={[false]=f(),[true]=f(),['x']=f(),[3]=f()};return n,t[false],t[true],t.x,t[3]".into(),
            "local k='a';local function f()k='b';return 7 end;local t={[k]=f()};return t.a,t.b".into(),
        ];
        for count in [254, 255, 256, 300] {
            let prefix = format!(
                "local pad={{{}}};",
                (1..=count)
                    .map(|n| n.to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            );
            sources.push(format!("{prefix}local log={{}};local function f(n)log[#log+1]=n;return n end;local t={{[false]=f(1),[true]=f(2),['marker']=f(3),[1]=f(4)}};return #pad,t[false],t[true],t.marker,t[1],table.concat(log,',')"));
        }
        check(sources);
    }
    #[test]
    fn reused_call_and_constructor_slots_keep_pending_objects_alive() {
        check([
            "local weak=setmetatable({},{__mode='v'});local obj={};weak[1]=obj;local function factory()local saved=obj;return function()return saved end end;local function arg()obj=nil;collectgarbage('collect');assert(weak[1]~=nil);return 1 end;local result=factory()(arg());return result==weak[1]",
            "local weak=setmetatable({},{__mode='v'});local obj={};weak[1]=obj;function obj:m(x)collectgarbage('collect');return self==weak[1],x end;local function factory()return obj end;local function arg()obj=nil;collectgarbage('collect');assert(weak[1]~=nil);return 7 end;return factory():m(arg())",
            "local weak=setmetatable({},{__mode='v'});local function item(n)local t={n};weak[n]=t;return t end;local function check()collectgarbage('collect');assert(weak[1]~=nil);return 7 end;local t={item(1),key=check(),item(2)};collectgarbage('collect');return t[1]==weak[1],t[2]==weak[2],t.key",
            "local weak=setmetatable({},{__mode='kv'});local key={};local function makekey()local k=key;weak[k]=true;key=nil;return k end;local function makevalue()collectgarbage('collect');return 7 end;local t={[makekey()]=makevalue()};local k=next(t);return t[k],weak[k]",
            "local t={};local co=coroutine.create(function()local function many()return t,nil,3 end;return coroutine.yield(many())end);local ok,a,b,c=coroutine.resume(co);collectgarbage('collect');local ok2,x,y,z=coroutine.resume(co,t,nil,5);return ok,a==t,b,c,ok2,x==t,y,z",
        ].into_iter().map(str::to_owned).collect());
    }
    #[test]
    fn generated_mixed_fields_preserve_calls_and_fifty_item_flushes() {
        let mut sources = Vec::new();
        for case in 0..128 {
            let mut fields = Vec::new();
            for i in 1..=80 + case % 12 {
                fields.push(if (i + case) % 5 == 0 {
                    format!("[mark('k{i}',{i})]=mark('v{i}',{})", i * 10)
                } else if (i + case) % 7 == 0 {
                    format!("[{i}]=mark('r{i}',{})", i * 11)
                } else {
                    format!("mark('a{i}',{i})")
                });
            }
            fields.push("pair()".into());
            sources.push(format!("local log={{}};local function mark(k,v)log[#log+1]=k;return v end;local function pair()log[#log+1]='pair';return 7,8 end;local t={{{}}};local sum=0;for i=1,100 do sum=sum+(t[i] or 0)end;return sum,t[1],t[50],table.concat(log,',')",fields.join(",")));
        }
        check(sources);
    }
    #[test]
    fn large_result_lists_reserve_slots_as_each_expression_finishes() {
        let names = (1..=199)
            .map(|i| format!("a{i}"))
            .collect::<Vec<_>>()
            .join(",");
        let args = (1..=247)
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let nested = format!("{}7{}", "{".repeat(190), "}".repeat(190));
        let rest = std::iter::repeat_n("1", 198).collect::<Vec<_>>().join(",");
        check(vec![
            format!(
                "local function f(...)return ... end;local {names}=f({args});collectgarbage('collect');return a1,a100,a199"
            ),
            format!(
                "local function f(...)return ... end;{names}=f({});return a1,a100,a199",
                (1..=48)
                    .map(|i| i.to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            format!(
                "local {names}={nested},{rest};local n=0;while type(a1)=='table' do n=n+1;a1=a1[1] end;return n,a1,a199"
            ),
            format!(
                "local function f()return {nested},{rest} end;local {names}=f();collectgarbage('collect');return type(a1),a199"
            ),
            format!(
                "local function f(...)local {names}=...;collectgarbage('collect');return a1,a199 end;return f({args})"
            ),
        ]);
    }
    #[test]
    fn many_literal_indexed_targets_use_bounded_storage() {
        check_bound(
            [100, 150, 190, 200]
                .into_iter()
                .map(indexed_assignment)
                .collect(),
            Some(203),
        );
    }
    #[test]
    fn deferred_index_keys_preserve_pool_order_conflicts_and_store_order() {
        let mut sources = [
            "local t={};local old=t;local function f()t={};return 7 end;t[1]=f();return old[1],t[1]",
            "t={};local old=t;local function f()t={};return 7 end;t[1]=f();return old[1],t[1]",
            "local t={};local old=t;local function f()local function g()t={};return 7 end;t[1]=g()end;f();return old[1],t[1]",
            "local t={};local old=t;t[1],t=7,{};return old[1],t[1]",
            "local t={};local old=t;t,t[1]={},7;return old[1],t[1]",
            "local t={};local old=t;t[t],t=7,{};return old[old],t[old]",
            "local t={};local k=1;t[k],k=7,2;return t[1],t[2],k",
            "local t={};local old=t;local k=t;t[k],k,t=7,1,{};return old[old],k,t[old]",
            "local t={};t[-0.0],t[1]=0.0,1;return 1/t[0],t[1]",
            "local log={};local t=setmetatable({},{__newindex=function(_,k,v)log[#log+1]=k..':'..v end});t.a,t.b,t.a=1,2,3;return table.concat(log,',')",
            "local n=0;local function f()n=n+1;return 7 end;local t={};local ok,e=pcall(function()t[nil]=f()end);return n,ok,e",
            "local function f(...)\nlocal t=nil\nt[\n1\n]=\n...\nend;return pcall(f,7)",
            "local function f(...)\nlocal t=nil;local i=1\nt[\ni\n]=\n...\nend;return pcall(f,7)",
            "local function f()\nlocal t=nil\nt[1]=math.abs(\n-1\n)\nend;return pcall(f)",
            "local log={};local function key(n)log[#log+1]='k'..n;return n end;local function value(n)log[#log+1]='v'..n;return n end;local t={};t[key(1)],t[key(2)]=value(1),value(2);return t[1],t[2],table.concat(log,',')",
        ].into_iter().map(str::to_owned).collect::<Vec<_>>();
        for count in [254, 255, 256, 300] {
            let prefix = format!(
                "local pad={{{}}};",
                (1..=count)
                    .map(|i| i.to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            );
            sources.push(format!("{prefix}local t={{}};t[false],t[true],t['marker'],t[-0.0]=1,2,3,0.0;return #pad,t[false],t[true],t.marker,1/t[0]"));
        }
        for case in 0..128 {
            let count = 24 + case % 24;
            let mut targets = (1..=count)
                .map(|i| match (i + case) % 4 {
                    0 => format!("t[{i}]"),
                    1 => format!("t.field{i}"),
                    2 => "t[i]".into(),
                    _ => format!("t[key({i})]"),
                })
                .collect::<Vec<_>>();
            targets.extend(["i".into(), "t".into()]);
            let mut values = (1..=count)
                .map(|i| format!("value({i})"))
                .collect::<Vec<_>>();
            values.extend(["7".into(), "{}".into()]);
            sources.push(format!("local log={{}};local t={{}};local old=t;local i=1;local function key(n)log[#log+1]='k'..n..':'..i;return n end;local function value(n)log[#log+1]='v'..n..':'..i;i=i+1;return n end;{}={};local sum,count=0,0;for _,v in pairs(old)do sum=sum+v;count=count+1 end;return sum,count,i,old~=t,table.concat(log,',')",targets.join(","),values.join(",")));
        }
        check(sources);
    }
    #[test]
    fn indexed_target_callbacks_keep_pending_objects_alive() {
        check([
            "local weak=setmetatable({},{__mode='v'});local function item(n)local t={n};weak[n]=t;return t end;local seen=0;local obj=setmetatable({},{__newindex=function(t,k,v)collectgarbage('collect');assert(weak[1]~=nil and weak[2]~=nil);rawset(t,k,v);seen=seen+1 end});obj.a,obj.b=item(1),item(2);collectgarbage();return seen,obj.a==weak[1],obj.b==weak[2]",
            "local weak=setmetatable({},{__mode='v'});local target={};weak[1]=target;local function lhs()local t=target;target=nil;return t end;local function rhs()collectgarbage('collect');assert(weak[1]~=nil);return 7 end;lhs().a=rhs();return weak[1].a",
            "local weak=setmetatable({},{__mode='v'});local function key()local k={};weak[1]=k;return k end;local function rhs()collectgarbage('collect');assert(weak[1]~=nil);return 7 end;local t={};t[key()]=rhs();return t[weak[1]]",
            "local obj={};local co=coroutine.create(function()obj.a,obj.b=coroutine.yield(1)end);local ok,n=coroutine.resume(co);collectgarbage();local ok2=coroutine.resume(co,7,9);return ok,n,ok2,obj.a,obj.b",
        ].into_iter().map(str::to_owned).collect());
    }
    #[test]
    fn reference_compact_keys_survive_larger_backend_pools() {
        let mut sources = Vec::new();
        for count in [100, 150, 190, 200] {
            let pad = (1..=253)
                .map(|n| n.to_string())
                .collect::<Vec<_>>()
                .join(",");
            sources.push(format!(
                "do local pad={{true,false,nil}}end;do local pad={{{pad}}}end;{}",
                indexed_assignment(count)
            ));
            sources.push(format!(
                "{}{}",
                (1000..1300)
                    .map(|n| format!("if {n} then end;"))
                    .collect::<String>(),
                indexed_assignment(count)
            ));
        }
        sources.push("a={};a['a']=7;return a.a".into());
        sources.push("local t=setmetatable({},{__index=function()return function(self)return self end end});local x=t:m();x['m']=7;return x.m".into());
        check_bound(sources, Some(203));
    }
    #[test]
    fn folded_keys_follow_reference_operands_and_preserve_effects() {
        let mut sources = Vec::new();
        for key in [
            "not nil",
            "not true",
            "false or 1",
            "nil or 'field'",
            "true and 1",
            "not false and 1",
            "(1+2)",
            "-0.0",
        ] {
            let targets = std::iter::repeat_n(format!("t[{key}]"), 150)
                .collect::<Vec<_>>()
                .join(",");
            let values = (1..=150)
                .map(|n| n.to_string())
                .collect::<Vec<_>>()
                .join(",");
            sources.push(format!("local t={{}};{targets}={values};return t[{key}]"));
        }
        sources.extend([
            "local log={};local function f(v)log[#log+1]=tostring(v);return v end;local t={};t[f(false) or 'field'],t[f(true) and 1]=f(7),f(9);return t.field,t[1],table.concat(log,',')",
            "local n=0;local function f()n=n+1;return 7 end;local t={};t[true or f()],t[false and f()]=1,2;return n,t[true],t[false]",
            "local t={[not nil]=7,[nil or 'field']=9,[true and 1]=11};return t[true],t.field,t[1]",
        ].into_iter().map(str::to_owned));
        check(sources);
    }
    #[test]
    fn zero_pool_signs_ignore_unused_conditions_and_stay_function_local() {
        let mut sources=[
            "if 0 then end;return 1/-0.0",
            "if -0.0 then end;return 1/0",
            "if 0 then end;local x=-0.0;return 1/x,1/0",
            "local x;x=-0.0;return 1/x,1/0",
            "local x;x=0;return 1/x,1/-0.0",
            "do local x=-0.0 end;return 1/0",
            "if 0 then end;local t={[-0.0]=0};return 1/t[0]",
            "if -0.0 then end;local t={[0]=-0.0};return 1/t[0]",
            "local function f()if 0 then end;return -0.0 end;local function g()if -0.0 then end;return 0 end;return 1/f(),1/g(),1/-0.0",
            "local f=assert(loadstring('if 0 then end;return -0.0'));return 1/f()",
        ].into_iter().map(str::to_owned).collect::<Vec<_>>();
        for prefix in [
            "if 0 then end;",
            "if -0.0 then end;",
            "",
            "do local t={true,false,nil}end;",
        ] {
            for first in ["0", "-0.0", "1-1", "-(1-1)"] {
                for second in ["0", "-0.0", "1-1", "-(1-1)"] {
                    for declaration in [true, false] {
                        sources.push(format!(
                            "{prefix}local x{};{}return 1/x,1/({second})",
                            if declaration {
                                format!("={first}")
                            } else {
                                String::new()
                            },
                            if declaration {
                                String::new()
                            } else {
                                format!("x={first};")
                            }
                        ));
                    }
                }
            }
        }
        check(sources);
    }
}
