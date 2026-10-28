#[cfg(any(windows, target_os = "linux"))]
mod supported {
    //! Exercise the runtime C adapter with a separately built native module.
    use deadlib_lua::{NativeCAbiLoader, Vm, compile_named};
    use std::sync::Arc;
    fn quote(bytes: &[u8]) -> String {
        let mut text = String::from("\"");
        for b in bytes {
            text.push_str(&format!("\\{b:03}"));
        }
        text.push('"');
        text
    }
    fn run() {
        let args: Vec<_> = std::env::args().skip(1).collect();
        let [shim, module, source] = args.as_slice() else {
            panic!("native_boundary shim module source.lua")
        };
        // SAFETY: the harness compiles this image from the runtime's own ABI source.
        let mut vm = Vm::new();
        let lifetime = if shim == "default" {
            None
        } else {
            let loader =
                Arc::new(unsafe { NativeCAbiLoader::from_library(shim.as_bytes()) }.unwrap());
            let lifetime = Arc::downgrade(&loader);
            vm.set_native_library_loader(Some(loader));
            Some(lifetime)
        };
        vm.set_instruction_limit(Some(10_000_000));
        let mut boot = String::new();
        for name in [
            "boundary_sum",
            "boundary_identity",
            "boundary_raise",
            "boundary_binary",
            "boundary_call",
            "boundary_stack",
            "boundary_mutate",
            "boundary_state",
            "boundary_state_reenter",
            "boundary_cached_top",
            "boundary_make_counter",
            "boundary_snapshot",
            "boundary_set_environment",
            "boundary_set_globals",
            "boundary_registry_roundtrip",
            "boundary_make_many",
            "boundary_table",
            "boundary_get",
            "boundary_set",
            "boundary_field",
            "boundary_raw",
            "boundary_metatable",
            "boundary_count",
            "boundary_shuffle",
            "boundary_userdata",
            "boundary_userdata_read",
            "boundary_userdata_finalize",
            "boundary_bytes",
            "boundary_environment",
            "boundary_capture_bytes",
            "boundary_length",
            "boundary_userdata_empty",
            "boundary_protected",
            "boundary_protected_negative",
            "boundary_protected_argument",
            "boundary_replace_handler",
            "boundary_scalar",
            "boundary_scalar_null",
            "boundary_scalar_capture",
            "boundary_integers",
            "boundary_compare",
            "boundary_concat",
            "boundary_concat_prefix",
            "boundary_format",
            "boundary_format_gc",
            "boundary_auxcheck",
            "boundary_auxwhere",
            "boundary_auxerror",
            "boundary_auxuserdata",
            "boundary_auxget",
            "boundary_auxnewmetatable",
            "boundary_auxmetafield",
            "boundary_auxopen",
            "boundary_auxfind",
            "boundary_auxrefs",
            "boundary_auxbuffer_info",
            "boundary_auxbuffer",
            "boundary_auxgsub",
            "boundary_upget",
            "boundary_upset",
            "boundary_upbind",
            "boundary_gc",
            "boundary_gc_counts",
            "boundary_load",
            "boundary_loadbuffer",
            "boundary_loadtop",
            "boundary_loadroots",
            "boundary_loadfile",
            "boundary_dump",
            "boundary_dump_null",
            "boundary_thread",
            "boundary_thread_status",
            "boundary_thread_pointer",
            "boundary_thread_capture",
            "boundary_pointer",
            "boundary_newthread",
            "boundary_thread_values",
            "boundary_thread_push",
            "boundary_thread_move",
            "boundary_thread_primitives",
            "boundary_thread_selfmove",
            "boundary_thread_resize",
            "boundary_thread_hold",
            "boundary_resume",
            "boundary_yield",
            "boundary_level",
            "boundary_setlevel",
            "boundary_level_reader",
            "boundary_cpcall",
            "boundary_debug_info",
            "boundary_debug_local",
            "boundary_debug_hold",
            "boundary_hook_set",
            "boundary_hook_info",
            "boundary_hook_restore",
            "boundary_context_make",
            "boundary_context_read",
            "boundary_context_write",
            "boundary_context_install",
            "boundary_return_stack",
            "boundary_body_call",
            "boundary_body_copy",
            "boundary_body_info",
            "boundary_body_hook",
            "boundary_library_handle",
            "boundary_inactive_objects",
            "boundary_thread_pcall",
            "boundary_thread_cpcall",
            "boundary_thread_yield_tail",
            "boundary_thread_called",
            "boundary_thread_load",
            "boundary_panic_swap",
            "boundary_panic_raise",
            "boundary_panic_protected_error",
            "boundary_panic_yield_tail",
            "boundary_panic_unhandled",
            "boundary_thread_file",
            "boundary_thread_file_panic",
            "boundary_allocation_gc",
            "boundary_execution_construct",
            "boundary_execution_format",
            "boundary_execution_concat",
            "boundary_execution_concat_panic",
            "boundary_execution_yield_tail",
            "boundary_execution_called",
            "boundary_concat_offsets",
            "boundary_gc_thread",
            "boundary_gc_thread_panic",
            "boundary_waiting_raise",
            "boundary_waiting_snapshot",
            "boundary_waiting_c_resume",
            "boundary_unprotected_call",
            "boundary_unprotected_panic",
            "boundary_unprotected_fail",
            "boundary_yield_call",
            "boundary_yield_resume",
            "boundary_allocation_yield_tail",
        ] {
            boot.push_str(&format!(
                "{name}=assert(package.loadlib({},{}));",
                quote(module.as_bytes()),
                quote(name.as_bytes())
            ));
        }
        vm.execute(&compile_named(boot.as_bytes(), b"=native bootstrap").unwrap())
            .unwrap();
        let state = vm.global(b"boundary_state");
        let before = vm.call(state, &[]).unwrap()[0];
        let mut vm = Box::new(vm);
        assert_eq!(vm.call(state, &[]).unwrap()[0], before);
        vm.begin_standalone().unwrap();
        let source = std::fs::read(source).unwrap();
        vm.execute(&compile_named(&source, b"@boundary.lua").unwrap())
            .unwrap();
        vm.end_standalone().unwrap();
        vm.collect_garbage().unwrap();
        let collected = NativeCAbiLoader::statistics().states_destroyed;
        assert!(collected > 0);
        drop(vm);
        assert!(lifetime.is_none_or(|owner| owner.upgrade().is_none()));
        let stats = NativeCAbiLoader::statistics();
        assert_eq!(stats.states_created, stats.states_destroyed);
        assert_eq!(stats.groups_created, stats.groups_destroyed);
        assert_eq!(stats.operations, stats.returned);
        assert_eq!(stats.libraries_opened, stats.libraries_closed);
        eprintln!(
            "BOUNDARY operations={} drops={} panic_abort={}",
            stats.operations,
            stats.returned,
            cfg!(panic = "abort")
        );
        eprintln!(
            "STATES created={} collected={} destroyed={}",
            stats.states_created, collected, stats.states_destroyed
        );
        eprintln!(
            "LIBRARIES opened={} closed={}",
            stats.libraries_opened, stats.libraries_closed
        );
    }
    pub fn main() {
        std::thread::Builder::new()
            .stack_size(16 * 1024 * 1024)
            .spawn(run)
            .unwrap()
            .join()
            .unwrap();
    }
}
fn main() {
    #[cfg(any(windows, target_os = "linux"))]
    supported::main();
    #[cfg(not(any(windows, target_os = "linux")))]
    panic!("native C ABI is not implemented on this platform");
}
