pub(super) fn resolve_course_stage(
    course_path: &Path,
    entry_index: usize,
    random_seed: u64,
    entry: &CourseEntry,
    by_group_song: &HashMap<(String, String), Arc<SongData>>,
    by_song: &HashMap<String, Arc<SongData>>,
    all_songs: &[Arc<SongData>],
    songs_by_group: &HashMap<String, Vec<Arc<SongData>>>,
    song_play_counts: &HashMap<String, u32>,
    song_grade_counts: &HashMap<String, CourseGradeCounts>,
    course_type: CourseType,
    selected_song_keys: &[String],
    chart_type: &str,
    course_difficulty: Difficulty,
) -> Option<ResolvedCourseStage> {
    let mut candidates = match &entry.song {
        CourseSong::Fixed { group, song } => {
            let song_key = song.trim().to_ascii_lowercase();
            let resolved = if let Some(group) = group.as_deref().map(str::trim) {
                let group_key = group.to_ascii_lowercase();
                by_group_song.get(&(group_key, song_key))
            } else {
                by_song.get(&song_key)
            }?;
            course_candidates(std::slice::from_ref(resolved), entry, chart_type)
        }
        CourseSong::SortPick { .. } | CourseSong::RandomAny => {
            course_candidates(all_songs, entry, chart_type)
        }
        CourseSong::RandomWithinGroup { group } => songs_by_group
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(group.trim()))
            .map(|(_, songs)| course_candidates(songs, entry, chart_type))
            .unwrap_or_default(),
        CourseSong::Select(select) => {
            select_course_candidates(select, all_songs, songs_by_group, entry, chart_type)
        }
        CourseSong::Unknown { .. } => return None,
    };

    let (sort, pick) = match &entry.song {
        CourseSong::SortPick { sort, index } => (Some(*sort), (*index).max(0) as usize),
        CourseSong::Select(select) => (select.sort, select.index.max(0) as usize),
        _ => (None, 0),
    };
    avoid_course_repeats(
        &mut candidates.items,
        course_type,
        sort.is_none(),
        selected_song_keys,
    );
    let candidate = pick_course_candidate(
        &mut candidates.items,
        sort,
        pick,
        song_play_counts,
        song_grade_counts,
        random_seed ^ ((course_difficulty as u64) << 32),
        course_path,
        entry_index,
    )?;
    let chart_pick = random_pick_index(
        random_seed ^ song_key_hash(candidate.song_key()),
        course_path,
        entry_index ^ usize::MAX,
        candidate.chart_indices.len(),
    );
    let base_chart = *candidates
        .chart_indices
        .get(candidate.chart_indices.clone())?
        .get(chart_pick)?;
    let chart_index = shifted_chart_index(
        candidate.song,
        base_chart,
        entry,
        chart_type,
        course_difficulty,
    );
    Some(ResolvedCourseStage {
        song: candidate.song.clone(),
        chart_index,
        modifiers: entry.modifiers.clone(),
        gain_seconds: entry.gain_seconds,
        gain_lives: entry.gain_lives,
    })
}

pub(super) fn pick_course_candidate<'a>(
    candidates: &mut Vec<CourseCandidate<'a>>,
    sort: Option<SongSort>,
    pick: usize,
    song_play_counts: &HashMap<String, u32>,
    song_grade_counts: &HashMap<String, CourseGradeCounts>,
    random_seed: u64,
    course_path: &Path,
    entry_index: usize,
) -> Option<CourseCandidate<'a>> {
    if candidates.is_empty() {
        return None;
    }
    let index = if let Some(sort) = sort {
        selected_course_candidate_index(
            candidates,
            sort,
            pick,
            song_play_counts,
            song_grade_counts,
        )?
    } else {
        random_pick_index(random_seed, course_path, entry_index, candidates.len())
    };
    Some(candidates.swap_remove(index))
}

pub(super) fn random_pick_index(
    seed: u64,
    course_path: &Path,
    entry_index: usize,
    len: usize,
) -> usize {
    if len == 0 {
        return 0;
    }
    let mut hasher = XxHash64::with_seed(seed);
    hasher.write(course_path.to_string_lossy().as_bytes());
    hasher.write_u64(entry_index as u64);
    (hasher.finish() as usize) % len
}
