//! Fixed regression expectations, consumed without loading another Lua runtime.
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    fmt::Debug,
    fs,
    path::PathBuf,
    sync::{Arc, Mutex, OnceLock},
};

type Observations = HashMap<(String, usize), String>;
#[path = "snapshot_paths.rs"]
mod snapshot_paths;
type FixtureCache = HashMap<String, Arc<Observations>>;
static FIXTURES: OnceLock<Mutex<FixtureCache>> = OnceLock::new();

thread_local! {
    static COUNTERS: RefCell<HashMap<String, usize>> = RefCell::new(HashMap::new());
    static PATHS: RefCell<Vec<(String, String)>> = const { RefCell::new(Vec::new()) };
    static NEXT_PATH: Cell<usize> = const { Cell::new(0) };
}

fn index(id: &str) -> (String, usize) {
    let thread = std::thread::current();
    let test = thread.name().unwrap_or("unnamed");
    let index = COUNTERS.with(|counters| {
        let mut counters = counters.borrow_mut();
        let next = counters.entry(id.into()).or_default();
        let index = *next;
        *next += 1;
        index
    });
    (test.into(), index)
}

pub(crate) fn register_path(path: &std::path::Path) {
    PATHS.with(|paths| {
        let mut paths = paths.borrow_mut();
        let index = NEXT_PATH.with(|next| {
            let index = next.get();
            next.set(index + 1);
            index
        });
        let text = path.to_string_lossy();
        for text in [text.to_string(), text.replace('\\', "/")] {
            let replacement = format!("$FIXTURE{index}");
            // Lua uses 60-byte debug IDs and 80-byte lexer IDs, each with
            // eight reserved bytes. These are aliases of the same host path.
            for retained in [52, 72] {
                if text.len() > retained {
                    let suffix = String::from_utf8_lossy(&text.as_bytes()[text.len() - retained..]);
                    paths.push((format!("...{suffix}"), replacement.clone()));
                }
            }
            paths.push((text, replacement));
        }
    });
}

fn normalize(mut text: String) -> String {
    PATHS.with(|paths| {
        for (path, replacement) in paths.borrow().iter() {
            // Debug output can contain a string or a Vec<u8>. Normalize the
            // complete registered path in either representation, preserving all
            // other bytes, numeric observations and diagnostic details.
            let quoted = format!("{path:?}");
            text = text.replace(&quoted[1..quoted.len() - 1], replacement);
            let bytes = format!("{:?}", path.as_bytes());
            text = text.replace(&bytes[1..bytes.len() - 1], replacement);
        }
    });
    text
}

/// Canonicalize fixture paths before formatting serialized Lua observations.
pub(crate) fn snapshot_bytes(bytes: &[u8]) -> Vec<u8> {
    PATHS.with(|paths| snapshot_paths::normalize(bytes, &paths.borrow()))
}

fn observation(id: &str) -> (Arc<Observations>, (String, usize)) {
    let position = index(id);
    let observations = {
        let mut cache = FIXTURES
            .get_or_init(|| Mutex::new(HashMap::new()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        Arc::clone(cache.entry(id.into()).or_insert_with(|| {
            let hash = id.bytes().fold(0xcbf29ce484222325u64, |hash, byte| {
                (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
            });
            let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/regression-expectations")
                .join(env!("CARGO_PKG_NAME"))
                .join(format!("{hash:016x}.snap"));
            let fixture = fs::read_to_string(&path).unwrap_or_else(|error| {
                panic!(
                    "missing regression expectations {id} at {}: {error}; run \
                     git submodule update --init tests/fixtures and \
                     python scripts/prepare_test_fixtures.py",
                    path.display()
                )
            });
            let mut lines = fixture.lines();
            assert_eq!(
                lines.next(),
                Some(id),
                "fixture identity: {}",
                path.display()
            );
            let mut observations = HashMap::new();
            let mut test = None;
            let mut index = 0;
            for line in lines {
                if let Some(name) = line.strip_prefix("test: ") {
                    test = Some(name.to_owned());
                    index = 0;
                    continue;
                }
                let test = test.as_ref().expect("fixture test name").clone();
                assert!(
                    observations
                        .insert((test, index), line.to_owned())
                        .is_none(),
                    "duplicate fixture case {id}"
                );
                index += 1;
            }
            Arc::new(observations)
        }))
    };
    assert!(
        observations.contains_key(&position),
        "missing regression expectation {id}: {} case {}",
        position.0,
        position.1
    );
    (observations, position)
}

pub(crate) fn assert_debug(id: &str, actual: &impl Debug) {
    let (observations, position) = observation(id);
    let expected = observations.get(&position).unwrap_or_else(|| {
        panic!(
            "missing regression expectation {id}: {} case {}",
            position.0, position.1
        )
    });
    let actual = normalize(format!("{actual:?}"));
    if actual != *expected {
        let offset = actual
            .bytes()
            .zip(expected.bytes())
            .position(|(a, b)| a != b)
            .unwrap_or(actual.len().min(expected.len()));
        let excerpt = |text: &str| {
            let mut start = offset.saturating_sub(80).min(text.len());
            let mut end = offset.saturating_add(160).min(text.len());
            while !text.is_char_boundary(start) {
                start -= 1;
            }
            while !text.is_char_boundary(end) {
                end += 1;
            }
            text[start..end].to_owned()
        };
        panic!(
            "regression expectation {id}: {} case {}, byte {offset}\nactual: {:?}\nexpected: {:?}",
            position.0,
            position.1,
            excerpt(&actual),
            excerpt(expected)
        );
    }
}

fn source_identity(source: impl AsRef<[u8]>) -> (usize, u64) {
    let source = source.as_ref();
    let hash = source.iter().fold(0xcbf29ce484222325u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    });
    (source.len(), hash)
}

pub(crate) fn assert_source_debug(id: &str, source: impl AsRef<[u8]>, actual: &impl Debug) {
    let (length, hash) = source_identity(source);
    assert_debug(id, &(length, hash, actual));
}

fn source_value(id: &str, source: impl AsRef<[u8]>) -> String {
    let (length, hash) = source_identity(source);
    let (observations, position) = observation(id);
    let value = &observations[&position];
    let prefix = format!("({length}, {hash}, ");
    value
        .strip_prefix(&prefix)
        .and_then(|value| value.strip_suffix(')'))
        .unwrap_or_else(|| {
            panic!(
                "fixture input changed for {id}: {} case {}; review the source and reference data",
                position.0, position.1
            )
        })
        .to_owned()
}

fn bytes(value: &str) -> Vec<u8> {
    let values = value
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .expect("byte fixture must be an array");
    if values.trim().is_empty() {
        return Vec::new();
    }
    values
        .split(',')
        .map(|value| value.trim().parse().expect("fixture byte in 0..=255"))
        .collect()
}

/// Fixed reference bytes, tied to the input selector's length and digest.
pub(crate) fn source_bytes(id: &str, source: impl AsRef<[u8]>) -> Vec<u8> {
    bytes(&source_value(id, source))
}

/// Original Lua 5.1 wire bytes, tied to the input program's length and digest.
pub(crate) fn native_chunk(id: &str, source: impl AsRef<[u8]>) -> Vec<u8> {
    source_bytes(id, source)
}

/// Fixture-controlled ordering, without retaining process-specific addresses.
pub(crate) fn source_indices(id: &str, source: impl AsRef<[u8]>) -> Vec<usize> {
    let value = source_value(id, source);
    let values = value
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .expect("index fixture must be an array");
    if values.trim().is_empty() {
        return Vec::new();
    }
    values
        .split(',')
        .map(|value| value.trim().parse().expect("fixture index must fit usize"))
        .collect()
}

/// Preserve the reference compiler's corpus selection, independently of this VM.
pub(crate) fn rejected_source(id: &str, source: impl AsRef<[u8]>) -> bool {
    source_bool(id, source)
}

/// Read a fixed Boolean observation bound to the authored input source.
pub(crate) fn source_bool(id: &str, source: impl AsRef<[u8]>) -> bool {
    source_value(id, source)
        .parse()
        .expect("source fixture must be Boolean")
}

/// Preserve the reference writer's block boundaries, including empty blocks.
pub(crate) fn native_writer_blocks(id: &str, source: impl AsRef<[u8]>) -> Vec<Vec<u8>> {
    let value = source_value(id, source);
    let mut remaining = value
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .expect("writer fixture must be an array")
        .trim();
    let mut blocks = Vec::new();
    while !remaining.is_empty() {
        let end = remaining.find(']').expect("writer block terminator");
        blocks.push(bytes(&remaining[..=end]));
        remaining = remaining[end + 1..].trim_start();
        if !remaining.is_empty() {
            remaining = remaining
                .strip_prefix(',')
                .expect("writer block separator")
                .trim_start();
        }
    }
    blocks
}
