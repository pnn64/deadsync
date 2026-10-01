// Frozen from 78094fd5c (0.5.1651).
// Unchanged plan parsing and the new token parser are shared with production
// to isolate removal of the intermediate plan vector.
use crate::script::*;
use smallvec::SmallVec;
use std::collections::HashMap;
type SpriteAnimationCommandRefs<'a> = SmallVec<[(&'a String, &'a String); 8]>;

fn append_sprite_animation_command_plans(
    script: &str,
    plans: &mut Vec<SpriteAnimationCommandPlan>,
) {
    let script = normalized_script_command(script);
    for raw_token in script.split(';') {
        let token = raw_token.trim();
        if token.is_empty() {
            continue;
        }
        let Some(token) = split_script_token(token) else {
            continue;
        };
        match token.command() {
            ScriptCommand::SetStateProperties => {
                let args = token.args();
                if let Some((frame_count, frame_delays)) = parse_script_state_properties(args) {
                    plans.push(SpriteAnimationCommandPlan::StateProperties(
                        SpriteStatePropertiesPlan {
                            frame_count,
                            frame_delays,
                        },
                    ));
                }
            }
            ScriptCommand::SetAllStateDelays => {
                let args = token.args();
                if let Some(delay) = args.first().and_then(|arg| parse_script_number(arg)) {
                    plans.push(SpriteAnimationCommandPlan::AllStateDelays(delay.max(0.0)));
                }
            }
            _ => {}
        }
    }
}

#[must_use]
pub fn sprite_animation_command_plans(script: &str) -> Vec<SpriteAnimationCommandPlan> {
    let mut plans = Vec::new();
    append_sprite_animation_command_plans(script, &mut plans);
    plans
}

#[must_use]
pub fn sprite_animation_command_plans_from_commands(
    commands: &HashMap<String, String>,
    default_is_beat_based: bool,
) -> (bool, Vec<SpriteAnimationCommandPlan>) {
    if commands.is_empty() {
        return (default_is_beat_based, Vec::new());
    }
    let sorted = sorted_sprite_animation_command_refs(commands);

    let mut beat_based = default_is_beat_based;
    for (_, script) in sorted.iter().copied() {
        if let Some(script_clock) = parse_script_effectclock_from_commands(script) {
            beat_based = script_clock;
        }
    }

    let mut plans = Vec::new();
    for (_, script) in sorted {
        append_sprite_animation_command_plans(script, &mut plans);
    }
    (beat_based, plans)
}

fn sorted_sprite_animation_command_refs(
    commands: &HashMap<String, String>,
) -> SpriteAnimationCommandRefs<'_> {
    let mut sorted = commands.iter().collect::<SpriteAnimationCommandRefs<'_>>();
    sorted.sort_unstable_by(|a, b| a.0.cmp(b.0));
    sorted
}

pub fn apply_sprite_animation_command_plans<T>(
    slot: &mut T,
    commands: &HashMap<String, String>,
    default_is_beat_based: bool,
    mut apply_plan: impl FnMut(&mut T, SpriteAnimationCommandPlan, bool),
) {
    let (beat_based, plans) =
        sprite_animation_command_plans_from_commands(commands, default_is_beat_based);
    for plan in plans {
        apply_plan(slot, plan, beat_based);
    }
}

pub fn apply_sprite_animation_script_plans<T>(
    slot: &mut T,
    script: &str,
    beat_based: bool,
    mut apply_plan: impl FnMut(&mut T, SpriteAnimationCommandPlan, bool),
) {
    for plan in sprite_animation_command_plans(script) {
        apply_plan(slot, plan, beat_based);
    }
}
