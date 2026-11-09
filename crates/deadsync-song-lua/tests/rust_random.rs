use deadlib_lua::{Value, compile};
use deadsync_song_lua::{SongLuaCompileContext, rust_engine::RustSongRuntime};

struct Pair {
    rust: RustSongRuntime,
}

impl Pair {
    fn new(seed: u32) -> Self {
        let mut context = SongLuaCompileContext::new(".", "Rust random regression");
        context.random_seed = seed;
        Self {
            rust: RustSongRuntime::new(&context).unwrap(),
        }
    }

    fn compare(&mut self, script: &str) -> Vec<u64> {
        let program = compile(script.as_bytes()).unwrap();
        let actual: Vec<_> = self
            .rust
            .vm_mut()
            .execute(&program)
            .unwrap()
            .iter()
            .map(|value| match value {
                Value::Number(value) => value.to_bits(),
                other => panic!("expected numeric output, got {other:?}"),
            })
            .collect();
        reference_test::assert_source_debug(
            "song/tests/rust_random.rs::Pair::compare",
            script,
            &actual,
        );
        actual
    }
}

#[test]
fn song_seed_and_both_namespaces_share_the_native_generator_stream() {
    for seed in [1, 73, 5489, i32::MAX as u32, u32::MAX] {
        let mut pair = Pair::new(seed);
        pair.compare(
            r#"
            assert(math.random==MersenneTwister.Random)
            assert(math.randomseed==MersenneTwister.Seed)
            local out={}
            for i=1,1024 do
                if i%4==0 then out[i]=math.random()
                elseif i%4==1 then out[i]=MersenneTwister.Random(-2147483648,2147483647)
                elseif i%4==2 then out[i]=math.random(-2147483648,0)
                else out[i]=math.random(100) end
            end
            return unpack(out)
        "#,
        );
    }
}

#[test]
fn reseeding_numeric_conversion_and_unit_intervals_match_legacy() {
    let mut pair = Pair::new(1);
    pair.compare(
        r#"
        local out={}
        for _,seed in ipairs({'73',-1.9,2147483647.9,-2147483648.9,17}) do
            math.randomseed(seed,'ignored')
            out[#out+1]=math.random(1)
            out[#out+1]=MersenneTwister.Random()
            out[#out+1]=math.random('-10.9','10.9')
            MersenneTwister.Seed(seed)
            out[#out+1]=math.random(1)
            out[#out+1]=math.random()
            out[#out+1]=MersenneTwister.Random(-10,10)
        end
        return unpack(out)
    "#,
    );
}

#[test]
fn rejected_calls_leave_the_stream_unchanged_and_do_not_poison_later_calls() {
    let mut pair = Pair::new(1);
    pair.compare(
        r#"
        local attempts={
            function() math.random(nil) end,
            function() math.random(0) end,
            function() math.random(2,1) end,
            function() math.random(2,2) end,
            function() math.random(1,2,3) end,
            function() math.random(0/0) end,
            function() math.random(math.huge) end,
            function() math.random(2147483648) end,
            function() math.random(false) end,
            function() math.random({}) end,
            function() math.randomseed() end,
            function() math.randomseed(nil) end,
            function() math.randomseed(0/0) end,
            function() math.randomseed(-2147483649) end,
        }
        local out={}
        for i,attempt in ipairs(attempts) do
            math.randomseed(73)
            local expected=math.random()
            math.randomseed(73)
            assert(not pcall(attempt))
            local actual=math.random()
            assert(actual==expected)
            out[i]=actual
        end
        return unpack(out)
    "#,
    );
}

#[test]
fn saved_callbacks_survive_gc_and_separate_song_vms_keep_separate_streams() {
    let mut first = Pair::new(73);
    let mut second = Pair::new(73);
    let a = first.compare("saved=math.random; return saved(),saved(),saved()");
    let b = second.compare("saved=math.random; return saved(),saved(),saved()");
    assert_eq!(a, b);
    first.compare("math.randomseed(31); return saved(),saved()");
    second.compare("math.random=nil; MersenneTwister=nil; collectgarbage('collect'); return saved(),saved(),saved()");
    first.compare("collectgarbage('collect'); return saved(),saved(),saved()");
}

#[test]
fn nested_baking_snapshots_restore_reseeds_and_survive_vm_drop() {
    let mut pair = Pair::new(73);
    let first = pair.rust.preserve_random();
    let mut control = Pair::new(73);
    let expected = control.compare("return math.random(),math.random()");
    let program = compile(b"math.randomseed(99); return math.random(),math.random()").unwrap();
    pair.rust.vm_mut().execute(&program).unwrap();
    let second = pair.rust.preserve_random();
    let next = compile(b"return math.random(),math.random()").unwrap();
    let advanced = pair.rust.vm_mut().execute(&next).unwrap().to_vec();
    pair.rust.vm_mut().execute(&program).unwrap();
    drop(second);
    assert_eq!(pair.rust.vm_mut().execute(&next).unwrap(), advanced);
    drop(first);
    assert_eq!(pair.compare("return math.random(),math.random()"), expected);
    let snapshot = pair.rust.preserve_random();
    drop(pair);
    drop(snapshot);
}

#[test]
fn explicit_zero_seed_selects_the_current_wall_clock_seed() {
    let mut context = SongLuaCompileContext::new(".", "zero seed");
    context.random_seed = 73;
    let mut runtime = RustSongRuntime::new(&context).unwrap();
    let before = chrono::Utc::now().timestamp();
    let zero =
        compile(b"math.randomseed(0); return math.random(),math.random(),math.random()").unwrap();
    let actual = runtime.vm_mut().execute(&zero).unwrap().to_vec();
    let after = chrono::Utc::now().timestamp();
    assert!(
        after >= before && after - before < 10,
        "clock changed during test"
    );
    let mut matches = false;
    for timestamp in before..=after {
        let script = format!(
            "math.randomseed({}); return math.random(),math.random(),math.random()",
            timestamp as i32
        );
        if runtime
            .vm_mut()
            .execute(&compile(script.as_bytes()).unwrap())
            .unwrap()
            == actual
        {
            matches = true;
        }
    }
    assert!(matches, "zero seed did not select a current second");
}

#[test]
fn worker_handoff_keeps_lua_closures_clock_captures_and_random_state() {
    let (mut runtime, first) = std::thread::spawn(|| {
        let mut context = SongLuaCompileContext::new(".", "worker-owned Rust song VM");
        context.random_seed = 73;
        let mut runtime = RustSongRuntime::new(&context).unwrap();
        runtime.set_values(4.0, 2.0).unwrap();
        let position = runtime.song_position().unwrap();
        runtime.vm_mut().set_global(b"position", position).unwrap();
        let program = compile(
            br#"
            local count=0
            local position=position
            function sample()
                count=count+1
                collectgarbage('collect')
                return count,position:GetSongBeat(),math.random()
            end
            return sample()
        "#,
        )
        .unwrap();
        let first = runtime.vm_mut().execute(&program).unwrap().to_vec();
        (runtime, first)
    })
    .join()
    .unwrap();
    let mut control = Pair::new(73);
    let first_random = control.compare("return math.random()")[0];
    assert_eq!(
        first,
        vec![
            Value::Number(1.0),
            Value::Number(4.0),
            Value::Number(f64::from_bits(first_random))
        ]
    );
    runtime.set_values(6.0, 3.0).unwrap();
    let second = runtime
        .vm_mut()
        .execute(&compile(b"position=nil; collectgarbage('collect'); return sample()").unwrap())
        .unwrap();
    let second_random = control.compare("return math.random()")[0];
    assert_eq!(
        second,
        &[
            Value::Number(2.0),
            Value::Number(6.0),
            Value::Number(f64::from_bits(second_random))
        ]
    );
}

#[cfg(windows)]
#[test]
fn draws_match_the_recorded_windows_native_random_probe() {
    // The committed native trace records these RNG arguments before actor f32
    // storage. Compare the draws themselves; this does not replay its actors.
    // This probe records Windows distribution conversion. The authored scripts
    // above also check seeding, stream state and ownership contracts.
    let trace: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/itgmania-song-lua-micro/random-probe.json"
    ))
    .unwrap();
    assert_eq!(trace["random_generator"], "ITGmania MersenneTwister");
    assert_eq!(trace["dropped_events"], 0);
    assert!(trace["runtime_errors"].as_array().unwrap().is_empty());
    assert!(trace["random_reseeds"].as_array().unwrap().is_empty());
    assert_eq!(trace["capabilities"]["raw_event_stream"], false);
    let mut draws = std::collections::BTreeMap::new();
    for track in trace["operation_tracks"].as_array().unwrap() {
        let slot = match track["operation"].as_str().unwrap() {
            "Quad.x" => 0,
            "Quad.y" => 1,
            _ => continue,
        };
        for sample in track["samples"].as_array().unwrap() {
            assert_eq!(sample[3].as_array().unwrap().len(), 1);
            let entry = draws
                .entry(sample[2].as_f64().unwrap().to_bits())
                .or_insert([None, None]);
            assert!(
                entry[slot]
                    .replace((sample[0].as_u64().unwrap(), sample[3][0].as_f64().unwrap()))
                    .is_none()
            );
        }
    }
    assert_eq!(draws.len(), 123);
    assert!(draws.values().all(|pair| pair.iter().all(Option::is_some)));
    let mut context = SongLuaCompileContext::new(".", "recorded native RNG control");
    context.random_seed = u32::try_from(trace["random_seed"].as_u64().unwrap()).unwrap();
    let mut runtime = RustSongRuntime::new(&context).unwrap();
    let program =
        compile(b"local beat=...; if beat>0.19 then return math.random(100,200),math.random() end")
            .unwrap();
    let update = Value::Function(runtime.vm_mut().try_load_program(&program).unwrap());
    let _root = runtime.vm_mut().pin(update).unwrap();
    let frames = trace["update_frames"].as_array().unwrap();
    assert_eq!(frames.len(), 241);
    // Trace observations are sampled. Advance every recorded external frame,
    // including frames absent from operation_tracks; never infer extra draws
    // from an expected random value or skip a mismatching observation.
    for frame in frames {
        let beat = f64::from(frame[0].as_f64().unwrap() as f32);
        let values = runtime
            .vm_mut()
            .call(update, &[Value::Number(beat)])
            .unwrap();
        if let Some(expected) = draws.remove(&frame[1].as_f64().unwrap().to_bits()) {
            assert_eq!(values.len(), 2);
            for (value, expected) in values.iter().zip(expected) {
                let (sequence, expected) = expected.unwrap();
                let Value::Number(actual) = value else {
                    panic!("numeric RNG result")
                };
                assert_eq!(
                    actual.to_bits(),
                    expected.to_bits(),
                    "native event {sequence}"
                );
            }
        }
    }
    assert!(draws.is_empty(), "unvisited native observations");
}

#[allow(dead_code)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;
