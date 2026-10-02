use super::*;
use std::hint::black_box;

include!("message_lookup_baseline.rs");

#[test]
fn folded_lookup_matches_parent_for_ascii_unicode_duplicates_and_long_names() {
    for length in [0, 1, 16, 127, 128, 129, 1024] {
        let command = "A".repeat(length);
        let indices = build_song_lua_message_command_indices([
            (17, command.as_str()),
            (0, "Hide"),
            (1, "HIDE"),
            (2, "ÄHIDE"),
            (3, "ähide"),
            (4, ""),
            (5, "command\0tail"),
        ]);
        for query in [
            command.as_str(),
            &command.to_lowercase(),
            "Hide",
            "hide",
            "HIDE",
            "ÄHIDE",
            "ähide",
            "",
            "missing",
            "COMMAND\0TAIL",
        ] {
            assert_eq!(
                old_song_lua_message_command_index(&indices, query),
                song_lua_message_command_index(&indices, query)
            );
        }
    }
    let commands: Vec<_> = (0..1024)
        .map(|i| format!("LongSharedCommandPrefix{i:04}Message"))
        .collect();
    let indices = build_song_lua_message_command_indices(
        commands.iter().enumerate().map(|(i, s)| (i, s.as_str())),
    );
    for command in commands {
        for query in [
            command.clone(),
            command.to_ascii_uppercase(),
            command.to_ascii_lowercase(),
        ] {
            assert_eq!(
                old_song_lua_message_command_index(&indices, &query),
                song_lua_message_command_index(&indices, &query)
            );
        }
    }
}

#[test]
fn all_message_lengths_preserve_zero_allocator_churn() {
    for length in [8, 128, 129, 1024] {
        let query = "C".repeat(length);
        let indices = build_song_lua_message_command_indices([(0, query.as_str())]);
        perf::assert_no_churn(|| {
            for _ in 0..128 {
                black_box(song_lua_message_command_index(&indices, black_box(&query)));
            }
        });
    }
}

#[test]
#[ignore = "manual release benchmark; run serially with --nocapture"]
fn benchmark_message_lookup() {
    let reverse = std::env::var_os("DEADSYNC_PERF_REVERSE").is_some();
    for (size, mode) in [
        (8, "upper"),
        (64, "upper"),
        (1024, "upper"),
        (64, "lower"),
        (64, "mixed"),
        (64, "long"),
        (64, "miss"),
    ] {
        let commands: Vec<_> = (0..size)
            .map(|i| {
                if mode == "long" {
                    format!("{}{i:04}", "A".repeat(256))
                } else {
                    format!("SharedCommandPrefix{i:04}Message")
                }
            })
            .collect();
        let indices = build_song_lua_message_command_indices(
            commands.iter().enumerate().map(|(i, s)| (i, s.as_str())),
        );
        let queries: Vec<_> = (0..128)
            .map(|i| match mode {
                "lower" => commands[i % size].to_ascii_lowercase(),
                "mixed" => commands[i % size].clone(),
                "miss" => format!("SHAREDCOMMANDPREFIX{:04}MISSING", i + size),
                _ => commands[i % size].to_ascii_uppercase(),
            })
            .collect();
        for query in &queries {
            assert_eq!(
                old_song_lua_message_command_index(&indices, query),
                song_lua_message_command_index(&indices, query)
            );
        }
        for old in if reverse {
            [false, true]
        } else {
            [true, false]
        } {
            let label = format!("message_{size}_{mode}_{}", if old { "old" } else { "new" });
            perf::measure_sampled(&label, 256, queries.len(), || {
                for query in &queries {
                    black_box(if old {
                        old_song_lua_message_command_index(&indices, black_box(query))
                    } else {
                        song_lua_message_command_index(&indices, black_box(query))
                    });
                }
            });
        }
    }
}
