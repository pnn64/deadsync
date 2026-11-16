use deadsync_version::{current, current_static, current_tag, user_agent};
use std::process::Command;
use std::sync::Mutex;


struct Capture(Mutex<Vec<String>>);
static CAPTURE: Capture = Capture(Mutex::new(Vec::new()));
impl log::Log for Capture {
    fn enabled(&self, meta: &log::Metadata<'_>) -> bool {
        meta.level() <= log::Level::Warn
    }
    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) {
            self.0.lock().unwrap().push(record.args().to_string());
        }
    }
    fn flush(&self) {}
}


#[test]
#[ignore = "subprocess probe; invoked by override_cases_use_isolated_environments"]
fn probe_version_override() {
    log::set_logger(&CAPTURE).unwrap();
    log::set_max_level(log::LevelFilter::Warn);
    let actual = (current(), user_agent(), current_tag());
    let version = std::env::var("DEADSYNC_TEST_EXPECTED_VERSION").unwrap();
    assert_eq!(actual.0.to_string(), version);
    assert_eq!(actual.1, format!("DeadSync/{version}"));
    assert_eq!(actual.2, format!("v{version}"));
    let warnings = std::env::var("DEADSYNC_TEST_EXPECT_WARNINGS").unwrap().parse::<usize>().unwrap();
    assert_eq!(CAPTURE.0.lock().unwrap().len(), warnings);
}

fn probe(value: Option<&std::ffi::OsStr>, expected: &str, warnings: usize) {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "probe_version_override",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("DEADSYNC_TEST_EXPECTED_VERSION", expected)
        .env("DEADSYNC_TEST_EXPECT_WARNINGS", warnings.to_string());
    if let Some(value) = value {
        command.env("DEADSYNC_VERSION_OVERRIDE", value);
    } else {
        command.env_remove("DEADSYNC_VERSION_OVERRIDE");
    }
    let result = command.output().unwrap();
    assert!(String::from_utf8_lossy(&result.stdout).contains("1 passed; 0 failed"), "version probe did not execute: {:?}", result);
    assert!(
        result.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn override_cases_use_isolated_environments() {
    for (raw, version, warnings) in [
        (None, current_static(), 0),
        (Some(""), current_static(), 0),
        (Some(" \u{2003}\t"), current_static(), 0),
        (Some("not-a-version"), current_static(), 3),
        (Some("V1.2.3"), current_static(), 3),
        (Some("01.2.3"), current_static(), 3),
        (Some("0.0.1"), "0.0.1", 0),
        (Some(" v1.2.3\r\n"), "1.2.3", 0),
        (
            Some("1.2.3-alpha.1+build.007"),
            "1.2.3-alpha.1+build.007",
            0,
        ),
    ] {
        probe(raw.map(std::ffi::OsStr::new), version, warnings);
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStringExt;
        let invalid = std::ffi::OsString::from_wide(&[0xd800]);
        probe(Some(&invalid), current_static(), 0);
    }
}
