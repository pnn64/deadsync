use super::*;
use lua_reference::{Function, Lua};

fn dump(source: &[u8], stripped: bool) -> Vec<u8> {
    let lua = Lua::new();
    let bytes = lua
        .load(source)
        .set_name("=(binary codec)")
        .into_function()
        .unwrap()
        .dump(false);
    if !stripped {
        return bytes;
    }
    // The Lua 5.1 C API has no strip argument. Construct its stripped wire
    // form explicitly, then exercise it through both loaders below.
    let mut decoder = decoded(&bytes);
    for p in &mut decoder.chunk.prototypes {
        p.source = None;
        p.lines.clear();
        p.locals.clear();
        p.names.clear();
    }
    encode(&decoder.chunk)
}
fn decoded(bytes: &[u8]) -> Decoder {
    let mut decoder = Decoder::new(0);
    assert!(decoder.resume(bytes, true).unwrap());
    decoder
}
fn encode(chunk: &Chunk) -> Vec<u8> {
    fn integer(output: &mut Vec<u8>, n: usize) {
        output.extend_from_slice(&(n as i32).to_ne_bytes());
    }
    fn string(output: &mut Vec<u8>, value: Option<&[u8]>) {
        output.extend_from_slice(&value.map_or(0, |s| s.len() + 1).to_ne_bytes());
        if let Some(s) = value {
            output.extend_from_slice(s);
            output.push(0);
        }
    }
    let mut output = header().to_vec();
    let mut work = vec![(0, false)];
    while let Some((id, debug)) = work.pop() {
        let p = &chunk.prototypes[id];
        if !debug {
            string(&mut output, p.source.as_deref());
            for n in p.defined {
                integer(&mut output, n as usize);
            }
            output.extend_from_slice(&[p.nups, p.params, p.vararg, p.stack]);
            integer(&mut output, p.code.len());
            for word in &p.code {
                output.extend_from_slice(&word.to_ne_bytes());
            }
            integer(&mut output, p.constants.len());
            for k in &p.constants {
                match k {
                    Constant::Nil => output.push(0),
                    Constant::Boolean(b) => output.extend_from_slice(&[1, u8::from(*b)]),
                    Constant::Number(n) => {
                        output.push(3);
                        output.extend_from_slice(&n.to_ne_bytes());
                    }
                    Constant::String(s) => {
                        output.push(4);
                        string(&mut output, Some(s));
                    }
                }
            }
            integer(&mut output, p.children.len());
            work.push((id, true));
            work.extend(p.children.iter().rev().map(|&id| (id, false)));
        } else {
            integer(&mut output, p.lines.len());
            for n in &p.lines {
                output.extend_from_slice(&n.to_ne_bytes());
            }
            integer(&mut output, p.locals.len());
            for l in &p.locals {
                string(&mut output, l.name.as_deref());
                integer(&mut output, l.start as usize);
                integer(&mut output, l.end as usize);
            }
            integer(&mut output, p.names.len());
            for name in &p.names {
                string(&mut output, name.as_deref());
            }
        }
    }
    output
}

#[test]
fn official_dumps_round_trip_across_every_stream_boundary() {
    let cases: &[&[u8]] = &[
        b"local a,b=...;local t={a,b,x='a\\0b'};return t.x,t[1],nil,true,false,-0",
        b"local x=1;return function(a,...)local y=2;local rest={...};return function(b)return x+y+a+b,rest[1] end end",
        b"local t={};for i=1,10 do t[i]=i end;for k,v in pairs(t)do if k<3 then t[k]=v+1 elseif k==4 then break end end;return t",
        b"local a,b=...;return a+b,a-b,a*b,a/b,a%b,a^b,-a,#b,not a,a..b,a==b,a~=b,a<=b,a>b,a>=b,a and b or a",
        b"local function f(...)return ... end;return f(f(1,2,3))",
    ];
    for source in cases {
        for stripped in [false, true] {
            let bytes = dump(source, stripped);
            crate::compile_named(&bytes, b"=binary")
                .unwrap()
                .verify()
                .unwrap();
            assert_eq!(encode(&decoded(&bytes).chunk), bytes);
            for split in 0..=bytes.len() {
                let mut decoder = Decoder::new(0);
                let _ = decoder.resume(&bytes[..split], false).unwrap();
                assert!(decoder.resume(&bytes, true).unwrap());
                assert_eq!(encode(&decoder.chunk), bytes);
            }
            let mut decoder = Decoder::new(0);
            for end in 0..bytes.len() {
                assert!(!decoder.resume(&bytes[..end], false).unwrap());
            }
            assert!(decoder.resume(&bytes, false).unwrap());
            assert_eq!(decoder.position, bytes.len());
        }
    }
}

#[test]
fn truncated_and_incompatible_headers_match_official_rejections() {
    let bytes = dump(b"return 7", false);
    let lua = Lua::new();
    for end in 1..bytes.len() {
        let error = Decoder::new(0).resume(&bytes[..end], true).unwrap_err();
        let native = lua
            .load(&bytes[..end])
            .set_name("=probe")
            .into_function()
            .unwrap_err()
            .to_string();
        assert_eq!(error, "unexpected end");
        assert!(
            native.contains("probe: unexpected end in precompiled chunk"),
            "{end}: {native}"
        );
    }
    for index in 1..HEADER_SIZE {
        let mut bad = bytes.clone();
        bad[index] ^= 1;
        assert_eq!(Decoder::new(0).resume(&bad, true), Err("bad header"));
        assert!(
            lua.load(&bad)
                .into_function()
                .unwrap_err()
                .to_string()
                .contains("bad header")
        );
    }
}

#[test]
fn all_instruction_bits_are_checked_without_native_execution() {
    let bytes = dump(
        b"local x=...;local t={a=x};if x then return t.a end;return nil",
        false,
    );
    let mut changed = 0;
    for pc in 0..decoded(&bytes).chunk.prototypes[0].code.len() {
        for bit in 0..32 {
            let mut decoder = decoded(&bytes);
            decoder.chunk.prototypes[0].code[pc] ^= 1 << bit;
            let result = Decoder::new(0).resume(&encode(&decoder.chunk), true);
            if result.is_err() {
                changed += 1;
            }
        }
    }
    assert!(changed > 100);
    let mut decoder = decoded(&bytes);
    decoder.chunk.prototypes[0].code[0] = 63;
    let error = Decoder::new(0)
        .resume(&encode(&decoder.chunk), true)
        .unwrap_err();
    assert_eq!(error, "bad code");
}

#[test]
fn oversized_counts_and_deep_trees_do_not_reserve_declared_storage() {
    let mut bytes = header().to_vec();
    bytes.extend_from_slice(&usize::MAX.to_ne_bytes());
    let mut decoder = Decoder::new(0);
    assert_eq!(decoder.resume(&bytes, true), Err("unexpected end"));
    assert!(decoder.heap_bytes() < 4096);
    let mut bytes = dump(b"return 1", true);
    let code_count = HEADER_SIZE + std::mem::size_of::<usize>() + 12;
    bytes[code_count..code_count + 4].copy_from_slice(&i32::MAX.to_ne_bytes());
    let mut decoder = Decoder::new(0);
    assert_eq!(decoder.resume(&bytes, true), Err("unexpected end"));
    assert!(decoder.heap_bytes() < 8192);
    let template = decoded(&dump(b"return", true)).chunk;
    let mut chunk = Chunk::default();
    for id in 0..201 {
        let mut p = Prototype {
            stack: 2,
            code: template.prototypes[0].code.clone(),
            ..Prototype::default()
        };
        if id < 200 {
            p.children.push(id + 1);
        }
        chunk.prototypes.push(p);
    }
    let bytes = encode(&chunk);
    assert_eq!(Decoder::new(0).resume(&bytes, true), Err("code too deep"));
}

#[test]
fn generated_bytes_and_trailing_data_are_bounded_and_deterministic() {
    let mut seed = 0x95ac871du32;
    for length in 0..256 {
        for _ in 0..8 {
            let mut bytes = header().to_vec();
            for _ in 0..length {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                bytes.push(seed as u8);
            }
            let mut decoder = Decoder::new(0);
            let _ = decoder.resume(&bytes, true);
            assert!(decoder.heap_bytes() < 100_000);
        }
    }
    let mut bytes = dump(b"return 4", false);
    let end = bytes.len();
    bytes.extend_from_slice(b"trailing bytes");
    let decoder = decoded(&bytes);
    assert_eq!(decoder.position, end);
    let lua = Lua::new();
    let function: Function = lua.load(&bytes).into_function().unwrap();
    assert_eq!(function.call::<i32>(()).unwrap(), 4);
}

#[test]
fn official_constructor_payloads_and_all_opcodes_are_preserved() {
    let cases: &[&[u8]]=&[
        b"global=3;local a,b=...;local t={x=a};t.x=b;t:method(a);return a+b,a-b,a*b,a/b,a%b,a^b,-a,#b,not a,a..b,a==b,a~=b,a<b,a<=b,a>b,a>=b,a and b or a,true,false,nil",
        b"local f;do local x=1;f=function()x=x+1;return function()return x end end end;return f(...)",
        b"local t={};for i=1,10 do t[i]=i end;for k,v in pairs(t)do if k==4 then break end end;repeat t.x=1 until t.x;while t.x do t.x=false end;return t",
    ];
    let large = format!("return {{{}}}", vec!["0"; 25_551].join(","));
    let mut coverage = 0u64;
    for source in cases
        .iter()
        .copied()
        .chain(std::iter::once(large.as_bytes()))
    {
        let bytes = dump(source, false);
        let decoder = decoded(&bytes);
        assert_eq!(encode(&decoder.chunk), bytes);
        for p in &decoder.chunk.prototypes {
            let mut pc = 0;
            while pc < p.code.len() {
                let w = p.code[pc];
                coverage |= 1u64 << (w & 63);
                pc += 1;
                if w & 63 == 34 && (w >> 14) & 511 == 0 {
                    pc += 1;
                }
            }
        }
    }
    assert_eq!(
        coverage,
        (1u64 << 38) - 1,
        "missing opcode mask: {:x}",
        ((1u64 << 38) - 1) & !coverage
    );
}

#[derive(Debug, PartialEq)]
enum Scalar {
    Nil,
    Boolean(bool),
    Number(u64),
    Bytes(Vec<u8>),
}
fn compare_execution(bytes: &[u8]) {
    compare_execution_against(bytes, bytes);
}

#[allow(unsafe_code)] // Only fixed/generated developer programs reach the oracle.
fn compare_execution_against(bytes: &[u8], expected_source: &[u8]) {
    let program = crate::compile_named(bytes, b"=ignored loader name").unwrap();
    program.verify().unwrap();
    let mut vm = crate::Vm::new();
    vm.set_instruction_limit(Some(10_000_000));
    let result = vm.execute(&program).unwrap().to_vec();
    let actual: Vec<_> = result
        .into_iter()
        .map(|v| match v {
            crate::Value::Nil => Scalar::Nil,
            crate::Value::Boolean(b) => Scalar::Boolean(b),
            crate::Value::Number(n) => Scalar::Number(n.to_bits()),
            crate::Value::String(id) => Scalar::Bytes(vm.string_bytes(id).unwrap().to_vec()),
            v => panic!("return scalar observations: {v:?}"),
        })
        .collect();
    // SAFETY: only the fixed valid official dumps in these tests are executed.
    let lua = unsafe { Lua::unsafe_new() };
    let expected: Vec<_> = lua
        .load(expected_source)
        .set_name("=(binary codec)")
        .eval::<lua_reference::MultiValue>()
        .unwrap()
        .into_iter()
        .map(|v| match v {
            lua_reference::Value::Nil => Scalar::Nil,
            lua_reference::Value::Boolean(b) => Scalar::Boolean(b),
            lua_reference::Value::Number(n) => Scalar::Number(n.to_bits()),
            lua_reference::Value::Integer(n) => Scalar::Number((n as f64).to_bits()),
            lua_reference::Value::String(s) => Scalar::Bytes(s.as_bytes().to_vec()),
            v => panic!("return reference scalar observations: {v:?}"),
        })
        .collect();
    assert_eq!(actual, expected);
}

#[test]
fn native_instructions_execute_with_shared_calls_tables_and_captures() {
    let cases: &[&[u8]] = &[
        b"local a,b=7,3;local x,y,z;return a+b,a-b,a*b,a/b,a%b,a^b,-a,not a,x==nil,true,false,a<b,a<=b,a>b,a>=b,a~=b,a and b or false",
        b"local t={x='a\\0b',7,8};function t:add(n)return self[1]+n end;global=t:add(5);t.x=t.x..4 ..'tail';return #t,t.x,t[2],global",
        b"local out={};for i=5,-3,-2 do out[#out+1]=i end;local n=0;for k,v in ipairs(out)do n=n+k*v end;local x=0;while x<3 do x=x+1 end;repeat x=x-1 until x==0;return n,x",
        b"local f,g;do local x=1;f=function(n)x=x+n;return x end;g=function()return x end end;collectgarbage();return f(3),g(),f(-1),g()",
        b"local function f(...)return ... end;local function g(a,...)local r={...};return a,#r,unpack(r)end;return f(g(3,7,8,9))",
        b"local function f(a,...)return a,arg.n,arg[1],arg[2]end;return f(4,5,6)",
        b"local function f(n,a)if n==0 then return a end;return f(n-1,a+n)end;return f(10000,0)",
        b"local fs={};for i=1,6 do local a=i;fs[i]=function()return a end end;collectgarbage();return fs[1](),fs[6]()",
        b"local out={};local mt={__add=function(a,b)return a.x+b.x end,__lt=function(a,b)return a.x<b.x end,__concat=function(a,b)return a.x..':'..b.x end};local a=setmetatable({x=2},mt);local b=setmetatable({x=3},mt);return a+b,a<b,a<=b,a..b",
        b"local co=coroutine.create(function()local x=coroutine.yield(4);return x+1 end);local a,b=coroutine.resume(co);local c,d=coroutine.resume(co,8);return a,b,c,d,coroutine.status(co)",
        b"local co=coroutine.create(function()for x in function()coroutine.yield(4)end do end end);return coroutine.resume(co)",
        b"local out={};local function f()return missing()end;local ok,e=xpcall(f,debug.traceback);return ok,e",
        b"local weak=setmetatable({},{__mode='v'});local function f(x)weak[1]=x;x=nil;collectgarbage();return weak[1]==nil end;local ok=f({});return ok",
        b"local n=0;local function it()n=n+1;for _ in it do end;return nil end;local ok,e=pcall(function()for _ in it do end end);return n,ok,e",
        b"local n=0;local function it()n=n+1;for _ in it do end;return nil end;local co=coroutine.create(function()local ok,e=pcall(function()for _ in it do end end);return n,ok,e end);return coroutine.resume(co)",
        br#"local out={};local mt={__concat=function(a,b)
          out[#out+1]=type(a)..':'..type(b)
          for i=1,20 do local n,v=debug.getlocal(2,i);if not n then break end;out[#out+1]=n..':'..type(v)end
          collectgarbage();return 'x'
        end};local a=setmetatable({},mt);local b=setmetatable({},mt)
        local text=3 .. a .. b .. 4 .. '';return text,table.concat(out,'|')"#,
    ];
    for source in cases {
        for stripped in [false, true] {
            compare_execution(&dump(source, stripped));
        }
    }
    let large = format!(
        "local t={{{}}};return #t,t[1],t[25551]",
        vec!["4"; 25_551].join(",")
    );
    compare_execution(&dump(large.as_bytes(), false));
}

#[test]
fn generated_native_control_flow_and_operand_forms_match_reference() {
    let mut state = 0xc71a_3d29u32;
    for case in 0..256 {
        state = state.wrapping_mul(1664525).wrapping_add(1013904223);
        let a = (state >> 12) % 100 + 1;
        let b = (state >> 20) % 17 + 1;
        let source = format!(
            "local a,b={a},{b};local out={{}};for i=1,4 do local x=a+i;local y=b-i;out[i]=(x<y and x or y)+(x==y and 7 or 3) end;local function f(x,...)local n=0;for k,v in ipairs({{...}})do n=n+v*k end;return x+n end;return f(out[1]+out[2]+out[3]+out[4],a,b),a/b,a%b,a^2,a..':'..b,(a<b)==(b>a),not (a and b)"
        );
        compare_execution(&dump(source.as_bytes(), case % 2 == 0));
    }
}

#[test]
fn native_hooks_debug_lines_names_and_locals_match_reference() {
    let cases: &[&[u8]] = &[
        br#"local out={};local function f()local d=debug.getinfo(1,'n');out[#out+1]=tostring(d.name)..':'..d.namewhat end;local function g()f()end;g();return table.concat(out,'|')"#,
        br#"
local out={}
local function f(x)
  local a=x+1
  local d=debug.getinfo(1,'Slu');out[#out+1]=d.source..':'..d.currentline..':'..d.linedefined..':'..d.nups
  for i=1,12 do local n,v=debug.getlocal(1,i);if not n then break end;out[#out+1]=n..':'..type(v)end
  return a
end
local x=f(3)
return x,table.concat(out,'|')
"#,
        br#"
local out={}
local function h(e,l)
  local d=debug.getinfo(2,'nSl')
  out[#out+1]=e..':'..tostring(l)..':'..d.what..':'..tostring(d.name)..':'..d.currentline
end
local function f(x)
  local y=x+1
  if y<4 then y=y*2 else y=0 end
  return y
end
debug.sethook(h,'crl',3)
local x=f(2)
debug.sethook()
return x,table.concat(out,'|')
"#,
        br#"
local out={}
local n=0
debug.sethook(function(e,l)n=n+1 end,'',1)
local sum=0;for i=1,4 do sum=sum+i end
for k,v in ipairs({3,4})do sum=sum+v end
local f;do local x=7;f=function()return x end end
local x=f();debug.sethook()
return n,sum,x
"#,
    ];
    for source in cases {
        for stripped in [false, true] {
            compare_execution(&dump(source, stripped));
        }
    }
}

#[test]
fn native_entry_cells_binding_sites_and_source_inheritance_are_independent() {
    let mut decoder = decoded(&dump(b"local a,b=10,20;local f=function()return a end;local g=function()return b end;return f(),g()",false));
    let p = &mut decoder.chunk.prototypes[0];
    let closures: Vec<_> = p
        .code
        .iter()
        .enumerate()
        .filter(|(_, w)| **w & 63 == 36)
        .map(|(i, _)| i)
        .collect();
    // Two sites instantiate the same child with different local cells.
    p.code[closures[1]] &= (1 << 14) - 1;
    compare_execution(&encode(&decoder.chunk));

    let mut decoder = decoded(&dump(b"return x", false));
    let p = &mut decoder.chunk.prototypes[0];
    p.nups = 1;
    p.names = vec![Some(Box::from(b"root".as_slice()))];
    for w in &mut p.code {
        if *w & 63 == 5 {
            *w = (*w & 0x3fc0) | 4;
        }
    }
    let bytes = encode(&decoder.chunk);
    compare_execution(&bytes);
    let program = crate::compile(&bytes).unwrap();
    let mut vm = crate::Vm::new();
    assert_eq!(vm.execute(&program).unwrap(), &[crate::Value::Nil]);
    vm.collect_garbage().unwrap();
    assert_eq!(vm.execute(&program).unwrap(), &[crate::Value::Nil]);

    let mut decoder = decoded(&dump(
        b"return (function()local d=debug.getinfo(1,'S');return d.source,d.short_src end)()",
        false,
    ));
    decoder.chunk.prototypes[1].source = Some(Box::from(b"@distinct-child.lua".as_slice()));
    compare_execution(&encode(&decoder.chunk));
}

#[test]
fn source_dumps_execute_in_the_native_reference_and_shared_vm() {
    let cases: &[&[u8]] = &[
        b"local a,b=7,3;return a+b,a-b,a*b,a/b,a%b,a^b,-a,a<b,a<=b,a>b,a>=b,a~=b,not a,true,false,nil",
        b"local t={x='ab',1,2,3};function t:add(x)return self[1]+x end;return t:add(8),t.x..4 ..'end',#t",
        b"local sum=0;for i=1,10 do sum=sum+i end;return sum",
        b"local sum=0;for k,v in ipairs({2,3,4})do local x=k*v;sum=sum+x end;return sum",
        b"local f;do local x=2;f=function(n)x=x+n;return x end end;return f(3),f(4)",
        b"local function f(...)return ... end;return f(1,2,3)",
        b"local function f(a,...)return a,arg.n,arg[1]end;return f(1,2,3)",
        b"local function f(...)return ... end;local a,b,c=f(1,2,3);return a,b,c,f(f(7,8,9))",
        b"local x=0;repeat x=x+1 until x>=3;while x<6 do x=x+1 end;return x",
        b"local function f()local a,b,c,d,e end;f();return 4",
        br#"local out={};for k,v in ipairs({3,4})do local x=k+v;for i=1,16 do local n,y=debug.getlocal(1,i);if not n or n=='(*temporary)' then break end;out[#out+1]=n..':'..type(y)end end;return table.concat(out,'|')"#,
        b"return pcall(function()return ('abc'):find({})end)",
        b"local out={};for k,v in ipairs({2,3})do for j,w in ipairs({4,5})do local x=v*w;out[#out+1]=function()return k,j,x end end end;return out[1](),out[4]()",
    ];
    for source in cases {
        let program = crate::compile_named(source, b"=(binary codec)").unwrap();
        let bytes = super::dump(&program.data, program.data.entry as usize)
            .unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(source)));
        compare_execution(&bytes);
        compare_execution_against(&bytes, source);
        // Exporting the imported form keeps the native instruction/debug form.
        let imported = crate::compile(&bytes).unwrap();
        assert_eq!(
            super::dump(&imported.data, imported.data.entry as usize).unwrap(),
            bytes
        );
    }
}

#[test]
fn source_dump_register_pressure_reaches_native_limits() {
    let lua = Lua::new();
    for width in [10, 100, 200, 240, 245, 246, 247, 248, 249] {
        let ones = vec!["1"; width].join(",");
        let vars = (0..width.min(195))
            .map(|i| format!("a{i}"))
            .collect::<Vec<_>>()
            .join(",");
        let sources = [
            format!("local function f(...)return select('#',...)end;return f({ones})"),
            format!("local {vars};return {}", vec!["1+2"; width].join(",")),
            format!("local x=3;return {}", vec!["x"; width].join("+")),
            format!("local t={{{ones}}};return #t"),
        ];
        for source in sources {
            if lua.load(source.as_bytes()).into_function().is_err() {
                continue;
            }
            let program = crate::compile(source.as_bytes()).unwrap();
            let bytes =
                super::dump(&program.data, program.data.entry as usize).unwrap_or_else(|e| {
                    panic!(
                        "width={width}, registers={}: {e}: {source}",
                        program.register_count()
                    )
                });
            compare_execution_against(&bytes, source.as_bytes());
        }
    }
}

#[test]
fn official_native_dumps_reexport_like_the_c_writer() {
    let lua = Lua::new();
    let cases: &[&[u8]]=&[
        b"global=3;local a,b=...;local t={x=a};t.x=b;t:method(a);return a+b,a-b,a*b,a/b,a%b,a^b,-a,#b,not a,a..b,a==b,a~=b,a<b,a<=b,a>b,a>=b,a and b or a,true,false,nil",
        b"local f;do local x=1;f=function()x=x+1;return function()return x end end end;return f(...)",
        b"local t={};for i=1,10 do t[i]=i end;for k,v in pairs(t)do if k==4 then break end end;repeat t.x=1 until t.x;while t.x do t.x=false end;return t",
    ];
    let large = format!("return {{{}}}", vec!["0"; 25_551].join(","));
    for source in cases
        .iter()
        .copied()
        .chain(std::iter::once(large.as_bytes()))
    {
        for stripped in [false, true] {
            let bytes = dump(source, stripped);
            let program = crate::compile(&bytes).unwrap();
            let actual = super::dump(&program.data, program.data.entry as usize).unwrap();
            let expected = lua.load(&bytes).into_function().unwrap().dump(false);
            assert_eq!(actual, expected);
        }
    }
}

#[test]
fn generated_source_dump_round_trips_keep_control_flow_and_captures() {
    for n in 0..128 {
        let source = format!(
            "local a,b={},{};local out={{}};for k,v in ipairs({{2,3,4}})do local x=a+k*v;out[k]=function(y)return x+y end end;local function f(...)return ... end;return a<b,a==b,a>=b,a and b or false,out[1](b),f(out[2](a),out[3](b))",
            n % 17,
            n % 7
        );
        let program = crate::compile(source.as_bytes()).unwrap();
        let bytes = super::dump(&program.data, program.data.entry as usize).unwrap();
        compare_execution_against(&bytes, source.as_bytes());
        compare_execution(&bytes);
    }
}
