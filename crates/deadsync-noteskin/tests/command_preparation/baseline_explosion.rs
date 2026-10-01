// Frozen from 78094fd5c (0.5.1651).
// Unchanged normalization, token parsing and animation parsing are shared.
use super::*;

pub(crate) fn parse_itg_tap_explosion_animation_commands(
    commands: &HashMap<String, String>,
    mode: ItgTapExplosionMode,
    command: &str,
) -> ExplosionAnimation {
    let mut sequence = [""; 4];
    let mut len = 0;
    for command in [
        commands.get("initcommand"),
        commands.get("judgmentcommand"),
        commands.get(mode.command_key()),
    ]
    .into_iter()
    .flatten()
    .map(|command| command.trim())
    .filter(|command| !command.is_empty())
    {
        sequence[len] = command;
        len += 1;
    }
    sequence[len] = command.trim();
    len += 1;

    if sequence[..len].iter().any(|command| {
        command
            .as_bytes()
            .windows(11)
            .any(|word| word.eq_ignore_ascii_case(b"playcommand"))
    }) {
        let mut expanded = String::new();
        let mut stack = ArrayVec::new();
        let mut remaining = 4096;
        for command in &sequence[..len] {
            if !expand_explosion_cmd(commands, command, &mut stack, &mut remaining, &mut expanded) {
                break;
            }
        }
        return parse_explosion_animation_parts([expanded.as_str()]);
    }

    if sequence[..len]
        .iter()
        .all(|command| !command.contains("self:"))
    {
        parse_explosion_animation_parts(sequence[..len].iter().copied())
    } else {
        parse_explosion_animation(&sequence[..len].join(";"))
    }
}

pub(super) fn expand_explosion_cmd<'a>(
    commands: &'a HashMap<String, String>,
    script: &str,
    stack: &mut ArrayVec<&'a str, 16>,
    remaining: &mut usize,
    output: &mut String,
) -> bool {
    let script = normalized_script_command(script);
    for raw in script
        .split(';')
        .map(str::trim)
        .filter(|token| !token.is_empty())
    {
        if *remaining == 0 || output.len().saturating_add(raw.len() + 1) > 256 * 1024 {
            warn!("Noteskin explosion command expansion exceeds its size limit");
            return false;
        }
        *remaining -= 1;
        if let Some(token) = split_script_token(raw)
            && token.command() == ScriptCommand::PlayCommand
        {
            let Some(name) = token.args().first() else {
                continue;
            };
            let key = format!(
                "{}command",
                name.trim().trim_matches(['\'', '"']).to_ascii_lowercase()
            );
            let Some((key, command)) = commands.get_key_value(&key) else {
                continue;
            };
            if stack.is_full() || stack.contains(&key.as_str()) {
                warn!("Recursive noteskin explosion command '{key}' skipped");
                continue;
            }
            stack.push(key);
            let complete = expand_explosion_cmd(commands, command, stack, remaining, output);
            stack.pop();
            if !complete {
                return false;
            }
        } else {
            if !output.is_empty() {
                output.push(';');
            }
            output.push_str(raw);
        }
    }
    true
}
