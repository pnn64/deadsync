//! Measure Rust noteskin preparation and verify repeatable complete output.
pub use deadsync_noteskin::{Quantization, actor, compiled, itg, lua};
use deadsync_noteskin::{compiled::CompiledNoteskinBundle, compiler};

use serde_json::json;
#[path = "../tests/support/snapshot.rs"]
mod snapshot;
use std::{fs, path::PathBuf, time::Instant};

#[cfg(windows)]
fn thread_cycles() -> Option<u64> {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentThread() -> *mut std::ffi::c_void;
        fn QueryThreadCycleTime(thread: *mut std::ffi::c_void, cycles: *mut u64) -> i32;
    }
    let mut cycles = 0;
    // SAFETY: the pseudo-handle selects this thread and the output is valid.
    assert_ne!(
        unsafe { QueryThreadCycleTime(GetCurrentThread(), &mut cycles) },
        0
    );
    Some(cycles)
}
#[cfg(not(windows))]
fn thread_cycles() -> Option<u64> {
    None
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let root = PathBuf::from(args.next().ok_or("expected NOTESKIN_ROOT REPORT SAMPLES")?);
    let report = PathBuf::from(args.next().ok_or("expected REPORT")?);
    let samples: usize = args
        .next()
        .ok_or("expected SAMPLES")?
        .to_str()
        .ok_or("invalid SAMPLES")?
        .parse()?;
    if args.next().is_some() || samples < 3 {
        return Err("expected NOTESKIN_ROOT REPORT SAMPLES (at least 3)".into());
    }
    let mut rows = Vec::new();
    for game in ["dance", "pump"] {
        for skin in itg::discover_skins(std::slice::from_ref(&root), game) {
            eprintln!("measure {game}/{skin}");
            let data = itg::load_noteskin_data(&root, game, &skin)?;
            let compile = || -> Result<CompiledNoteskinBundle, String> {
                let bundle = compiler::compile_data(game, &data, "same-input", "Mirror")?;
                assert_eq!((bundle.loader.version, bundle.actors.version), (30, 30));
                Ok(bundle)
            };
            // Establish a warm baseline for repeatability across independent VMs.
            let expected_bundle = compile()?;
            let expected = snapshot::bundle(&expected_bundle);
            let mut measurements = Vec::new();
            for sample in 0..samples {
                let cycles = thread_cycles();
                let start = Instant::now();
                let bundle = std::hint::black_box(compile()?);
                let seconds = start.elapsed().as_secs_f64();
                let cycles = cycles
                    .zip(thread_cycles())
                    .map(|(before, after)| after - before);
                assert_eq!(
                    snapshot::bundle(&bundle),
                    expected,
                    "{game}/{skin}, sample {sample}"
                );
                measurements.push(json!({"sample":sample,"backend":"rust",
                        "wall_seconds":seconds,"thread_cpu_cycles":cycles}));
            }
            rows.push(json!({"game":game,"skin":skin,"loader_entries":expected_bundle.loader.entries.len(),
                "actor_files":expected_bundle.actors.files.len(),"repeat_output_equal":true,"samples":measurements}));
        }
    }
    fs::write(
        report,
        serde_json::to_vec_pretty(&json!({
            "os":std::env::consts::OS,"arch":std::env::consts::ARCH,"debug_assertions":cfg!(debug_assertions),
            "samples_per_backend":samples,"player_options":"Mirror","cases":rows,
            "scope":"independent VM per call; source reads, compilation, preparation and VM destruction included; input discovery, output comparison and bundle destruction excluded; OS/source/path caches warmed",
            "limitations":"wall time and Windows thread cycles only; no allocation or peak-memory measurements; not a general VM or LuaJIT benchmark"
        }))?,
    )?;
    Ok(())
}
