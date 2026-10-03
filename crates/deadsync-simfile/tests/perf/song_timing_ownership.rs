use super::*;
use std::hint::black_box;
use std::sync::Arc;

#[path = "song_timing_ownership/baseline.rs"]
mod baseline;

struct Fixture {
    root: PathBuf,
    path: PathBuf,
    data: Vec<u8>,
    options: ParseSongOptions,
}

impl Fixture {
    fn new(count: usize, lua: bool) -> Self {
        let root = std::env::temp_dir().join(format!(
            "deadsync-timing-ownership-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("overlay.lua"), "return Def.ActorFrame{}").unwrap();
        let mut data = String::from("#TITLE:Timing ownership;\n#BPMS:0=150;\n#STOPS:");
        for index in 0..count {
            if index != 0 {
                data.push(',');
            }
            data.push_str(&format!("{}=0.125", index * 4));
        }
        data.push_str(";\n");
        if lua {
            data.push_str("#FGCHANGES:0=overlay.lua=1;\n");
        }
        data.push_str("#NOTES:dance-single::Challenge:12:0,0,0,0,0:\n1000\n0100\n0010\n0001\n;\n");
        Self {
            path: root.join("song.sm"),
            root,
            data: data.into_bytes(),
            options: ParseSongOptions::new(Vec::new(), Vec::new(), Vec::new()),
        }
    }

    fn analyzed(&self) -> (Option<SimfileSummary>, Vec<Vec<CachedParsedNote>>) {
        let analyzer = SongAnalyzer::new(&self.options);
        let mut scratch = AnalysisScratch::default();
        let mut notes = Vec::new();
        let summary = analyze_prepared_in_with_notes(
            &self.data,
            "sm",
            &analyzer.prepared,
            &mut scratch,
            &mut notes,
            cached_note_from_rssp,
        )
        .unwrap();
        (Some(summary), notes)
    }

    fn build(
        &self,
        state: &mut (Option<SimfileSummary>, Vec<Vec<CachedParsedNote>>),
        old: bool,
    ) -> SerializableSongData {
        let input = SongBuildInput {
            path: &self.path,
            simfile_dir: &self.root,
            simfile_data: &self.data,
            song_music_path: None,
            music_length_seconds: 10.0,
            options: &self.options,
            parsed_notes: &mut state.1,
        };
        let summary = state.0.take().unwrap();
        if old {
            baseline::build_song_data(summary, input)
        } else {
            build_song_data(summary, input)
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}

#[test]
fn owning_global_timing_preserves_serialized_songs_and_shared_sources() {
    for lua in [false, true] {
        for count in [0, 1, 32] {
            let fixture = Fixture::new(count, lua);
            for shared in [false, true] {
                let mut new_state = fixture.analyzed();
                let retained = shared
                    .then(|| Arc::clone(&new_state.0.as_ref().unwrap().global_timing_segments));
                let before = retained.as_ref().map(|segments| segments.stops.clone());
                let old = fixture.build(&mut fixture.analyzed(), true);
                let new = fixture.build(&mut new_state, false);
                assert_eq!(new.has_lua, lua);
                assert_eq!(new.song_timing.is_some(), lua && count != 0);
                let encode =
                    |song| bincode::encode_to_vec(song, bincode::config::standard()).unwrap();
                assert_eq!(encode(new), encode(old));
                if let Some(retained) = retained {
                    assert_eq!(retained.stops, before.unwrap());
                }
            }
        }
    }
}

#[test]
fn owning_global_timing_reduces_churn_in_the_song_builder() {
    let fixture = Fixture::new(4096, true);
    let mut old = fixture.analyzed();
    let mut new = fixture.analyzed();
    crate::perf::assert_reduced_churn(
        || {
            black_box(fixture.build(&mut old, true));
        },
        || {
            black_box(fixture.build(&mut new, false));
        },
    );
}

#[test]
#[ignore = "manual release benchmark"]
fn song_timing_ownership_benchmark() {
    for count in [32, 4096] {
        let fixture = Fixture::new(count, true);
        let order = if std::env::var_os("DEADSYNC_BENCH_NEW_FIRST").is_some() {
            [false, true]
        } else {
            [true, false]
        };
        for old in order {
            crate::perf::measure_sampled_with_setup(
                &format!(
                    "song/timing={count}/{}",
                    if old { "original" } else { "current" }
                ),
                32,
                1,
                || fixture.analyzed(),
                |state| {
                    black_box(fixture.build(state, old));
                },
            );
        }
    }
}
