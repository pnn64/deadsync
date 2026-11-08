use super::*;

#[path = "file_open_before.rs"]
mod before;

const ROOT: &str = "C:/song-lua-file-open-benchmark";

#[test]
fn reused_paths_match_exact_platform_spellings() {
    let bases = [
        "",
        ".",
        "..",
        "/",
        "a/../b",
        "C:",
        "C:/",
        "C:/songs/a",
        "C:relative/..",
        "//server/share/dir",
        r"\\?\C:\songs\..\a",
        r"\\?\UNC\server\share\x",
        r"\\.\device\x",
    ];
    let paths = [
        "",
        ".",
        "..",
        "../..",
        "file.ini",
        "a/../file.ini",
        "./a//b/../../file.ini",
        "C:",
        "C:other",
        "c:other",
        "D:other",
        "C:/file.ini",
        "/root/file.ini",
        r"\root\file.ini",
        r"a\..\file.ini",
        "//server/share/file",
        r"\\?\C:\x\..\file.ini",
        "é/../тест.ini",
        "file\0.ini",
    ];
    let parts = [
        "", ".", "..", "abc", "é", "x:", "/", "\\", "//server", "C:", "C:/", r"\\?\C:\",
    ];
    let mut result = PathBuf::new();
    for base in bases {
        let base = Path::new(base);
        for raw in paths
            .into_iter()
            .map(str::to_owned)
            .chain(parts.into_iter().flat_map(|a| {
                parts
                    .into_iter()
                    .flat_map(move |b| parts.into_iter().map(move |c| format!("{a}/{b}/{c}")))
            }))
        {
            let expected = before::file_path(base, &raw);
            set_file_path(&mut result, base, &raw);
            assert_eq!(
                result.as_os_str(),
                expected.as_os_str(),
                "base={base:?}, raw={raw:?}"
            );
            assert_eq!(file_path_text(&raw).as_ref(), before::file_path_text(&raw));
            assert_eq!(
                matches!(file_path_text(&raw), std::borrow::Cow::Borrowed(_)),
                !raw.contains('\\')
            );
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStringExt;
        let base = PathBuf::from(std::ffi::OsString::from_wide(&[67, 58, 47, 0xd800, 47, 97]));
        for raw in paths {
            set_file_path(&mut result, &base, raw);
            assert_eq!(
                result.as_os_str(),
                before::file_path(&base, raw).as_os_str()
            );
        }
    }
}

#[test]
fn reopened_lua_files_keep_snapshots_errors_and_side_effect_order() {
    let outputs = [false, true].map(|current| {
        let lua = Lua::new();
        let util = if current {create_rage_file_util_table(&lua, Path::new(ROOT))}
            else {before::create_rage_file_util_table(&lua, Path::new(ROOT))}.unwrap();
        lua.globals().set("Util", util).unwrap();
        let mut result = lua.load(r#"
            local result={}
            local function record(x) result[#result+1]=tostring(x) end
            local writer=Util.CreateRageFile();local reader=Util.CreateRageFile()
            for _,size in ipairs{0,1,64,32768,17,131072,3} do
                assert(writer:Open('file.bin',2));writer:Write(string.rep('x',size));writer:Close()
                assert(reader:Open('nested/.././file.bin',1));record(reader:ReadBytes(2));record(reader:Tell());record(reader:AtEOF())
                assert(writer:Open('file.bin',2));writer:Write('updated');writer:Close()
                record(#reader:Read());record(reader:Tell());record(reader:AtEOF());reader:Close()
                assert(reader:Open('nested\\..\\file.bin',1));record(reader:Read());reader:Close()
            end
            local bad='invalid-'..string.char(255)..'.bin'
            assert(writer:Open(bad,2));writer:Write('lossy-path');writer:Close()
            assert(reader:Open(bad..'\0ignored',1));record(reader:Read());reader:Close()
            for _,mode in ipairs{0,3,1} do
                record(reader:Open('missing-file.bin',mode));record(reader:GetError())
                local ok,err=pcall(function() return reader:Read() end);record(ok);record(err)
            end
            writer:destroy();reader:destroy()
            return result
        "#).set_name("file-open-reuse-contract").eval::<Vec<String>>().unwrap();
        result.push(crate::runtime::song_lua_side_effect_count(&lua).unwrap().to_string());
        result
    });
    assert_eq!(outputs[0], outputs[1]);
}

#[test]
fn reused_buffers_keep_binary_snapshots_and_observe_physical_updates() {
    let lua = Lua::new();
    lua.set_app_data(FileWrites::default());
    let path = PathBuf::from("C:/song-lua-file-open-binary-contract");
    let mut file = RageFile {
        path: path.clone(),
        ..Default::default()
    };
    for count in [1, 8] {
        for i in 0..count {
            lua.app_data_mut::<FileWrites>()
                .unwrap()
                .insert(Path::new(&format!("key-{i}")), vec![0]);
        }
        for size in [0, 1, 64, 65536, 17, 131072, 2] {
            let bytes = (0..size)
                .map(|i| ((i * 31 + 7) % 256) as u8)
                .collect::<Vec<_>>();
            lua.app_data_mut::<FileWrites>()
                .unwrap()
                .insert(&path, bytes.clone());
            file.bytes.clear();
            file.load(&lua).unwrap();
            assert_eq!(file.bytes, bytes);
            lua.app_data_mut::<FileWrites>()
                .unwrap()
                .insert(&path, vec![99]);
            assert_eq!(file.bytes, bytes);
        }
    }
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "deadsync-song-lua-file-open-{}-{unique}",
        std::process::id()
    ));
    std::fs::create_dir(&dir).unwrap();
    let disk = dir.join("data.bin");
    file.path = disk.clone();
    for bytes in [
        b"old\0bytes".as_slice(),
        b"updated".as_slice(),
        b"".as_slice(),
    ] {
        std::fs::write(&disk, bytes).unwrap();
        file.bytes.clear();
        file.load(&lua).unwrap();
        assert_eq!(file.bytes, bytes);
    }
    std::fs::remove_file(&disk).unwrap();
    file.bytes.clear();
    assert_eq!(
        file.load(&lua).unwrap_err().kind(),
        std::io::ErrorKind::NotFound
    );
    assert!(file.bytes.is_empty());
    std::fs::remove_dir(&dir).unwrap();
}

fn bindings(current: bool, size: usize, count: usize) -> (Lua, Table, Vec<mlua::LuaString>) {
    let lua = Lua::new();
    let util = if current {
        create_rage_file_util_table(&lua, Path::new(ROOT))
    } else {
        before::create_rage_file_util_table(&lua, Path::new(ROOT))
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

#[test]
fn warm_paths_and_virtual_snapshots_remove_temporary_allocations() {
    use crate::draw_capture::perf::allocations;
    let text = "file-0000.bin";
    let mut path = PathBuf::new();
    set_file_path(&mut path, Path::new(ROOT), text);
    let (_, a) = allocations::measure(|| std::hint::black_box(before::file_path_text(text)));
    let (_, b) = allocations::measure(|| std::hint::black_box(file_path_text(text)));
    assert_eq!(a.allocs, 1);
    assert_eq!((b.allocs, b.reallocs, b.allocated_bytes), (0, 0, 0));
    let (_, a) =
        allocations::measure(|| std::hint::black_box(before::file_path(Path::new(ROOT), text)));
    let (_, b) = allocations::measure(|| {
        set_file_path(&mut path, Path::new(ROOT), text);
        std::hint::black_box(&path);
    });
    assert!(a.allocs >= 3);
    assert_eq!((b.allocs, b.reallocs, b.allocated_bytes), (0, 0, 0));
    let (old_lua, _old_file, _old_names) = bindings(false, 65536, 1);
    let (new_lua, _new_file, _new_names) = bindings(true, 65536, 1);
    let mut file = RageFile {
        path,
        bytes: vec![0; 65536],
        ..Default::default()
    };
    file.load(&new_lua).unwrap();
    let (_, a) = allocations::measure(|| {
        std::hint::black_box(before::written_file(&old_lua, &file.path).unwrap())
    });
    let (_, b) = allocations::measure(|| {
        file.load(&new_lua).unwrap();
        std::hint::black_box(&file.bytes);
    });
    assert_eq!((a.allocs, a.allocated_bytes), (1, 65536));
    assert_eq!((b.allocs, b.reallocs, b.allocated_bytes), (0, 0, 0));
}

#[cfg(windows)]
#[test]
#[ignore = "release performance measurement; run alone with --nocapture"]
fn benchmark_file_open() {
    use crate::draw_capture::perf::{allocations, cycles, pin_cpu2};
    use std::{hint::black_box, time::Instant};
    fn measure<W: FnMut()>(
        name: &str,
        current: bool,
        unit: &str,
        count: usize,
        mut setup: impl FnMut() -> W,
    ) {
        let mut samples = Vec::new();
        for sample in 0..10 {
            let mut work = setup();
            let start = Instant::now();
            let start_cycles = cycles();
            let (_, churn) = allocations::measure(|| {
                for _ in 0..count {
                    work();
                }
            });
            let used = cycles() - start_cycles;
            let nanos = start.elapsed().as_nanos();
            if sample > 0 {
                samples.push(serde_json::json!({"ns":nanos,"thread_cycles":used,"units":count,"allocations":churn.allocs,"reallocations":churn.reallocs,"allocated_bytes":churn.allocated_bytes,"frees":churn.frees}));
            }
        }
        println!(
            "DRAW_CAPTURE_METRIC {}",
            serde_json::json!({"workload":name,"variant":if current {"after"}else{"before"},"samples":samples,"unit":unit,"baseline_commit":"6a09e355d"})
        );
    }
    let _affinity = pin_cpu2();
    let reverse = std::env::var_os("DEADSYNC_PERF_REVERSE").is_some();
    let variants = if reverse {
        [true, false]
    } else {
        [false, true]
    };
    for (name, text) in [
        ("path_text_ascii", "file-0000.bin"),
        ("path_text_unicode", "music/é-曲/file.ini"),
        ("path_text_backslash", r"nested\..\file-0000.bin"),
    ] {
        for current in variants {
            measure(name, current, "path_texts", 20000, || {
                move || {
                    if current {
                        black_box(file_path_text(black_box(text)));
                    } else {
                        black_box(before::file_path_text(black_box(text)));
                    }
                }
            });
        }
    }
    for (name, base, text, cold) in [
        ("path_relative", ROOT, "file-0000.bin", false),
        ("path_nested", ROOT, "nested/../file-0000.bin", false),
        ("path_backslash", ROOT, r"nested\..\file-0000.bin", false),
        (
            "path_absolute",
            ROOT,
            "C:/song-lua-file-open-benchmark/file-0000.bin",
            false,
        ),
        ("path_drive_relative", ROOT, "C:file-0000.bin", false),
        (
            "path_verbatim_base",
            r"\\?\C:\song-lua-open",
            "nested/../file-0000.bin",
            false,
        ),
        ("path_cold_relative", ROOT, "file-0000.bin", true),
    ] {
        for current in variants {
            measure(name, current, "paths", 2000, || {
                let mut result = PathBuf::new();
                set_file_path(&mut result, Path::new(base), text);
                move || {
                    if current {
                        if cold {
                            let mut path = PathBuf::new();
                            set_file_path(&mut path, black_box(Path::new(base)), black_box(text));
                            black_box(path);
                        } else {
                            set_file_path(&mut result, black_box(Path::new(base)), black_box(text));
                            black_box(&result);
                        }
                    } else {
                        black_box(before::file_path(
                            black_box(Path::new(base)),
                            black_box(text),
                        ));
                    }
                }
            });
        }
    }
    for (name, size, count) in [("snapshot_64", 64, 2000), ("snapshot_64k", 65536, 128)] {
        for current in variants {
            measure(name, current, "snapshot_copies", count, || {
                let fixture = bindings(current, size, 1);
                let path = before::file_path(Path::new(ROOT), "file-0000.bin");
                let mut file = RageFile {
                    path,
                    bytes: vec![0; size],
                    ..Default::default()
                };
                move || {
                    let (lua, _, _) = black_box(&fixture);
                    if current {
                        file.load(lua).unwrap();
                        black_box(&file.bytes);
                    } else {
                        black_box(before::written_file(lua, &file.path).unwrap());
                    }
                }
            });
        }
    }
    for (name, size, count, style, reads) in [
        ("reopen_single", 64, 1, 0, 512),
        ("reopen_many", 64, 256, 0, 512),
        ("reopen_large", 65536, 1, 0, 128),
        ("reopen_alias", 64, 1, 1, 512),
        ("reopen_backslash", 64, 1, 2, 512),
        ("reopen_alternating", 65536, 2, 3, 128),
    ] {
        for current in variants {
            measure(name, current, "virtual_opens", reads, || {
                let fixture = bindings(current, size, count);
                let (lua, file, names) = &fixture;
                let open = file.get::<Function>("Open").unwrap();
                let raw = match style {
                    1 => format!("nested/../{}", names.last().unwrap().to_str().unwrap()),
                    2 => format!("nested\\..\\{}", names.last().unwrap().to_str().unwrap()),
                    _ => names.last().unwrap().to_str().unwrap().to_string(),
                };
                let target = lua.create_string(raw).unwrap();
                if style == 3 {
                    assert!(
                        open.call::<bool>((file.clone(), names[0].clone(), 2))
                            .unwrap()
                    );
                    file.get::<Function>("Write")
                        .unwrap()
                        .call::<usize>((file.clone(), "small"))
                        .unwrap();
                    file.get::<Function>("Close")
                        .unwrap()
                        .call::<Value>(file.clone())
                        .unwrap();
                }
                assert!(
                    open.call::<bool>((file.clone(), target.clone(), 1))
                        .unwrap()
                );
                let mut index = 0;
                move || {
                    let (_, file, names) = black_box(&fixture);
                    let name = if style == 3 {
                        index += 1;
                        &names[index % 2]
                    } else {
                        &target
                    };
                    black_box(open.call::<bool>((file.clone(), name.clone(), 1)).unwrap());
                }
            });
        }
    }
}
