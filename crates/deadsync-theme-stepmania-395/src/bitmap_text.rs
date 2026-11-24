use std::{fs, io, path::Path, sync::Arc};

use deadlib_present::actors::{Actor, SpriteSource, TextContent};
use deadlib_present::anim::{EffectMode, EffectState, TweenSeq, TweenState};
use deadlib_present::dsl::{SpriteBuilder, TextBuilder};
use deadsync_noteskin::script::split_script_token;

use crate::ActorAnimation;
use crate::animation::numbers;
use crate::scene::CANVAS;

pub(crate) struct BitmapText {
    content: Arc<str>,
    pub(crate) font: String,
    on: StyleCommand,
    focused: StyleCommand,
    unfocused: StyleCommand,
    disabled: StyleCommand,
}

struct StyleCommand {
    animation: Option<ActorAnimation>,
    stop_tweening: bool,
    shadow: Option<f32>,
    mode: Option<EffectMode>,
    period: Option<f32>,
    colors: [Option<[f32; 4]>; 2],
    wrap_width: Option<f32>,
    max_width: Option<f32>,
}

#[derive(Clone)]
pub(crate) struct StyledPlayback {
    tween: TweenSeq,
    shadow: f32,
    effect: EffectState,
    wrap_width: Option<f32>,
    max_width: Option<f32>,
}

impl BitmapText {
    pub(crate) fn read(path: &Path) -> io::Result<Self> {
        let invalid = |detail: &str| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{}: {detail}", path.display()),
            )
        };
        let xml = fs::read_to_string(path)?;
        let document = roxmltree::Document::parse(xml.trim_start_matches('\u{feff}'))
            .map_err(|error| invalid(&error.to_string()))?;
        let mut node = document.root_element();
        for wrapper in ["ActorFrame", "children"] {
            if node.has_tag_name(wrapper) {
                if node.attributes().next().is_some() {
                    return Err(invalid("bitmap text wrapper attributes are unsupported"));
                }
                let mut children = node.children().filter(roxmltree::Node::is_element);
                node = children
                    .next()
                    .filter(|_| children.next().is_none())
                    .ok_or_else(|| invalid("bitmap text wrapper must have exactly one child"))?;
            }
        }
        if !node.has_tag_name("BitmapText") || node.children().any(|child| child.is_element()) {
            return Err(invalid("expected a BitmapText actor"));
        }
        for attr in node.attributes() {
            if !matches!(
                attr.name(),
                "Text"
                    | "File"
                    | "OnCommand"
                    | "GainFocusCommand"
                    | "LoseFocusCommand"
                    | "EnabledCommand"
                    | "DisabledCommand"
            ) {
                return Err(invalid(&format!(
                    "unsupported BitmapText attribute {}",
                    attr.name()
                )));
            }
        }
        let command = |name| {
            StyleCommand::parse(node.attribute(name).unwrap_or_default())
                .map_err(|error| invalid(&format!("{name}: {error}")))
        };
        let text = node
            .attribute("Text")
            .ok_or_else(|| invalid("BitmapText requires Text"))?;
        if text.starts_with('@') {
            return Err(invalid("computed bitmap text is unsupported"));
        }
        Ok(Self {
            content: Arc::from(text),
            font: node
                .attribute("File")
                .filter(|file| !file.is_empty())
                .ok_or_else(|| invalid("BitmapText requires a font File"))?
                .to_owned(),
            on: command("OnCommand")?,
            focused: command("GainFocusCommand")?,
            unfocused: command("LoseFocusCommand")?,
            disabled: command("DisabledCommand")?,
        })
    }

    pub(crate) fn start(&self, enabled: bool) -> StyledPlayback {
        let mut playback = StyledPlayback::new(TweenState::default());
        playback.run(&self.on);
        if !enabled {
            playback.run(&self.disabled);
        }
        playback
    }

    pub(crate) fn focus(&self, playback: &mut StyledPlayback, focused: bool) {
        playback.run(if focused {
            &self.focused
        } else {
            &self.unfocused
        });
    }

    pub(crate) fn draw(
        &self,
        playback: &StyledPlayback,
        font: &'static str,
        at: [f32; 2],
        alpha: f32,
    ) -> Option<Actor> {
        playback.actor(
            font,
            TextContent::Shared(self.content.clone()),
            at,
            [1.0, 1.0, 1.0, alpha],
        )
    }
}

impl StyleCommand {
    fn parse(raw: &str) -> Result<Self, String> {
        let mut tween = Vec::new();
        let mut stop_tweening = false;
        let mut shadow = None;
        let mut mode = None;
        let mut period = None;
        let mut colors = [None, None];
        let mut wrap_width = None;
        let mut max_width = None;
        for statement in raw.split(';') {
            let Some(token) = split_script_token(statement) else {
                continue;
            };
            let name = token.command().as_str().to_ascii_lowercase();
            let invalid = || format!("invalid arguments for {name}");
            match name.as_str() {
                "wrapwidthpixels" => {
                    let [width] = numbers(token.args()).ok_or_else(invalid)?;
                    wrap_width = Some(width);
                }
                "maxwidth" => {
                    let [width] = numbers(token.args()).ok_or_else(invalid)?;
                    max_width = Some(width);
                }
                "shadowlength" => {
                    let [length] = numbers(token.args()).ok_or_else(invalid)?;
                    shadow = Some(length);
                }
                "effectperiod" => {
                    let [duration] = numbers(token.args())
                        .filter(|[duration]| *duration > 0.0)
                        .ok_or_else(invalid)?;
                    period = Some(duration);
                }
                "effectcolor1" | "effectcolor2" => {
                    colors[usize::from(name == "effectcolor2")] =
                        Some(numbers(token.args()).ok_or_else(invalid)?);
                }
                "diffuseshift" | "glowshift" | "stopeffect" | "stoptweening"
                    if token.args().is_empty() =>
                {
                    match name.as_str() {
                        "diffuseshift" => mode = Some(EffectMode::DiffuseShift),
                        "glowshift" => mode = Some(EffectMode::GlowShift),
                        "stopeffect" => mode = Some(EffectMode::None),
                        _ if tween.is_empty() => stop_tweening = true,
                        _ => return Err("StopTweening must precede tween commands".to_owned()),
                    }
                }
                _ => tween.push(statement),
            }
        }
        let animation = (!tween.is_empty())
            .then(|| ActorAnimation::parse(&tween.join(";"), CANVAS))
            .transpose()
            .map_err(|error| error.to_string())?;
        Ok(Self {
            animation,
            stop_tweening,
            shadow,
            mode,
            period,
            colors,
            wrap_width,
            max_width,
        })
    }
}

impl StyledPlayback {
    pub(crate) fn new(state: TweenState) -> Self {
        Self {
            tween: TweenSeq::new(state),
            shadow: 0.0,
            effect: EffectState::default(),
            wrap_width: None,
            max_width: None,
        }
    }

    pub(crate) fn styled(state: TweenState, commands: &str) -> Result<Self, String> {
        let mut playback = Self::new(state);
        playback.run(&StyleCommand::parse(commands)?);
        Ok(playback)
    }

    pub(crate) fn scale_x(&self) -> f32 {
        self.tween.state().scale[0]
    }

    pub(crate) fn sprite(&self, source: SpriteSource, alpha: f32, z: i16) -> Actor {
        let mut state = *self.tween.state();
        state.tint[3] *= alpha;
        state.glow[3] *= alpha;
        let mut sprite = SpriteBuilder::with_source(source);
        sprite.apply_tween_state(&state);
        sprite.shadowlength(self.shadow);
        sprite.effectmode(self.effect.mode);
        sprite.effectperiod(self.effect.period);
        sprite.effectcolor1(self.effect.color1);
        sprite.effectcolor2(self.effect.color2);
        sprite.z(z);
        sprite.build(0)
    }

    pub(crate) fn actor(
        &self,
        font: &'static str,
        content: TextContent,
        at: [f32; 2],
        tint: [f32; 4],
    ) -> Option<Actor> {
        let state = self.tween.state();
        if !state.visible {
            return None;
        }
        let mut text = TextBuilder::new();
        text.font(font);
        text.settext(content);
        text.xy(at[0] + state.x, at[1] + state.y);
        text.align(state.hx, state.vy);
        text.zoomx(state.scale[0]);
        text.zoomy(state.scale[1]);
        text.diffuse(std::array::from_fn(|channel| {
            state.tint[channel] * tint[channel]
        }));
        let [r, g, b, a] = state.glow;
        text.glow([r, g, b, a * tint[3]]);
        text.shadowlength(self.shadow);
        text.effectmode(self.effect.mode);
        text.effectperiod(self.effect.period);
        text.effectcolor1(self.effect.color1);
        text.effectcolor2(self.effect.color2);
        if let Some(width) = self.wrap_width {
            text.wrapwidthpixels(width);
        }
        if let Some(width) = self.max_width {
            text.maxwidth(width);
        }
        text.z(1);
        Some(text.build(0))
    }

    pub(crate) fn advance(&mut self, delta: f32) {
        self.tween.update(delta);
    }

    fn run(&mut self, command: &StyleCommand) {
        if command.stop_tweening {
            self.tween.clear();
        }
        if let Some(animation) = &command.animation {
            animation.queue_on(&mut self.tween);
        }
        self.shadow = command.shadow.unwrap_or(self.shadow);
        self.effect.mode = command.mode.unwrap_or(self.effect.mode);
        self.effect.period = command.period.unwrap_or(self.effect.period);
        self.effect.color1 = command.colors[0].unwrap_or(self.effect.color1);
        self.effect.color2 = command.colors[1].unwrap_or(self.effect.color2);
        self.wrap_width = command.wrap_width.or(self.wrap_width);
        self.max_width = command.max_width.or(self.max_width);
    }
}
