use std::{path::Path, sync::OnceLock};

type ProbeLogScope = fn(&Path, &mut dyn FnMut());

static PROBE_LOG_SCOPE: OnceLock<ProbeLogScope> = OnceLock::new();

/// Install the host's source-file logging scope for codec probes.
///
/// The scope must invoke the supplied probe exactly once, synchronously on
/// the calling thread. Codec probes run directly when no scope is installed.
pub fn init_probe_log_scope(scope: ProbeLogScope) -> Result<(), &'static str> {
    PROBE_LOG_SCOPE
        .set(scope)
        .map_err(|_| "audio probe log scope already initialized")
}

pub(crate) fn with_probe_log_scope<T>(path: &Path, probe: impl FnOnce() -> T) -> T {
    let Some(scope) = PROBE_LOG_SCOPE.get() else {
        return probe();
    };

    let mut probe = Some(probe);
    let mut result = None;
    scope(path, &mut || {
        let probe = probe
            .take()
            .expect("audio log scope invoked the probe twice");
        result = Some(probe());
    });
    result.expect("audio log scope did not invoke the probe")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::RefCell, path::PathBuf};

    thread_local! {
        static SOURCE: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
    }

    #[test]
    fn probe_log_scope_preserves_source_and_result() {
        init_probe_log_scope(|path, probe| {
            let previous = SOURCE.replace(Some(path.to_owned()));
            probe();
            SOURCE.set(previous);
        })
        .unwrap();

        let outer = Path::new("outer.ogg");
        let inner = Path::new("inner.mp3");
        let result = with_probe_log_scope(outer, || {
            SOURCE.with_borrow(|source| assert_eq!(source.as_deref(), Some(outer)));
            let result = with_probe_log_scope(inner, || {
                SOURCE.with_borrow(|source| assert_eq!(source.as_deref(), Some(inner)));
                Err::<(), _>("bad stream")
            });
            SOURCE.with_borrow(|source| assert_eq!(source.as_deref(), Some(outer)));
            result
        });
        assert_eq!(result, Err("bad stream"));
        SOURCE.with_borrow(|source| assert!(source.is_none()));
    }
}
