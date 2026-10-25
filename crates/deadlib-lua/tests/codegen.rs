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
}
