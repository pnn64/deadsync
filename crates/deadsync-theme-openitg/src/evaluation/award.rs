use std::sync::{Arc, LazyLock};

use deadlib_present::actors::{Actor, TextContent};
use deadlib_present::render::{MeshSampler, SamplerFilter, SamplerWrap, TexturedMeshVertex};
use deadsync_rules::timing::WindowCounts;
use deadsync_theme::views::EvaluationPlayerOverview;
use glam::{Mat4, Vec3};

use super::grade::{Scene, read_indexed_mesh};
use crate::presentation::Layout;
use crate::resources::MENU_NORMAL_FONT;

#[repr(usize)]
#[derive(Clone, Copy)]
enum Texture {
    FlagGreen,
    FlagOrange,
    FlagPole,
    FlagReflection,
    MetalReflection,
    RibbonHeadBlue,
    RibbonHeadGreen,
    RibbonHeadOrange,
    RibbonHeadPink,
    RibbonHeadPurple,
    RibbonHeadRed,
    RibbonTailBlue,
    RibbonTailGreen,
    RibbonTailOrange,
    RibbonTailPink,
    RibbonTailPurple,
    RibbonTailRed,
    MetalBlue,
    MetalBronze,
    MetalGold,
    MetalGreen,
    MetalSilver,
    Wood,
    YouRock,
}

static TEXTURES: LazyLock<[Arc<str>; Texture::YouRock as usize + 1]> = LazyLock::new(|| {
    [
        "flag-green",
        "flag-orange",
        "flag-pole",
        "flag-sphere-add",
        "metal-sphere-add",
        "ribbon-head-blue",
        "ribbon-head-green",
        "ribbon-head-orange",
        "ribbon-head-pink",
        "ribbon-head-purple",
        "ribbon-head-red",
        "ribbon-tail-blue",
        "ribbon-tail-green",
        "ribbon-tail-orange",
        "ribbon-tail-pink",
        "ribbon-tail-purple",
        "ribbon-tail-red",
        "sphere-blue",
        "sphere-bronze",
        "sphere-gold",
        "sphere-green",
        "sphere-silver",
        "wood",
        "you-rock",
    ]
    .map(|name| Arc::from(format!("openitg/evaluation-award-{name}.png")))
});

#[repr(usize)]
#[derive(Clone, Copy)]
enum Part {
    FlagPole,
    FlagFabric,
    Itg,
    PlaqueFace,
    PlaqueWood,
    PlaqueEdge,
    PlaqueDecal,
    RibbonTail,
    RibbonHead,
    Trophy,
}

static MODELS: LazyLock<[Arc<[TexturedMeshVertex]>; Part::Trophy as usize + 1]> =
    LazyLock::new(|| {
        let mut bytes = include_bytes!("../../assets/evaluation-award-models.bin").as_slice();
        // For each submesh in the export, select regular UVs, additive sphere
        // mapping only, or sphere mapping for both material layers.
        let models = [0.0, 6.0, 7.0, 7.0, 0.0, 7.0, 0.0, 0.0, 0.0, 7.0]
            .map(|texture_mode| read_indexed_mesh(&mut bytes, texture_mode));
        assert!(bytes.is_empty());
        models
    });

#[derive(Clone, Copy)]
enum Model {
    Flag(Texture),
    Itg(Texture),
    Plaque(Texture),
    Ribbon { head: Texture, tail: Texture },
    Trophy(Texture),
}

struct Award {
    label: &'static str,
    model: Model,
}

use Model::{Flag, Itg, Plaque, Ribbon, Trophy};
use Texture::*;

const FULL_GREAT: Award = Award {
    label: "Full Great Combo",
    model: Ribbon {
        head: RibbonHeadBlue,
        tail: RibbonTailBlue,
    },
};
const SINGLE_GREAT: Award = Award {
    label: "Single Digit Greats",
    model: Ribbon {
        head: RibbonHeadPink,
        tail: RibbonTailPink,
    },
};
const FULL_EXCELLENT: Award = Award {
    label: "Full Excellent Combo",
    model: Plaque(MetalBronze),
};
const SINGLE_EXCELLENT: Award = Award {
    label: "Single Digit Excellents",
    model: Plaque(MetalSilver),
};
const FULL_FANTASTIC: Award = Award {
    label: "Full Fantastic Combo",
    model: Trophy(MetalBronze),
};
const ONE_GREAT: Award = Award {
    label: "One Great",
    model: Flag(FlagGreen),
};
const ONE_EXCELLENT: Award = Award {
    label: "One Excellent",
    model: Flag(FlagOrange),
};
const GREAT_80: Award = Award {
    label: "80% Greats",
    model: Ribbon {
        head: RibbonHeadGreen,
        tail: RibbonTailGreen,
    },
};
const GREAT_90: Award = Award {
    label: "90% Greats",
    model: Plaque(MetalGreen),
};
const GREAT_100: Award = Award {
    label: "100% Greats",
    model: Trophy(MetalGreen),
};

const PEAK: [Award; 10] = [
    Award {
        label: "1000 Peak Combo",
        model: Ribbon {
            head: RibbonHeadPurple,
            tail: RibbonTailPurple,
        },
    },
    Award {
        label: "2000 Peak Combo",
        model: Ribbon {
            head: RibbonHeadRed,
            tail: RibbonTailRed,
        },
    },
    Award {
        label: "3000 Peak Combo",
        model: Ribbon {
            head: RibbonHeadOrange,
            tail: RibbonTailOrange,
        },
    },
    Award {
        label: "4000 Peak Combo",
        model: Plaque(MetalSilver),
    },
    Award {
        label: "5000 Peak Combo",
        model: Plaque(MetalGold),
    },
    Award {
        label: "6000 Peak Combo",
        model: Trophy(MetalSilver),
    },
    Award {
        label: "7000 Peak Combo",
        model: Trophy(MetalGold),
    },
    Award {
        label: "8000 Peak Combo",
        model: Itg(MetalSilver),
    },
    Award {
        label: "9000 Peak Combo",
        model: Itg(MetalGold),
    },
    Award {
        label: "10000 Peak Combo",
        model: Itg(MetalBlue),
    },
];

fn difficulty_award(counts: WindowCounts, dropped_hold_or_roll: bool) -> Option<&'static Award> {
    let fantastic = u64::from(counts.w0) + u64::from(counts.w1);
    let excellent = u64::from(counts.w2);
    let great = u64::from(counts.w3);
    let worse = u64::from(counts.w4) + u64::from(counts.w5) + u64::from(counts.miss);
    let total = fantastic + excellent + great + worse;
    if total == 0 {
        return None;
    }
    // OpenITG retains the last matching award. Great percentages win even
    // when the stage included lower judgments or a dropped hold.
    let award = if great == total {
        &GREAT_100
    } else if great * 100 >= total * 90 {
        &GREAT_90
    } else if great * 100 >= total * 80 {
        &GREAT_80
    } else if worse > 0 || dropped_hold_or_roll {
        return None;
    } else {
        match (great, excellent) {
            (0, 0) => &FULL_FANTASTIC,
            (0, 1) => &ONE_EXCELLENT,
            (0, 2..=9) => &SINGLE_EXCELLENT,
            (0, _) => &FULL_EXCELLENT,
            (1, _) => &ONE_GREAT,
            (2..=9, _) => &SINGLE_GREAT,
            _ => &FULL_GREAT,
        }
    };
    Some(award)
}

fn peak_award(max_combo: u32, carried: u32) -> Option<&'static Award> {
    let peak = (max_combo / 1000).min(PEAK.len() as u32);
    (carried / 1000 < peak).then(|| &PEAK[(peak - 1) as usize])
}

pub(super) fn push_awards(
    actors: &mut Vec<Actor>,
    layout: Layout,
    panel_x: f32,
    player: &EvaluationPlayerOverview<'_>,
    elapsed: f32,
) {
    if player.disqualified {
        return;
    }
    if player.score_valid
        && let Some(award) = difficulty_award(player.row_window_counts, player.dropped_hold_or_roll)
    {
        push_award(actors, layout, panel_x + 271.0, award, true, elapsed);
    }
    let carried = player.combo_runs.first().map_or(0, |run| run.carried);
    if let Some(award) = peak_award(player.max_combo, carried) {
        push_award(actors, layout, panel_x + 1.0, award, false, elapsed);
    }
}

fn push_award(
    actors: &mut Vec<Actor>,
    layout: Layout,
    x: f32,
    award: &Award,
    right_aligned: bool,
    elapsed: f32,
) {
    let outward = if right_aligned { -1.0 } else { 1.0 };
    let wag = (elapsed * (std::f32::consts::TAU / 8.0)).sin();
    let spin = elapsed * 40.0;
    let (scale, rotation, dy) = match award.model {
        Flag(_) => (Vec3::splat(0.8), 20.0 * wag, -7.0),
        Itg(_) => (Vec3::new(1.2, 0.8, 1.0), spin - 90.0, 0.0),
        Plaque(_) => (Vec3::splat(1.6), 20.0 * wag, 0.0),
        Ribbon { .. } => (Vec3::splat(0.6), 15.0 * wag, 0.0),
        Trophy(_) => (Vec3::splat(4.6), spin, 0.0),
    };
    let transform = Mat4::from_rotation_y((rotation - 15.0 * outward).to_radians())
        * Mat4::from_scale(scale * layout.size([0.35, 0.0])[0]);
    let mut scene = Scene::orthographic(layout, [x + outward * 30.0, 265.0 + dy], transform, 17);
    scene.push(actors, |scene, actors| {
        let mut push = |part: Part, texture: Texture, additive: Option<Texture>| {
            actors.push(scene.mesh(
                &MODELS[part as usize],
                &TEXTURES[texture as usize],
                additive.map(|key| &TEXTURES[key as usize]),
                Some(MeshSampler::Override {
                    filter: SamplerFilter::Linear,
                    wrap: SamplerWrap::Repeat,
                }),
                0.0,
            ));
        };
        match award.model {
            Flag(texture) => {
                push(Part::FlagPole, FlagPole, None);
                push(Part::FlagFabric, texture, Some(FlagReflection));
            }
            Itg(texture) => push(Part::Itg, texture, Some(MetalReflection)),
            Plaque(texture) => {
                push(Part::PlaqueFace, texture, Some(MetalReflection));
                push(Part::PlaqueWood, Wood, None);
                push(Part::PlaqueEdge, texture, Some(MetalReflection));
                push(Part::PlaqueDecal, YouRock, None);
            }
            Ribbon { head, tail } => {
                push(Part::RibbonTail, tail, None);
                push(Part::RibbonHead, head, None);
            }
            Trophy(texture) => push(Part::Trophy, texture, Some(MetalReflection)),
        }
    });
    let alignment = if right_aligned { 1.0 } else { 0.0 };
    let mut label = layout.text(
        [x, 305.0],
        alignment,
        TextContent::Static(award.label),
        0.5,
        [1.0; 4],
    );
    label.font(MENU_NORMAL_FONT);
    label.maxwidth(layout.size([125.0, 0.0])[0]);
    label.shadowlength(layout.size([1.0, 0.0])[0]);
    actors.push(label.build(0));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn award_precedence_uses_judged_rows_and_dropped_holds() {
        let counts = WindowCounts {
            w0: 70,
            w1: 20,
            w2: 1,
            ..Default::default()
        };
        assert_eq!(
            difficulty_award(counts, false).map(|award| award.label),
            Some("One Excellent")
        );
        assert!(difficulty_award(counts, true).is_none());
        let counts = WindowCounts {
            w3: 9,
            miss: 1,
            ..Default::default()
        };
        assert_eq!(
            difficulty_award(counts, true).map(|award| award.label),
            Some("90% Greats")
        );
        assert!(difficulty_award(WindowCounts::default(), false).is_none());
    }

    #[test]
    fn peak_award_requires_a_new_threshold_even_at_the_cap() {
        assert!(peak_award(1999, 1001).is_none());
        assert_eq!(
            peak_award(8158, 1999).map(|award| award.label),
            Some("8000 Peak Combo")
        );
        assert_eq!(
            peak_award(12_000, 9999).map(|award| award.label),
            Some("10000 Peak Combo")
        );
        assert!(peak_award(12_000, 10_000).is_none());
    }
}
