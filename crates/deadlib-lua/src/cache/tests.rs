use super::*;
use crate::{Value, Vm, bytecode::Constant};
use std::sync::Arc;

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let root = std::env::temp_dir().canonicalize().unwrap();
        loop {
            let path = root.join(format!(
                "deadlib-lua-cache-{}-{}",
                std::process::id(),
                TEMP_ID.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => panic!("{e}"),
            }
        }
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        assert_eq!(
            self.0.canonicalize().unwrap().parent(),
            Some(std::env::temp_dir().canonicalize().unwrap().as_path())
        );
        for entry in fs::read_dir(&self.0).unwrap() {
            let entry = entry.unwrap();
            assert!(entry.file_type().unwrap().is_file());
            fs::remove_file(entry.path()).unwrap();
        }
        fs::remove_dir(&self.0).unwrap();
    }
}
fn encode(program: &Program, source: &[u8], name: &[u8]) -> Vec<u8> {
    codec::encode(
        program,
        source,
        name,
        "build-a",
        &[0; 128],
        1024 * 1024,
        1024 * 1024,
    )
    .unwrap()
}
fn decode(bytes: &[u8], source: &[u8], name: &[u8]) -> Option<Program> {
    codec::decode(bytes, source, name, "build-a", &[0; 128], 1024 * 1024)
}
fn results(program: &Program) -> Vec<Vec<u8>> {
    let mut vm = Vm::new();
    let values = vm.execute(program).unwrap().to_vec();
    values
        .into_iter()
        .map(|v| match v {
            Value::Nil => b"nil".to_vec(),
            Value::Boolean(b) => vec![u8::from(b)],
            Value::Number(n) => n.to_bits().to_le_bytes().to_vec(),
            Value::String(id) => vm.string_bytes(id).unwrap().to_vec(),
            _ => panic!("expose identity through Lua predicates"),
        })
        .collect()
}

#[test]
fn source_cache_round_trips_complete_metadata_and_number_bits() {
    for source in [
        b"return -0,0,'a\\000b\\255c',true,false,nil".as_slice(),
        b"local x=7;local function f(a,...)local t={...};return function(b)return x+a+b,t[1]end end;local g=f(4,13);local c=coroutine.create(function()local y=g(2);coroutine.yield(y);return g(3)end);local ok,a=coroutine.resume(c);local ok2,b,d=coroutine.resume(c);collectgarbage();return ok,a,ok2,b,d,g==g",
        b"local trace={};debug.sethook(function(e,l)trace[#trace+1]=e..':'..l end,'l');local a=1;if a<4 then a=a+3 end;debug.sethook();local function f(...)return arg.n,arg[1]end;return a,f(12,nil),table.concat(trace,'|')",
    ] {
        let mut program=crate::compile_named(source,b"@cache-source.lua").unwrap();
        let data=Arc::make_mut(&mut program.data);
        let entry=data.entry as usize;
        for bits in [0x8000_0000_0000_0000,0x7ff8_0000_0000_1234,0xfff0_0000_0000_0000,1] {
            let p=&mut data.prototypes[entry];
            let mut constants=p.constants.to_vec();
            constants.push(Constant::Number(f64::from_bits(bits)));
            p.constants=constants.into_boxed_slice();
        }
        let bytes=encode(&program,source,b"@cache-source.lua");
        let loaded=decode(&bytes,source,b"@cache-source.lua").unwrap();
        assert_eq!(bytes,encode(&loaded,source,b"@cache-source.lua"));
        assert_eq!(program.instruction_count(),loaded.instruction_count());
        assert_eq!(program.register_count(),loaded.register_count());
        assert_eq!(results(&program),results(&loaded));
    }
}

#[test]
fn imported_chunks_retain_native_words_sources_captures_and_debug_data() {
    for source in [
        b"local x=11;return function(a)return x+a end".as_slice(),
        b"local function f(a,...)local t={...};return a,t[2],debug.getinfo(1,'S').source end;return f(4,5,6)",
        b"local t={};for i=1,150 do t[i]=i end;return #t,t[149]",
    ] {
        let lua=lua_reference::Lua::new();
        let native=lua.load(source).set_name("@cached-native.lua").into_function().unwrap().dump(false);
        let program=crate::compile_named(&native,b"=load-error-name").unwrap();
        let bytes=encode(&program,&native,b"=load-error-name");
        let loaded=decode(&bytes,&native,b"=load-error-name").unwrap();
        assert_eq!(bytes,encode(&loaded,&native,b"=load-error-name"));
        assert_eq!(
            crate::chunk51::dump(&program.data,program.data.entry as usize).unwrap(),
            crate::chunk51::dump(&loaded.data,loaded.data.entry as usize).unwrap()
        );
        if !source.ends_with(b"end") {assert_eq!(results(&program),results(&loaded));}
        let again=crate::compile(&crate::chunk51::dump(&loaded.data,loaded.data.entry as usize).unwrap()).unwrap();
        assert_eq!(again.instruction_count(),loaded.instruction_count());
    }
}

#[test]
fn retained_native_export_ranges_and_tree_edges_are_checked() {
    let lua = lua_reference::Lua::new();
    let source = lua
        .load("local x=1;return function()return x end")
        .into_function()
        .unwrap()
        .dump(false);
    let good = crate::compile(&source).unwrap();
    for change in 0..7 {
        let mut bad = good.clone();
        let data = Arc::make_mut(&mut bad.data);
        let p = &mut data.prototypes[0];
        let n = p.debug.native.as_mut().unwrap();
        match change {
            0 => n.wire.children = Box::new([0]),
            1 => n.wire.children = Box::new([usize::MAX]),
            2 => n.wire.locals = Box::new([[0, u32::MAX]]),
            3 => n.wire.constants = p.constants.len() + 1,
            4 => n.wire.names = p.debug.upvalues.len() + 1,
            5 => n.wire.vararg ^= 2,
            _ => n.source = u32::MAX,
        }
        // usize::MAX cannot be represented on the wire and is rejected by the
        // encoder; all representable cases must fail the decoder/verifier.
        if let Some(bytes) = codec::encode(
            &bad,
            &source,
            b"=native",
            "build-a",
            &[0; 128],
            1024 * 1024,
            1024 * 1024,
        ) {
            assert!(decode(&bytes, &source, b"=native").is_none());
        } else {
            assert_eq!(change, 1);
        }
    }
}

#[test]
fn exact_source_name_build_and_lexical_context_are_required() {
    let source = b"return 3";
    let program = crate::compile(source).unwrap();
    let bytes = encode(&program, source, b"=one");
    assert!(decode(&bytes, b"return 4", b"=one").is_none());
    assert!(decode(&bytes, source, b"=two").is_none());
    assert!(codec::decode(&bytes, source, b"=one", "build-b", &[0; 128], 1024 * 1024).is_none());
    assert!(codec::decode(&bytes, source, b"=one", "build-a", &[1; 128], 1024 * 1024).is_none());
    let directory = Directory::new();
    let cache = ProgramCache::new(&directory.0);
    // Deliberately place an entry at a different source's index.
    let build = env!("DEADLIB_LUA_CACHE_BUILD");
    let context = lexer_context();
    let stale = codec::encode(
        &program,
        source,
        b"=one",
        build,
        &context,
        1024 * 1024,
        1024 * 1024,
    )
    .unwrap();
    fs::write(cache.path(b"return 4", b"=one"), stale).unwrap();
    let loaded = cache.compile_named(b"return 4", b"=one").unwrap();
    assert_eq!(loaded.status, CacheStatus::Stored);
    assert_eq!(
        results(&loaded.program),
        vec![4f64.to_bits().to_le_bytes().to_vec()]
    );
    assert_eq!(
        cache.compile_named(b"return 4", b"=one").unwrap().status,
        CacheStatus::Hit
    );
}

#[test]
fn checksum_matches_bitwise_crc_across_offsets_blocks_and_binary_inputs() {
    fn bitwise(bytes: &[u8]) -> u32 {
        let mut crc = u32::MAX;
        for &byte in bytes {
            crc ^= u32::from(byte);
            for _ in 0..8 {
                crc = if crc & 1 == 0 {
                    crc >> 1
                } else {
                    (crc >> 1) ^ 0x82f63b78
                };
            }
        }
        !crc
    }
    let mut random = 0x123456789abcdef0u64;
    let bytes: Vec<_> = (0..8192)
        .map(|_| {
            random ^= random << 13;
            random ^= random >> 7;
            random ^= random << 17;
            random as u8
        })
        .collect();
    for offset in 0..16 {
        for len in 0..=512 {
            let input = &bytes[offset..offset + len];
            assert_eq!(
                codec::checksum(input),
                bitwise(input),
                "offset {offset}, length {len}"
            );
        }
        for len in [1023, 1024, 1025, 4095, 4096, 4097, 8192 - 16] {
            let input = &bytes[offset..offset + len];
            assert_eq!(codec::checksum(input), bitwise(input));
        }
    }
    for byte in [0, 1, 127, 128, 255] {
        let input = vec![byte; 4097];
        assert_eq!(codec::checksum(&input), bitwise(&input));
    }
    for value in 0..=255u8 {
        assert_eq!(codec::checksum(&[value]), bitwise(&[value]));
    }
}

#[test]
fn truncated_damaged_and_trailing_cache_bytes_are_rejected() {
    assert_eq!(codec::checksum(b"123456789"), 0xe3069283);
    let source = b"local t={1,2,3};return t[2]";
    let bytes = encode(&crate::compile(source).unwrap(), source, b"=cache");
    for end in 0..bytes.len() {
        assert!(decode(&bytes[..end], source, b"=cache").is_none());
    }
    for i in 0..bytes.len() {
        let mut bad = bytes.clone();
        bad[i] ^= 1;
        assert!(decode(&bad, source, b"=cache").is_none(), "byte {i}");
    }
    let mut trailing = bytes;
    trailing.push(0);
    assert!(decode(&trailing, source, b"=cache").is_none());
}

#[test]
fn cache_bytecode_verification_and_decode_budgets_are_enforced() {
    let source = b"local x=3;return x";
    let good = crate::compile(source).unwrap();
    let bytes = encode(&good, source, b"=cache");
    for limit in [0, 1, 64, 128] {
        assert!(codec::decode(&bytes, source, b"=cache", "build-a", &[0; 128], limit).is_none());
        assert!(
            codec::encode(
                &good,
                source,
                b"=cache",
                "build-a",
                &[0; 128],
                limit,
                1024 * 1024
            )
            .is_none()
        );
        assert!(
            codec::encode(
                &good,
                source,
                b"=cache",
                "build-a",
                &[0; 128],
                1024 * 1024,
                limit
            )
            .is_none()
        );
    }
    for change in 0..4 {
        let mut bad = good.clone();
        let data = Arc::make_mut(&mut bad.data);
        let p = &mut data.prototypes[data.entry as usize];
        match change {
            0 => p.code[0].c = u32::MAX,
            1 => p.gc_live[0] = u32::MAX,
            2 => p.debug.hook_lines = Box::default(),
            _ => p.registers = 0,
        }
        assert!(decode(&encode(&bad, source, b"=cache"), source, b"=cache").is_none());
    }
    // Mutate valid payloads with a corrected integrity checksum to reach the
    // bounded reader/verifier, rather than testing only the checksum gate.
    for i in (0..bytes.len() - 4).step_by(3) {
        let mut bad = bytes.clone();
        let end = bad.len() - 4;
        bad[i..(i + 4).min(end)].fill(255);
        let crc = codec::checksum(&bad[..end]);
        bad[end..].copy_from_slice(&crc.to_le_bytes());
        if let Some(program) = decode(&bad, source, b"=cache") {
            program.verify().unwrap();
            assert_eq!(bad, encode(&program, source, b"=cache"));
        }
    }
}

#[test]
fn disk_cache_replaces_corruption_and_preserves_source_errors() {
    let directory = Directory::new();
    let cache = ProgramCache::new(&directory.0);
    let source = b"return 23";
    assert_eq!(cache.compile(source).unwrap().status, CacheStatus::Stored);
    assert_eq!(
        cache.clone().compile(source).unwrap().status,
        CacheStatus::Hit
    );
    let path = cache.path(source, b"=(deadlib-lua)");
    fs::write(&path, b"damaged").unwrap();
    assert_eq!(cache.compile(source).unwrap().status, CacheStatus::Stored);
    let before = fs::read(&path).unwrap();
    let invalid = b"local =";
    let expected = crate::compile(invalid).unwrap_err();
    let actual = cache.compile(invalid).unwrap_err();
    assert_eq!(actual, expected);
    assert_eq!(fs::read(&path).unwrap(), before);
    assert!(
        fs::read_dir(&directory.0)
            .unwrap()
            .all(|e| e.unwrap().path().extension().unwrap() == "dlc")
    );
}

#[test]
fn file_and_decode_limits_and_io_errors_bypass_cache_without_rejecting_lua() {
    let directory = Directory::new();
    let source = b"return 17";
    for (file, decoded) in [(0, 0), (1024 * 1024, 0), (0, 1024 * 1024)] {
        let cache = ProgramCache::new(&directory.0).with_limits(file, decoded);
        let output = cache.compile(source).unwrap();
        assert_eq!(output.status, CacheStatus::Bypassed);
        assert_eq!(
            results(&output.program),
            vec![17f64.to_bits().to_le_bytes().to_vec()]
        );
    }
    let occupied = directory.0.join("ordinary-file");
    fs::write(&occupied, b"keep").unwrap();
    let output = ProgramCache::new(&occupied).compile(source).unwrap();
    assert_eq!(output.status, CacheStatus::Bypassed);
    assert_eq!(fs::read(occupied).unwrap(), b"keep");
}

#[test]
fn parallel_writers_publish_complete_entries_without_temporary_leaks() {
    let directory = Directory::new();
    std::thread::scope(|scope| {
        let barrier = Arc::new(std::sync::Barrier::new(8));
        for _ in 0..8 {
            let barrier = barrier.clone();
            let cache = ProgramCache::new(&directory.0);
            scope.spawn(move || {
                barrier.wait();
                for _ in 0..8 {
                    let program = cache.compile(b"return 91").unwrap().program;
                    assert_eq!(
                        results(&program),
                        vec![91f64.to_bits().to_le_bytes().to_vec()]
                    );
                }
            });
        }
    });
    assert_eq!(
        ProgramCache::new(&directory.0)
            .compile(b"return 91")
            .unwrap()
            .status,
        CacheStatus::Hit
    );
    assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 1);
}

#[test]
fn reloaded_programs_have_independent_vm_bindings_and_shared_clone_lifetimes() {
    let directory = Directory::new();
    let cache = ProgramCache::new(&directory.0);
    let source = b"x=(x or 0)+1;return x,function()return x end";
    let cold = cache.compile(source).unwrap().program;
    drop(cold);
    let loaded = cache.compile(source).unwrap();
    assert_eq!(loaded.status, CacheStatus::Hit);
    let program = loaded.program;
    let clone = program.clone();
    let mut a = Vm::new();
    let mut b = Vm::new();
    assert_eq!(a.execute(&program).unwrap()[0], Value::Number(1.));
    assert_eq!(a.execute(&clone).unwrap()[0], Value::Number(2.));
    assert_eq!(b.execute(&program).unwrap()[0], Value::Number(1.));
    a.collect_garbage().unwrap();
    assert_eq!(a.execute(&program).unwrap()[0], Value::Number(3.));
    let closure = a.execute(&program).unwrap()[1];
    let root = a.pin(closure).unwrap();
    drop(program);
    drop(clone);
    a.collect_garbage().unwrap();
    assert_eq!(a.call(closure, &[]).unwrap(), &[Value::Number(4.)]);
    a.unpin(root).unwrap();
}
