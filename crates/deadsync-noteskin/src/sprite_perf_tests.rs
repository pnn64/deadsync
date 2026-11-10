use super::*;
use crate::perf as alloc;
use crate::script::SpriteStatePropertiesPlan;


fn slot(animated: bool) -> SpriteSlotPlan {
    let key = "noteskin/example/tap-note 8x8.png".to_owned();
    SpriteSlotPlan {
        def: SpriteDefinition {
            src: [1, 2],
            size: [24, 32],
            ..Default::default()
        },
        source_size: [96, 128],
        source: if animated {
            SpriteSourcePlan::Animated {
                texture_key: key,
                tex_dims: (1024, 1024),
                frame_size: [128, 128],
                grid: (8, 8),
                frame_count: 3,
                frame_indices: Some(vec![2, 1, 0]),
                rate: AnimationRate::FramesPerBeat(3.0),
                frame_durations: Some(vec![0.1, 0.2, 0.3]),
            }
        } else {
            SpriteSourcePlan::Atlas {
                texture_key: key,
                tex_dims: (1024, 1024),
            }
        },
        note_color_translate: true,
    }
}


#[test]
fn state_properties_reuse_texture_and_delay_allocations() {
    let slot = slot(false);
    let SpriteSourcePlan::Atlas { texture_key, .. } = &slot.source else {
        unreachable!()
    };
    let key_pointer = texture_key.as_ptr();
    let delays = vec![0.125; 64];
    let delay_pointer = delays.as_ptr();
    let command = SpriteAnimationCommandPlan::StateProperties(SpriteStatePropertiesPlan {
        frame_count: 64,
        frame_delays: delays,
    });
    let (result, churn) = alloc::measure(|| {
        itg_sprite_animation_slot_plan(slot, command, false, |_| (8, 8), |_, _, _| (128, 128))
    });
    assert_eq!(
        churn,
        alloc::Churn::default(),
        "expected no allocation churn"
    );
    let SpriteSourcePlan::Animated {
        texture_key,
        frame_durations: Some(durations),
        ..
    } = result.unwrap().source
    else {
        unreachable!()
    };
    assert_eq!(texture_key.as_ptr(), key_pointer);
    assert_eq!(durations.as_ptr(), delay_pointer);
}
