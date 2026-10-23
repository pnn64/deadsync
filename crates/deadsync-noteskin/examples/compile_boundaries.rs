//! Complete noteskin compilation with exact serialized output and CPU accounting.
use deadsync_noteskin::{compiler::compile_data, itg::NoteskinData};

#[path = "../../../tests/support/song_lua_metrics.rs"]
mod metrics;

fn main() {
    #[cfg(windows)]
    {
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetCurrentThread() -> *mut std::ffi::c_void;
            fn SetThreadAffinityMask(thread: *mut std::ffi::c_void, mask: usize) -> usize;
        }
        // SAFETY: only this disposable worker's affinity changes.
        assert_ne!(
            unsafe { SetThreadAffinityMask(GetCurrentThread(), 1 << 2) },
            0
        );
    }
    let args: Vec<_> = std::env::args().skip(1).collect();
    assert_eq!(args.len(), 3, "compile_boundaries INPUT_DIR REPORT SAMPLES");
    let data = NoteskinData {
        name: "aliases-fixture".into(),
        overrides: vec![],
        metrics: Default::default(),
        search_dirs: vec![args[0].clone().into()],
    };
    let compile = || compile_data("dance", &data, "fixture-source", "Mirror").unwrap();
    let expected = bincode::encode_to_vec(compile(), bincode::config::standard()).unwrap();
    let mut rows = Vec::new();
    for _ in 0..args[2].parse::<usize>().unwrap() {
        let measurement = metrics::Measurement::start();
        let output = compile();
        rows.push(measurement.finish());
        assert_eq!(
            bincode::encode_to_vec(output, bincode::config::standard()).unwrap(),
            expected
        );
    }
    std::fs::write(&args[1], serde_json::to_vec_pretty(&serde_json::json!({
        "samples": rows, "snapshot": expected,
        "scope":"disk read through complete compiled bundle; serialization and final destruction excluded"
    })).unwrap()).unwrap();
}
