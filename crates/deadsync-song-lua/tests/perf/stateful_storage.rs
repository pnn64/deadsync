use super::*;
use std::hint::black_box;

#[path = "stateful_storage_baseline.rs"]
mod baseline;

struct Fixture {
    _lua: Lua,
    actors: Vec<Table>,
    capture: SongLuaOverlayUpdateCapture,
}

impl Fixture {
    fn new(count: usize, name: Option<&str>) -> Self {
        let lua = Lua::new();
        let actors: Vec<_> = (0..count + 1)
            .map(|_| lua.create_table().unwrap())
            .collect();
        let mut capture = SongLuaOverlayUpdateCapture::new(
            actors[..count]
                .iter()
                .enumerate()
                .map(|(i, actor)| (actor.to_pointer() as usize, i))
                .collect(),
        );
        capture.active_broadcast = name.map(str::to_owned);
        Self {
            _lua: lua,
            actors,
            capture,
        }
    }

    fn record(&mut self, old: bool, actor: usize, index: usize) {
        let target = [SongLuaOverlayUpdateTarget::X, SongLuaOverlayUpdateTarget::Y][index % 2];
        let value = SongLuaOverlayUpdateValue::F32(index as f32);
        let args = (&self.actors[actor], index as f32, target, value);
        if old {
            baseline::record_stateful_message(
                &mut self.capture,
                args.0,
                args.1,
                args.2,
                args.3,
                0.25,
                0.5,
                None,
                Some(-0.0),
            );
        } else {
            self.capture.record_stateful_message(
                args.0,
                args.1,
                args.2,
                args.3,
                0.25,
                0.5,
                None,
                Some(-0.0),
            );
        }
    }

    fn batch(&mut self, old: bool, count: usize, actor: usize, cold: bool) {
        if cold {
            self.capture.stateful_messages.clear();
            self.capture.stateful_writes.clear();
        } else {
            for writes in self.capture.stateful_writes.values_mut() {
                writes.clear();
            }
        }
        for index in 0..count {
            self.record(old, actor, index);
        }
        black_box(&self.capture.stateful_writes);
    }
}

fn assert_captures(actual: &Fixture, expected: &Fixture) {
    assert_eq!(
        actual.capture.active_broadcast,
        expected.capture.active_broadcast
    );
    assert_eq!(actual.capture.touched, expected.capture.touched);
    assert_eq!(actual.capture.touched_flags, expected.capture.touched_flags);
    assert_eq!(
        actual.capture.stateful_messages,
        expected.capture.stateful_messages
    );
    assert_eq!(
        actual.capture.stateful_writes,
        expected.capture.stateful_writes
    );
}

#[test]
fn stateful_capture_matches_parent_for_broadcast_switches_unknown_actors_and_write_order() {
    let mut old = Fixture::new(8, None);
    let mut new = Fixture::new(8, None);
    for (tick, name) in [
        None,
        Some(""),
        Some("A"),
        Some("B"),
        Some("A"),
        None,
        Some("B"),
    ]
    .into_iter()
    .enumerate()
    {
        old.capture.active_broadcast = name.map(str::to_owned);
        new.capture.active_broadcast = name.map(str::to_owned);
        for index in 0..100 {
            let actor = (index * 7 + tick) % 9;
            old.record(true, actor, index);
            new.record(false, actor, index);
        }
        assert_captures(&new, &old);
    }
    let writes = &new.capture.stateful_writes["A"];
    assert_eq!(writes[0].overlay_index, 2);
    assert_eq!(writes[0].value, SongLuaOverlayUpdateValue::F32(0.0));
    assert_eq!(writes[0].opt1.unwrap().to_bits(), (-0.0_f32).to_bits());
    assert_eq!(new.capture.stateful_messages["A"][&2].len(), 2);
}

#[test]
fn stateful_capture_preserves_owned_metadata_and_shared_vertex_colors() {
    let mut old = Fixture::new(1, Some("Glow"));
    let mut new = Fixture::new(1, Some("Glow"));
    let colors = Arc::new([[0.0, -0.0, 0.5, 1.0]; 4]);
    for (is_old, fixture) in [(true, &mut old), (false, &mut new)] {
        let value = SongLuaOverlayUpdateValue::VertexColors(colors.clone());
        if is_old {
            baseline::record_stateful_message(
                &mut fixture.capture,
                &fixture.actors[0],
                -0.0,
                SongLuaOverlayUpdateTarget::VertexColors,
                value,
                -2.0,
                3.0,
                Some("decelerate".to_owned()),
                Some(0.75),
            );
        } else {
            fixture.capture.record_stateful_message(
                &fixture.actors[0],
                -0.0,
                SongLuaOverlayUpdateTarget::VertexColors,
                value,
                -2.0,
                3.0,
                Some("decelerate".to_owned()),
                Some(0.75),
            );
        }
    }
    assert_captures(&new, &old);
    let write = &new.capture.stateful_writes["Glow"][0];
    assert_eq!(write.beat.to_bits(), (-0.0_f32).to_bits());
    let SongLuaOverlayUpdateValue::VertexColors(value) = &write.value else {
        panic!("lost vertex colors")
    };
    assert!(Arc::ptr_eq(value, &colors));
    assert_eq!(Arc::strong_count(&colors), 3);
}

#[test]
fn repeated_stateful_writes_reuse_names_and_output_capacity_without_churn() {
    let mut fixture = Fixture::new(1, Some("Repeated broadcast with a retained name"));
    fixture.batch(false, 128, 0, false);
    crate::perf::assert_no_churn(|| {
        for _ in 0..16 {
            fixture.batch(false, 128, 0, false);
        }
    });
    crate::perf::assert_no_churn(|| fixture.batch(false, 16, 1, false));
    assert!(fixture.capture.stateful_writes.values().all(Vec::is_empty));
}

fn pair(name: &str, units: usize, mut old: impl FnMut(), mut new: impl FnMut()) {
    if std::env::var_os("DEADSYNC_PERF_REVERSE").is_some() {
        crate::perf::measure_sampled(&format!("{name}/new"), 512, units, &mut new);
        crate::perf::measure_sampled(&format!("{name}/old"), 512, units, &mut old);
    } else {
        crate::perf::measure_sampled(&format!("{name}/old"), 512, units, &mut old);
        crate::perf::measure_sampled(&format!("{name}/new"), 512, units, &mut new);
    }
}

#[test]
#[ignore = "manual paired release benchmark; run serially with --nocapture"]
fn overlay_storage_bench_stateful() {
    for (count, len, cold) in [
        (1, 8, false),
        (16, 8, false),
        (128, 8, false),
        (128, 256, false),
        (1, 8, true),
        (128, 8, true),
    ] {
        let name = "B".repeat(len);
        let mut old = Fixture::new(1, Some(&name));
        let mut new = Fixture::new(1, Some(&name));
        old.batch(true, count, 0, cold);
        new.batch(false, count, 0, cold);
        assert_captures(&new, &old);
        pair(
            &format!("stateful_{count}_name_{len}_cold_{cold}"),
            count,
            || old.batch(true, count, 0, cold),
            || new.batch(false, count, 0, cold),
        );
    }
    for (label, name, actor) in [
        ("inactive", None, 0),
        ("unknown", Some("Active broadcast"), 1),
    ] {
        let mut old = Fixture::new(1, name);
        let mut new = Fixture::new(1, name);
        pair(
            &format!("stateful_{label}"),
            1,
            || old.record(true, actor, 0),
            || new.record(false, actor, 0),
        );
        assert_captures(&new, &old);
    }
}
