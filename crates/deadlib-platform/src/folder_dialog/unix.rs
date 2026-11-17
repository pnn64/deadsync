use super::DialogError;
use dbus::{
    arg::{PropMap, Variant, prop_cast},
    blocking::Connection,
    message::MatchRule,
};
use std::{
    ffi::OsString,
    os::unix::ffi::{OsStrExt, OsStringExt},
    path::PathBuf,
    process::{Command, Output, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

const PORTAL: &str = "org.freedesktop.portal.Desktop";
const REQUEST: &str = "org.freedesktop.portal.Request";
const DESKTOP_PATH: &str = "/org/freedesktop/portal/desktop";
static NEXT_REQUEST: AtomicU64 = AtomicU64::new(0);

pub(super) fn pick_folder(title: &str) -> Result<Option<PathBuf>, DialogError> {
    with_fallback(portal_folder(title), || zenity_folder(title))
}

fn with_fallback(
    portal: Result<Option<PathBuf>, DialogError>,
    fallback: impl FnOnce() -> Result<Option<PathBuf>, DialogError>,
) -> Result<Option<PathBuf>, DialogError> {
    match portal {
        Ok(selection) => Ok(selection),
        Err(portal_error) => {
            log::debug!("Folder portal failed; trying Zenity: {portal_error}");
            fallback().map_err(|error| {
                DialogError::new(format!(
                    "Folder portal failed: {portal_error}; Zenity failed: {error}"
                ))
            })
        }
    }
}

fn portal_folder(title: &str) -> Result<Option<PathBuf>, DialogError> {
    // Each dialog owns its connection and signal queue. Closing the connection
    // also releases its match rules and any outstanding portal request.
    let connection = Connection::new_session().map_err(DialogError::new)?;
    let token = format!(
        "deadlib_{}_{}",
        std::process::id(),
        NEXT_REQUEST.fetch_add(1, Ordering::Relaxed)
    );
    let mut options = PropMap::new();
    options.insert("handle_token".into(), Variant(Box::new(token)));
    options.insert("directory".into(), Variant(Box::new(true)));
    options.insert("multiple".into(), Variant(Box::new(false)));

    // Subscribe before OpenFile so a fast Response cannot be lost. Listen to
    // this interface first, then filter on the actual returned request path;
    // that also supports portals predating predictable handle_token paths.
    // https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Request.html
    let response_rule = MatchRule::new_signal(REQUEST, "Response").with_sender(PORTAL);
    connection
        .add_match_no_cb(&response_rule.match_str())
        .map_err(DialogError::new)?;
    let owner_rule = MatchRule::new_signal("org.freedesktop.DBus", "NameOwnerChanged")
        .with_sender("org.freedesktop.DBus");
    connection
        .add_match_no_cb(&format!("{},arg0='{PORTAL}'", owner_rule.match_str()))
        .map_err(DialogError::new)?;
    let proxy = connection.with_proxy(PORTAL, DESKTOP_PATH, Duration::from_secs(30));
    let (handle,): (dbus::Path<'static>,) = proxy
        .method_call(
            "org.freedesktop.portal.FileChooser",
            "OpenFile",
            ("", title, options),
        )
        .map_err(DialogError::new)?;
    let response_rule = response_rule.with_path(handle);
    let channel = connection.channel();
    loop {
        // Drain queued signals before blocking. No user-interaction timeout:
        // choosing a folder may take arbitrarily long. Disconnects and portal
        // exits do return errors instead of leaving the worker stuck forever.
        while let Some(message) = channel.pop_message() {
            if response_rule.matches(&message) {
                return parse_response(&message);
            }
            if owner_rule.matches(&message) {
                let (name, old, new): (String, String, String) =
                    message.read3().map_err(DialogError::new)?;
                if name == PORTAL && !old.is_empty() && old != new {
                    return Err(DialogError::new(
                        "Folder portal exited while choosing a directory",
                    ));
                }
            }
        }
        channel
            .read_write(None)
            .map_err(|()| DialogError::new("Disconnected from the folder portal"))?;
    }
}

fn parse_response(message: &dbus::Message) -> Result<Option<PathBuf>, DialogError> {
    let (code, results): (u32, PropMap) = message.read2().map_err(DialogError::new)?;
    match code {
        0 => {
            let uris = prop_cast::<Vec<String>>(&results, "uris")
                .ok_or_else(|| DialogError::new("Folder portal returned no URI array"))?;
            let [uri] = uris.as_slice() else {
                return Err(DialogError::new(
                    "Folder portal must return exactly one directory",
                ));
            };
            path_from_uri(uri).map(Some)
        }
        1 => Ok(None),
        code => Err(DialogError::new(format!(
            "Folder portal failed (response {code})"
        ))),
    }
}

fn path_from_uri(uri: &str) -> Result<PathBuf, DialogError> {
    let url = url::Url::parse(uri).map_err(DialogError::new)?;
    if url.query().is_some() || url.fragment().is_some() {
        return Err(DialogError::new(
            "Folder portal returned a URI with a query or fragment",
        ));
    }
    let path = url
        .to_file_path()
        .map_err(|()| DialogError::new("Folder portal returned a non-local file URI"))?;
    validate_path(path)
}

fn validate_path(path: PathBuf) -> Result<PathBuf, DialogError> {
    if !path.is_absolute() || path.as_os_str().as_bytes().contains(&0) {
        return Err(DialogError::new(
            "Folder dialog returned an invalid filesystem path",
        ));
    }
    Ok(path)
}

fn zenity_folder(title: &str) -> Result<Option<PathBuf>, DialogError> {
    let output = Command::new("zenity")
        .args(["--file-selection", "--directory", "--title", title])
        .stdin(Stdio::null())
        .output()
        .map_err(DialogError::new)?;
    parse_zenity_output(output)
}

fn parse_zenity_output(output: Output) -> Result<Option<PathBuf>, DialogError> {
    match output.status.code() {
        Some(0) => {
            let mut bytes = output.stdout;
            // Remove only the terminator Zenity adds. Spaces, non-UTF-8 bytes,
            // and even a newline within the selected filename are significant.
            if bytes.last() == Some(&b'\n') {
                bytes.pop();
            }
            validate_path(PathBuf::from(OsString::from_vec(bytes))).map(Some)
        }
        Some(1) => Ok(None),
        _ => Err(DialogError::new(format!(
            "Zenity exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::process::ExitStatusExt;

    fn response(code: u32, uris: Option<Vec<String>>) -> dbus::Message {
        let mut results = PropMap::new();
        if let Some(uris) = uris {
            results.insert("uris".into(), Variant(Box::new(uris)));
        }
        dbus::Message::new_signal("/request/test", REQUEST, "Response")
            .unwrap()
            .append2(code, results)
    }

    #[test]
    fn portal_decodes_one_local_path_without_losing_filename_bytes() {
        let reply = response(0, Some(vec!["file:///tmp/Player%20%FF%0A".into()]));
        let path = parse_response(&reply).unwrap().unwrap();
        assert_eq!(path.as_os_str().as_bytes(), b"/tmp/Player \xff\n");
        assert_eq!(
            path_from_uri("file://localhost/tmp/profile").unwrap(),
            PathBuf::from("/tmp/profile")
        );
    }

    #[test]
    fn portal_cancel_does_not_open_another_dialog() {
        let canceled = parse_response(&response(1, None));
        assert!(
            with_fallback(canceled, || panic!("cancellation must not fall back"))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn portal_failure_uses_fallback() {
        let failure = parse_response(&response(2, None));
        let selected = with_fallback(failure, || Ok(Some("/tmp/profile".into()))).unwrap();
        assert_eq!(selected, Some("/tmp/profile".into()));
        let error = with_fallback(Err(DialogError::new("portal missing")), || {
            Err(DialogError::new("zenity missing"))
        })
        .unwrap_err();
        assert!(error.to_string().contains("portal missing"));
        assert!(error.to_string().contains("zenity missing"));
    }

    #[test]
    fn portal_rejects_missing_multiple_remote_and_invalid_results() {
        for uris in [
            None,
            Some(vec![]),
            Some(vec!["file:///a".into(), "file:///b".into()]),
        ] {
            assert!(parse_response(&response(0, uris)).is_err());
        }
        for uri in [
            "https://example.com/folder",
            "file://remote/tmp/profile",
            "file:///tmp/%00",
            "file:///tmp/a?b",
            "file:///tmp/a#b",
        ] {
            assert!(path_from_uri(uri).is_err(), "{uri}");
        }
        let malformed = dbus::Message::new_signal("/request/test", REQUEST, "Response")
            .unwrap()
            .append1("not a status");
        assert!(parse_response(&malformed).is_err());
    }

    fn output(code: i32, stdout: &[u8]) -> Output {
        Output {
            status: std::process::ExitStatus::from_raw(code << 8),
            stdout: stdout.into(),
            stderr: vec![],
        }
    }

    #[test]
    fn zenity_preserves_path_bytes_and_only_strips_one_newline() {
        let path = parse_zenity_output(output(0, b"/tmp/ \xff\n\n"))
            .unwrap()
            .unwrap();
        assert_eq!(path.as_os_str().as_bytes(), b"/tmp/ \xff\n");
        assert!(parse_zenity_output(output(0, b"")).is_err());
        assert!(parse_zenity_output(output(0, b"relative\n")).is_err());
        assert!(parse_zenity_output(output(0, b"/tmp/\0\n")).is_err());
    }

    #[test]
    fn zenity_distinguishes_cancel_from_failure_even_with_stdout() {
        assert!(
            parse_zenity_output(output(1, b"/tmp/ignored\n"))
                .unwrap()
                .is_none()
        );
        assert!(parse_zenity_output(output(2, b"/tmp/ignored\n")).is_err());
        assert!(
            parse_zenity_output(Output {
                status: std::process::ExitStatus::from_raw(9),
                stdout: vec![],
                stderr: vec![]
            })
            .is_err()
        );
    }
}
