#[path = "support/song_lua_source_identity.rs"]
mod source_identity;

use serde_json::json;
use sha2::{Digest, Sha256};
use std::{fs, path::PathBuf};

fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

struct Capture {
    _dir: tempfile::TempDir,
    trace: PathBuf,
    simfile: PathBuf,
    script: PathBuf,
    texture: PathBuf,
    archive: PathBuf,
}

fn capture() -> Capture {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let song = root.join("relocated-song");
    fs::create_dir(&song).unwrap();
    let files: [(&str, &[u8]); 3] = [
        ("chart.sm", b"#OFFSET:0.000;\n"),
        ("scene.lua", b"return Def.ActorFrame {}\n"),
        ("image.png", b"recorded texture bytes"),
    ];
    let members: Vec<_> = files
        .iter()
        .map(|(name, bytes)| {
            fs::write(song.join(name), bytes).unwrap();
            json!({"path": format!("song/{name}"), "bytes": bytes.len(), "sha256": hash(bytes)})
        })
        .collect();
    let source = serde_json::to_vec(&json!({
        "chart": {"simfile": "song/chart.sm"},
        "files": members,
    }))
    .unwrap();
    let mut tar = tar::Builder::new(Vec::new());
    let mut header = tar::Header::new_gnu();
    header.set_size(source.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    tar.append_data(&mut header, "manifest.json", source.as_slice())
        .unwrap();
    for (name, bytes) in files {
        let mut header = tar::Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        tar.append_data(&mut header, format!("song/{name}"), bytes)
            .unwrap();
    }
    let compressed = zstd::stream::encode_all(tar.into_inner().unwrap().as_slice(), 1).unwrap();
    let archive_name = format!("{}.tar.zst", hash(&compressed));
    let archive = root.join(&archive_name);
    fs::write(&archive, compressed).unwrap();
    let trace = root.join("selected.trace.json.zst");
    fs::write(&trace, b"immutable native observations").unwrap();
    fs::write(
        root.join("manifest.json"),
        serde_json::to_vec(&json!({
            "source_archive": archive_name,
            "captures": [{
                "trace": "selected.trace.json.zst",
                "compressed_sha256": hash(b"immutable native observations"),
            }],
        }))
        .unwrap(),
    )
    .unwrap();
    Capture {
        trace,
        simfile: song.join("chart.sm"),
        script: song.join("scene.lua"),
        texture: song.join("image.png"),
        archive,
        _dir: dir,
    }
}

#[test]
fn accepts_relocated_sources_and_records_the_verified_inputs() {
    let c = capture();
    let report = source_identity::verify(&c.trace, &c.simfile).unwrap();
    assert_eq!(report["status"], "verified");
    assert_eq!(report["verified_song_files"], 3);
    assert_eq!(report["simfile_sha256"], hash(b"#OFFSET:0.000;\n"));
    assert_eq!(
        report["trace_sha256"],
        hash(b"immutable native observations")
    );
}

#[test]
fn rejects_resynced_simfile_before_reporting_engine_parity() {
    let c = capture();
    fs::write(&c.simfile, b"#OFFSET:0.036;\n").unwrap();
    let error = source_identity::verify(&c.trace, &c.simfile).unwrap_err();
    assert!(error.contains("chart.sm"), "{error}");
    assert!(
        error.contains("input mismatch, not an engine parity result"),
        "{error}"
    );
}

#[test]
fn rejects_changed_lua_media_and_missing_dependencies() {
    for kind in ["script", "texture", "missing"] {
        let c = capture();
        let path = if kind == "texture" {
            &c.texture
        } else {
            &c.script
        };
        if kind == "missing" {
            fs::remove_file(path).unwrap();
        } else {
            fs::write(path, b"changed authored input").unwrap();
        }
        let error = source_identity::verify(&c.trace, &c.simfile).unwrap_err();
        assert!(
            error.contains(path.file_name().unwrap().to_str().unwrap()),
            "{error}"
        );
    }
}

#[test]
fn rejects_modified_trace_or_source_archive() {
    for trace in [true, false] {
        let c = capture();
        fs::write(if trace { &c.trace } else { &c.archive }, b"changed").unwrap();
        let error = source_identity::verify(&c.trace, &c.simfile).unwrap_err();
        assert!(error.contains("identity mismatch"), "{error}");
    }
}

#[test]
fn does_not_silently_skip_an_unlisted_capture() {
    let c = capture();
    let renamed = c.trace.with_file_name("unlisted.trace.json.zst");
    fs::rename(&c.trace, &renamed).unwrap();
    assert!(
        source_identity::verify(&renamed, &c.simfile)
            .unwrap_err()
            .contains("not listed")
    );
}

#[test]
fn distinguishes_legacy_traces_without_source_provenance() {
    let c = capture();
    fs::remove_file(c.trace.with_file_name("manifest.json")).unwrap();
    let report = source_identity::verify(&c.trace, &c.simfile).unwrap();
    assert_eq!(report["status"], "not-recorded");
    assert_eq!(report["simfile_sha256"], hash(b"#OFFSET:0.000;\n"));
}
