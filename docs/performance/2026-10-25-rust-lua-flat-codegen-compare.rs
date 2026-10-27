use std::{hint::black_box, time::Instant};

#[path = "../../crates/deadlib-lua/tests/codegen.rs"]
mod fixtures;
fn main() {
    let mut sources = fixtures::generated();
    sources.extend([
        fixtures::nested_functions(59), fixtures::nested_blocks(70),
        fixtures::scoped_captures(70), fixtures::logical_chain(64, false),
        fixtures::logical_chain(64, true),
    ]);
    for (index, source) in sources.iter().enumerate() {
        let parent = lua_parent::compile(source.as_bytes()).unwrap();
        let current = lua_current::compile(source.as_bytes()).unwrap();
        assert_eq!(format!("{parent:?}"), format!("{current:?}"), "metadata fixture {index}");
    }
    eprintln!("{} complete program/metadata comparisons passed", sources.len());
    let scopes = format!(
        "local function consume(x,y)return x end;\n{}return 7",
        "do local x=1;consume(x,consume(x,0))end;\n".repeat(1000)
    );
    let captures = format!(
        "{}return 7",
        "do local a,b=1,2;local function f()return a+b end end;\n".repeat(1000)
    );
    let distinct = format!(
        "local {};return 7",
        (1..=190).map(|i| format!("a{i}")).collect::<Vec<_>>().join(",")
    );
    for (name, source, repeats) in [
        ("tiny", "local x=7;return x", 1000),
        ("scopes_1000", scopes.as_str(), 5),
        ("captures_1000", captures.as_str(), 5),
        ("distinct_190", distinct.as_str(), 100),
    ] {
        let p = lua_parent::compile(source.as_bytes()).unwrap();
        let c = lua_current::compile(source.as_bytes()).unwrap();
        assert_eq!(lua_parent::Vm::new().execute(&p).unwrap(), &[lua_parent::Value::Number(7.0)]);
        assert_eq!(lua_current::Vm::new().execute(&c).unwrap(), &[lua_current::Value::Number(7.0)]);
        for _ in 0..5 {
            black_box(lua_parent::compile(source.as_bytes()).unwrap());
            black_box(lua_current::compile(source.as_bytes()).unwrap());
        }
        for sample in 0..9 {
            for engine in if sample % 2 == 0 { [0, 1] } else { [1, 0] } {
                let now = Instant::now();
                for _ in 0..repeats {
                    if engine == 0 {
                        black_box(lua_parent::compile(black_box(source.as_bytes())).unwrap());
                    } else {
                        black_box(lua_current::compile(black_box(source.as_bytes())).unwrap());
                    }
                }
                println!("{name}\t{engine}\t{sample}\t{repeats}\t{}", now.elapsed().as_nanos() / repeats);
            }
        }
    }
}
