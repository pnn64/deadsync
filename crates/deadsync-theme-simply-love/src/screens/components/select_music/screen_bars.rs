use crate::act;
use crate::assets::i18n::tr;
use crate::assets::{FontRole, machine_font_key};
use crate::config::MachineFont;
use crate::screens::components::shared::screen_bar::{
    self, AvatarParams, ScreenBarParams, ScreenBarPosition, ScreenBarTitlePlacement,
};
use deadlib_present::actors::{Actor, SizeSpec};
use deadlib_present::space::screen_center_x;
use std::cell::RefCell;
use std::sync::Arc;

#[derive(Clone, Copy)]
pub struct Player<'a> {
    pub joined: bool,
    pub guest: bool,
    pub display_name: &'a str,
    pub avatar_texture_key: Option<&'a str>,
}

pub fn push(
    out: &mut Vec<Actor>,
    top_title: &str,
    players: [Player<'_>; 2],
    visual_policy: crate::views::SimplyLoveVisualPolicyView,
) {
    let [p1, p2] = players;
    let p1_avatar = p1
        .avatar_texture_key
        .map(|texture_key| AvatarParams { texture_key });
    let p2_avatar = p2
        .avatar_texture_key
        .map(|texture_key| AvatarParams { texture_key });

    let insert_card = tr("Common", "InsertCard");
    let press_start = tr("Common", "PressStart");
    let event_mode = tr("Common", "EventMode");

    let (footer_left, left_avatar) = if p1.joined {
        (
            Some(if p1.guest {
                insert_card.as_ref()
            } else {
                p1.display_name
            }),
            if p1.guest { None } else { p1_avatar },
        )
    } else {
        (Some(press_start.as_ref()), None)
    };
    let (footer_right, right_avatar) = if p2.joined {
        (
            Some(if p2.guest {
                insert_card.as_ref()
            } else {
                p2.display_name
            }),
            if p2.guest { None } else { p2_avatar },
        )
    } else {
        (Some(press_start.as_ref()), None)
    };

    out.push(screen_bar::build_select_music(ScreenBarParams {
        title: top_title,
        title_placement: ScreenBarTitlePlacement::Left,
        position: ScreenBarPosition::Top,
        transparent: false,
        fg_color: [1.0; 4],
        left_text: None,
        center_text: None,
        right_text: None,
        left_avatar: None,
        right_avatar: None,
        visual_policy,
    }));
    out.push(screen_bar::build_select_music(ScreenBarParams {
        title: event_mode.as_ref(),
        title_placement: ScreenBarTitlePlacement::Center,
        position: ScreenBarPosition::Bottom,
        transparent: false,
        fg_color: [1.0; 4],
        left_text: footer_left,
        center_text: None,
        right_text: footer_right,
        left_avatar,
        right_avatar,
        visual_policy,
    }));
}

pub fn build_stage_display(stage_number: usize, machine_font: MachineFont) -> Actor {
    STAGE_DISPLAY_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        let screen_center_x_bits = screen_center_x().to_bits();
        if let Some(cached) = cache.as_ref()
            && cached.stage_number == stage_number
            && cached.machine_font == machine_font
            && cached.screen_center_x_bits == screen_center_x_bits
        {
            return cached.actor.clone();
        }

        let actor = shared_stage_display(build_stage_display_uncached(stage_number, machine_font));
        *cache = Some(CachedStageDisplay {
            stage_number,
            machine_font,
            screen_center_x_bits,
            actor: actor.clone(),
        });
        actor
    })
}

struct CachedStageDisplay {
    stage_number: usize,
    machine_font: MachineFont,
    screen_center_x_bits: u32,
    actor: Actor,
}

thread_local! {
    static STAGE_DISPLAY_CACHE: RefCell<Option<CachedStageDisplay>> = const { RefCell::new(None) };
}

fn build_stage_display_uncached(stage_number: usize, machine_font: MachineFont) -> Actor {
    let text = format!("Stage {stage_number}");
    Actor::Frame {
        align: [0.0, 0.0],
        offset: [screen_center_x(), 40.0],
        size: [SizeSpec::Px(0.0), SizeSpec::Px(0.0)],
        background: None,
        z: 124,
        children: vec![
            act!(quad:
                align(0.5, 0.5):
                xy(0.0, 0.0):
                zoomto(75.0, 22.5):
                diffuse(0.0, 0.0, 0.0, 0.4):
                fadeleft(0.2):
                faderight(0.2):
                z(0)
            ),
            act!(text:
                font(machine_font_key(machine_font, FontRole::Normal)):
                settext(text):
                align(0.5, 0.5):
                xy(0.0, -0.75):
                zoom(0.75):
                shadowlength(0.75):
                strokecolor(0.0, 0.0, 0.0, 1.0):
                diffuse(1.0, 1.0, 1.0, 1.0):
                z(1):
                horizalign(center)
            ),
        ],
    }
}

fn shared_stage_display(actor: Actor) -> Actor {
    let Actor::Frame {
        align,
        offset,
        size,
        children,
        background,
        z,
    } = actor
    else {
        unreachable!("stage display builder always returns a frame");
    };
    Actor::SharedFrame {
        align,
        offset,
        size,
        children: Arc::from(children),
        background,
        z,
        tint: [1.0; 4],
        blend: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cached_stage_display_matches_legacy_children_and_refreshes_stage() {
        let machine_font = MachineFont::default();
        let Actor::Frame {
            children: legacy_children,
            ..
        } = build_stage_display_uncached(17, machine_font)
        else {
            panic!("legacy stage display should be a frame");
        };
        let Actor::SharedFrame {
            children: cached_children,
            ..
        } = build_stage_display(17, machine_font)
        else {
            panic!("cached stage display should be shared");
        };
        let Actor::SharedFrame {
            children: repeated_children,
            ..
        } = build_stage_display(17, machine_font)
        else {
            panic!("repeated stage display should be shared");
        };
        assert_eq!(
            format!("{legacy_children:?}"),
            format!("{:?}", cached_children.as_ref())
        );
        assert!(Arc::ptr_eq(&cached_children, &repeated_children));

        let Actor::SharedFrame {
            children: changed_children,
            ..
        } = build_stage_display(18, machine_font)
        else {
            panic!("changed stage display should be shared");
        };
        assert!(!Arc::ptr_eq(&cached_children, &changed_children));
    }
}
