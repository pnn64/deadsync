//! Outside inputs a bake consumed while its scripts ran.
//!
//! The persistent bake cache may reuse a bake only for a session that would
//! give the scripts the same answers. Host bindings report here what they
//! handed to Lua. Reports are recorded only on the thread that is running a
//! bake under [`record`]; everywhere else they cost one thread-local read.
//!
//! This log is the cache's soundness boundary: an input that reaches a script
//! without passing through one of these reports, the compile context, or the
//! song folder is invisible to the cache.
use std::cell::RefCell;
use std::path::{Path, PathBuf};

/// What one preparation read from outside its compile context.
#[derive(Debug, Default)]
pub struct BakeReads {
    /// First input that can never match a later session, such as a draw from
    /// the wall-time random stream. Such a bake is not persisted.
    pub volatile: Option<&'static str>,
    /// Named host answers in first-read order. A repeated name keeps its
    /// first answer; a different later answer makes the bake volatile.
    pub answers: Vec<(String, u64)>,
    /// Text files the scripts loaded, wherever they live.
    pub files: Vec<PathBuf>,
}

impl BakeReads {
    fn merge(&mut self, inner: &Self) {
        self.volatile = self.volatile.or(inner.volatile);
        for (name, value) in &inner.answers {
            self.answer(name, *value);
        }
        for path in &inner.files {
            self.file(path);
        }
    }

    fn answer(&mut self, name: &str, value: u64) {
        match self.answers.iter().find(|(known, _)| known == name) {
            Some((_, known)) if *known == value => {}
            Some(_) => {
                self.volatile = self
                    .volatile
                    .or(Some("a host answer changed during the bake"));
            }
            None => self.answers.push((name.to_owned(), value)),
        }
    }

    fn file(&mut self, path: &Path) {
        if !self.files.iter().any(|known| known == path) {
            self.files.push(path.to_owned());
        }
    }
}

thread_local! {
    static ACTIVE: RefCell<Option<BakeReads>> = const { RefCell::new(None) };
}

/// Ends a recording and reinstates the enclosing one, including when the
/// bake unwinds. `None` once the recording has ended.
struct Recording {
    outer: Option<Option<BakeReads>>,
}

impl Recording {
    fn end(&mut self) -> BakeReads {
        let Some(mut outer) = self.outer.take() else {
            return BakeReads::default();
        };
        ACTIVE.with(|active| {
            let mut active = active.borrow_mut();
            let reads = active.take().unwrap_or_default();
            // Nesting never hides an input from the enclosing recording.
            if let Some(outer) = outer.as_mut() {
                outer.merge(&reads);
            }
            *active = outer;
            reads
        })
    }
}

impl Drop for Recording {
    fn drop(&mut self) {
        self.end();
    }
}

/// Run a bake on this thread and return what its scripts read.
pub fn record<T>(bake: impl FnOnce() -> T) -> (T, BakeReads) {
    let outer = ACTIVE.with(|active| active.borrow_mut().replace(BakeReads::default()));
    let mut recording = Recording { outer: Some(outer) };
    let result = bake();
    (result, recording.end())
}

fn with_active(report: impl FnOnce(&mut BakeReads)) {
    ACTIVE.with(|active| {
        // A report from inside another report cannot happen: none calls Lua.
        if let Ok(mut active) = active.try_borrow_mut()
            && let Some(reads) = active.as_mut()
        {
            report(reads);
        }
    });
}

/// The script received something no later session can reproduce.
pub(crate) fn volatile(reason: &'static str) {
    with_active(|reads| reads.volatile = reads.volatile.or(Some(reason)));
}

/// The script asked the host a question whose answer may differ next time.
pub(crate) fn answer(name: &str, value: u64) {
    with_active(|reads| reads.answer(name, value));
}

/// The script loaded this file's contents.
pub(crate) fn file(path: &Path) {
    with_active(|reads| reads.file(path));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_outside_a_bake_are_ignored() {
        volatile("ignored");
        answer("Year", 2026);
        let ((), reads) = record(|| {});
        assert!(reads.volatile.is_none());
        assert!(reads.answers.is_empty());
    }

    #[test]
    fn a_bake_collects_its_reads_once() {
        let ((), reads) = record(|| {
            answer("Year", 2026);
            answer("Year", 2026);
            file(Path::new("/pack/shared.lua"));
            file(Path::new("/pack/shared.lua"));
        });
        assert_eq!(reads.answers, [("Year".to_owned(), 2026)]);
        assert_eq!(reads.files, [PathBuf::from("/pack/shared.lua")]);
        assert!(reads.volatile.is_none());
    }

    #[test]
    fn a_changing_answer_is_volatile() {
        let ((), reads) = record(|| {
            answer("Hour", 23);
            answer("Hour", 0);
        });
        assert!(reads.volatile.is_some());
    }

    #[test]
    fn nested_recordings_report_to_both() {
        let (inner, outer) = record(|| {
            answer("Year", 2026);
            let ((), inner) = record(|| volatile("random"));
            inner
        });
        assert_eq!(inner.volatile, Some("random"));
        assert!(inner.answers.is_empty());
        assert_eq!(outer.volatile, Some("random"));
        assert_eq!(outer.answers, [("Year".to_owned(), 2026)]);
    }

    #[test]
    fn a_panicking_bake_leaves_no_recording_behind() {
        let unwound = std::panic::catch_unwind(|| record(|| panic!("bake failed")));
        assert!(unwound.is_err());
        answer("Year", 2026);
        let ((), reads) = record(|| {});
        assert!(reads.answers.is_empty());
    }
}
