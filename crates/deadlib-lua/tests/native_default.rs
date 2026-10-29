#![cfg(all(feature = "native-c-api", any(windows, target_os = "linux")))]
use deadlib_lua::{NativeCAbiLoader, Vm, compile};
use std::{
    path::PathBuf,
    process::Command,
    sync::{Arc, Mutex, OnceLock},
};
static VMS: Mutex<()> = Mutex::new(());
struct Fixture {
    folder: PathBuf,
    module: PathBuf,
    second: PathBuf,
}
fn fixture() -> &'static Fixture {
    static FIXTURE: OnceLock<Fixture> = OnceLock::new();
    FIXTURE.get_or_init(|| {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let folder = manifest
            .join("../../target/lua-native-default")
            .join(format!("{}-{}", std::process::id(), std::env::consts::ARCH));
        std::fs::create_dir_all(&folder).unwrap();
        let module = folder.join("native.dll");
        let headers = manifest.join("../../vendor/lua-src/lua-5.1.5");
        let source = manifest.join("tests/native-default/module.c");
        let output = if cfg!(windows) {
            let finder = PathBuf::from(std::env::var_os("ProgramFiles(x86)").unwrap())
                .join("Microsoft Visual Studio/Installer/vswhere.exe");
            let found = Command::new(finder)
                .args([
                    "-latest",
                    "-products",
                    "*",
                    "-find",
                    "VC/Auxiliary/Build/vcvars64.bat",
                ])
                .output()
                .unwrap();
            assert!(found.status.success());
            let setup = String::from_utf8(found.stdout).unwrap().trim().to_string();
            let response = folder.join("module.rsp");
            let args = [
                "/nologo".into(),
                "/LD".into(),
                "/O2".into(),
                "/MT".into(),
                "/DLUA_BUILD_AS_DLL".into(),
                format!("/I{}", headers.display()),
                source.to_string_lossy().into_owned(),
                PathBuf::from(env!("OUT_DIR"))
                    .join("lua51.lib")
                    .to_string_lossy()
                    .into_owned(),
                "/link".into(),
                format!("/OUT:{}", module.display()),
                format!("/IMPLIB:{}", folder.join("native.lib").display()),
            ];
            std::fs::write(
                &response,
                args.iter()
                    .map(|a| format!("\"{a}\""))
                    .collect::<Vec<_>>()
                    .join(" "),
            )
            .unwrap();
            let script = folder.join("module.cmd");
            std::fs::write(
                &script,
                format!(
                    "@call \"{setup}\" >nul\n@cl @\"{}\"\n@exit /b %errorlevel%\n",
                    response.display()
                ),
            )
            .unwrap();
            Command::new("cmd")
                .args(["/d", "/c"])
                .arg(script)
                .current_dir(&folder)
                .output()
                .unwrap()
        } else {
            let mut cc = Command::new("cc");
            cc.args(["-shared", "-fPIC", "-O2"]);
            if cfg!(target_arch = "x86") {
                cc.args(["-m32", "-msse2", "-mfpmath=sse"]);
            }
            cc.arg("-I")
                .arg(headers)
                .arg(source)
                .arg("-o")
                .arg(&module)
                .output()
                .unwrap()
        };
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let second = folder.join("second.dll");
        std::fs::write(&second, std::fs::read(&module).unwrap()).unwrap();
        Fixture {
            folder,
            module,
            second,
        }
    })
}
fn script(vm: &mut Vm, source: &[u8]) {
    vm.execute(&compile(source).unwrap()).unwrap();
}

#[test]
fn inactive_threads_own_tables_userdata_closures_and_capture_writes() {
    let _guard = VMS.lock().unwrap();
    let f = fixture();
    let mut vm = Vm::new();
    let path = vm.intern(f.module.to_string_lossy().as_bytes());
    vm.set_global(b"library_path", path).unwrap();
    script(&mut vm, b"boundary_inactive_objects=assert(package.loadlib(library_path,'luaopen_native'))('inactive').inactive");
    script(
        &mut vm,
        include_bytes!("native-boundary/inactive-objects.lua"),
    );
}

#[test]
fn calls_on_other_threads_preserve_their_pending_execution() {
    let _guard = VMS.lock().unwrap();
    let f = fixture();
    let mut vm = Vm::new();
    let path = vm.intern(f.module.to_string_lossy().as_bytes());
    vm.set_global(b"library_path", path).unwrap();
    script(&mut vm,b"local m=assert(package.loadlib(library_path,'luaopen_native'))('calls');boundary_thread_pcall=m.pcall_thread;boundary_thread_yield_tail=m.yield_tail;boundary_thread_called=m.called;boundary_thread_cpcall=m.cpcall_thread");
    script(&mut vm, include_bytes!("native-boundary/thread-calls.lua"));
}

#[test]
fn other_thread_api_yields_preserve_live_frames_and_resume_results() {
    let _guard = VMS.lock().unwrap();
    let f = fixture();
    let mut vm = Vm::new();
    let path = vm.intern(f.module.to_string_lossy().as_bytes());
    vm.set_global(b"library_path", path).unwrap();
    script(&mut vm,b"local m=assert(package.loadlib(library_path,'luaopen_native'))('yields');boundary_thread_pcall=m.pcall_thread;boundary_thread_cpcall=m.cpcall_thread;boundary_thread_yield_tail=m.yield_tail");
    script(&mut vm, include_bytes!("native-boundary/api-yields.lua"));
}

#[test]
fn other_thread_loading_preserves_reader_context_and_pending_computation() {
    let _guard = VMS.lock().unwrap();
    let f = fixture();
    let mut vm = Vm::new();
    let path = vm.intern(f.module.to_string_lossy().as_bytes());
    vm.set_global(b"library_path", path).unwrap();
    script(&mut vm, b"local m=assert(package.loadlib(library_path,'luaopen_native'))('loads');boundary_thread_load=m.load_thread;boundary_thread_yield_tail=m.yield_tail;boundary_thread_called=m.called");
    script(&mut vm, include_bytes!("native-boundary/thread-loads.lua"));
}

#[test]
fn completed_reader_yields_preserve_stack_frames_and_current_environments() {
    let _guard = VMS.lock().unwrap();
    let f = fixture();
    let mut vm = Vm::new();
    let path = vm.intern(f.module.to_string_lossy().as_bytes());
    vm.set_global(b"library_path", path).unwrap();
    script(&mut vm, b"local m=assert(package.loadlib(library_path,'luaopen_native'))('reader-continuations');boundary_thread_load=m.load_thread");
    script(
        &mut vm,
        include_bytes!("native-boundary/reader-continuations.lua"),
    );
    script(
        &mut vm,
        include_bytes!("native-boundary/reader-environments.lua"),
    );
}

#[test]
fn native_panic_callbacks_share_one_vm_and_restore_the_error_target() {
    let _guard = VMS.lock().unwrap();
    let f = fixture();
    let start = NativeCAbiLoader::statistics();
    for _ in 0..3 {
        let mut vm = Vm::new();
        let value = vm.intern(f.module.to_string_lossy().as_bytes());
        vm.set_global(b"library_path", value).unwrap();
        script(&mut vm,b"local m=assert(package.loadlib(library_path,'luaopen_native'))('panic');boundary_panic_swap=m.panic_swap;boundary_panic_raise=m.panic_raise;boundary_panic_protected_error=m.panic_protected_error;boundary_panic_yield_tail=m.yield_tail");
        script(&mut vm, include_bytes!("native-boundary/native-panic.lua"));
    }
    let end = NativeCAbiLoader::statistics();
    assert_eq!(end.groups_created - start.groups_created, 3);
    assert_eq!(end.groups_destroyed - start.groups_destroyed, 3);
    assert_eq!(
        end.states_created - start.states_created,
        end.states_destroyed - start.states_destroyed
    );
    assert_eq!(
        end.operations - start.operations,
        end.returned - start.returned
    );
}

#[test]
fn selected_thread_files_preserve_status_stack_environment_and_pending_frames() {
    let _guard = VMS.lock().unwrap();
    let f = fixture();
    std::fs::create_dir_all(f.folder.join("thread-file-directory")).unwrap();
    let mut vm = Vm::new();
    let path = vm.intern(f.module.to_string_lossy().as_bytes());
    vm.set_global(b"library_path", path).unwrap();
    let path = vm.intern(f.folder.join("thread-file").to_string_lossy().as_bytes());
    vm.set_global(b"THREAD_FILE_PREFIX", path).unwrap();
    script(&mut vm,b"local m=assert(package.loadlib(library_path,'luaopen_native'))('files');boundary_thread_file=m.file_thread;boundary_thread_yield_tail=m.yield_tail;boundary_thread_called=m.called");
    script(&mut vm, include_bytes!("native-boundary/thread-files.lua"));
}

#[test]
fn selected_thread_stdin_shares_buffered_input_and_keeps_it_open() {
    use std::io::Write;
    use std::process::Stdio;
    let f = fixture();
    let mut source=b"local m=assert(package.loadlib('./native.dll','luaopen_native'))('stdin');boundary_thread_file=m.file_thread;boundary_thread_yield_tail=m.yield_tail\n".to_vec();
    source.extend_from_slice(include_bytes!("native-boundary/thread-file-stdin.lua"));
    let mut child = Command::new(env!("CARGO_BIN_EXE_deadlua"))
        .arg("-e")
        .arg(std::str::from_utf8(&source).unwrap())
        .current_dir(&f.folder)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"7return marker,59\n")
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.stderr.is_empty());
    assert!(String::from_utf8_lossy(&out.stdout).contains("thread-file-stdin-complete\t5"));
}

#[test]
fn selected_file_finalizer_errors_panic_on_the_target_with_the_latest_callback() {
    let _guard = VMS.lock().unwrap();
    let f = fixture();
    let mut vm = Vm::new();
    let path = vm.intern(f.module.to_string_lossy().as_bytes());
    vm.set_global(b"library_path", path).unwrap();
    let path = vm.intern(
        f.folder
            .join("thread-file-panic")
            .to_string_lossy()
            .as_bytes(),
    );
    vm.set_global(b"THREAD_FILE_PREFIX", path).unwrap();
    script(&mut vm,b"local m=assert(package.loadlib(library_path,'luaopen_native'))('file panic');boundary_thread_file_panic=m.file_panic;boundary_thread_yield_tail=m.yield_tail;boundary_panic_swap=m.panic_swap");
    script(
        &mut vm,
        include_bytes!("native-boundary/thread-file-panic.lua"),
    );
}

#[test]
fn native_allocation_checks_run_finalizers_at_the_api_boundary() {
    let _guard = VMS.lock().unwrap();
    let f = fixture();
    let mut vm = Vm::new();
    let path = vm.intern(f.module.to_string_lossy().as_bytes());
    vm.set_global(b"library_path", path).unwrap();
    script(&mut vm,b"local m=assert(package.loadlib(library_path,'luaopen_native'))('allocation GC');boundary_allocation_gc=m.allocation_gc;boundary_allocation_yield_tail=m.yield_tail");
    script(&mut vm, include_bytes!("native-boundary/allocation-gc.lua"));
}

#[test]
fn selected_construction_formatting_and_concat_preserve_context_and_identity() {
    let _guard = VMS.lock().unwrap();
    let f = fixture();
    let mut vm = Vm::new();
    let path = vm.intern(f.module.to_string_lossy().as_bytes());
    vm.set_global(b"library_path", path).unwrap();
    script(&mut vm,b"local m=assert(package.loadlib(library_path,'luaopen_native'))('execution');boundary_execution_construct=m.construct_thread;boundary_execution_format=m.format_thread;boundary_execution_concat=m.concat_thread;boundary_execution_yield_tail=m.yield_tail;boundary_execution_called=m.called");
    script(
        &mut vm,
        include_bytes!("native-boundary/thread-execution.lua"),
    );
}

#[test]
fn selected_concat_errors_reach_the_target_panic_callback_and_close_upvalues() {
    let _guard = VMS.lock().unwrap();
    let f = fixture();
    let mut vm = Vm::new();
    let path = vm.intern(f.module.to_string_lossy().as_bytes());
    vm.set_global(b"library_path", path).unwrap();
    script(&mut vm,b"local m=assert(package.loadlib(library_path,'luaopen_native'))('concat panic');boundary_execution_concat_panic=m.concat_thread_panic;boundary_execution_yield_tail=m.yield_tail");
    script(
        &mut vm,
        include_bytes!("native-boundary/thread-execution-panic.lua"),
    );
}

#[test]
fn selected_concat_yields_keep_signed_offsets_and_completed_target_resumptions() {
    let _guard = VMS.lock().unwrap();
    let f = fixture();
    let mut vm = Vm::new();
    let path = vm.intern(f.module.to_string_lossy().as_bytes());
    vm.set_global(b"library_path", path).unwrap();
    script(&mut vm,b"local m=assert(package.loadlib(library_path,'luaopen_native'))('concat yields');boundary_execution_concat_panic=m.concat_thread_panic;boundary_execution_yield_tail=m.yield_tail;boundary_concat_offsets=m.stack_offsets");
    script(&mut vm, include_bytes!("native-boundary/concat-yields.lua"));
}

#[test]
fn selected_gc_controls_and_finalizers_preserve_target_context_and_stack() {
    let _guard = VMS.lock().unwrap();
    let f = fixture();
    let mut vm = Vm::new();
    let path = vm.intern(f.module.to_string_lossy().as_bytes());
    vm.set_global(b"library_path", path).unwrap();
    script(&mut vm,b"local m=assert(package.loadlib(library_path,'luaopen_native'))('thread GC');boundary_gc_thread=m.gc_thread;boundary_execution_yield_tail=m.yield_tail;boundary_execution_called=m.called");
    script(&mut vm, include_bytes!("native-boundary/thread-gc.lua"));
}

#[test]
fn selected_gc_errors_reach_the_latest_target_panic_callback() {
    let _guard = VMS.lock().unwrap();
    let f = fixture();
    let mut vm = Vm::new();
    let path = vm.intern(f.module.to_string_lossy().as_bytes());
    vm.set_global(b"library_path", path).unwrap();
    script(&mut vm,b"local m=assert(package.loadlib(library_path,'luaopen_native'))('GC panic');boundary_gc_thread_panic=m.gc_thread_panic;boundary_execution_yield_tail=m.yield_tail;boundary_panic_swap=m.panic_swap");
    script(
        &mut vm,
        include_bytes!("native-boundary/thread-gc-panic.lua"),
    );
}

#[test]
fn waiting_errors_follow_the_target_protection_chain_and_preserve_abandoned_threads() {
    let _guard = VMS.lock().unwrap();
    let f = fixture();
    let mut vm = Vm::new();
    let path = vm.intern(f.module.to_string_lossy().as_bytes());
    vm.set_global(b"library_path", path).unwrap();
    script(&mut vm,b"local m=assert(package.loadlib(library_path,'luaopen_native'))('waiting errors');boundary_execution_called=m.called;boundary_execution_yield_tail=m.yield_tail;boundary_gc_thread=m.gc_thread;boundary_thread_pcall=m.pcall_thread;boundary_waiting_raise=m.waiting_raise;boundary_waiting_snapshot=m.waiting_snapshot;boundary_waiting_c_resume=m.waiting_c_resume");
    script(
        &mut vm,
        include_bytes!("native-boundary/waiting-errors.lua"),
    );
}

#[test]
fn unprotected_selected_calls_preserve_results_errors_panic_state_and_open_cells() {
    let _guard = VMS.lock().unwrap();
    let f = fixture();
    let mut vm = Vm::new();
    let path = vm.intern(f.module.to_string_lossy().as_bytes());
    vm.set_global(b"library_path", path).unwrap();
    script(&mut vm,b"local m=assert(package.loadlib(library_path,'luaopen_native'))('unprotected calls');boundary_execution_yield_tail=m.yield_tail;boundary_execution_called=m.called;boundary_unprotected_call=m.call_thread;boundary_unprotected_panic=m.call_thread_panic;boundary_unprotected_fail=m.fail;boundary_panic_swap=m.panic_swap;boundary_thread_pcall=m.pcall_thread;boundary_waiting_snapshot=m.waiting_snapshot;boundary_panic_raise=m.panic_raise;boundary_waiting_raise=m.waiting_raise;boundary_waiting_c_resume=m.waiting_c_resume");
    script(
        &mut vm,
        include_bytes!("native-boundary/thread-unprotected.lua"),
    );
}

#[test]
fn unprotected_waiting_panic_without_a_recovery_callback_terminates() {
    let f = fixture();
    for kind in ["fresh", "completed", "yielded", "failed", "partial"] {
        for native in [false, true] {
            for mode in [0, 3] {
                let source = format!(
                    "local m=assert(package.loadlib('./native.dll','luaopen_native'))('waiting exit');boundary_execution_yield_tail=m.yield_tail;boundary_unprotected_call=m.call_thread;boundary_panic_swap=m.panic_swap;boundary_panic_unhandled=m.panic_unhandled;boundary_waiting_c_resume=m.waiting_c_resume;WAITING_KIND={kind:?};WAITING_NATIVE={native};WAITING_PANIC={mode};{}",
                    include_str!("native-boundary/thread-unprotected-exit.lua")
                );
                let out = Command::new(env!("CARGO_BIN_EXE_deadlua"))
                    .args(["-e", &source])
                    .current_dir(&f.folder)
                    .output()
                    .unwrap();
                assert_eq!(out.status.code(), Some(1), "{kind}/{native}/{mode}");
                assert!(
                    out.stderr.is_empty(),
                    "{}",
                    String::from_utf8_lossy(&out.stderr)
                );
                let expected = format!(
                    "waiting-panic-exit\t{kind}\t{native}\t{mode}\n{}",
                    if mode == 3 {
                        "panic-return\t2\t1\n"
                    } else {
                        ""
                    }
                );
                assert_eq!(
                    String::from_utf8_lossy(&out.stdout).replace('\r', ""),
                    expected
                );
                assert_eq!(
                    std::fs::read(f.folder.join("waiting-exit-buffered-output.bin")).unwrap(),
                    b"waiting\0buffered\n"
                );
            }
        }
    }
}

#[test]
fn selected_calls_resume_after_interleaved_calls_with_expired_handlers_and_live_cells() {
    let _guard = VMS.lock().unwrap();
    let f = fixture();
    let mut vm = Vm::new();
    let path = vm.intern(f.module.to_string_lossy().as_bytes());
    vm.set_global(b"library_path", path).unwrap();
    script(&mut vm,b"local m=assert(package.loadlib(library_path,'luaopen_native'))('call yields');boundary_yield_call=m.yield_call;boundary_yield_resume=m.yield_resume;boundary_waiting_snapshot=m.waiting_snapshot");
    script(&mut vm, include_bytes!("native-boundary/call-yields.lua"));
}

#[test]
fn collector_integer_controls_follow_native_width_and_preserve_finalizer_errors() {
    let _guard = VMS.lock().unwrap();
    let f = fixture();
    let mut vm = Vm::new();
    let path = vm.intern(f.module.to_string_lossy().as_bytes());
    vm.set_global(b"library_path", path).unwrap();
    script(&mut vm,b"local m=assert(package.loadlib(library_path,'luaopen_native'))('collector integers');boundary_gc_thread=m.gc_thread;boundary_gc_thread_panic=m.gc_thread_panic;boundary_execution_yield_tail=m.yield_tail");
    script(&mut vm, include_bytes!("native-boundary/gc-integers.lua"));
}

#[test]
fn selected_table_apis_execute_metamethods_and_route_unprotected_errors() {
    std::thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(selected_table_corpus)
        .unwrap()
        .join()
        .unwrap();
}

fn selected_table_corpus() {
    let _guard = VMS.lock().unwrap();
    let f = fixture();
    let mut vm = Vm::new();
    let path = vm.intern(f.module.to_string_lossy().as_bytes());
    vm.set_global(b"library_path", path).unwrap();
    script(&mut vm,b"local m=assert(package.loadlib(library_path,'luaopen_native'))('selected tables');boundary_table_invoke=m.invoke;boundary_table_thread=m.table_thread;boundary_table_thread_panic=m.table_thread_panic;boundary_execution_yield_tail=m.yield_tail;boundary_execution_called=m.called;boundary_thread_pcall=m.pcall_thread;boundary_waiting_snapshot=m.waiting_snapshot;boundary_waiting_c_resume=m.waiting_c_resume");
    script(&mut vm, include_bytes!("native-boundary/thread-tables.lua"));
}

#[test]
fn selected_table_tag_methods_preserve_yields_and_later_resumptions() {
    let _guard = VMS.lock().unwrap();
    let f = fixture();
    let mut vm = Vm::new();
    let path = vm.intern(f.module.to_string_lossy().as_bytes());
    vm.set_global(b"library_path", path).unwrap();
    script(&mut vm,b"local m=assert(package.loadlib(library_path,'luaopen_native'))('table yields');boundary_table_thread_panic=m.table_thread_panic;boundary_table_yield=m.table_yield;boundary_table_c_yield_all=m.table_c_yield_all;boundary_table_c_yield_tail=m.table_c_yield_tail;boundary_table_c_yield_zero=m.table_c_yield_zero;boundary_execution_yield_tail=m.yield_tail");
    script(&mut vm, include_bytes!("native-boundary/table-yields.lua"));
}

#[test]
fn file_close_bodies_keep_c_windows_context_errors_and_yields() {
    let _guard = VMS.lock().unwrap();
    let fixture = fixture();
    let mut vm = Vm::new();
    let path = vm.intern(fixture.module.to_string_lossy().as_bytes());
    vm.set_global(b"library_path", path).unwrap();
    script(&mut vm,b"local m=assert(package.loadlib(library_path,'luaopen_native'))('file closers');boundary_file_closer=m.file_closer;boundary_file_capture=m.file_closer_capture;boundary_file_body=m.file_body;boundary_file_hook=m.file_hook");
    script(&mut vm, include_bytes!("native-boundary/io-closers.lua"));
}

#[test]
fn hooks_preserve_popped_physical_slots_until_overwrite_or_collection() {
    let _guard = VMS.lock().unwrap();
    let fixture = fixture();
    let mut vm = Vm::new();
    let path = vm.intern(fixture.module.to_string_lossy().as_bytes());
    vm.set_global(b"hook_stack_library", path).unwrap();
    vm.set_output_capture(true);
    script(&mut vm, include_bytes!("native-boundary/hook-stacks.lua"));
    assert_eq!(
        std::str::from_utf8(vm.output().unwrap())
            .unwrap()
            .lines()
            .collect::<Vec<_>>(),
        include_str!("native-boundary/hook-stacks.expected")
            .lines()
            .collect::<Vec<_>>()
    );
}

#[test]
fn yielded_hooks_retain_their_selected_bases_and_public_stack_contents() {
    let _guard = VMS.lock().unwrap();
    let fixture = fixture();
    let mut vm = Vm::new();
    let path = vm.intern(fixture.module.to_string_lossy().as_bytes());
    vm.set_global(b"hook_stack_library", path).unwrap();
    vm.set_output_capture(true);
    script(&mut vm, include_bytes!("native-boundary/hook-yields.lua"));
    assert_eq!(
        std::str::from_utf8(vm.output().unwrap())
            .unwrap()
            .lines()
            .collect::<Vec<_>>(),
        include_str!("native-boundary/hook-yields.expected")
            .lines()
            .collect::<Vec<_>>()
    );
}

#[test]
fn signed_hook_offsets_preserve_raw_stacks_moves_and_resume_failures() {
    let _guard = VMS.lock().unwrap();
    let fixture = fixture();
    let mut vm = Vm::new();
    let path = vm.intern(fixture.module.to_string_lossy().as_bytes());
    vm.set_global(b"hook_stack_library", path).unwrap();
    for (source, expected) in [
        (
            include_bytes!("native-boundary/hook-offsets.lua").as_slice(),
            include_str!("native-boundary/hook-offsets.expected"),
        ),
        (
            include_bytes!("native-boundary/hook-offsets-wrapped.lua").as_slice(),
            include_str!("native-boundary/hook-offsets-wrapped.expected"),
        ),
        (
            include_bytes!("native-boundary/hook-offsets-zero.lua").as_slice(),
            include_str!("native-boundary/hook-offsets-zero.expected"),
        ),
    ] {
        vm.set_output_capture(true);
        script(&mut vm, source);
        assert_eq!(
            std::str::from_utf8(vm.output().unwrap())
                .unwrap()
                .lines()
                .collect::<Vec<_>>(),
            expected.lines().collect::<Vec<_>>()
        );
    }
    script(&mut vm, br#"
local m=assert(package.loadlib(hook_stack_library,'luaopen_native'))('I/O hook offset')
local file=assert(io.tmpfile());local original=debug.getfenv(file)
debug.setfenv(file,{__close=m.file_closer('yield-zero',false)})
local fn=function(f)return 17 end;local co=coroutine.create(fn);m.file_hook(io.close,fn,co)
local function pack(...)return{n=select('#',...),...}end
local first=pack(coroutine.resume(co,file));debug.setfenv(file,original)
assert(first.n==2 and first[1]==false and rawequal(first[2],co) and coroutine.status(co)=='suspended')
local second=pack(coroutine.resume(co));assert(second.n==2 and second[1]and second[2]==17 and coroutine.status(co)=='dead')
assert(file:close())
"#);
}

#[test]
fn default_cli_loads_an_unmodified_public_header_module() {
    let f = fixture();
    for entry in ["direct", "require"] {
        let source = if entry == "direct" {
            b"local init=assert(package.loadlib('./native.dll','luaopen_native'));local m=init('direct');assert(m.name=='direct'and m.sum(17,25)==42);print('native-default-ok')".as_slice()
        } else {
            b"package.path='';package.cpath='./?.dll';local m=require('native.root.leaf');assert(m.name=='native.root.leaf'and m.environment==package and m.sum(17,25)==42);assert(require('native.root.leaf')==m);print('native-default-ok')".as_slice()
        };
        let out = Command::new(env!("CARGO_BIN_EXE_deadlua"))
            .arg("-e")
            .arg(std::str::from_utf8(source).unwrap())
            .current_dir(&f.folder)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{entry}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(out.stderr.is_empty());
        assert_eq!(
            String::from_utf8_lossy(&out.stdout).trim(),
            "native-default-ok"
        );
    }
}

#[test]
fn default_provider_replacement_and_child_calls_keep_one_c_api_image() {
    let _guard = VMS.lock().unwrap();
    let f = fixture();
    let mut vm = Vm::new();
    let start = NativeCAbiLoader::statistics();
    let p = vm.intern(f.module.to_string_lossy().as_bytes());
    vm.set_global(b"first_library", p).unwrap();
    script(&mut vm,br#"
local init=assert(package.loadlib(first_library,'luaopen_native'))
local co=coroutine.create(function()object=init('child');counter=object.counter(17);return object.state()end)
local ok,ptr,main=coroutine.resume(co);assert(ok and not main and counter()==18)
child_pointer=ptr;local pointer,main=object.state();assert(main and pointer~=ptr)
local token={};local ok,e=pcall(object.fail,token);assert(not ok and e==token)
assert(object.body(math.abs,-29)==29)
assert(object.invoke(function(x)return x+1 end,41)==42)
local co=coroutine.create(function()return object.yield(17,29)end)
local ok,a,b=coroutine.resume(co);assert(ok and a==17 and b==29)
ok,a,b=coroutine.resume(co,42,57);assert(ok and a==42 and b==57)
"#);
    vm.set_native_library_loader(Some(Arc::new(NativeCAbiLoader::default())));
    let p = vm.intern(f.second.to_string_lossy().as_bytes());
    vm.set_global(b"second_library", p).unwrap();
    script(&mut vm,br#"
other=assert(package.loadlib(second_library,'luaopen_native'))('other')
assert(other.sum(17,25)==42 and counter()==19)
local co=coroutine.create(function()local pointer,main=other.state();assert(not main);return pointer end)
local ok,pointer=coroutine.resume(co);assert(ok and pointer~=child_pointer)
do local box=other.box();assert(type(box)=='userdata')end
collectgarbage();collectgarbage();assert(debug.getregistry()['native.default.finalized']==1)
"#);
    vm.collect_garbage().unwrap();
    assert!(NativeCAbiLoader::statistics().states_destroyed > start.states_destroyed);
    drop(vm);
    let end = NativeCAbiLoader::statistics();
    assert_eq!(
        end.states_created - start.states_created,
        end.states_destroyed - start.states_destroyed
    );
    assert_eq!(
        end.operations - start.operations,
        end.returned - start.returned
    );
    assert_eq!(
        end.libraries_opened - start.libraries_opened,
        end.libraries_closed - start.libraries_closed
    );
}

#[test]
fn parallel_vms_keep_independent_c_states_and_release_shared_images() {
    let _guard = VMS.lock().unwrap();
    let path = fixture().module.to_string_lossy().into_owned();
    let ready = Arc::new(std::sync::Barrier::new(8));
    let start = NativeCAbiLoader::statistics();
    let workers:Vec<_>=(0..8).map(|index|{
        let path=path.clone();let ready=ready.clone();
        std::thread::spawn(move||{
            let mut vm=Vm::new();let value=vm.intern(path.as_bytes());vm.set_global(b"library_path",value).unwrap();
            let value=vm.intern(if index%2==0 { b"1" }else{b"2"});vm.set_global(b"panic_mode",value).unwrap();
            let values=vm.execute(&compile(br#"local m=assert(package.loadlib(library_path,'luaopen_native'))('parallel');local n=m.counter(17);for i=1,20 do assert(n()==17+i and m.sum(i,1)==i+1)end;assert(m.panic_swap(nil,tonumber(panic_mode))==4);return m.state()"#).unwrap()).unwrap();
            let deadlib_lua::Value::LightUserdata(pointer)=values[0]else{panic!("C state identity")};
            ready.wait();assert_eq!(vm.execute(&compile(b"local m=assert(package.loadlib(library_path,'luaopen_native'))('parallel');return m.panic_swap(nil,-1)").unwrap()).unwrap(),&[deadlib_lua::Value::Number(if index%2==0 {1.}else{2.})]);drop(vm);pointer
        })
    }).collect();
    let pointers: std::collections::BTreeSet<_> =
        workers.into_iter().map(|w| w.join().unwrap()).collect();
    assert_eq!(pointers.len(), 8);
    let end = NativeCAbiLoader::statistics();
    assert_eq!(end.states_created - start.states_created, 8);
    assert_eq!(end.states_destroyed - start.states_destroyed, 8);
    assert_eq!(end.groups_created - start.groups_created, 8);
    assert_eq!(end.groups_destroyed - start.groups_destroyed, 8);
    assert_eq!(
        end.operations - start.operations,
        end.returned - start.returned
    );
    assert_eq!(
        end.libraries_opened - start.libraries_opened,
        end.libraries_closed - start.libraries_closed
    );
}
