//! The bake cache reuses a bake only for the session that produced it.
use deadsync_chart::SongData;
use deadsync_song_lua::SongLuaCompileContext;
use deadsync_song_lua::bake_cache::{BakeCache, BakeCacheStatus, prepare_song_lua_cached};
use std::cell::Cell;
use std::path::{Path, PathBuf};

#[path = "support/paths.rs"]
mod paths;

type Prepared =
    deadsync_song_lua::playback::PreparedGameplaySongLua<deadsync_assets::noteskin::SpriteSlot>;

const SCENE: &str = r#"
return Def.ActorFrame{
    Def.Quad{
        Name="Panel",
        OnCommand=function(self)
            self:zoomto(SCREEN_WIDTH / 2, 64):diffuse(1, 0, 0, 1)
            self:linear(0.125):x(SCREEN_WIDTH / 4):diffusealpha(0.5)
        end,
    },
    Def.ActorFrame{
        Name="Group",
        OnCommand=function(self) self:sleep(0.0625):decelerate(0.125):zoom(2) end,
    },
}
"#;

struct Fixture {
    /// Holds a pack folder with the song folder inside it.
    root: tempfile::TempDir,
    cache_dir: tempfile::TempDir,
    song: SongData,
    context: SongLuaCompileContext,
    compiles: Cell<usize>,
}

impl Fixture {
    fn new(foreground: &str) -> Self {
        paths::init_paths();
        let root = tempfile::tempdir().unwrap();
        let song_dir = root.path().join("pack").join("song");
        std::fs::create_dir_all(&song_dir).unwrap();
        std::fs::write(song_dir.join("foreground.lua"), foreground).unwrap();
        let simfile = song_dir.join("song.ssc");
        std::fs::write(
            &simfile,
            "#TITLE:Bake cache;\n#BPMS:0=120;\n#FGCHANGES:0=foreground.lua=1=0=0=1;\n\
             #NOTEDATA:;\n#STEPSTYPE:dance-single;\n#DIFFICULTY:Challenge;\n#METER:1;\n\
             #NOTES:\n1000\n0000\n0100\n0000\n;\n",
        )
        .unwrap();
        let song = deadsync_simfile::app_runtime::parse_song_for_test(&simfile, 0.0).unwrap();
        assert_eq!(song.foreground_lua_changes.len(), 1);
        let mut context = SongLuaCompileContext::new(&song_dir, &song.title);
        context.music_length_seconds = 0.5;
        Self {
            root,
            cache_dir: tempfile::tempdir().unwrap(),
            song,
            context,
            compiles: Cell::new(0),
        }
    }

    fn song_dir(&self) -> PathBuf {
        self.root.path().join("pack").join("song")
    }

    fn cache(&self) -> BakeCache {
        BakeCache::new(self.cache_dir.path(), "test host")
    }

    fn prepare(&self) -> (Prepared, BakeCacheStatus) {
        self.prepare_with(&self.cache(), &self.context, &[])
    }

    fn prepare_with(
        &self,
        cache: &BakeCache,
        context: &SongLuaCompileContext,
        host_inputs: &[(String, u64)],
    ) -> (Prepared, BakeCacheStatus) {
        prepare_song_lua_cached(
            cache,
            &self.song,
            context,
            host_inputs,
            |path, context| {
                self.compiles.set(self.compiles.get() + 1);
                deadsync_assets::song_lua::compile_song_lua(path, context)
            },
            |paths, primary, context| {
                self.compiles.set(self.compiles.get() + 1);
                deadsync_assets::song_lua::compile_song_lua_layers(paths, primary, context)
            },
        )
    }

    /// Replace a song file with content of a different length, so the change
    /// is visible even on a filesystem with coarse modification times.
    fn rewrite(&self, name: &str, content: &str) {
        let path = self.song_dir().join(name);
        assert_ne!(
            std::fs::metadata(&path).map(|metadata| metadata.len()).ok(),
            Some(content.len() as u64),
        );
        std::fs::write(path, content).unwrap();
    }

    fn entries(&self) -> Vec<PathBuf> {
        fn collect(directory: &Path, found: &mut Vec<PathBuf>) {
            for entry in std::fs::read_dir(directory).into_iter().flatten().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    collect(&path, found);
                } else if path
                    .extension()
                    .is_some_and(|extension| extension == "bake")
                {
                    found.push(path);
                }
            }
        }
        let mut found = Vec::new();
        collect(self.cache_dir.path(), &mut found);
        found.sort();
        found
    }
}

fn not_stored(status: &BakeCacheStatus) -> &str {
    match status {
        BakeCacheStatus::NotStored(reason) => reason,
        other => panic!("expected an unstored bake, got {other:?}"),
    }
}

#[test]
fn a_cached_load_equals_the_fresh_bake_and_runs_no_script() {
    let fixture = Fixture::new(SCENE);
    let (baked, status) = fixture.prepare();
    assert_eq!(status, BakeCacheStatus::Stored);
    assert!(baked.compile_succeeded());
    let compiles = fixture.compiles.get();
    assert!(compiles > 0);
    assert_eq!(fixture.entries().len(), 1);

    let (cached, status) = fixture.prepare();
    assert_eq!(status, BakeCacheStatus::Hit);
    assert_eq!(fixture.compiles.get(), compiles, "a hit must not compile");
    assert_eq!(format!("{cached:?}"), format!("{baked:?}"));
    assert!(
        format!("{cached:?}").contains("Panel"),
        "the scene was baked"
    );
    assert_eq!(fixture.entries().len(), 1);
}

#[test]
fn a_changed_answer_misses_and_is_stored_beside_the_first() {
    let fixture = Fixture::new(SCENE);
    assert_eq!(fixture.prepare().1, BakeCacheStatus::Stored);

    let mut wide = fixture.context.clone();
    wide.screen_width *= 2.0;
    let (baked, status) = fixture.prepare_with(&fixture.cache(), &wide, &[]);
    assert_eq!(status, BakeCacheStatus::Stored);
    assert_eq!(fixture.entries().len(), 2);

    // Each session still finds its own bake.
    let (first, status) = fixture.prepare();
    assert_eq!(status, BakeCacheStatus::Hit);
    let (second, status) = fixture.prepare_with(&fixture.cache(), &wide, &[]);
    assert_eq!(status, BakeCacheStatus::Hit);
    assert_eq!(format!("{second:?}"), format!("{baked:?}"));
    assert_ne!(format!("{first:?}"), format!("{second:?}"));
}

#[test]
fn every_player_option_is_an_answer() {
    let fixture = Fixture::new(SCENE);
    assert_eq!(fixture.prepare().1, BakeCacheStatus::Stored);
    let mut mini = fixture.context.clone();
    mini.players[0].mini = 0.5;
    assert_eq!(
        fixture.prepare_with(&fixture.cache(), &mini, &[]).1,
        BakeCacheStatus::Stored,
    );
    let mut rate = fixture.context.clone();
    rate.song_music_rate = 1.5;
    assert_eq!(
        fixture.prepare_with(&fixture.cache(), &rate, &[]).1,
        BakeCacheStatus::Stored,
    );
}

#[test]
fn the_random_seed_is_not_an_answer_for_a_bake_that_never_draws() {
    let fixture = Fixture::new(SCENE);
    assert_eq!(fixture.prepare().1, BakeCacheStatus::Stored);
    let mut later = fixture.context.clone();
    later.random_seed = later.random_seed.wrapping_add(12345);
    assert_eq!(
        fixture.prepare_with(&fixture.cache(), &later, &[]).1,
        BakeCacheStatus::Hit,
    );
}

#[test]
fn a_supplied_host_input_is_an_answer() {
    let fixture = Fixture::new(SCENE);
    let cache = fixture.cache();
    let noteskin = |stamp| [("noteskin".to_owned(), stamp)];
    assert_eq!(
        fixture
            .prepare_with(&cache, &fixture.context, &noteskin(1))
            .1,
        BakeCacheStatus::Stored,
    );
    assert_eq!(
        fixture
            .prepare_with(&cache, &fixture.context, &noteskin(1))
            .1,
        BakeCacheStatus::Hit,
    );
    assert_eq!(
        fixture
            .prepare_with(&cache, &fixture.context, &noteskin(2))
            .1,
        BakeCacheStatus::Stored,
    );
}

#[test]
fn a_changed_song_file_misses_and_drops_the_stale_bake() {
    let fixture = Fixture::new(SCENE);
    let (original, status) = fixture.prepare();
    assert_eq!(status, BakeCacheStatus::Stored);

    fixture.rewrite("foreground.lua", &SCENE.replace("Panel", "Rewritten"));
    let (rebaked, status) = fixture.prepare();
    assert_eq!(status, BakeCacheStatus::Stored);
    assert!(format!("{rebaked:?}").contains("Rewritten"));
    assert!(!format!("{original:?}").contains("Rewritten"));
    assert_eq!(fixture.entries().len(), 1, "the stale bake was removed");
    assert_eq!(fixture.prepare().1, BakeCacheStatus::Hit);
}

#[test]
fn a_script_edit_that_keeps_size_and_timestamp_still_misses() {
    let fixture = Fixture::new(SCENE);
    assert_eq!(fixture.prepare().1, BakeCacheStatus::Stored);
    let path = fixture.song_dir().join("foreground.lua");
    let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
    std::fs::write(&path, SCENE.replace("Panel", "Panes")).unwrap();
    std::fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(modified)
        .unwrap();
    assert_eq!(
        std::fs::metadata(&path).unwrap().modified().unwrap(),
        modified
    );
    assert_eq!(std::fs::metadata(&path).unwrap().len(), SCENE.len() as u64);

    let (rebaked, status) = fixture.prepare();
    assert_eq!(status, BakeCacheStatus::Stored);
    assert!(format!("{rebaked:?}").contains("Panes"));
}

#[test]
fn a_file_the_scripts_never_named_still_misses() {
    // Scripts can discover files at run time, so the whole folder is identity.
    let fixture = Fixture::new(SCENE);
    assert_eq!(fixture.prepare().1, BakeCacheStatus::Stored);
    std::fs::write(fixture.song_dir().join("extra.png"), b"not an image").unwrap();
    assert_eq!(fixture.prepare().1, BakeCacheStatus::Stored);
}

#[test]
fn another_host_build_misses() {
    let fixture = Fixture::new(SCENE);
    assert_eq!(fixture.prepare().1, BakeCacheStatus::Stored);
    let other = BakeCache::new(fixture.cache_dir.path(), "another host");
    assert_eq!(
        fixture.prepare_with(&other, &fixture.context, &[]).1,
        BakeCacheStatus::Stored,
    );
}

#[test]
fn a_bake_that_draws_random_numbers_is_not_stored() {
    let fixture = Fixture::new(
        r#"
local jitter = math.random()
return Def.ActorFrame{ OnCommand=function(self) self:x(jitter) end }
"#,
    );
    for _ in 0..2 {
        let (baked, status) = fixture.prepare();
        assert!(baked.compile_succeeded());
        assert!(not_stored(&status).contains("random"), "{status:?}");
    }
    assert!(fixture.entries().is_empty());
}

#[test]
fn a_bake_that_reads_the_clock_is_not_stored() {
    for source in [
        "local now = os.time() return Def.ActorFrame{}",
        "local now = os.clock() return Def.ActorFrame{}",
        "local now = Second() return Def.ActorFrame{}",
        "local now = Minute() return Def.ActorFrame{}",
    ] {
        let fixture = Fixture::new(source);
        let (baked, status) = fixture.prepare();
        assert!(baked.compile_succeeded(), "{source}");
        not_stored(&status);
        assert!(fixture.entries().is_empty(), "{source}");
    }
}

#[test]
fn a_bake_that_reads_the_date_is_stored_with_that_date() {
    let fixture = Fixture::new("local year = Year() return Def.ActorFrame{}");
    assert_eq!(fixture.prepare().1, BakeCacheStatus::Stored);
    assert_eq!(fixture.prepare().1, BakeCacheStatus::Hit);
}

#[test]
fn a_script_loaded_from_outside_the_song_folder_is_an_answer() {
    let fixture = Fixture::new(
        r#"
local name = loadfile("../shared.lua")()
return Def.ActorFrame{ Def.Quad{ Name=name } }
"#,
    );
    let shared = fixture.root.path().join("pack").join("shared.lua");
    std::fs::write(&shared, "return 'First'").unwrap();
    let (baked, status) = fixture.prepare();
    assert!(baked.compile_succeeded());
    assert_eq!(status, BakeCacheStatus::Stored);
    assert!(format!("{baked:?}").contains("First"));
    assert_eq!(fixture.prepare().1, BakeCacheStatus::Hit);

    std::fs::write(&shared, "return 'Second, and longer'").unwrap();
    let (rebaked, status) = fixture.prepare();
    assert_eq!(status, BakeCacheStatus::Stored);
    assert!(format!("{rebaked:?}").contains("Second, and longer"));
}

#[test]
fn a_song_that_runs_lua_during_gameplay_is_not_stored() {
    let fixture = Fixture::new(
        r#"
return Def.ActorFrame{
    OnCommand=function(self)
        SCREENMAN:GetTopScreen():AddInputCallback(function() return false end)
    end
}
"#,
    );
    let (mut prepared, status) = fixture.prepare();
    assert!(
        not_stored(&status).contains("during gameplay"),
        "{status:?}"
    );
    assert!(prepared.take_live_scene().unwrap().is_some());
    assert!(fixture.entries().is_empty());
}

#[test]
fn runtime_mode_bypasses_the_cache() {
    let fixture = Fixture::new(SCENE);
    let mut live = fixture.context.clone();
    live.live = true;
    assert_eq!(
        fixture.prepare_with(&fixture.cache(), &live, &[]).1,
        BakeCacheStatus::Bypassed,
    );
    assert!(fixture.entries().is_empty());
}

#[test]
fn a_script_that_does_not_compile_is_not_stored() {
    let fixture = Fixture::new("return ???");
    let (prepared, status) = fixture.prepare();
    assert!(!prepared.compile_succeeded());
    not_stored(&status);
    assert!(fixture.entries().is_empty());
}

#[test]
fn a_bake_that_recovered_from_a_compile_error_is_not_stored() {
    // The independent-layer retry yields a scene, but not the one a clean
    // shared session would have produced.
    let fixture = Fixture::new(SCENE);
    let (prepared, status): (Prepared, _) = prepare_song_lua_cached(
        &fixture.cache(),
        &fixture.song,
        &fixture.context,
        &[],
        deadsync_assets::song_lua::compile_song_lua,
        |_, _, _| Err("shared scene failed".into()),
    );
    assert!(
        format!("{prepared:?}").contains("Panel"),
        "the retry baked the scene"
    );
    assert!(!prepared.compile_succeeded());
    assert!(
        not_stored(&status).contains("failed to compile"),
        "{status:?}"
    );
    assert!(fixture.entries().is_empty());
}

#[test]
fn a_damaged_entry_is_ignored_and_rebaked() {
    let fixture = Fixture::new(SCENE);
    let (baked, status) = fixture.prepare();
    assert_eq!(status, BakeCacheStatus::Stored);
    let entry = fixture.entries().pop().unwrap();
    let intact = std::fs::read(&entry).unwrap();

    let damage: [(&str, Box<dyn Fn(&mut Vec<u8>)>); 5] = [
        ("empty", Box::new(|bytes| bytes.clear())),
        ("truncated header", Box::new(|bytes| bytes.truncate(40))),
        (
            "truncated payload",
            Box::new(|bytes| bytes.truncate(bytes.len() - 8)),
        ),
        ("flipped header byte", Box::new(|bytes| bytes[30] ^= 0x55)),
        (
            "flipped payload byte",
            Box::new(|bytes| {
                let last = bytes.len() - 5;
                bytes[last] ^= 0x55;
            }),
        ),
    ];
    for (name, apply) in damage {
        let mut bytes = intact.clone();
        apply(&mut bytes);
        std::fs::write(&entry, bytes).unwrap();
        let (rebaked, status) = fixture.prepare();
        assert_eq!(status, BakeCacheStatus::Stored, "{name}");
        assert!(rebaked.compile_succeeded(), "{name}");
        assert_eq!(fixture.entries().len(), 1, "{name}");
        assert_eq!(fixture.prepare().1, BakeCacheStatus::Hit, "{name}");
    }
    assert!(baked.compile_succeeded());
}

#[test]
fn the_least_recently_used_session_is_evicted() {
    let fixture = Fixture::new(SCENE);
    let cache = fixture.cache().with_entries_per_song(2);
    let session = |width: f32| {
        let mut context = fixture.context.clone();
        context.screen_width = width;
        context
    };
    let (first, second, third) = (session(640.0), session(854.0), session(1280.0));
    let pause = || std::thread::sleep(std::time::Duration::from_millis(20));
    assert_eq!(
        fixture.prepare_with(&cache, &first, &[]).1,
        BakeCacheStatus::Stored
    );
    pause();
    assert_eq!(
        fixture.prepare_with(&cache, &second, &[]).1,
        BakeCacheStatus::Stored
    );
    pause();
    // Using the first session again makes the second the oldest.
    assert_eq!(
        fixture.prepare_with(&cache, &first, &[]).1,
        BakeCacheStatus::Hit
    );
    pause();
    assert_eq!(
        fixture.prepare_with(&cache, &third, &[]).1,
        BakeCacheStatus::Stored
    );
    assert_eq!(fixture.entries().len(), 2);
    assert_eq!(
        fixture.prepare_with(&cache, &first, &[]).1,
        BakeCacheStatus::Hit
    );
    assert_eq!(
        fixture.prepare_with(&cache, &third, &[]).1,
        BakeCacheStatus::Hit
    );
    assert_eq!(
        fixture.prepare_with(&cache, &second, &[]).1,
        BakeCacheStatus::Stored
    );
}

#[test]
fn storing_can_be_disabled() {
    let fixture = Fixture::new(SCENE);
    let cache = fixture.cache().with_entries_per_song(0);
    let status = fixture.prepare_with(&cache, &fixture.context, &[]).1;
    assert!(not_stored(&status).contains("disabled"));
    assert!(fixture.entries().is_empty());
}
