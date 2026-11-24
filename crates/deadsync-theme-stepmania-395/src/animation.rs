use std::fmt;

use deadlib_present::anim::{self, SegmentBuilder, Step, TweenSeq, TweenState};
use deadsync_noteskin::script::{bind_screen_args, parse_script_number, split_script_token};

use crate::MetricLookupError;

/// A compiled numeric actor program for the presentation tween engine.
#[derive(Debug)]
pub struct ActorAnimation {
    initial: Step,
    steps: Vec<Step>,
    anchors: [Option<f32>; 2],
    visible: Option<bool>,
    repeat_texture: Option<bool>,
}

#[derive(Debug)]
pub enum AnimationError {
    Metric(MetricLookupError),
    InvalidCanvas([f32; 2]),
    InvalidArguments { statement: usize, command: String },
    UnsupportedCommand { statement: usize, command: String },
}

impl fmt::Display for AnimationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Metric(error) => error.fmt(f),
            Self::InvalidCanvas(canvas) => write!(f, "invalid logical canvas {canvas:?}"),
            Self::InvalidArguments { statement, command } => {
                write!(
                    f,
                    "invalid arguments for {command:?} at statement {statement}"
                )
            }
            Self::UnsupportedCommand { statement, command } => {
                write!(
                    f,
                    "unsupported actor command {command:?} at statement {statement}"
                )
            }
        }
    }
}

impl std::error::Error for AnimationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Metric(error) => Some(error),
            _ => None,
        }
    }
}

impl ActorAnimation {
    /// Parses timed transforms, color changes, and delays on a logical canvas.
    pub fn parse(commands: &str, canvas: [f32; 2]) -> Result<Self, AnimationError> {
        if canvas
            .iter()
            .any(|value| !value.is_finite() || *value <= 0.0)
        {
            return Err(AnimationError::InvalidCanvas(canvas));
        }
        let commands = bind_screen_args(commands, canvas[0], canvas[1]);
        let mut segments = Vec::new();
        let mut current = anim::linear(0.0);
        let mut pending = true;
        let mut anchors = [None, None];
        let mut visible = None;
        let mut repeat_texture = None;
        for (index, raw) in commands.split(';').enumerate() {
            let Some(token) = split_script_token(raw) else {
                continue;
            };
            let name = token.command().as_str().to_ascii_lowercase();
            let invalid = || AnimationError::InvalidArguments {
                statement: index + 1,
                command: name.clone(),
            };
            if matches!(name.as_str(), "horizalign" | "vertalign") {
                let axis = usize::from(name == "vertalign");
                let value = anchor(axis, token.args()).ok_or_else(invalid)?;
                anchors[axis] = Some(value);
            } else if matches!(name.as_str(), "hidden" | "visible" | "texturewrapping") {
                let [value] = numbers(token.args()).ok_or_else(invalid)?;
                let enabled = value.trunc() != 0.0;
                match name.as_str() {
                    "hidden" => visible = Some(!enabled),
                    "visible" => visible = Some(enabled),
                    _ => repeat_texture = Some(enabled),
                }
            } else if let Some(tween) = timing_command(&name) {
                let [duration] = numbers(token.args()).ok_or_else(invalid)?;
                if duration < 0.0 {
                    return Err(invalid());
                }
                if pending {
                    segments.push(current.build());
                }
                current = match tween {
                    TimingCommand::Tween(build) => {
                        pending = true;
                        build(duration)
                    }
                    TimingCommand::Sleep => {
                        segments.push(anim::sleep(duration));
                        pending = false;
                        // Setters after a sleep take effect at its end.
                        anim::linear(0.0)
                    }
                };
            } else if let Some(set) = scalar_command(&name) {
                let [value] = numbers(token.args()).ok_or_else(invalid)?;
                current = set(current, value);
                pending = true;
            } else {
                current = match name.as_str() {
                    "diffuse" => {
                        let [r, g, b, a] = numbers(token.args()).ok_or_else(invalid)?;
                        current.diffuse(r, g, b, a)
                    }
                    "glow" => {
                        let [r, g, b, a] = numbers(token.args()).ok_or_else(invalid)?;
                        current.glow(r, g, b, a)
                    }
                    "zoom" => {
                        let [scale] = numbers(token.args()).ok_or_else(invalid)?;
                        current.zoom(scale, scale)
                    }
                    "zoomto" => {
                        let [width, height] = numbers(token.args()).ok_or_else(invalid)?;
                        current.zoomto(width, height)
                    }
                    "stretchto" => {
                        let [left, top, right, bottom] =
                            numbers(token.args()).ok_or_else(invalid)?;
                        let width = right - left;
                        let height = bottom - top;
                        current
                            .xy(left + width * 0.5, top + height * 0.5)
                            .zoomto(width, height)
                    }
                    "xy" => {
                        let [x, y] = numbers(token.args()).ok_or_else(invalid)?;
                        current.xy(x, y)
                    }
                    _ => {
                        return Err(AnimationError::UnsupportedCommand {
                            statement: index + 1,
                            command: name,
                        });
                    }
                };
                pending = true;
            }
        }
        if pending {
            segments.push(current.build());
        }
        let mut segments = segments.into_iter();
        // The leading segment contains setters before the first timed command.
        let initial = segments
            .next()
            .expect("animation always has a leading segment");
        Ok(Self {
            initial,
            steps: segments.collect(),
            anchors,
            visible,
            repeat_texture,
        })
    }

    pub(crate) const fn repeat_texture(&self) -> Option<bool> {
        self.repeat_texture
    }

    /// Applies the initial setters and queues the timed part of the animation.
    #[must_use]
    pub fn start(&self, mut initial_state: TweenState) -> TweenSeq {
        self.apply_immediate(&mut initial_state);
        let mut timeline = TweenSeq::new(self.initial.end_state(initial_state));
        for step in &self.steps {
            timeline.push_step(step.clone());
        }
        timeline
    }

    pub(crate) fn queue_on(&self, timeline: &mut TweenSeq) {
        if timeline.is_empty() {
            *timeline = self.start(*timeline.state());
        } else {
            self.apply_immediate(timeline.state_mut());
            timeline.push_step(self.initial.clone());
            for step in &self.steps {
                timeline.push_step(step.clone());
            }
        }
    }

    fn apply_immediate(&self, state: &mut TweenState) {
        // These properties apply when the command runs, regardless of tween position.
        state.hx = self.anchors[0].unwrap_or(state.hx);
        state.vy = self.anchors[1].unwrap_or(state.vy);
        state.visible = self.visible.unwrap_or(state.visible);
    }
}

enum TimingCommand {
    Tween(fn(f32) -> SegmentBuilder),
    Sleep,
}

fn timing_command(name: &str) -> Option<TimingCommand> {
    let build: fn(f32) -> SegmentBuilder = match name {
        "linear" => anim::linear,
        "accelerate" => anim::accelerate,
        "decelerate" => anim::decelerate,
        "smooth" => anim::smooth,
        "spring" => anim::spring,
        "sleep" => return Some(TimingCommand::Sleep),
        _ => return None,
    };
    Some(TimingCommand::Tween(build))
}

fn scalar_command(name: &str) -> Option<fn(SegmentBuilder, f32) -> SegmentBuilder> {
    let set: fn(SegmentBuilder, f32) -> SegmentBuilder = match name {
        "x" => SegmentBuilder::x,
        "y" => SegmentBuilder::y,
        "z" => SegmentBuilder::z,
        "addx" => SegmentBuilder::addx,
        "addy" => SegmentBuilder::addy,
        "addz" => SegmentBuilder::addz,
        "zoomx" => SegmentBuilder::zoomx,
        "zoomy" => SegmentBuilder::zoomy,
        "zoomtowidth" => SegmentBuilder::zoom_to_width,
        "zoomtoheight" => SegmentBuilder::zoom_to_height,
        "rotationx" => SegmentBuilder::rotationx,
        "rotationy" => SegmentBuilder::rotationy,
        "rotationz" => SegmentBuilder::rotationz,
        "addrotationx" => SegmentBuilder::addrotationx,
        "addrotationy" => SegmentBuilder::addrotationy,
        "addrotationz" => SegmentBuilder::addrotationz,
        "cropleft" => SegmentBuilder::cropleft,
        "cropright" => SegmentBuilder::cropright,
        "croptop" => SegmentBuilder::croptop,
        "cropbottom" => SegmentBuilder::cropbottom,
        "fadeleft" => SegmentBuilder::fadeleft,
        "faderight" => SegmentBuilder::faderight,
        "fadetop" => SegmentBuilder::fadetop,
        "fadebottom" => SegmentBuilder::fadebottom,
        "diffusealpha" => SegmentBuilder::alpha,
        _ => return None,
    };
    Some(set)
}

pub(crate) fn numbers<const N: usize>(args: &[&str]) -> Option<[f32; N]> {
    if args.len() != N {
        return None;
    }
    let mut values = [0.0; N];
    for (value, arg) in values.iter_mut().zip(args) {
        *value = parse_script_number(arg).filter(|number| number.is_finite())?;
    }
    Some(values)
}

fn anchor(axis: usize, args: &[&str]) -> Option<f32> {
    let [value] = args else {
        return None;
    };
    let names = if axis == 0 {
        ["left", "center", "right"]
    } else {
        ["top", "middle", "bottom"]
    };
    names
        .iter()
        .position(|name| value.eq_ignore_ascii_case(name))
        .map(|index| index as f32 * 0.5)
}
