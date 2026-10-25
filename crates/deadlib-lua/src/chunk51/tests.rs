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
            assert_eq!(
                crate::compile_named(&bytes, b"=binary").unwrap_err().kind(),
                ErrorKind::Unsupported
            );
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
