use super::*;

#[path = "rage_file_before.rs"]
mod before;

fn state(bytes: Vec<u8>) -> RageFile {
    RageFile {
        bytes,
        mode: 1,
        open: true,
        ..Default::default()
    }
}

#[test]
fn borrowed_reads_match_bytes_cursor_eof_and_error_precedence() {
    for len in [0usize, 1, 2, 15, 16, 31, 32, 63, 64, 65, 127, 128, 4096] {
        for nul in [None, Some(0), Some(len / 2), Some(len.saturating_sub(1))] {
            let mut data = (0..len)
                .map(|i| ((i * 13 + 17) % 256) as u8)
                .collect::<Vec<_>>();
            if let Some(index) = nul.filter(|&index| index < len) {
                data[index] = 0;
            }
            for start in [0, len / 2, len, len + 9] {
                for count in [-2, -1, 0, 1, 7, 8192] {
                    let mut old = before::state(data.clone());
                    let mut new = state(data.clone());
                    before::configure(&mut old, start, 1, true, false);
                    new.pos = start;
                    let a = old.read(count).map_err(|e| e.to_string());
                    let b = new
                        .read(count)
                        .map(<[u8]>::to_vec)
                        .map_err(|e| e.to_string());
                    assert_eq!(a, b, "len={len}, start={start}, count={count}, nul={nul:?}");
                    assert_eq!(before::status(&old), (new.pos, new.eof));
                }
            }
        }
    }
    for (mode, open, destroyed) in [
        (0, true, false),
        (2, true, false),
        (1, false, false),
        (1, true, true),
        (2, false, true),
    ] {
        for count in [-2, -1, 0, 10] {
            let mut old = before::state(b"data".to_vec());
            let mut new = state(b"data".to_vec());
            before::configure(&mut old, 0, mode, open, destroyed);
            new.mode = mode;
            new.open = open;
            new.destroyed = destroyed;
            assert_eq!(
                old.read(count).map_err(|e| e.to_string()),
                new.read(count)
                    .map(<[u8]>::to_vec)
                    .map_err(|e| e.to_string())
            );
            assert_eq!(before::status(&old), (new.pos, new.eof));
        }
    }
}

#[test]
fn vectorized_line_scans_preserve_crlf_nul_and_delayed_eof() {
    for data in [
        b"".to_vec(),
        b"\r".to_vec(),
        b"\n".to_vec(),
        b"\r\n\nalpha\rZ\0hidden\nlast\r".to_vec(),
        [
            vec![b'a'; 63],
            b"\0\r\n".to_vec(),
            vec![b'b'; 4096],
            b"\nend".to_vec(),
        ]
        .concat(),
    ] {
        for start in [0, 1, data.len(), data.len() + 8] {
            let mut old = before::state(data.clone());
            let mut new = state(data.clone());
            before::configure(&mut old, start, 1, true, false);
            new.pos = start;
            for _ in 0..10 {
                let a = old.get_line().map_err(|e| e.to_string());
                let b = new
                    .get_line()
                    .map(<[u8]>::to_vec)
                    .map_err(|e| e.to_string());
                assert_eq!(a, b);
                assert_eq!(before::status(&old), (new.pos, new.eof));
            }
        }
    }
    for len in [0, 1, 31, 32, 33, 63, 64, 65, 1024] {
        for at in 0..=len {
            let mut data = vec![b'x'; len];
            if at < len {
                data[at] = 0;
            }
            let expected = &data[..at.min(len)];
            assert_eq!(c_string(&data), expected);
        }
    }
    use crate::draw_capture::perf::allocations;
    let mut old = before::state(vec![b'x'; 65536]);
    let mut new = state(vec![b'x'; 65536]);
    let (_, a) = allocations::measure(|| old.read(-1).unwrap());
    let (_, b) = allocations::measure(|| {
        std::hint::black_box(new.read(-1).unwrap());
    });
    assert_eq!((a.allocs, a.allocated_bytes), (1, 65536));
    assert_eq!((b.allocs, b.reallocs, b.allocated_bytes), (0, 0, 0));
}

const CONTRACT: &str = r#"
local result={}
local function record(x) result[#result+1]=tostring(x) end
local f=Util.CreateRageFile()
assert(f:Open('data.bin',2)); assert(f:Write('alpha\r\n\nbeta\rZ')==14); assert(f:PutLine('end')==2)
assert(f:Close()==f.Close)
assert(f:Open('data.bin',1))
record(f:GetLine());record(f:Tell());record(f:AtEOF())
record(f:GetLine());record(f:Tell());record(f:AtEOF())
record(f:GetLine());record(f:Tell());record(f:AtEOF())
record(f:GetLine());record(f:AtEOF())
record(f:Seek(0));record(f:ReadBytes(5));record(f:Tell());record(f:AtEOF())
record(f:ReadBytes(0));record(f:AtEOF());record(f:Read());record(f:AtEOF())
for _,n in ipairs{-2,-1,0,100} do
 local ok,value=pcall(function() return f:ReadBytes(n) end)
 record(ok);record(value)
end
record(f:Seek(-1));record(f:Seek(999));record(f:ReadBytes(1));record(f:Tell());record(f:AtEOF())
assert(f:Close()==f.Close)
local ok,err=pcall(function() return f:GetLine() end);record(ok);record(err)
assert(f:Open('nul.bin',2));record(f:Write('a\0discarded'));f:Close()
assert(f:Open('nul.bin',1));record(f:Read());record(f:Tell())
record(f:destroy()==f.destroy)
local ok,err=pcall(function() return f:ReadBytes(-2) end);record(ok);record(err)
return result
"#;

#[test]
fn lua_file_bindings_keep_read_seek_close_and_binary_contracts() {
    let outputs = [false, true].map(|current| {
        let lua = Lua::new();
        let dir = Path::new("C:/song-lua-virtual-contract");
        let util = if current {
            create_rage_file_util_table(&lua, dir)
        } else {
            before::create_rage_file_util_table(&lua, dir)
        }
        .unwrap();
        lua.globals().set("Util", util).unwrap();
        lua.load(CONTRACT)
            .set_name("rage-file-performance-contract")
            .eval::<Vec<String>>()
            .unwrap()
    });
    assert_eq!(outputs[0], outputs[1]);
}

#[test]
fn indexed_virtual_stores_preserve_aliases_snapshots_and_vm_isolation() {
    let outputs=[false,true].map(|current| {
        let lua=Lua::new();let dir=Path::new("C:/song-lua-virtual-store-contract");
        let util=if current {create_rage_file_util_table(&lua,dir)}else{before::create_rage_file_util_table(&lua,dir)}.unwrap();
        lua.globals().set("Util",util).unwrap();
        lua.load(r#"
            local writer=Util.CreateRageFile()
            for i=1,128 do assert(writer:Open('data-'..i..'.bin',2));writer:Write('value-'..i);writer:Close() end
            local snapshot=Util.CreateRageFile();assert(snapshot:Open('data-8.bin',1))
            assert(writer:Open('nested/.././data-8.bin',2));writer:Write('replaced');writer:Close()
            assert(snapshot:Read()=='value-8');snapshot:Close()
            assert(snapshot:Open('./data-8.bin',1));assert(snapshot:Read()=='replaced');snapshot:Close()
            local result={}
            for i=128,1,-1 do assert(snapshot:Open('data-'..i..'.bin',1));result[#result+1]=snapshot:Read();snapshot:Close() end
            return result
        "#).set_name("rage-file-index-contract").eval::<Vec<String>>().unwrap()
    });
    assert_eq!(outputs[0], outputs[1]);
    let mut store = FileWrites::default();
    for i in 0..4 {
        store.insert(Path::new(&format!("file-{i}")), vec![i]);
    }
    assert!(matches!(store, FileWrites::Small(_)));
    store.insert(Path::new("file-4"), vec![4]);
    assert!(matches!(store, FileWrites::Large(_)));
    store.insert(Path::new("file-0"), vec![99]);
    assert_eq!(store.get(Path::new("file-0")).unwrap(), &[99]);
    let lua = Lua::new();
    let util =
        create_rage_file_util_table(&lua, Path::new("C:/song-lua-virtual-store-contract")).unwrap();
    lua.globals().set("Util", util).unwrap();
    lua.load("assert(not Util.CreateRageFile():Open('data-8.bin',1))")
        .exec()
        .unwrap();
}

fn bindings(current: bool, size: usize, count: usize) -> (Lua, Table, Vec<mlua::LuaString>) {
    let lua = Lua::new();
    let dir = Path::new("C:/song-lua-virtual-benchmark");
    let util = if current {
        create_rage_file_util_table(&lua, dir)
    } else {
        before::create_rage_file_util_table(&lua, dir)
    }
    .unwrap();
    let file = util
        .get::<Function>("CreateRageFile")
        .unwrap()
        .call::<Table>(())
        .unwrap();
    let open = file.get::<Function>("Open").unwrap();
    let write = file.get::<Function>("Write").unwrap();
    let close = file.get::<Function>("Close").unwrap();
    let payload = lua.create_string(vec![b'x'; size]).unwrap();
    let mut names = Vec::new();
    for i in 0..count {
        let name = lua.create_string(format!("file-{i:04}.bin")).unwrap();
        assert!(open.call::<bool>((file.clone(), name.clone(), 2)).unwrap());
        write
            .call::<usize>((file.clone(), payload.clone()))
            .unwrap();
        close.call::<Value>(file.clone()).unwrap();
        names.push(name);
    }
    assert!(
        open.call::<bool>((file.clone(), names.last().unwrap().clone(), 1))
            .unwrap()
    );
    (lua, file, names)
}

#[cfg(windows)]
#[test]
#[ignore = "release performance measurement; run alone with --nocapture"]
fn benchmark_rage_file() {
    use crate::draw_capture::perf::{allocations, cycles, pin_cpu2};
    use std::{hint::black_box, time::Instant};
    let _affinity = pin_cpu2();
    let reverse = std::env::var_os("DEADSYNC_PERF_REVERSE").is_some();
    for (name, size, line, reads) in [
        ("file_core_read_64", 64, false, 20_000),
        ("file_core_read_64k", 65536, false, 1000),
        ("file_core_line_64", 64, true, 20_000),
        ("file_core_line_64k", 65536, true, 1000),
    ] {
        for current in if reverse {
            [true, false]
        } else {
            [false, true]
        } {
            let mut samples = Vec::new();
            for sample in 0..10 {
                let mut old = before::state(vec![b'x'; size]);
                let mut new = state(vec![b'x'; size]);
                let start = Instant::now();
                let start_cycles = cycles();
                let (_, churn) = allocations::measure(|| {
                    for _ in 0..reads {
                        if current {
                            new.pos = 0;
                            new.eof = false;
                            if line {
                                black_box(new.get_line().unwrap());
                            } else {
                                black_box(new.read(-1).unwrap());
                            }
                        } else {
                            before::reset(&mut old);
                            if line {
                                black_box(old.get_line().unwrap());
                            } else {
                                black_box(old.read(-1).unwrap());
                            }
                        }
                    }
                });
                let used = cycles() - start_cycles;
                let nanos = start.elapsed().as_nanos();
                if sample > 0 {
                    samples.push(serde_json::json!({"ns":nanos,"thread_cycles":used,"units":reads,"allocations":churn.allocs,"reallocations":churn.reallocs,"allocated_bytes":churn.allocated_bytes,"frees":churn.frees}));
                }
            }
            println!(
                "DRAW_CAPTURE_METRIC {}",
                serde_json::json!({"workload":name,"variant":if current {"after"}else{"before"},"samples":samples,"unit":"core_reads","baseline_commit":"d63852fe4"})
            );
        }
    }
    for (name, size, line, reads) in [
        ("file_api_read_64", 64, false, 1000),
        ("file_api_read_64k", 65536, false, 128),
        ("file_api_line_64k", 65536, true, 128),
    ] {
        for current in if reverse {
            [true, false]
        } else {
            [false, true]
        } {
            let mut samples = Vec::new();
            for sample in 0..10 {
                let (_lua, file, _names) = bindings(current, size, 1);
                let seek = file.get::<Function>("Seek").unwrap();
                let read = file
                    .get::<Function>(if line { "GetLine" } else { "ReadBytes" })
                    .unwrap();
                let start = Instant::now();
                let start_cycles = cycles();
                let (_, churn) = allocations::measure(|| {
                    for _ in 0..reads {
                        seek.call::<i32>((file.clone(), 0)).unwrap();
                        let bytes = if line {
                            read.call::<mlua::LuaString>(file.clone())
                        } else {
                            read.call::<mlua::LuaString>((file.clone(), size as i32))
                        }
                        .unwrap();
                        black_box(bytes);
                    }
                });
                let used = cycles() - start_cycles;
                let nanos = start.elapsed().as_nanos();
                if sample > 0 {
                    samples.push(serde_json::json!({"ns":nanos,"thread_cycles":used,"units":reads,"allocations":churn.allocs,"reallocations":churn.reallocs,"allocated_bytes":churn.allocated_bytes,"frees":churn.frees}));
                }
            }
            println!(
                "DRAW_CAPTURE_METRIC {}",
                serde_json::json!({"workload":name,"variant":if current {"after"}else{"before"},"samples":samples,"unit":"seek_read_pairs","baseline_commit":"d63852fe4"})
            );
        }
    }
    for (name, count) in [
        ("file_open_single", 1),
        ("file_open_eight", 8),
        ("file_open_many", 256),
    ] {
        for current in if reverse {
            [true, false]
        } else {
            [false, true]
        } {
            let mut samples = Vec::new();
            let reads = 512;
            for sample in 0..10 {
                let (_lua, file, names) = bindings(current, 64, count);
                let open = file.get::<Function>("Open").unwrap();
                let target = names.last().unwrap();
                let start = Instant::now();
                let start_cycles = cycles();
                let (_, churn) = allocations::measure(|| {
                    for _ in 0..reads {
                        black_box(
                            open.call::<bool>((file.clone(), target.clone(), 1))
                                .unwrap(),
                        );
                    }
                });
                let used = cycles() - start_cycles;
                let nanos = start.elapsed().as_nanos();
                if sample > 0 {
                    samples.push(serde_json::json!({"ns":nanos,"thread_cycles":used,"units":reads,"allocations":churn.allocs,"reallocations":churn.reallocs,"allocated_bytes":churn.allocated_bytes,"frees":churn.frees}));
                }
            }
            println!(
                "DRAW_CAPTURE_METRIC {}",
                serde_json::json!({"workload":name,"variant":if current {"after"}else{"before"},"samples":samples,"unit":"virtual_opens","baseline_commit":"d63852fe4"})
            );
        }
    }
}
