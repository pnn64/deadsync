use std::collections::HashMap;
use std::sync::Arc;

use deadsync_chart::SongPack;
use deadsync_profile::app_runtime::{machine_record_local, recent_played_chart_hashes_for_machine};
use deadsync_score::leaderboard::{CabinetCourseRecord, CabinetRecords, CabinetSongRecord};
use deadsync_simfile::course::{CourseType, course_type, is_autogen_course_path};
use deadsync_simfile::course_selection::course_score_hash;
use deadsync_simfile::runtime_cache::{CourseData, get_course_cache, get_song_cache};

/// Resolve cabinet scores against the currently loaded chart and course identities.
pub fn recent_records(song_limit: usize, course_limit: usize) -> CabinetRecords {
    let history = recent_played_chart_hashes_for_machine();
    let ranks: HashMap<_, _> = history
        .iter()
        .enumerate()
        .map(|(rank, hash)| (hash.as_str(), rank))
        .collect();
    let song_labels = if ranks.is_empty() || song_limit == 0 {
        Vec::new()
    } else {
        recent_song_labels(&get_song_cache(), &ranks, song_limit)
    };
    let [marathon, survival] = course_labels(&get_course_cache(), &ranks, course_limit);

    let best = |hash: &str| {
        machine_record_local(hash)
            .map(|(initials, record)| (initials, 100.0 * record.score_percent))
    };
    let course_records = |labels: Vec<(Arc<str>, String)>| {
        labels
            .into_iter()
            .map(|(title, hash)| CabinetCourseRecord {
                title,
                best: if ranks.contains_key(hash.as_str()) {
                    best(&hash)
                } else {
                    None
                },
            })
            .collect()
    };
    CabinetRecords {
        songs: song_labels
            .into_iter()
            .filter_map(|(rank, title, difficulty)| {
                let (initials, percent) = best(&history[rank])?;
                Some(CabinetSongRecord {
                    title,
                    difficulty,
                    initials,
                    percent,
                })
            })
            .collect(),
        marathon: course_records(marathon),
        survival: course_records(survival),
    }
}

fn recent_song_labels(
    library: &[SongPack],
    ranks: &HashMap<&str, usize>,
    limit: usize,
) -> Vec<(usize, Arc<str>, Arc<str>)> {
    let mut labels: Vec<_> = library
        .iter()
        .flat_map(|pack| &pack.songs)
        .flat_map(|song| {
            song.charts.iter().filter_map(move |chart| {
                let rank = *ranks.get(chart.short_hash.as_str())?;
                Some((rank, song.title.as_str(), chart.difficulty.as_str()))
            })
        })
        .collect();
    labels.sort_by_key(|(rank, _, _)| *rank);
    labels.dedup_by_key(|(rank, _, _)| *rank);
    labels
        .into_iter()
        .take(limit)
        .map(|(rank, title, difficulty)| (rank, Arc::from(title), Arc::from(difficulty)))
        .collect()
}

fn course_labels(
    library: &[CourseData],
    ranks: &HashMap<&str, usize>,
    limit: usize,
) -> [Vec<(Arc<str>, String)>; 2] {
    let mut groups = [Vec::new(), Vec::new()];
    for (path, course) in library {
        if is_autogen_course_path(path) {
            continue;
        }
        let group = match course_type(course) {
            CourseType::Nonstop => 0,
            CourseType::Survival => 1,
            CourseType::Oni | CourseType::Endless => continue,
        };
        let hash = course_score_hash(path);
        let rank = ranks.get(hash.as_str()).copied().unwrap_or(usize::MAX);
        let title = if course.name.trim().is_empty() {
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("")
        } else {
            &course.name
        };
        groups[group].push((rank, title, hash));
    }
    groups.map(|mut group| {
        group.sort_unstable_by(|a, b| {
            a.0.cmp(&b.0)
                .then_with(|| a.1.cmp(b.1))
                .then_with(|| a.2.cmp(&b.2))
        });
        group
            .into_iter()
            .take(limit)
            .map(|(_, title, hash)| (Arc::from(title), hash))
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use deadsync_simfile::course::{CourseEntry, CourseFile, CourseSong, Difficulty, StepsSpec};

    fn course(title: &str, gain_seconds: f32, repeat: bool) -> CourseData {
        (
            format!("courses/{title}.crs").into(),
            CourseFile {
                name: title.into(),
                name_translit: String::new(),
                scripter: String::new(),
                description: String::new(),
                banner: String::new(),
                background: String::new(),
                repeat,
                lives: 0,
                meters: [None; 6],
                entries: vec![CourseEntry {
                    song: CourseSong::RandomAny,
                    steps: StepsSpec::Difficulty(Difficulty::Medium),
                    modifiers: String::new(),
                    secret: false,
                    no_difficult: false,
                    gain_seconds,
                    gain_lives: 0,
                }],
            },
        )
    }

    #[test]
    fn course_rankings_use_recent_score_keys_and_keep_unplayed_courses() {
        let mut autogen = course("AA Autogen", 0.0, false);
        autogen
            .0
            .set_file_name("__deadsync_autogen_nonstop_random.crs");
        let library = [
            course("A Marathon", 0.0, false),
            course("Z Marathon", 0.0, false),
            course("B Marathon", 0.0, false),
            course("Survival", 5.0, false),
            course("Endless", 0.0, true),
            autogen,
        ];
        let marathon_hash = course_score_hash(&library[1].0);
        let survival_hash = course_score_hash(&library[3].0);
        let ranks = HashMap::from([(marathon_hash.as_str(), 5), (survival_hash.as_str(), 1)]);
        let [marathon, survival] = course_labels(&library, &ranks, 2);
        assert_eq!(marathon[0], (Arc::from("Z Marathon"), marathon_hash));
        assert_eq!(marathon[1].0.as_ref(), "A Marathon");
        assert_eq!(survival, [(Arc::from("Survival"), survival_hash)]);
    }
}
