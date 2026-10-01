use super::*;
use crate::perf;
use crate::runtime::itg_slot_with_active_model_draw;
use std::hint::black_box;

mod baseline_program {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/model_loading/baseline_program.rs"
    ));
}
mod baseline_runtime {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/model_loading/baseline_runtime.rs"
    ));
}

type Program = (ModelDrawState, Arc<[ModelTweenSegment]>, ModelEffectState);

fn assert_draw_bits(a: ModelDrawState, b: ModelDrawState) {
    let values = |d: ModelDrawState| {
        std::iter::once(d.texture_seconds)
            .chain(d.pos)
            .chain(d.rot)
            .chain(d.zoom)
            .chain(d.tint)
            .chain(d.glow)
            .chain(d.fade)
            .chain(std::iter::once(d.vert_align))
            .map(f32::to_bits)
            .collect::<Vec<_>>()
    };
    assert_eq!(values(a), values(b));
    assert_eq!((a.blend_add, a.visible), (b.blend_add, b.visible));
}

fn assert_program(a: &Program, b: &Program) {
    assert_draw_bits(a.0, b.0);
    assert_eq!(a.1.len(), b.1.len());
    for (a, b) in a.1.iter().zip(b.1.iter()) {
        assert_eq!(a.start.to_bits(), b.start.to_bits());
        assert_eq!(a.duration.to_bits(), b.duration.to_bits());
        assert_eq!(format!("{:?}", a.tween), format!("{:?}", b.tween));
        assert_draw_bits(a.from, b.from);
        assert_draw_bits(a.to, b.to);
    }
    assert_eq!(format!("{:?}", a.2), format!("{:?}", b.2));
    let effect_values = |e: ModelEffectState| {
        e.color1
            .into_iter()
            .chain(e.color2)
            .chain([e.period, e.offset])
            .chain(e.timing)
            .chain(e.magnitude)
            .map(f32::to_bits)
            .collect::<Vec<_>>()
    };
    assert_eq!(effect_values(a.2), effect_values(b.2));
}

fn table(init: &str, active: &str) -> HashMap<String, String> {
    HashMap::from([
        ("initcommand".to_string(), init.to_string()),
        ("nonecommand".to_string(), active.to_string()),
        ("holdingoncommand".to_string(), active.to_string()),
        ("unrelatedcommand".to_string(), "zoom,42".to_string()),
    ])
}

#[test]
fn inline_model_groups_match_parent_bits_and_flush_boundaries() {
    let parts = [
        "",
        "x,1;addx,2;addx,-0.25;y,-3;z,4",
        "zoom,-2;zoomx,-0;zoomy,3",
        "linear,0.2;addx,4;addy,8;sleep,0.1;addz,3",
        "linear,0;zoom,2;linear,-1;zoom,3",
        "linear,0.2;linear,0.3;setallstatedelays,0.1;addx,9;pause;addx,1",
        "zoom,2;setbasezoom,0.5;linear,0.5;zoomx,-4;setbasezoomx,2",
        "addrotationx,30;rotationz,90;diffuse,2,-1,0.5,1;glow,1,0,0,0.3;fadeleft,0.2",
        "linear,0.3;x,9;effectclock,beat;wag;effectmagnitude,1,2,3;effectoffset,0.25",
        "pulse;effectperiod,2;effecttiming,0,0,1,0,1;stopeffect;visible,false;blend,BlendMode_Add",
        "linear,0.1;zoom,2;ztest,true;zwrite,false;customtexturerect,0,0,1,1;rate,2;addy,4",
        "function(self) self:zoom(0.5):linear(0.2):addx(8):rotationz(45) end",
        "x,nan;addx,1;zoomx,inf;diffusealpha,nan;rotationz,-inf",
        "x,bad;linear,bad;effectclock,invalid;unknown,3",
    ];
    for init in parts {
        for active in parts {
            let commands = table(init, active);
            assert_program(
                &baseline_program::model_draw_program(&commands),
                &model_draw_program(&commands),
            );
        }
    }
    let mut seed = 0x1653_u64;
    for _ in 0..512 {
        let mut init = String::new();
        let mut active = String::new();
        for script in [&mut init, &mut active] {
            for _ in 0..16 {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                script.push_str(parts[(seed >> 32) as usize % parts.len()]);
                script.push(';');
            }
        }
        let commands = table(&init, &active);
        assert_program(
            &baseline_program::model_draw_program(&commands),
            &model_draw_program(&commands),
        );
    }
    for count in [1, 4, 8, 9, 32, 1024] {
        let commands = table(
            "addx,1",
            &std::iter::repeat_n("addx,0.25", count)
                .collect::<Vec<_>>()
                .join(";"),
        );
        assert_program(
            &baseline_program::model_draw_program(&commands),
            &model_draw_program(&commands),
        );
    }
}

#[derive(Clone, Default)]
struct Slot {
    marker: usize,
    calls: usize,
    program: Option<Program>,
}

fn apply(
    slot: &mut Slot,
    draw: ModelDrawState,
    timeline: Arc<[ModelTweenSegment]>,
    effect: ModelEffectState,
) {
    slot.calls += 1;
    slot.program = Some((draw, timeline, effect));
}

#[test]
fn borrowed_active_commands_match_parent_aliases_missing_keys_and_callbacks() {
    for init in ["", "zoom,0.5", "linear,0.2;addx,3"] {
        for active in ["", "addx,8", "linear,0.5;zoom,2;wag;effectclock,beat"] {
            let commands = table(init, active);
            for mask in 0..4 {
                let mut commands = commands.clone();
                if mask & 1 != 0 {
                    commands.remove("initcommand");
                }
                if mask & 2 != 0 {
                    commands.remove("holdingoncommand");
                }
                for key in [
                    "holdingoncommand",
                    "initcommand",
                    "nonecommand",
                    "unrelatedcommand",
                    "missing",
                    "",
                ] {
                    let input = Slot {
                        marker: 42,
                        calls: 3,
                        program: None,
                    };
                    let old = baseline_runtime::itg_slot_with_active_model_draw(
                        &input, &commands, key, apply,
                    );
                    let new = itg_slot_with_active_model_draw(&input, &commands, key, apply);
                    assert_eq!((old.marker, old.calls), (new.marker, new.calls));
                    assert_eq!((new.marker, new.calls), (42, 4));
                    assert!(input.program.is_none());
                    assert_eq!(input.calls, 3);
                    assert_program(old.program.as_ref().unwrap(), new.program.as_ref().unwrap());
                    let owned = baseline_program::itg_active_model_commands(&commands, key);
                    assert_program(
                        &baseline_program::model_draw_program(&owned),
                        new.program.as_ref().unwrap(),
                    );
                }
            }
        }
    }
}

#[test]
fn model_modifier_groups_fit_inline_and_oversized_groups_grow_less() {
    for count in 1..=8 {
        let commands = table(
            "",
            &std::iter::repeat_n("addx,0.25", count)
                .collect::<Vec<_>>()
                .join(";"),
        );
        perf::assert_churn_budget(1, 16, || {
            black_box(model_draw_program(&commands));
        });
        perf::assert_reduced_churn(
            || {
                black_box(baseline_program::model_draw_program(&commands));
            },
            || {
                black_box(model_draw_program(&commands));
            },
        );
    }
    for count in [9, 16, 32, 1024] {
        let commands = table(
            "",
            &std::iter::repeat_n("addx,0.25", count)
                .collect::<Vec<_>>()
                .join(";"),
        );
        perf::assert_reduced_growth(
            || {
                black_box(baseline_program::model_draw_program(&commands));
            },
            || {
                black_box(model_draw_program(&commands));
            },
        );
    }
    let commands = table("zoom,0.5", "linear,0.2;addx,3;addy,4;sleep,0.1;zoom,2");
    perf::assert_reduced_churn(
        || {
            black_box(baseline_program::model_draw_program(&commands));
        },
        || {
            black_box(model_draw_program(&commands));
        },
    );
}

#[test]
fn borrowed_active_selection_removes_map_and_string_churn() {
    for commands in [
        table("zoom,0.5", "linear,0.2;zoom,2"),
        table("", ""),
        HashMap::from([("holdingoncommand".to_string(), "addx,3".to_string())]),
    ] {
        perf::assert_reduced_churn(
            || {
                black_box(baseline_runtime::itg_slot_with_active_model_draw(
                    &Slot::default(),
                    &commands,
                    "holdingoncommand",
                    apply,
                ));
            },
            || {
                black_box(itg_slot_with_active_model_draw(
                    &Slot::default(),
                    &commands,
                    "holdingoncommand",
                    apply,
                ));
            },
        );
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
fn benchmark_model_loading() {
    for (kind, script) in [
        ("empty", String::new()),
        ("one", "addx,0.25".to_string()),
        (
            "eight",
            std::iter::repeat_n("addx,0.25", 8)
                .collect::<Vec<_>>()
                .join(";"),
        ),
        (
            "many",
            std::iter::repeat_n("addx,0.25", 256)
                .collect::<Vec<_>>()
                .join(";"),
        ),
        (
            "tween",
            "linear,0.2;addx,3;addy,4;zoom,0.5;sleep,0.1;zoom,2".to_string(),
        ),
        (
            "timeline",
            std::iter::repeat_n("linear,0.2;addx,3;addy,4;zoom,0.5;sleep,0.1", 16)
                .collect::<Vec<_>>()
                .join(";"),
        ),
        (
            "lua",
            "function(self) self:zoom(0.5):linear(0.2):addx(8):rotationz(45) end".to_string(),
        ),
    ] {
        let commands = table("", &script);
        pairs(|label, new| {
            perf::measure_sampled(
                &format!("load_model_{kind}_{label}"),
                if kind == "many" || kind == "timeline" {
                    2048
                } else {
                    16384
                },
                1,
                || {
                    if new {
                        black_box(model_draw_program(black_box(&commands)));
                    } else {
                        black_box(baseline_program::model_draw_program(black_box(&commands)));
                    }
                },
            )
        });
    }
    for (kind, commands, key) in [
        ("empty", HashMap::new(), "holdingoncommand"),
        (
            "short",
            table("zoom,0.5", "linear,0.2;zoom,2"),
            "holdingoncommand",
        ),
        (
            "long",
            table(
                &std::iter::repeat_n("addx,0.25", 64)
                    .collect::<Vec<_>>()
                    .join(";"),
                &std::iter::repeat_n("linear,0.2;zoom,2", 32)
                    .collect::<Vec<_>>()
                    .join(";"),
            ),
            "holdingoncommand",
        ),
        (
            "alias",
            table("addx,1;zoom,0.5", "linear,0.2;zoom,2"),
            "initcommand",
        ),
        ("missing", table("zoom,0.5", "linear,0.2;zoom,2"), "missing"),
    ] {
        let slot = Slot::default();
        pairs(|label, new| {
            perf::measure_sampled(
                &format!("load_active_{kind}_{label}"),
                if kind == "long" { 2048 } else { 16384 },
                1,
                || {
                    if new {
                        black_box(itg_slot_with_active_model_draw(
                            black_box(&slot),
                            black_box(&commands),
                            black_box(key),
                            apply,
                        ));
                    } else {
                        black_box(baseline_runtime::itg_slot_with_active_model_draw(
                            black_box(&slot),
                            black_box(&commands),
                            black_box(key),
                            apply,
                        ));
                    }
                },
            )
        });
    }
}
