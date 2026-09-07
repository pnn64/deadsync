#![cfg(any(
    windows,
    target_os = "linux",
    target_os = "freebsd",
    target_os = "macos"
))]

use deadsync_updater::{UpdaterError, apply_journal, cli, download};
use std::fs;
#[cfg(windows)]
use std::io::Write;

#[cfg(windows)]
fn archive() -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    zip.start_file(
        "release/deadsync.exe",
        zip::write::SimpleFileOptions::default(),
    )
    .expect("start executable entry");
    zip.write_all(b"new executable").expect("write executable");
    zip.finish().expect("finish ZIP").into_inner()
}

#[cfg(unix)]
fn archive() -> Vec<u8> {
    let mut tar = tar::Builder::new(flate2::write::GzEncoder::new(
        Vec::new(),
        flate2::Compression::fast(),
    ));
    let mut header = tar::Header::new_gnu();
    header.set_size(b"new executable".len() as u64);
    header.set_mode(0o755);
    header.set_cksum();
    tar.append_data(&mut header, "release/deadsync", &b"new executable"[..])
        .expect("write executable");
    tar.into_inner()
        .expect("finish tar")
        .finish()
        .expect("finish gzip")
}

#[test]
fn verified_apply_preserves_user_data_and_leaves_cleanup_for_next_launch() {
    let root = std::env::temp_dir().join(format!("deadsync-cli-apply-{}", std::process::id()));
    fs::create_dir(&root).expect("create isolated install");
    let executable = root.join(if cfg!(windows) {
        "deadsync.exe"
    } else {
        "deadsync"
    });
    fs::write(&executable, b"old executable").expect("write old executable");
    fs::write(root.join("deadsync.ini"), b"user settings").expect("write settings");
    let archive_path = root.join("archive");
    let bytes = archive();
    let digest = download::sha256_of(&bytes);
    fs::write(&archive_path, &bytes).expect("write archive");

    // Corruption must be rejected before touching installed files or journaling.
    fs::write(&archive_path, b"corrupt archive").expect("corrupt archive");
    let error = cli::apply_archive(&archive_path, &digest, &root).expect_err("reject corruption");
    assert!(matches!(error, UpdaterError::ChecksumMismatch { .. }));
    assert_eq!(
        fs::read(&executable).expect("read old executable"),
        b"old executable"
    );
    assert!(!apply_journal::journal_path(&root).exists());

    fs::write(&archive_path, &bytes).expect("restore archive");
    cli::apply_archive(&archive_path, &digest, &root).expect("install without relaunching");
    assert_eq!(
        fs::read(&executable).expect("read executable"),
        b"new executable"
    );
    assert_eq!(
        fs::read(root.join("deadsync.ini")).expect("read settings"),
        b"user settings"
    );
    assert!(apply_journal::journal_path(&root).exists());
    let report = apply_journal::recover(&root);
    assert!(report.journal_removed);
    assert_eq!(report.backups_removed, 1);
    assert_eq!(
        fs::read(&executable).expect("read executable"),
        b"new executable"
    );
    fs::remove_dir_all(&root).expect("remove isolated install");
}

#[test]
fn unresolved_journal_blocks_update_before_network_or_download() {
    let root = std::env::temp_dir().join(format!("deadsync-cli-journal-{}", std::process::id()));
    fs::create_dir(&root).expect("create isolated install");
    let journal = apply_journal::journal_path(&root);
    fs::write(&journal, b"corrupt journal").expect("write unresolved journal");
    let error = cli::run_update(&root, true).expect_err("preserve unresolved update");
    assert!(error.to_string().contains("previous update"));
    assert_eq!(
        fs::read(&journal).expect("read journal"),
        b"corrupt journal"
    );
    fs::remove_dir_all(&root).expect("remove isolated install");
}
