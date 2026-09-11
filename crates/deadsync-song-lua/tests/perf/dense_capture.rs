use super::*;
use std::collections::HashMap;
use std::hint::black_box;

struct CaptureFixture {
    lua: Lua,
    context: SongLuaCompileContext,
    overlays: Vec<SongLuaOverlayCompileActor<()>>,
    baseline: Vec<SongLuaOverlayState>,
    from: Vec<SongLuaOverlayState>,
    update: Vec<SongLuaOverlayState>,
    to: Vec<SongLuaOverlayState>,
    tracks: Vec<SongLuaOverlayUpdateTrack>,
    indices: HashMap<(usize, SongLuaOverlayUpdateTarget), usize>,
    scheduled: Vec<SongLuaScheduledOverlaySample>,
    scratch: OverlaySampleScratch,
}

impl CaptureFixture {
    fn new(count: usize) -> Self {
        let lua = Lua::new();
        let overlays: Vec<_> = (0..count)
            .map(|_| {
                let table = lua.create_table().unwrap();
                reset_actor_capture(&lua, &table).unwrap();
                SongLuaOverlayCompileActor {
                    table,
                    actor: crate::SongLuaOverlayActor {
                        kind: (),
                        name: None,
                        parent_index: None,
                        initial_state: SongLuaOverlayState::default(),
                        message_commands: vec![],
                    },
                    message_sounds: vec![],
                }
            })
            .collect();
        crate::lua_util::begin_overlay_update_capture(
            &lua,
            overlays
                .iter()
                .enumerate()
                .map(|(index, overlay)| (overlay.table.to_pointer() as usize, index))
                .collect(),
        );
        let baseline = vec![SongLuaOverlayState::default(); count];
        Self {
            lua,
            context: SongLuaCompileContext::new(std::path::PathBuf::new(), "Dense capture"),
            overlays,
            from: baseline.clone(),
            update: baseline.clone(),
            to: baseline.clone(),
            baseline,
            tracks: vec![],
            indices: HashMap::new(),
            scheduled: vec![],
            scratch: OverlaySampleScratch::default(),
        }
    }

    fn write(&self, index: usize, key: &str, value: f32) {
        crate::lua_util::capture_block_set_f32(&self.lua, &self.overlays[index].table, key, value)
            .unwrap();
    }

    fn capture(&mut self, beat: f32, next: f32, restored: &[usize]) {
        capture_update_overlay_samples(
            &self.lua,
            &self.context,
            &self.overlays,
            &self.baseline,
            &self.from,
            &mut self.update,
            &mut self.to,
            restored,
            &mut self.tracks,
            &mut self.indices,
            beat,
            next,
            f64::from(next),
            &mut self.scheduled,
            &mut self.scratch,
        )
        .unwrap();
    }

    fn dense_tick(&mut self) {
        for index in 0..self.overlays.len() {
            for (key, value) in [
                ("zoom_x", 1.0),
                ("zoom_y", 1.0),
                ("rot_x_deg", 0.0),
                ("rot_y_deg", 0.0),
            ] {
                self.write(index, key, value);
            }
        }
        self.capture(0.0, 1.0, &[]);
    }

    fn values(
        &self,
        actor: usize,
        target: SongLuaOverlayUpdateTarget,
    ) -> Vec<(f32, SongLuaOverlayUpdateValue)> {
        self.tracks[self.indices[&(actor, target)]]
            .samples
            .iter()
            .map(|sample| (sample.beat, sample.value.clone()))
            .collect()
    }
}

#[test]
fn dense_capture_preserves_first_writes_and_message_precedence_across_ticks() {
    use SongLuaOverlayUpdateTarget::{ZoomX, ZoomY};
    use SongLuaOverlayUpdateValue::F32;
    let mut fixture = CaptureFixture::new(128);
    fixture.dense_tick();
    assert_eq!(fixture.tracks.len(), 512);
    // Even an unchanged write owns this tick: a restored message must not
    // overwrite it. A different, unwritten target must follow the message.
    fixture.to[0].zoom_x = 8.0;
    fixture.to[1].zoom_x = 9.0;
    fixture.write(0, "zoom_x", 1.0);
    fixture.capture(1.0, 2.0, &[0, 1]);
    assert_eq!(fixture.values(0, ZoomX), vec![(1.0, F32(1.0))]);
    assert_eq!(
        fixture.values(1, ZoomX),
        vec![(1.0, F32(1.0)), (2.0, F32(9.0))]
    );
    assert_eq!(fixture.to[0].zoom_x, 8.0);
    // Next tick has no writes: membership must reset, so the first actor's
    // pending persistent message is synchronized too.
    fixture.capture(2.0, 3.0, &[]);
    assert_eq!(
        fixture.values(0, ZoomX),
        vec![(1.0, F32(1.0)), (2.0, F32(1.0)), (3.0, F32(8.0))]
    );
    fixture.write(0, "zoom_y", 4.0);
    fixture.capture(3.0, 4.0, &[]);
    assert_eq!(fixture.update[0].zoom_y, 4.0);
    assert_eq!(fixture.to[0].zoom_y, 4.0);
    assert_eq!(fixture.values(0, ZoomY).last(), Some(&(4.0, F32(4.0))));
    assert!(fixture.scheduled.is_empty());
}

#[test]
fn capture_accepts_new_tracks_after_scheduled_track_completion() {
    use SongLuaOverlayUpdateTarget::{X, Y};
    use SongLuaOverlayUpdateValue::F32;
    let mut fixture = CaptureFixture::new(2);
    fixture.overlays[0]
        .table
        .set("__songlua_capture_duration", 1.0)
        .unwrap();
    fixture.write(0, "x", 12.0);
    fixture.capture(0.0, 0.0, &[]);
    assert_eq!(fixture.scheduled.len(), 1);
    assert!(fixture.tracks.is_empty());
    fixture.write(1, "y", 5.0);
    fixture.capture(0.0, 1.0, &[]);
    assert!(fixture.scheduled.is_empty());
    assert_eq!(fixture.tracks.len(), 2);
    assert_eq!(fixture.values(0, X).last(), Some(&(1.0, F32(12.0))));
    assert_eq!(fixture.values(1, Y), vec![(1.0, F32(5.0))]);
    fixture.dense_tick();
    assert_eq!(fixture.tracks.len(), 10);
}

#[test]
#[ignore = "manual release benchmark; run serially"]
fn dense_capture_hot_path_bench() {
    for count in [32, 128, 512] {
        let mut fixture = CaptureFixture::new(count);
        crate::perf::measure_sampled(
            &format!("capture_{}_targets", count * 4),
            64,
            count * 4,
            || black_box(&mut fixture).dense_tick(),
        );
        assert_eq!(fixture.tracks.len(), count * 4);
        assert!(fixture.tracks.iter().all(|track| track.samples.len() == 1));
    }
}

#[test]
fn dense_capture_reuses_scratch_storage_after_warmup() {
    let mut fixture = CaptureFixture::new(128);
    fixture.dense_tick();
    fixture.to[0].zoom_x = 9.0;
    fixture.capture(1.0, 2.0, &[]);
    let capacities = (
        fixture.scratch.reset_indices.capacity(),
        fixture.scratch.captured_tracks.capacity(),
        fixture.scratch.message_targets.capacity(),
    );
    assert!(capacities.0 >= 128 && capacities.1 >= 512 && capacities.2 > 0);
    fixture.to.copy_from_slice(&fixture.baseline);
    for _ in 0..8 {
        fixture.dense_tick();
        assert!(
            fixture
                .scratch
                .captured_tracks
                .iter()
                .all(|captured| *captured)
        );
        assert!(fixture.scratch.message_targets.is_empty());
        assert_eq!(
            capacities,
            (
                fixture.scratch.reset_indices.capacity(),
                fixture.scratch.captured_tracks.capacity(),
                fixture.scratch.message_targets.capacity(),
            )
        );
    }
}
