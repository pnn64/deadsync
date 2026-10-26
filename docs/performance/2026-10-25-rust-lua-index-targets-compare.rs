use std::{hint::black_box, time::Instant};

#[path = "../../crates/deadlib-lua/tests/codegen.rs"]
mod fixtures;
macro_rules! observe {
    ($engine:ident, $program:expr) => {{
        let mut vm = $engine::Vm::new();
        let values = vm.execute($program).unwrap().to_vec();
        values.into_iter().map(|v| match v {
            $engine::Value::Nil => (0u8,0u64,Vec::<u8>::new()),
            $engine::Value::Boolean(v) => (1,u64::from(v),Vec::new()),
            $engine::Value::Number(v) => (2,v.to_bits(),Vec::new()),
            $engine::Value::String(id) => (3,0,vm.string_bytes(id).unwrap().to_vec()),
            other => panic!("unexpected {other:?}"),
        }).collect::<Vec<_>>()
    }};
}
fn main() {
    let mut sources = fixtures::generated();
    sources.extend([
        fixtures::nested_functions(59), fixtures::nested_blocks(70),
        fixtures::scoped_captures(70), fixtures::logical_chain(64, false),
        fixtures::logical_chain(64, true), fixtures::arithmetic_chain(64,"+"),
        fixtures::index_chain(64),
        fixtures::nested_constructor(40,0), fixtures::nested_constructor(40,1),
        fixtures::nested_constructor(40,4), fixtures::call_chain(40,0),
        fixtures::call_chain(40,1), fixtures::call_chain(40,2),
        fixtures::indexed_assignment(80),
    ]);
    for (index, source) in sources.iter().enumerate() {
        let parent = lua_parent::compile(source.as_bytes()).unwrap();
        let current = lua_current::compile(source.as_bytes()).unwrap();
        assert_eq!(observe!(lua_parent,&parent), observe!(lua_current,&current), "fixture {index}");
    }
    eprintln!("{} scalar-result comparisons passed", sources.len());

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
    let quoted = format!("local s='{}';return 7", "x".repeat(100_000));
    for (name, source, repeats) in [
        ("tiny", "local x=7;return x", 1000),
        ("scopes_1000", scopes.as_str(), 5),
        ("captures_1000", captures.as_str(), 5),
        ("distinct_190", distinct.as_str(), 100),
        ("quoted_100000", quoted.as_str(), 10),
    ] {
        let p = lua_parent::compile(source.as_bytes()).unwrap();
        let c = lua_current::compile(source.as_bytes()).unwrap();
        println!("layout\t{name}\t{}\t{}\t{}\t{}",p.register_count(),c.register_count(),p.instruction_count(),c.instruction_count());
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

    for (name,source,expected,repeats) in [
        ("execute_indexed_assignment","local t={};for n=1,1000 do t[1],t[2],t[3],t[4],t[5],t[6],t[7],t[8],t[9],t[10],t[11],t[12],t[13],t[14],t[15],t[16]=n+0,n+1,n+2,n+3,n+4,n+5,n+6,n+7,n+8,n+9,n+10,n+11,n+12,n+13,n+14,n+15 end;return t[16]",1015.0,30),
        ("execute_for","local s=0;for i=1,100000 do s=s+i end;return s",5000050000.0,20),
        ("execute_while","local i,s=0,0;while i<100000 do i=i+1;s=s+i end;return s",5000050000.0,20),
        ("execute_fib","local function f(n)if n<2 then return n end;return f(n-1)+f(n-2)end;return f(20)",6765.0,10),
        ("execute_operand_reuse","local x=1;local y=0;for i=1,10000 do y=(x+x)+(x+x)+(x+x)+(x+x)end;return y",8.0,50),
        ("execute_call_table","local function f(t)return t end;local sum=0;for i=1,1000 do sum=sum+f({key=i}).key end;return sum",500500.0,30),
    ] {
        let p=lua_parent::compile(source.as_bytes()).unwrap();let c=lua_current::compile(source.as_bytes()).unwrap();
        println!("layout\t{name}\t{}\t{}\t{}\t{}",p.register_count(),c.register_count(),p.instruction_count(),c.instruction_count());
        let mut pv=lua_parent::Vm::new();let mut cv=lua_current::Vm::new();
        assert_eq!(pv.execute(&p).unwrap(),&[lua_parent::Value::Number(expected)]);
        assert_eq!(cv.execute(&c).unwrap(),&[lua_current::Value::Number(expected)]);
        for _ in 0..5 {black_box(pv.execute(&p).unwrap());black_box(cv.execute(&c).unwrap());}
        for sample in 0..9 {for engine in if sample%2==0 {[0,1]}else{[1,0]} {
            let now=Instant::now();for _ in 0..repeats {
                if engine==0 {black_box(pv.execute(black_box(&p)).unwrap());}
                else {black_box(cv.execute(black_box(&c)).unwrap());}
            }
            println!("{name}\t{engine}\t{sample}\t{repeats}\t{}",now.elapsed().as_nanos()/repeats);
        }}
    }
}
