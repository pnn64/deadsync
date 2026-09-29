//! Per-song semantic parity against `itgmania-harness-rs` fixtures.
//!
//! Every song has its own ignored test that compiles the song with DeadSync,
//! runs every semantic comparator plus the runtime modifier audit against its
//! ITGmania fixture, and reports `passed/total ok` per comparator. Two sets
//! exist: `corpora::allowed::` for the curated `allowed/` corpus and `corpora::lua_songs::` for
//! selected songs of the `lua-songs/` corpus. Run one song or a whole set with:
//!
//! ```text
//! cargo test --test song_lua_itgmania_semantic_parity corpora::allowed::flip69 -- --ignored
//! cargo test --test song_lua_itgmania_semantic_parity corpora::lua_songs:: -- --ignored
//! ```
//!
//! Regenerate a set's fixtures from `itgmania-harness-rs` after changing its
//! songs or the oracle; the `lua-songs` set must be generated from a corpus
//! holding only its songs:
//!
//! ```text
//! cargo run -- song-lua-semantic-baseline ../allowed \
//!   --out ../deadsync/tests/fixtures/itgmania-song-lua-allowed
//! ```

use super::*;
use std::io::Write as _;

struct Corpus {
    fixtures: &'static str,
    songs: &'static str,
    /// The corpus holds exactly the tested songs; otherwise it only has to
    /// contain them.
    exclusive: bool,
    tests: &'static [&'static str],
}

macro_rules! corpus_tests {
    ($corpus:ident, $fixtures:literal, $songs:literal, $exclusive:literal,
     $($name:ident => $simfile:literal,)+) => {
        const $corpus: Corpus = Corpus {
            fixtures: $fixtures,
            songs: $songs,
            exclusive: $exclusive,
            tests: &[$($simfile),+],
        };
        $(
            #[test]
            #[ignore = "compiles one song and reports every semantic check"]
            fn $name() {
                assert_song_parity(&$corpus, $simfile);
            }
        )+

        #[test]
        fn fixtures_cover_corpus_and_tests() {
            assert_corpus_coverage(&$corpus);
        }
    };
}

mod allowed {
    use super::*;

    corpus_tests! {
        CORPUS, "tests/fixtures/itgmania-song-lua-allowed", "allowed", true,
        gemini_in_clockland => "Gemini in Clockland/Gemini in Clockland.ssc",
        spooky => "[07] Spooky (SM) [Scrypts]/Spooky-chart.ssc",
        riddle => "[10] Riddle (DX) [Brother Mojo remixes A. Astral]/Riddle.ssc",
        flip69 => "[10] flip69 (DX) [Telperion]/flip69.ssc",
        media_offline => "[10] media offline (SM) [Snap]/media offline.ssc",
        cosmic_railroad => "[11] CO5M1C R4ILR0AD (SH) [TaroNuke vs. Scrypts]/CO5M1C R4ILR0AD-chart.ssc",
        kenpo_saito => "[11] KENPO SAITO (DX) [Scrypts]/KENPO SAITO-chart.ssc",
        godspeed => "[12] Godspeed (SX) [G. Rosewood]/godspeed.ssc",
    }
}

mod lua_songs {
    use super::*;

    corpus_tests! {
        CORPUS, "tests/fixtures/itgmania-song-lua-selected", "lua-songs", false,
        botanic_panic => "Cuphead [TaroNuke]/botanic.sm",
        seventh_gear => "7th Gear/7th Gear.ssc",
        who_the_hell_is_edgar => "[09] Who the Hell Is Edgar (SX) [Telperion]/Who the Hell Is Edgar.ssc",
        brogamer => "BroGamer/BroGamer.ssc",
        step_your_game_up => "Step Your Game Up (Director's Cut)/stepyourgameup.ssc",
        slamurai => "Slamurai/Slamurai.ssc",
        bank_account => "Bank Account/Bank Account.ssc",
    }
}

fn fixture_root(corpus: &Corpus) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(corpus.fixtures)
}

fn corpus_root(corpus: &Corpus) -> PathBuf {
    workspace_root().join(corpus.songs)
}

fn read_manifest(corpus: &Corpus) -> SemanticManifest {
    let path = fixture_root(corpus).join(SEMANTIC_MANIFEST);
    serde_json::from_slice(
        &fs::read(&path)
            .unwrap_or_else(|error| panic!("missing manifest {}: {error}", path.display())),
    )
    .unwrap_or_else(|error| panic!("invalid manifest {}: {error}", path.display()))
}

fn collect_simfiles(directory: &Path, root: &Path, out: &mut Vec<String>) {
    for entry in fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("failed to scan {}: {error}", directory.display()))
    {
        let path = entry.expect("failed to read corpus entry").path();
        if path.is_dir() {
            collect_simfiles(&path, root, out);
        } else if path.extension().is_some_and(|extension| {
            ["sm", "sma", "ssc", "ats"]
                .iter()
                .any(|simfile| extension.eq_ignore_ascii_case(simfile))
        }) {
            let relative = path
                .strip_prefix(root)
                .expect("corpus entries stay in root");
            out.push(relative.to_string_lossy().replace('\\', "/"));
        }
    }
}

fn assert_song_parity(corpus: &Corpus, simfile: &str) {
    crate::paths::init();
    let manifest = read_manifest(corpus);
    let entry = manifest
        .simfiles
        .iter()
        .find(|entry| entry.simfile == simfile)
        .unwrap_or_else(|| panic!("{simfile} has no fixture in {}", corpus.fixtures));
    assert_eq!(entry.status, "ok", "incomplete fixture: {simfile}");
    let trace = read_trace_file(&fixture_root(corpus).join(&entry.fixture));
    assert_eq!(trace.oracle, "itgmania_song_lua_headless_semantic_trace");
    let (compiled, primary_index, context) =
        compile_trace_song_at(&trace, &corpus_root(corpus).join(simfile));
    let mut parity = compare_semantics(&trace, &compiled, primary_index, &context);
    runtime_modifiers::compare_runtime_modifiers(&trace, &compiled, &context, &mut parity);
    // Written past libtest's output capture so passing songs report their
    // tally as well, not only the failing ones.
    writeln!(std::io::stderr().lock(), "\n{}", parity.summary(simfile))
        .expect("stderr accepts the parity summary");
    parity.assert_complete(simfile);
}

fn assert_corpus_coverage(corpus: &Corpus) {
    let manifest = read_manifest(corpus);
    assert_eq!(manifest.itgmania.execution, "embedded_bundled_lua");
    assert!(!manifest.itgmania.launches_executable);

    let mut fixtures = manifest
        .simfiles
        .iter()
        .map(|entry| entry.simfile.clone())
        .collect::<Vec<_>>();
    fixtures.sort();
    let mut tests = corpus.tests.to_vec();
    tests.sort_unstable();
    assert_eq!(
        fixtures, tests,
        "every {} fixture needs its own test",
        corpus.fixtures
    );
    let root = corpus_root(corpus);
    if corpus.exclusive {
        let mut songs = Vec::new();
        collect_simfiles(&root, &root, &mut songs);
        songs.sort();
        assert_eq!(
            fixtures, songs,
            "regenerate {} after changing the {} corpus",
            corpus.fixtures, corpus.songs
        );
    }
    for entry in &manifest.simfiles {
        assert!(
            root.join(&entry.simfile).is_file(),
            "missing song {}",
            entry.simfile
        );
        assert_eq!(entry.status, "ok", "incomplete fixture: {}", entry.simfile);
        assert_eq!(entry.runtime_errors, 0, "runtime errors: {}", entry.simfile);
        assert_eq!(entry.dropped_events, 0, "dropped events: {}", entry.simfile);
        assert!(
            fixture_root(corpus).join(&entry.fixture).is_file(),
            "missing fixture {}",
            entry.fixture.display()
        );
    }
}
