//! Presentation compatible with Martin Natano's Simply Love Twitch module.
//! Original copyright and permission notice: deadsync-online/src/twitch-LICENSE.
use crate::{act, color, screens::flow::SimplyLoveScreen as Screen};
use deadlib_assets::AssetManager;
use deadlib_present::{
    actors::{Actor, DynamicText, TextAttribute, TextAttributes, TextContent},
    font,
};
use deadsync_config::streaming::TwitchOptions;
use deadsync_online::twitch::{ChatMessage, Entry};
use std::sync::Arc;

const ZOOM: f32 = 0.75;
const PADDING: f32 = 5.0;
const LINE_HEIGHT: i32 = 24;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct GameplayLayout {
    pub two_players: bool,
    pub p2: bool,
    pub centered: bool,
    pub field_left: f32,
    pub field_right: f32,
    pub statistics: bool,
}

/// Logical coordinates follow the original module, constrained to free field space.
pub fn placement(
    screen: Screen,
    w: f32,
    h: f32,
    sides: [bool; 2],
    gameplay: Option<GameplayLayout>,
    tournament: bool,
) -> Option<Rect> {
    let cx = w * 0.5;
    let cy = h * 0.5;
    let p2 = sides[1] && !sides[0];
    let both = sides[0] && sides[1];
    let (x, y, rw, rh) = match screen {
        Screen::Menu => (cx + 160.0, 20.0, cx - 180.0, h - 54.0),
        Screen::Options
        | Screen::PlayerOptions
        | Screen::Mappings
        | Screen::ManageLocalProfiles => (10.0, h - 72.0, w - 20.0, 64.0),
        Screen::Input => (10.0, h - 110.0, w - 20.0, 100.0),
        Screen::SelectProfile
        | Screen::SelectColor
        | Screen::SelectStyle
        | Screen::SelectPlayMode => (10.0, 40.0, w - 20.0, 82.0),
        Screen::SelectMusic | Screen::SelectCourse => {
            if w / h > 1.5 {
                (cx - 330.0, 33.0, 320.0, 126.0)
            } else {
                (cx - 323.0, 34.0, 314.0, 124.0)
            }
        }
        Screen::Gameplay => {
            let g = gameplay?;
            if g.two_players || (g.centered && g.statistics && w / h <= 1.5) {
                return None;
            }
            let right = if g.centered {
                !g.p2 ^ (g.statistics || tournament)
            } else {
                !g.p2
            };
            let (mut x, y, mut rw, rh) = if g.centered {
                if right {
                    (
                        g.field_right + 10.0,
                        90.0,
                        w - g.field_right - 20.0,
                        h - 100.0,
                    )
                } else {
                    (10.0, 90.0, g.field_left - 20.0, h - 100.0)
                }
            } else if g.statistics {
                (
                    if right { cx + 105.0 } else { 0.0 },
                    cy + 10.0,
                    cx - 105.0,
                    105.0,
                )
            } else {
                (
                    if right { cx + 50.0 } else { 10.0 },
                    90.0,
                    cx - 60.0,
                    h - 100.0,
                )
            };
            if right {
                let edge = x.max(g.field_right + 10.0);
                rw -= edge - x;
                x = edge;
            } else {
                rw = rw.min(g.field_left - 10.0 - x);
            }
            if rw < 80.0 {
                return None;
            }
            (x, y, rw, rh)
        }
        Screen::Evaluation if !both => {
            (if p2 { 10.0 } else { cx + 156.0 }, 38.0, cx - 166.0, 143.0)
        }
        Screen::EvaluationSummary if !both => {
            (if p2 { 10.0 } else { cx + 80.0 }, 42.0, cx - 90.0, h - 52.0)
        }
        Screen::Initials if !both => (
            if p2 { 10.0 } else { cx + 10.0 },
            cy - 20.0 - h / 14.0,
            cx - 20.0,
            cy + h / 14.0 + 10.0,
        ),
        _ => return None,
    };
    let x = x.max(0.0);
    let rw = rw.min(w - x);
    (rw > 10.0 && rh > 10.0).then_some(Rect { x, y, w: rw, h: rh })
}

fn badge(name: &str) -> Option<&'static str> {
    Some(match name.split('/').next()? {
        "artist" => "🖌",
        "broadcaster" => "🎥",
        "moderator" => "🗡",
        "no_audio" => "🔇",
        "no_video" => "👁",
        "subscriber" => "💃",
        "vip" => "💎",
        _ => return None,
    })
}

fn name_color(chat: &ChatMessage) -> [f32; 4] {
    if let Some(raw) = chat.color.as_deref().filter(|s| {
        s.len() == 7 && s.starts_with('#') && s[1..].bytes().all(|b| b.is_ascii_hexdigit())
    }) {
        return deadlib_present::color::rgba_hex(raw);
    }
    let hash = chat.login.bytes().fold(5381_u32, |hash, b| {
        hash.wrapping_mul(33).wrapping_add(u32::from(b))
    });
    color::DECORATIVE_RGBA[hash as usize % color::DECORATIVE_RGBA.len()]
}

fn attribute(start: usize, length: usize, color: [f32; 4]) -> TextAttribute {
    TextAttribute {
        start,
        length,
        color,
        vertex_colors: None,
        glow: None,
    }
}

fn format_entry(entry: &Entry, options: TwitchOptions) -> (String, Vec<TextAttribute>) {
    match entry {
        Entry::Chat(chat) => {
            let mut text = String::new();
            if options.show_badges {
                if chat.announcement {
                    text.push_str("📣");
                }
                for name in &chat.badges {
                    if let Some(icon) = badge(name) {
                        text.push_str(icon);
                    }
                }
                if !text.is_empty() {
                    text.push(' ');
                }
            }
            let start = text.chars().count();
            text.push_str(&chat.display_name);
            let name_len = text.chars().count() - start;
            text.push_str(": ");
            // The reference replaces whitespace-delimited tokens, including its
            // custom SL aliases. Unknown emotes retain their original spelling.
            for (i, word) in chat.text.split_whitespace().enumerate() {
                if i > 0 {
                    text.push(' ');
                }
                text.push_str(if options.show_emotes {
                    emote(word).unwrap_or(word)
                } else {
                    word
                });
            }
            (text, vec![attribute(start, name_len, name_color(chat))])
        }
        Entry::Status { text, notice } => {
            let icon = if options.show_badges {
                match notice.as_deref() {
                    Some("subgift" | "submysterygift") => "🎁 ",
                    Some("sub" | "resub") => "🎉 ",
                    _ => "",
                }
            } else {
                ""
            };
            let text = format!("{icon}{text}");
            let attrs = vec![attribute(
                0,
                text.chars().count(),
                [0.667, 0.667, 0.667, 1.0],
            )];
            (text, attrs)
        }
        Entry::Error(text) => (
            text.clone(),
            vec![attribute(0, text.chars().count(), [1.0, 0.0, 0.0, 1.0])],
        ),
    }
}

fn wrapped_lines(text: &str, width: i32, font: &font::Font, fonts: &font::FontMap) -> usize {
    let space = font::measure_line_width_logical(font, " ", fonts);
    text.split('\n')
        .map(|line| {
            let mut lines = 1;
            let mut used = 0;
            for word in line.split(' ').filter(|word| !word.is_empty()) {
                let word_width = font::measure_line_width_logical(font, word, fonts);
                if used > 0 && used + space + word_width > width {
                    lines += 1;
                    used = word_width;
                } else {
                    used += if used > 0 {
                        space + word_width
                    } else {
                        word_width
                    };
                }
            }
            lines
        })
        .sum()
}

#[derive(Default)]
pub struct Overlay {
    key: Option<(u64, Rect, bool, bool)>,
    text: Option<Actor>,
}

impl Overlay {
    pub fn clear(&mut self) {
        self.key = None;
        self.text = None;
    }

    pub fn push(
        &mut self,
        actors: &mut Vec<Actor>,
        entries: &[Arc<Entry>],
        revision: u64,
        rect: Rect,
        options: TwitchOptions,
        assets: &AssetManager,
    ) {
        let key = (revision, rect, options.show_badges, options.show_emotes);
        if self.key != Some(key) {
            self.key = Some(key);
            self.text = None;
            assets.with_fonts(|fonts| {
                let Some(font) = fonts.get("miso") else {
                    self.key = None;
                    return;
                };
                let width = ((rect.w - 2.0 * PADDING) / ZOOM).floor() as i32;
                let max_lines = ((rect.h - 2.0 * PADDING) / ZOOM / LINE_HEIGHT as f32).floor() as usize;
                let mut visible = Vec::new();
                let mut count = 0;
                for entry in entries.iter().rev().take(deadsync_online::twitch::MAX_MESSAGES) {
                    let (text, attributes) = format_entry(entry, options);
                    let lines = wrapped_lines(&text, width, font, fonts);
                    if count + lines > max_lines {
                        break;
                    }
                    count += lines;
                    visible.push((text, attributes));
                }
                if visible.is_empty() {
                    return;
                }
                let mut text = String::new();
                let mut attributes = Vec::new();
                let mut chars = 0;
                for (line, attrs) in visible.into_iter().rev() {
                    if !text.is_empty() {
                        text.push('\n');
                        chars += 1;
                    }
                    attributes.extend(attrs.into_iter().map(|mut attr| {
                        attr.start += chars;
                        attr
                    }));
                    chars += line.chars().count();
                    text.push_str(&line);
                }
                let extra_padding = ((rect.h - 2.0 * PADDING) - max_lines as f32 * LINE_HEIGHT as f32 * ZOOM) * 0.5;
                let y = rect.y + rect.h - PADDING - extra_padding - (LINE_HEIGHT - 19) as f32 * ZOOM;
                let mut actor = act!(text:
                    align(0.0, 1.0): xy(rect.x + PADDING, y): font("miso"):
                    settext(""): zoom(ZOOM): wrapwidthpixels(width): z(901)
                );
                if let Actor::Text { content, attributes: attrs, clip, line_spacing, .. } = &mut actor {
                    // Bypass theme marker expansion: chat is literal user text.
                    *content = TextContent::Dynamic(Arc::new(DynamicText { text, slot: 0 }));
                    *attrs = TextAttributes::Shared(attributes.into());
                    *clip = Some([rect.x + PADDING, rect.y + PADDING, rect.w - 2.0 * PADDING, rect.h - 2.0 * PADDING]);
                    *line_spacing = Some(LINE_HEIGHT);
                }
                self.text = Some(actor);
            });
        }
        if let Some(text) = &self.text {
            actors.push(act!(quad: align(0.0,0.0): xy(rect.x,rect.y): zoomto(rect.w,rect.h): diffuse(0.0,0.0,0.0,187.0/255.0): z(900)));
            actors.push(text.clone());
        }
    }
}

// Emoji substitutions from the supplied SL module.
fn emote(word: &str) -> Option<&'static str> {
    Some(match word {
        "Crab" => "🦀",
        "Snowman" => "⛄",
        "Hug" => "🤗",
        "Thonk" => "🤔",
        "Wave" => "🌊",
        "Burger" => "🍔",
        "Baguette" => "🥖",
        "Strong" => "💪",
        "Lips" => "👄",
        "Rainbow" => "🌈",
        "Copter" => "🚁",
        "Candle" => "🕯",
        "Tulip" => "🌷",
        "Sweat" => "💦",
        "Tower" => "🗼",
        "RedHeart" => "❤",
        "YellowHeart" => "💛",
        "GreenHeart" => "💚",
        "BlueHeart" => "💙",
        "PurpleHeart" => "💜",
        "BlackHeart" => "🖤",
        "Airplane" => "✈",
        "Star" => "⭐",
        "Quad" => "🌟🌟🌟🌟",
        "Tada" => "🎉",
        ":(" | ":-(" => "🙁",
        ":)" | ":-)" => "🙂",
        ":/" | ":\\" | ":-/" | ":-\\" => "😕",
        ":|" | ":-|" => "😐",
        ";)" | ";-)" => "😉",
        ">(" => "😠",
        "<3" => "💜",
        "8-)" | "B)" | "B-)" => "😎",
        ":-D" | ":D" => "😀",
        ":-o" | ":o" | ":-O" | ":O" => "😮",
        "o.o" | "o.O" | "O.o" | "O.O" | "o_o" | "o_O" | "O_o" | "O_O" => "👀",
        ":p" | ":P" | ":-p" | ":-P" => "😛",
        ";p" | ";P" | ";-p" | ";-P" => "😜",
        ":-z" | ":z" | ":-Z" | ":Z" => "😑",
        "HolidayPresent" => "🎁",
        "HolidayTree" => "🎄",
        "HolidaySanta" => "🎅",
        "PopCorn" => "🍿",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Entry {
        Entry::Chat(ChatMessage {
            login: "viewer".into(),
            display_name: "日本語".into(),
            text: "Crab &START; &#65; UnknownEmote".into(),
            color: Some("#123456".into()),
            badges: vec!["moderator".into()],
            user_id: None,
            id: None,
            announcement: true,
        })
    }

    #[test]
    fn twitch_unicode_attributes_and_emote_toggles_preserve_literal_text() {
        let (text, attrs) = format_entry(&sample(), TwitchOptions::default());
        assert_eq!(text, "📣🗡 日本語: 🦀 &START; &#65; UnknownEmote");
        assert_eq!(attrs[0].start, 3);
        assert_eq!(attrs[0].length, 3);
        assert_eq!(attrs[0].color, deadlib_present::color::rgba_hex("#123456"));
        let (text, attrs) = format_entry(
            &sample(),
            TwitchOptions {
                show_badges: false,
                show_emotes: false,
                ..Default::default()
            },
        );
        assert!(text.starts_with("日本語: Crab "));
        assert_eq!(attrs[0].start, 0);
    }

    #[test]
    fn twitch_placements_hide_two_players_and_avoid_centered_fields() {
        for w in [640.0, 854.0] {
            for screen in [
                Screen::Gameplay,
                Screen::Evaluation,
                Screen::EvaluationSummary,
                Screen::Initials,
            ] {
                assert!(
                    placement(
                        screen,
                        w,
                        480.0,
                        [true, true],
                        Some(GameplayLayout {
                            two_players: true,
                            p2: false,
                            centered: false,
                            field_left: 30.0,
                            field_right: 286.0,
                            statistics: false
                        }),
                        false
                    )
                    .is_none()
                );
            }
            for p2 in [false, true] {
                for statistics in [false, true] {
                    let game = GameplayLayout {
                        two_players: false,
                        p2,
                        centered: true,
                        field_left: w / 2.0 - 128.0,
                        field_right: w / 2.0 + 128.0,
                        statistics,
                    };
                    if let Some(rect) =
                        placement(Screen::Gameplay, w, 480.0, [!p2, p2], Some(game), false)
                    {
                        assert!(
                            rect.x + rect.w <= game.field_left - 10.0
                                || rect.x >= game.field_right + 10.0
                        );
                    }
                }
            }
        }
        assert!(placement(Screen::Credits, 854.0, 480.0, [true, false], None, false).is_none());
    }

    #[test]
    fn twitch_overlay_reuses_text_and_clears_when_last_message_cannot_fit() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let fonts = deadlib_assets::parse_font_asset_specs(
            crate::resources::FONT_ASSETS
                .iter()
                .copied()
                .filter(|spec| matches!(spec.name, "miso" | "game" | "cjk" | "emoji")),
            &[root.join("assets")],
            |path| root.join(path),
        )
        .unwrap();
        let mut assets = AssetManager::new();
        assets.register_fonts(fonts.into_iter().map(|f| (f.name, f.font)));
        let rect = Rect {
            x: 0.0,
            y: 0.0,
            w: 320.0,
            h: 126.0,
        };
        let mut overlay = Overlay::default();
        let mut actors = Vec::new();
        let mut entries = vec![Arc::new(sample())];
        overlay.push(
            &mut actors,
            &entries,
            1,
            rect,
            TwitchOptions::default(),
            &assets,
        );
        let Actor::Text {
            content: TextContent::Dynamic(first),
            ..
        } = &actors[1]
        else {
            panic!()
        };
        assert!(first.text.contains("&START; &#65;"));
        let first = Arc::clone(first);
        actors.clear();
        overlay.push(
            &mut actors,
            &entries,
            1,
            rect,
            TwitchOptions::default(),
            &assets,
        );
        let Actor::Text {
            content: TextContent::Dynamic(second),
            ..
        } = &actors[1]
        else {
            panic!()
        };
        assert!(Arc::ptr_eq(&first, second));
        entries.push(Arc::new(Entry::Error("word ".repeat(1000))));
        actors.clear();
        overlay.push(
            &mut actors,
            &entries,
            2,
            rect,
            TwitchOptions::default(),
            &assets,
        );
        assert!(actors.is_empty());
    }

    #[test]
    #[ignore = "requires a GPU; exports a visual fixture to target/twitch-chat-preview.png"]
    fn twitch_render_preview() {
        use deadlib_present::{
            compose::{self, ComposeScratch, TextLayoutCache},
            space::Metrics,
        };
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let roots = [root.join("assets")];
        let fonts = deadlib_assets::parse_font_asset_specs(
            crate::resources::FONT_ASSETS
                .iter()
                .copied()
                .filter(|spec| matches!(spec.name, "miso" | "game" | "cjk" | "emoji")),
            &roots,
            |path| root.join(path),
        )
        .unwrap();
        let mut builder = winit::event_loop::EventLoop::builder();
        #[cfg(target_os = "windows")]
        {
            use winit::platform::windows::EventLoopBuilderExtWindows;
            builder.with_any_thread(true);
        }
        let event_loop = builder.build().unwrap();
        #[allow(deprecated)]
        let window = Arc::new(
            event_loop
                .create_window(
                    winit::window::Window::default_attributes()
                        .with_visible(false)
                        .with_inner_size(winit::dpi::PhysicalSize::new(854, 480)),
                )
                .unwrap(),
        );
        let metrics = Metrics::centered(854.0, 480.0);
        let mut backend = deadlib_render::create_backend(
            deadlib_render::BackendType::VulkanWgpu,
            window,
            metrics.projection(),
            false,
            deadlib_render::PresentModePolicy::Immediate,
            false,
            true,
        )
        .unwrap();
        let mut assets = AssetManager::new();
        assets.load_textures(&mut backend, Vec::new()).unwrap();
        for asset in &fonts {
            for texture in deadlib_assets::prepare_required_font_textures(
                &asset.font,
                &asset.required_textures,
                &roots,
                |key| assets.has_texture_key(key),
            )
            .unwrap()
            {
                assets
                    .update_texture_for_key_with_sampler(
                        &mut backend,
                        &texture.key,
                        &texture.image,
                        texture.hints.sampler_desc(),
                    )
                    .unwrap();
            }
        }
        assets.register_fonts(fonts.into_iter().map(|asset| (asset.name, asset.font)));
        let entries = vec![
            Arc::new(Entry::Status {
                text: "Welcome to the chat room!".into(),
                notice: None,
            }),
            Arc::new(sample()),
            Arc::new(Entry::Chat(ChatMessage {
                login: "natano".into(),
                display_name: "Natano".into(),
                text: "Hello! This longer message should wrap over several lines. Quad <3".into(),
                color: None,
                badges: vec!["broadcaster".into(), "subscriber".into()],
                user_id: None,
                id: None,
                announcement: false,
            })),
            Arc::new(Entry::Status {
                text: "Someone subscribed!".into(),
                notice: Some("sub".into()),
            }),
            Arc::new(Entry::Error("Connection interrupted. Reconnecting…".into())),
        ];
        let mut actors = vec![
            act!(text: align(0.0,0.0): xy(20.0,15.0): font("miso"): settext("Twitch chat / native font and wrapping preview")),
        ];
        let mut overlay = Overlay::default();
        overlay.push(
            &mut actors,
            &entries,
            1,
            Rect {
                x: 20.0,
                y: 60.0,
                w: 320.0,
                h: 360.0,
            },
            TwitchOptions::default(),
            &assets,
        );
        // A second slot lets the same fixture show the compact select-music panel.
        let mut compact = Vec::new();
        overlay.push(
            &mut compact,
            &entries,
            1,
            Rect {
                x: 380.0,
                y: 60.0,
                w: 320.0,
                h: 126.0,
            },
            TwitchOptions::default(),
            &assets,
        );
        for actor in &mut compact {
            if let Actor::Text {
                content: TextContent::Dynamic(text),
                ..
            } = actor
            {
                Arc::make_mut(text).slot = 1;
            }
        }
        actors.extend(compact);
        let frame = compose::build_screen_cached_with_scratch_and_texture_context(
            &actors,
            [0.08, 0.12, 0.17, 1.0],
            &metrics,
            assets.fonts(),
            0.0,
            &mut TextLayoutCache::default(),
            &mut ComposeScratch::default(),
            assets.texture_context(),
        );
        backend.request_screenshot();
        backend.draw(&frame, assets.textures(), false).unwrap();
        let image = backend.capture_frame().unwrap();
        assert!(image.get_pixel(25, 65)[0] < image.get_pixel(10, 65)[0], "the translucent panel must render over the background");
        image.save(root.join("target/twitch-chat-preview.png")).unwrap();
    }
}
