use crate::assets::i18n::tr;
use crate::screens::components::shared::screen_bar::{
    self, AvatarParams, ScreenBarParams, ScreenBarPosition, ScreenBarTitlePlacement,
};
use crate::views::SelectFlowPlayerView;
use deadlib_present::actors::Actor;

fn player_params<'a>(
    player: &'a SelectFlowPlayerView,
    insert_card: &'a str,
    press_start: &'a str,
) -> (Option<&'a str>, Option<AvatarParams<'a>>) {
    if !player.joined {
        return (Some(press_start), None);
    }
    if player.guest {
        return (Some(insert_card), None);
    }
    (
        Some(player.display_name.as_str()),
        player
            .avatar_texture_key
            .as_deref()
            .map(|texture_key| AvatarParams { texture_key }),
    )
}

pub fn push(
    actors: &mut Vec<Actor>,
    players: &[SelectFlowPlayerView; 2],
    visual_policy: crate::views::SimplyLoveVisualPolicyView,
) {
    let insert_card = tr("Common", "InsertCard");
    let press_start = tr("Common", "PressStart");
    let (left_text, left_avatar) =
        player_params(&players[0], insert_card.as_ref(), press_start.as_ref());
    let (right_text, right_avatar) =
        player_params(&players[1], insert_card.as_ref(), press_start.as_ref());
    let event_mode = tr("Common", "EventMode");
    actors.push(screen_bar::build(ScreenBarParams {
        title: &event_mode,
        title_placement: ScreenBarTitlePlacement::Center,
        position: ScreenBarPosition::Bottom,
        transparent: false,
        left_text,
        center_text: None,
        right_text,
        left_avatar,
        right_avatar,
        fg_color: [1.0; 4],
        visual_policy,
    }));
}
