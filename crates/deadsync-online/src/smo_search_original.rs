// Frozen implementation from main 0a26982ae for paired regression checks.
use super::*;

pub(super) fn runtime_search(query: &str) {
    let query = query.trim().to_owned();
    let generation = {
        let mut runtime = lock_runtime();
        if runtime.snapshot.query == query
            && matches!(
                runtime.snapshot.phase,
                SearchPhase::Loading | SearchPhase::Ready
            )
        {
            return;
        }
        runtime.generation = runtime.generation.wrapping_add(1);
        let generation = runtime.generation;
        publish(
            &mut runtime,
            SearchSnapshot {
                phase: if query.is_empty() {
                    SearchPhase::Idle
                } else {
                    SearchPhase::Loading
                },
                query: query.clone(),
                hits: Arc::from(Vec::new()),
                capped: false,
                revision: 0,
                message: None,
            },
        );
        generation
    };

    if query.chars().count() < MIN_CHARS {
        // Too short to run, but the empty result still belongs to this query
        // rather than to whatever was searched before it.
        let mut runtime = lock_runtime();
        if runtime.generation == generation {
            let mut snapshot = (*runtime.snapshot).clone();
            snapshot.phase = SearchPhase::Ready;
            publish(&mut runtime, snapshot);
        }
        return;
    }

    let spawn = thread::Builder::new()
        .name("smo-search".to_owned())
        .spawn(move || run(generation, query));
    if spawn.is_err() {
        let mut runtime = lock_runtime();
        if runtime.generation == generation {
            let mut snapshot = (*runtime.snapshot).clone();
            snapshot.phase = SearchPhase::Error;
            snapshot.message = Some("could not start the search".to_owned());
            publish(&mut runtime, snapshot);
        }
    }
}

pub(super) fn sort_and_cap(acc: &mut Accumulator, details: &smo_details::DetailsSnapshot) {
    // Score first, then newest, then by name -- so equal-scoring packs come
    // out in a stable and useful order rather than in whatever order the
    // passes happened to add them.
    let date_of = |pack_id: u64| {
        details
            .by_id
            .get(&pack_id)
            .and_then(|entry| entry.date_added.clone())
            .unwrap_or_default()
    };
    acc.hits.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| date_of(b.pack_id).cmp(&date_of(a.pack_id)))
            .then_with(|| a.pack_id.cmp(&b.pack_id))
    });
    acc.capped = acc.hits.len() > MAX_ROWS;
    acc.hits.truncate(MAX_ROWS);
}

pub(super) fn percent_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char);
            }
            b' ' => out.push_str("%20"),
            _ => out.push_str(format!("%{byte:02X}").as_str()),
        }
    }
    out
}
