use super::*;

#[test]
fn loader_cleanup_preserves_active_jobs_and_borrowed_files_without_allocating() {
    let mut vm = Vm::new_bare();
    vm.frames.try_reserve_exact(25).unwrap();
    let mut files = Vec::new();
    for code in [LOAD_READER, LOAD_FILE, DO_FILE, CONT_FILE, CONT_HOST_LOAD] {
        for pc in 0..5 {
            let owner = vm.frames.len();
            vm.frames.push(Frame {
                function: FrameKind::Native(code),
                pc,
                base: 0,
                stack_start: 0,
                result: 0,
                wanted: 0,
                barriers: 0,
                vararg_start: 0,
                vararg_count: 0,
                tail_calls: 0,
                c_level_bias: 0,
            });
            for owned in [false, true] {
                // Closed streams exercise arena ownership without operating-system
                // handles, so the same cleanup case can run under Miri.
                let key = vm.io.files.insert(io::Stream::closed());
                vm.start_native_load(
                    owner,
                    b"=cleanup",
                    0,
                    Some(FileReader {
                        key,
                        owned,
                        phase: 0,
                    }),
                )
                .unwrap();
                files.push((key, owned, matches!(pc, 1 | 3)));
            }
        }
    }
    // An out-of-range owner must be discarded without indexing the frame stack.
    vm.start_native_load(usize::MAX, b"=orphan", 0, None)
        .unwrap();
    let capacity = vm.loads.capacity();
    let pooled = vm.buffers.len();
    let ((), stats) = crate::allocation_probe::probe(0, || vm.trim_loads());
    assert_eq!(
        stats.attempts, 0,
        "cleanup allocated a temporary active-frame list"
    );
    assert_eq!(vm.loads.len(), 20);
    assert_eq!(vm.loads.capacity(), capacity);
    assert_eq!(vm.buffers.len(), pooled);
    for &(key, owned, active) in &files {
        assert_eq!(vm.io.files.contains(key), !owned || active);
    }
    // Repeating cleanup under persistent rejection must leave active jobs intact.
    let ((), stats) = crate::allocation_probe::probe(1, || vm.trim_loads());
    assert_eq!(stats.attempts, 0);
    assert_eq!(vm.loads.len(), 20);
    vm.frames.clear();
    let ((), stats) = crate::allocation_probe::probe(1, || vm.trim_loads());
    assert_eq!(stats.attempts, 0);
    assert!(vm.loads.is_empty());
    assert_eq!(vm.loads.capacity(), capacity);
    assert_eq!(vm.buffers.len(), pooled);
    for (key, owned, _) in files {
        assert_eq!(vm.io.files.contains(key), !owned);
    }
}
