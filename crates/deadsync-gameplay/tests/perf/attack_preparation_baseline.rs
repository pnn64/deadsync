// Frozen from parent cd3fc61634525c0b85552474d9986185839d5090 (0.5.1687).
// Normalization dependencies are frozen; scalar chunk and tail helpers stay unchanged.

fn old_buffered_attack_token_key<'a>(
    token: &str,
    buffer: &'a mut [u8; ATTACK_KEY_STACK_BYTES],
) -> BufferedAttackKey<'a> {
    let mut len = 0usize;
    for byte in token.bytes().filter(u8::is_ascii_alphanumeric) {
        if len == 0 && byte.is_ascii_digit() {
            continue;
        }
        if len == buffer.len() {
            return BufferedAttackKey::Heap(attack_token_key(token));
        }
        buffer[len] = byte.to_ascii_lowercase();
        len += 1;
    }
    BufferedAttackKey::Borrowed(
        std::str::from_utf8(&buffer[..len]).expect("attack keys contain only ASCII bytes"),
    )
}

fn old_song_lua_runtime_attack_key<'a, const BUFFERED: bool>(
    token: &str,
    buffer: &'a mut [u8; ATTACK_KEY_STACK_BYTES],
) -> BufferedAttackKey<'a> {
    if BUFFERED {
        old_buffered_attack_token_key(token, buffer)
    } else {
        BufferedAttackKey::Heap(attack_token_key(token))
    }
}

fn old_parse_attack_mods(mods: &str) -> ParsedAttackMods {
    let mut out = ParsedAttackMods::default();
    let mut key_buffer = [0u8; ATTACK_KEY_STACK_BYTES];
    for token in mods.split(',') {
        let (approach_speed, token) = parse_attack_approach_prefix(token);
        if token.is_empty() {
            continue;
        }
        if let Some(scroll_speed) = parse_attack_scroll_override(token) {
            out.scroll_speed = Some(scroll_speed);
            continue;
        }
        let (percent_value, token_key) = parse_attack_level_token(token);
        let key = old_buffered_attack_token_key(token_key, &mut key_buffer);
        let key = key.as_str();
        if key.is_empty() {
            continue;
        }
        match key {
            "clearall" => {
                out = ParsedAttackMods {
                    clear_all: true,
                    ..ParsedAttackMods::default()
                };
            }
            _ => apply_runtime_mod(&mut out, key, percent_value, approach_speed),
        }
    }
    out
}

fn old_parse_song_lua_runtime_mods_core<const BUFFERED: bool>(mods: &str) -> ParsedAttackMods {
    let mut out = ParsedAttackMods::default();
    let mut key_buffer = [0u8; ATTACK_KEY_STACK_BYTES];
    for token in mods.split(',') {
        let mut parts = token.trim().split_ascii_whitespace();
        let Some(first) = parts.next() else {
            continue;
        };
        let Some(second) = parts.next() else {
            if let Some(scroll_speed) = parse_attack_scroll_override(first) {
                out.scroll_speed = Some(scroll_speed);
                continue;
            }
            let key = old_song_lua_runtime_attack_key::<BUFFERED>(first, &mut key_buffer);
            let key = key.as_str();
            if key.is_empty() {
                continue;
            }
            if key == "clearall" {
                out = ParsedAttackMods {
                    clear_all: true,
                    ..ParsedAttackMods::default()
                };
                continue;
            }
            apply_runtime_mod(&mut out, key, Some(100.0), 1.0);
            continue;
        };

        if first.starts_with('*') {
            let approach_speed = parse_attack_approach_prefix(first).0;
            let Some(third) = parts.next() else {
                if let Some(scroll_speed) = parse_attack_scroll_override(second) {
                    out.scroll_speed = Some(scroll_speed);
                    continue;
                }
                let key = old_song_lua_runtime_attack_key::<BUFFERED>(second, &mut key_buffer);
                let key = key.as_str();
                if !key.is_empty() {
                    apply_runtime_mod(&mut out, key, Some(100.0), approach_speed);
                }
                continue;
            };
            let key = old_song_lua_runtime_attack_key::<BUFFERED>(third, &mut key_buffer);
            let key = key.as_str();
            if key.is_empty() {
                continue;
            }
            let amount = parse_song_lua_mod_amount(second).unwrap_or(0.0);
            apply_runtime_mod(&mut out, key, Some(amount), approach_speed);
            continue;
        }

        let key = old_song_lua_runtime_attack_key::<BUFFERED>(second, &mut key_buffer);
        let key = key.as_str();
        if key.is_empty() {
            continue;
        }
        let amount = parse_song_lua_mod_amount(first).unwrap_or(0.0);
        apply_runtime_mod(&mut out, key, Some(amount), 1.0);
    }
    out
}

fn old_build_attack_mask_windows_for_mode(
    chart_attacks: Option<&str>,
    attack_mode: GameplayAttackMode,
    player: usize,
    base_seed: u64,
    song_length_seconds: f32,
) -> Vec<AttackMaskWindow> {
    if attack_mode == GameplayAttackMode::On {
        let Some(raw) = chart_attacks else {
            return Vec::new();
        };
        let mut windows = Vec::with_capacity(ChartAttackChunks::new(raw).count());
        for attack in ChartAttackChunks::new(raw).filter_map(parse_chart_attack_chunk) {
            if let Some(window) = attack_mask_window_from_values(
                attack.start_second,
                attack.len_seconds,
                old_parse_attack_mods(attack.mods),
            ) {
                windows.push(window);
            }
        }
        return windows;
    }

    let attacks = build_attack_windows_for_mode(
        chart_attacks,
        attack_mode,
        player,
        base_seed,
        song_length_seconds,
    );
    build_attack_mask_windows(&attacks)
}

fn old_song_lua_extend_column_offset_tails(out: &mut [SongLuaColumnOffsetWindowRuntime]) {
    const SAME_TICK_EPSILON: f32 = 0.001;

    if out.iter().any(|window| !window.start_second.is_finite()) {
        song_lua_extend_column_offset_tails_nonfinite(out);
        return;
    }

    with_song_lua_tail_indices(out.len(), |indices| {
        indices.sort_unstable_by(|&left, &right| {
            out[left]
                .column
                .cmp(&out[right].column)
                .then_with(|| out[left].target.cmp(&out[right].target))
                .then_with(|| out[left].start_second.total_cmp(&out[right].start_second))
                .then_with(|| left.cmp(&right))
        });
        let mut group_start = 0;
        while group_start < indices.len() {
            let column = out[indices[group_start]].column;
            let target = out[indices[group_start]].target;
            let group_end = indices[group_start..].partition_point(|&index| {
                out[index].column == column && out[index].target == target
            }) + group_start;
            let mut next = group_start + 1;
            for position in group_start..group_end {
                next = next.max(position + 1);
                let index = indices[position];
                let start_second = out[index].start_second;
                while next < group_end
                    && out[indices[next]].start_second <= start_second + SAME_TICK_EPSILON
                {
                    next += 1;
                }
                let window = &out[index];
                let default_end =
                    if window.sustain_end_second > window.end_second + SAME_TICK_EPSILON {
                        window.sustain_end_second
                    } else {
                        f32::MAX
                    };
                out[index].sustain_end_second = if next < group_end {
                    default_end.min(out[indices[next]].start_second)
                } else {
                    default_end
                };
            }
            group_start = group_end;
        }
    });
}
