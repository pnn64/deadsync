//! Observable live output, retaining the original comparisons' source fields.
#![allow(dead_code)]

use deadsync_song_lua::{
    SongLuaActorSound, SongLuaLiveOutput, SongLuaOverlayState, SongLuaTrackedActorTarget,
};

pub fn output(output: &SongLuaLiveOutput) -> impl std::fmt::Debug + '_ {
    let broadcasts = output
        .source_broadcasts
        .iter()
        .map(|event| (event.beat, event.music_seconds, &event.name, &event.params))
        .collect::<Vec<_>>();
    (
        &output.request,
        &output.source_requests,
        &output.broadcasts,
        &output.sounds,
        &output.player_writes,
        broadcasts,
        &output.coverage_gaps,
        &output.draw_frames,
        output.errors.len(),
        output.actor_sounds.len(),
    )
}

fn canonical_json(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(entries) => {
            let mut entries = entries.iter().collect::<Vec<_>>();
            entries.sort_by(|left, right| left.0.cmp(right.0));
            serde_json::Value::Object(
                entries
                    .into_iter()
                    .map(|(key, value)| (key.clone(), canonical_json(value)))
                    .collect(),
            )
        }
        serde_json::Value::Array(entries) => {
            serde_json::Value::Array(entries.iter().map(canonical_json).collect())
        }
        value => value.clone(),
    }
}

pub fn canonical_coverage(gaps: &[String]) -> Vec<String> {
    let missing = |gap: &str| gap.starts_with("missing captured subscriber identity for ");
    let mut names = gaps
        .iter()
        .filter(|gap| missing(gap))
        .cloned()
        .collect::<Vec<_>>();
    names.sort();
    let mut names = names.into_iter();
    gaps.iter()
        .map(|gap| {
            if missing(gap) {
                names.next().unwrap()
            } else {
                gap.clone()
            }
        })
        .collect()
}

pub fn scene_output(output: &SongLuaLiveOutput) -> impl std::fmt::Debug + '_ {
    // JSON equality ignores object key order. Missing subscriber identities have
    // no captured delivery rank, so compare their diagnostics as a multiset.
    // Keep all event, request, write, draw and array orders intact.
    let broadcasts = output
        .source_broadcasts
        .iter()
        .map(|event| {
            let params = event
                .params
                .as_ref()
                .map(|value| value.as_ref().map(canonical_json));
            (event.beat, event.music_seconds, &event.name, params)
        })
        .collect::<Vec<_>>();
    let gaps = canonical_coverage(&output.coverage_gaps);
    (
        &output.request,
        &output.source_requests,
        &output.broadcasts,
        &output.sounds,
        &output.player_writes,
        broadcasts,
        gaps,
        &output.draw_frames,
        output.errors.len(),
        output.actor_sounds.len(),
    )
}

pub fn sounds(events: Vec<SongLuaActorSound>, owners: &[u64]) -> Vec<SongLuaActorSound> {
    let owner_index = |owner| owners.iter().position(|&id| id == owner).unwrap() as u64;
    events
        .into_iter()
        .map(|event| match event {
            SongLuaActorSound::Play { owner, path } => SongLuaActorSound::Play {
                owner: owner_index(owner),
                path,
            },
            SongLuaActorSound::Stop { owner } => SongLuaActorSound::Stop {
                owner: owner_index(owner),
            },
            SongLuaActorSound::Pause { owner, paused } => SongLuaActorSound::Pause {
                owner: owner_index(owner),
                paused,
            },
        })
        .collect()
}

pub fn tracked(
    rows: impl Iterator<Item = (SongLuaTrackedActorTarget, SongLuaOverlayState)>,
) -> Vec<((u8, usize), SongLuaOverlayState)> {
    rows.map(|(target, state)| {
        let target = match target {
            SongLuaTrackedActorTarget::Player(index) => (0, index),
            SongLuaTrackedActorTarget::PlayerJudgment(index) => (1, index),
            SongLuaTrackedActorTarget::PlayerCombo(index) => (2, index),
            SongLuaTrackedActorTarget::SongForeground => (3, 0),
            SongLuaTrackedActorTarget::ScreenLayer(index) => (4, index),
        };
        (target, state)
    })
    .collect()
}
