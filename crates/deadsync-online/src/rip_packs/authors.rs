//! The author picker's list: the charters a rip can be asked for by name,
//! best first.
//!
//! The ITGmania Content Browser's site builds it on a schedule
//! (`scripts/booster-authors.mts`) and publishes it as `authors.json`:
//!
//! * **Who released a pack.** Whoever is credited on at least a third of its
//!   songs, by the site's whole credit line -- which is what a rip will ask
//!   SMO's exact-author search for.
//! * **Which packs.** Those uploaded to stepmaniaonline.net in the last two
//!   years.
//! * **The order.** How much of what they released is on arrowcloud.dance's
//!   most-played list, and only then how much they released. The best 300.
//!
//! Building it means reading a page for every pack of those two years, some
//! 2,500, which is a job for the site once a week and not for every player's
//! machine. So the finished list is read instead ([`lists::fetch_file`]),
//! kept on disk, and asked for again -- conditionally, so an unchanged list
//! costs a 304 -- once the copy is a day old. The site not answering, the
//! copy on disk is offered however old; with no copy either, the picker says
//! so and can be asked to try again.

use super::lists;
use super::{AuthorsPhase, RipAuthor};
use serde::Deserialize;
use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;

/// How old the copy on disk may get before the site is asked again. The site
/// rebuilds the list weekly; asking daily picks a rebuild up within a day,
/// and in between costs a 304.
const MAX_AGE: Duration = Duration::from_secs(24 * 60 * 60);
/// What the picker says with no list to offer. Whatever went wrong -- the
/// site down, the machine offline, a file that would not read -- the player
/// can do the same thing about it: try again later.
const UNREACHABLE: &str = "could not reach itgcontent.net";

/// `authors.json`. Only what the picker shows is read; the rest (`packs`,
/// `score`, `last`, and the header's counts) is the site's working.
#[derive(Deserialize)]
struct AuthorsFile {
    authors: Vec<Entry>,
}

#[derive(Deserialize)]
struct Entry {
    name: String,
    #[serde(default)]
    popular: u32,
}

/// Runs on the thread `runtime_ensure_authors` started, and ends by reporting
/// through [`super::authors_publish`] with Ready or Error -- unless the build
/// went stale, when it stops without a word.
pub(super) fn run(generation: u64, cache_dir: PathBuf) {
    let wanted = || super::authors_wanted(generation);
    // A copy past its age is offered before the site is asked: on a network
    // that hangs rather than fails, the picker would otherwise wait out the
    // whole timeout for the list it already has.
    let offered = lists::stale_copy(lists::AUTHORS_FILE, MAX_AGE, &cache_dir)
        .and_then(|bytes| parse(&bytes).ok());
    let had_copy = offered.is_some();
    if let Some(authors) = offered {
        super::authors_publish(generation, AuthorsPhase::Ready, Some(authors), None);
    }
    let fetched = lists::fetch_file(lists::AUTHORS_FILE, MAX_AGE, &cache_dir, &wanted);
    if !wanted() {
        return;
    }
    // With a copy offered, a failure changes nothing: the copy stands.
    if had_copy && fetched.is_err() {
        return;
    }
    let (phase, authors, message) = outcome(fetched);
    super::authors_publish(generation, phase, authors, message);
}

/// What a fetch comes to for the picker.
fn outcome(
    fetched: Result<Vec<u8>, String>,
) -> (AuthorsPhase, Option<Vec<RipAuthor>>, Option<String>) {
    match fetched.and_then(|bytes| parse(&bytes)) {
        Ok(authors) => (AuthorsPhase::Ready, Some(authors), None),
        Err(error) => {
            log::warn!("Rip Packs authors: no author list: {error}");
            (AuthorsPhase::Error, None, Some(UNREACHABLE.to_owned()))
        }
    }
}

/// The list as the file ranks it. Names are trimmed, and a blank one or one
/// already listed (case aside, as a rip matches credits) is passed over. A
/// file that names nobody is no list: [`lists`] turns it away rather than let
/// it replace one that did.
pub(super) fn parse(bytes: &[u8]) -> Result<Vec<RipAuthor>, String> {
    let file: AuthorsFile = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
    let mut seen = HashSet::with_capacity(file.authors.len());
    let authors: Vec<RipAuthor> = file
        .authors
        .into_iter()
        .filter_map(|entry| {
            let name = entry.name.trim();
            (!name.is_empty() && seen.insert(name.to_lowercase())).then(|| RipAuthor {
                name: name.to_owned(),
                popular: entry.popular,
            })
        })
        .collect();
    if authors.is_empty() {
        return Err("it names nobody".to_owned());
    }
    Ok(authors)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The head and foot of the live file (2026-10-17), as the site writes
    /// it.
    const LIVE: &str = r#"{
 "built": "2026-10-17",
 "since": "2024-10-17",
 "packs": 2545,
 "unread": 0,
 "authors": [
  {
   "name": "Zaia",
   "packs": 30,
   "popular": 14,
   "score": 500.33,
   "last": "2026-10-03"
  },
  {
   "name": "Exschwasion",
   "packs": 8,
   "popular": 8,
   "score": 378.25,
   "last": "2026-09-20"
  },
  {
   "name": "G. Rosewood",
   "packs": 8,
   "popular": 8,
   "score": 359.42,
   "last": "2026-10-02"
  },
  {
   "name": "aperson",
   "packs": 1,
   "popular": 0,
   "score": 1,
   "last": "2026-05-28"
  },
  {
   "name": "April YXL",
   "packs": 1,
   "popular": 0,
   "score": 1,
   "last": "2026-01-17"
  }
 ]
}
"#;

    fn author(name: &str, popular: u32) -> RipAuthor {
        RipAuthor {
            name: name.to_owned(),
            popular,
        }
    }

    #[test]
    fn the_live_file_reads_in_its_own_order() {
        assert_eq!(
            parse(LIVE.as_bytes()).expect("an author list"),
            vec![
                author("Zaia", 14),
                author("Exschwasion", 8),
                author("G. Rosewood", 8),
                author("aperson", 0),
                author("April YXL", 0),
            ]
        );
    }

    #[test]
    fn names_are_trimmed_and_each_offered_once() {
        let file = r#"{"authors": [
            {"name": "  dimo ", "popular": 8},
            {"name": "   "},
            {"name": "DIMO", "popular": 2},
            {"name": "freyja"},
            {"name": "Freyja ", "popular": 9}
        ]}"#;
        assert_eq!(
            parse(file.as_bytes()).expect("an author list"),
            vec![author("dimo", 8), author("freyja", 0)],
            "the first spelling stands, and a missing count is none"
        );
    }

    #[test]
    fn a_file_that_names_nobody_or_does_not_read_is_turned_away() {
        for spoiled in [
            "",
            "<!DOCTYPE html><title>404</title>",
            "{\"authors\": []}",
            "{\"authors\": [{\"name\": \" \"}]}",
            "{\"built\": \"2026-10-17\"}",
            "{\"authors\": [{\"popular\": 3}]}",
            "{\"authors\": [{\"name\": 12}]}",
            "{\"authors\": [{\"name\": \"Zaia\", \"popular\": -1}]}",
            "{\"authors\": [{\"name\": \"Zaia\"}",
        ] {
            assert!(parse(spoiled.as_bytes()).is_err(), "{spoiled:?}");
        }
    }

    #[test]
    fn a_fetch_comes_to_a_list_or_one_plain_message() {
        let (phase, authors, message) = outcome(Ok(LIVE.as_bytes().to_vec()));
        assert_eq!(phase, AuthorsPhase::Ready);
        assert_eq!(authors.map(|list| list.len()), Some(5));
        assert_eq!(message, None);

        for failed in [
            Err("could not fetch authors.json from itgcontent.net: request timed out".to_owned()),
            Ok(b"{\"authors\": []}".to_vec()),
        ] {
            let (phase, authors, message) = outcome(failed);
            assert_eq!(phase, AuthorsPhase::Error);
            assert_eq!(authors, None, "whatever was offered before stays offered");
            assert_eq!(message.as_deref(), Some("could not reach itgcontent.net"));
        }
    }
}
