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
}
