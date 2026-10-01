use super::*;
use crate::perf;
use crate::script::{self, ScriptCommand, SpriteAnimationCommandPlan};
use std::hint::black_box;

mod baseline_token {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/command_preparation/baseline_token.rs"
    ));
}
mod baseline_plans {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/command_preparation/baseline_plans.rs"
    ));
}
mod baseline_explosion {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/command_preparation/baseline_explosion.rs"
    ));
}

fn same_token(text: &str) {
    let old = baseline_token::split_script_token(text);
    let new = script::split_script_token(text);
    assert_eq!(
        old.as_ref().map(|t| (t.command(), t.args())),
        new.as_ref().map(|t| (t.command(), t.args())),
        "{text:?}"
    );
}

#[test]
fn token_grammar_matches_parent_for_nested_quoted_empty_and_unbalanced_fields() {
    for command in [
        "",
        "zoom",
        "DIFFUSE",
        "custom",
        "\u{6771}\u{4eac}",
        "effectmagnitude",
    ] {
        for arg in [
            "",
            "1",
            " 1 ",
            "'a,b'",
            "\"\u{e9},x\"",
            "(1,2)",
            "{a={1,2},b=3}",
            "[a,b]",
            "')'",
            "\"unterminated",
            "]",
            "Sprite.LinearFrames(4,0.5)",
        ] {
            for count in [0, 1, 5, 6, 7, 12, 32] {
                let args = std::iter::repeat_n(arg, count)
                    .collect::<Vec<_>>()
                    .join(",");
                same_token(&format!("\u{2003},,{command},,{args},, \u{2003}"));
            }
        }
    }
    let alphabet: Vec<char> = "abZ09, (){}[]'\";\\\t\n\u{e9}\u{6771}\u{2003}"
        .chars()
        .collect();
    let mut seed = 0x1652_u64;
    for _ in 0..4096 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let len = (seed >> 32) as usize % 129;
        let mut text = String::new();
        for _ in 0..len {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            text.push(alphabet[(seed >> 32) as usize % alphabet.len()]);
        }
        same_token(&text);
    }
    same_token(&format!(
        "custom,{}",
        std::iter::repeat_n("argument", 1024)
            .collect::<Vec<_>>()
            .join(",")
    ));
}

#[test]
fn token_arguments_use_inline_storage_through_six_and_one_spill_above_it() {
    for count in 0..=6 {
        let text = format!(
            "custom,{}",
            std::iter::repeat_n("arg", count)
                .collect::<Vec<_>>()
                .join(",")
        );
        perf::assert_no_churn(|| {
            black_box(script::split_script_token(&text));
        });
    }
    for count in [6, 7, 12, 24] {
        let text = format!(
            "custom,{}",
            std::iter::repeat_n("arg", count)
                .collect::<Vec<_>>()
                .join(",")
        );
        perf::assert_reduced_churn(
            || {
                black_box(baseline_token::split_script_token(&text));
            },
            || {
                black_box(script::split_script_token(&text));
            },
        );
    }
}

fn commands(count: usize, properties: bool) -> HashMap<String, String> {
    (0..count).rev().map(|i| {
        let clock = if i % 2 == 0 { "beat" } else { "time" };
        let script = if properties {
            format!("effectclock,{clock};setstateproperties,Sprite.LinearFrames(4,0.5);setallstatedelays,0.125;zoom,1")
        } else {
            format!("effectclock,{clock};setallstatedelays,0.125;setallstatedelays,0.25;zoom,1")
        };
        (format!("command{i:03}"), script)
    }).collect()
}

#[test]
fn streaming_plans_preserve_callbacks_final_clock_and_owning_apis() {
    let scripts = [
        "",
        "zoom,1",
        "setallstatedelays,0.5;setallstatedelays,-2",
        "setstateproperties,Sprite.LinearFrames(4,0.5);setallstatedelays,0.2",
        "effectclock,beat;setstateproperties,invalid;effectclock,time;setallstatedelays,1",
        "function(self) self:setstateproperties(Sprite.LinearFrames(3,0.6)):setallstatedelays(0.2) end",
        "setallstatedelays,bad;setstateproperties,Sprite.LinearFrames(0,1)",
    ];
    for script in scripts {
        let mut old = Vec::new();
        let mut new = Vec::new();
        baseline_plans::apply_sprite_animation_script_plans(
            &mut old,
            script,
            true,
            |out, plan, clock| out.push((plan, clock)),
        );
        script::apply_sprite_animation_script_plans(&mut new, script, true, |out, plan, clock| {
            out.push((plan, clock))
        });
        assert_eq!(old, new);
        assert_eq!(
            baseline_plans::sprite_animation_command_plans(script),
            script::sprite_animation_command_plans(script)
        );
    }
    for count in [0, 1, 7, 8, 9, 32] {
        for properties in [false, true] {
            let mut table = commands(count, properties);
            for (i, text) in scripts.iter().enumerate() {
                table.insert(format!("extra{i}"), (*text).to_string());
            }
            for default in [false, true] {
                let mut old = Vec::new();
                let mut new = Vec::new();
                baseline_plans::apply_sprite_animation_command_plans(
                    &mut old,
                    &table,
                    default,
                    |out, plan, clock| out.push((plan, clock)),
                );
                script::apply_sprite_animation_command_plans(
                    &mut new,
                    &table,
                    default,
                    |out, plan, clock| out.push((plan, clock)),
                );
                assert_eq!(old, new);
                assert_eq!(
                    baseline_plans::sprite_animation_command_plans_from_commands(&table, default),
                    script::sprite_animation_command_plans_from_commands(&table, default)
                );
                let clock = script::sprite_animation_command_plans_from_commands(&table, default).0;
                assert!(new.iter().all(|(_, actual)| *actual == clock));
            }
        }
    }
}

fn consume_plan(state: &mut (usize, f32), plan: SpriteAnimationCommandPlan, clock: bool) {
    state.0 += 1 + usize::from(clock);
    match plan {
        SpriteAnimationCommandPlan::AllStateDelays(delay) => state.1 += delay,
        SpriteAnimationCommandPlan::StateProperties(plan) => {
            state.1 += plan.frame_delays.iter().sum::<f32>()
        }
    }
}

#[test]
fn direct_plan_application_removes_intermediate_vector_churn() {
    let table = commands(8, false);
    perf::assert_no_churn(|| {
        script::apply_sprite_animation_command_plans(&mut (0, 0.0), &table, false, consume_plan);
    });
    let script = "setallstatedelays,0.1;setallstatedelays,0.2;setallstatedelays,0.3";
    perf::assert_no_churn(|| {
        script::apply_sprite_animation_script_plans(&mut (0, 0.0), script, false, consume_plan);
    });
    for count in [1, 8, 9, 32] {
        for properties in [false, true] {
            let table = commands(count, properties);
            perf::assert_reduced_churn(
                || {
                    baseline_plans::apply_sprite_animation_command_plans(
                        &mut (0, 0.0),
                        &table,
                        false,
                        consume_plan,
                    );
                },
                || {
                    script::apply_sprite_animation_command_plans(
                        &mut (0, 0.0),
                        &table,
                        false,
                        consume_plan,
                    );
                },
            );
        }
    }
}

fn expansion_fixture(name: &str, repeats: usize) -> (HashMap<String, String>, String) {
    let mut table = HashMap::new();
    let key = format!(
        "{}command",
        name.trim().trim_matches(['\'', '"']).to_ascii_lowercase()
    );
    table.insert(
        key,
        "zoom,1;linear,0.05;diffusealpha,0;rotationz,45".to_string(),
    );
    let script = std::iter::repeat_n(format!("playcommand,'{name}'"), repeats)
        .collect::<Vec<_>>()
        .join(";");
    (table, script)
}

fn same_expansion(table: &HashMap<String, String>, script: &str, budget: usize) {
    let mut old = String::new();
    let mut new = String::new();
    let mut old_budget = budget;
    let mut new_budget = budget;
    let a = baseline_explosion::expand_explosion_cmd(
        table,
        script,
        &mut ArrayVec::new(),
        &mut old_budget,
        &mut old,
    );
    let b = expand_explosion_cmd(
        table,
        script,
        &mut ArrayVec::new(),
        &mut new_budget,
        &mut new,
    );
    assert_eq!((a, old_budget, &old), (b, new_budget, &new));
    for mode in [ItgTapExplosionMode::Dim, ItgTapExplosionMode::Bright] {
        let a = baseline_explosion::parse_itg_tap_explosion_animation_commands(table, mode, script);
        let b = parse_itg_tap_explosion_animation_commands(table, mode, script);
        assert_eq!(format!("{a:?}"), format!("{b:?}"));
        for time in [0.0, 0.025, 0.1, 1.0, f32::INFINITY, f32::NAN] {
            assert_eq!(
                format!("{:?}", a.state_at_seeded(time, 0.1, 42)),
                format!("{:?}", b.state_at_seeded(time, 0.1, 42))
            );
        }
    }
}

#[test]
fn expansion_preserves_name_normalization_recursion_missing_names_and_limits() {
    for name in [
        "".to_string(),
        "Glow".to_string(),
        "  GLOW  ".to_string(),
        "\u{e9}Glow\u{6771}".to_string(),
        "X".repeat(121),
        "X".repeat(122),
        "\u{e9}".repeat(60),
        "\u{e9}".repeat(61),
        "X".repeat(1024),
    ] {
        let (mut table, text) = expansion_fixture(&name, 8);
        table.insert("initcommand".to_string(), "zoom,2".to_string());
        table.insert("judgmentcommand".to_string(), "rotationz,10".to_string());
        table.insert("dimcommand".to_string(), "diffusealpha,1".to_string());
        for budget in [0, 1, 5, 4096] {
            same_expansion(&table, &text, budget);
        }
    }
    let mut table = HashMap::from([
        (
            "loopcommand".to_string(),
            "playcommand,'Loop';zoom,2".to_string(),
        ),
        ("acommand".to_string(), "playcommand,'B';x,1".to_string()),
        ("bcommand".to_string(), "playcommand,'A';y,2".to_string()),
    ]);
    for text in [
        "playcommand,'Loop'",
        "playcommand,'A'",
        "playcommand,'Missing';zoom,2",
        "playcommand;playcommand,\"\"",
        "function(self) self:playcommand('A'):zoom(3) end",
    ] {
        same_expansion(&table, text, 4096);
    }
    for depth in 0..20 {
        table.insert(
            format!("c{depth}command"),
            format!("playcommand,'C{}';zoom,1", depth + 1),
        );
    }
    same_expansion(&table, "playcommand,'C0'", 4096);
    same_expansion(&HashMap::new(), &"x".repeat(256 * 1024 + 1), 4096);
}

#[test]
fn short_command_expansion_has_zero_key_churn_and_oversized_names_spill_once() {
    let (table, text) = expansion_fixture("GLOW", 16);
    let mut out = String::with_capacity(4096);
    perf::assert_no_churn(|| {
        assert!(expand_explosion_cmd(
            &table,
            &text,
            &mut ArrayVec::new(),
            &mut 4096,
            &mut out
        ));
    });
    perf::assert_reduced_churn(
        || {
            let mut out = String::with_capacity(4096);
            black_box(baseline_explosion::expand_explosion_cmd(
                &table,
                &text,
                &mut ArrayVec::new(),
                &mut 4096,
                &mut out,
            ));
        },
        || {
            let mut out = String::with_capacity(4096);
            black_box(expand_explosion_cmd(
                &table,
                &text,
                &mut ArrayVec::new(),
                &mut 4096,
                &mut out,
            ));
        },
    );
    for name in ["X".repeat(122), "\u{e9}".repeat(61), "X".repeat(1024)] {
        let (table, text) = expansion_fixture(&name, 1);
        let mut out = String::with_capacity(4096);
        perf::assert_churn_budget(1, name.len() + 7, || {
            black_box(expand_explosion_cmd(
                &table,
                &text,
                &mut ArrayVec::new(),
                &mut 4096,
                &mut out,
            ));
        });
    }
}

fn pairs(mut work: impl FnMut(&str, bool)) {
    if std::env::var("DEADSYNC_PERF_ORDER").as_deref() == Ok("new-first") {
        work("new", true);
        work("old", false);
    } else {
        work("old", false);
        work("new", true);
    }
}

#[test]
#[ignore = "manual paired release CPU, throughput and allocation benchmark"]
fn benchmark_command_preparation() {
    for (name, text) in [
        ("one", "zoom,1"),
        ("rgba", "diffuse,1,0.5,0.25,1"),
        ("nested", "setstateproperties,Sprite.LinearFrames(4,0.5)"),
        ("six", "custom,1,2,3,4,5,6"),
        ("seven", "custom,1,2,3,4,5,6,7"),
        (
            "many",
            "custom,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24",
        ),
    ] {
        pairs(|label, new| {
            perf::measure_sampled(&format!("command_token_{name}_{label}"), 256, 1024, || {
                for _ in 0..1024 {
                    if new {
                        black_box(script::split_script_token(black_box(text)));
                    } else {
                        black_box(baseline_token::split_script_token(black_box(text)));
                    }
                }
            })
        });
    }
    for count in [1, 8, 32] {
        for properties in [false, true] {
            let table = commands(count, properties);
            let kind = if properties { "properties" } else { "delays" };
            pairs(|label, new| {
                perf::measure_sampled(
                    &format!("command_plans_{count}_{kind}_{label}"),
                    4096,
                    count * 2,
                    || {
                        let mut state = (0, 0.0);
                        if new {
                            script::apply_sprite_animation_command_plans(
                                &mut state,
                                black_box(&table),
                                false,
                                consume_plan,
                            );
                        } else {
                            baseline_plans::apply_sprite_animation_command_plans(
                                &mut state,
                                black_box(&table),
                                false,
                                consume_plan,
                            );
                        }
                        black_box(state)
                    },
                )
            });
        }
    }
    for (name, text) in [
        ("none", "zoom,1;linear,0.2;diffusealpha,0"),
        ("one", "setallstatedelays,0.1"),
        (
            "many",
            "setallstatedelays,0.1;setallstatedelays,0.2;setallstatedelays,0.3",
        ),
    ] {
        pairs(|label, new| {
            perf::measure_sampled(&format!("command_script_{name}_{label}"), 8192, 1, || {
                let mut state = (0, 0.0);
                if new {
                    script::apply_sprite_animation_script_plans(
                        &mut state,
                        black_box(text),
                        false,
                        consume_plan,
                    );
                } else {
                    baseline_plans::apply_sprite_animation_script_plans(
                        &mut state,
                        black_box(text),
                        false,
                        consume_plan,
                    );
                }
                black_box(state)
            })
        });
    }
    for (kind, name) in [
        ("short", "Glow".to_string()),
        ("boundary", "X".repeat(121)),
        ("spill", "X".repeat(122)),
        ("unicode", "\u{e9}Glow\u{6771}".to_string()),
        ("long", "X".repeat(512)),
    ] {
        let (table, text) = expansion_fixture(&name, 16);
        pairs(|label, new| {
            perf::measure_sampled(&format!("command_expand_{kind}_{label}"), 2048, 16, || {
                let mut out = String::new();
                let mut budget = 4096;
                let complete = if new {
                    expand_explosion_cmd(
                        black_box(&table),
                        black_box(&text),
                        &mut ArrayVec::new(),
                        &mut budget,
                        &mut out,
                    )
                } else {
                    baseline_explosion::expand_explosion_cmd(
                        black_box(&table),
                        black_box(&text),
                        &mut ArrayVec::new(),
                        &mut budget,
                        &mut out,
                    )
                };
                black_box((complete, budget, out));
            })
        });
    }
}
