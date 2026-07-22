use crate::actors::{Actor, IntoTextureKey, SizeSpec, SpriteSource, TextAlign, TextContent};
use crate::texture::{TextureContext, cached_static_texture_source};
use crate::{anim, font, runtime};
use deadlib_render::BlendMode;
use glam::Mat4 as Matrix4;
use smallvec::SmallVec;
use std::sync::atomic::AtomicU64;

#[doc(hidden)]
pub type TweenSteps = SmallVec<[anim::Step; 4]>;

#[doc(hidden)]
pub struct TweenProgramTarget;

impl TweenProgramTarget {
    pub fn xy(&mut self, _: f32, _: f32) {}
    pub fn x(&mut self, _: f32) {}
    pub fn y(&mut self, _: f32) {}
    pub fn addx(&mut self, _: f32) {}
    pub fn addy(&mut self, _: f32) {}
    pub fn diffuse(&mut self, _: [f32; 4]) {}
    pub fn alpha(&mut self, _: f32) {}
    pub fn glow(&mut self, _: [f32; 4]) {}
    pub fn zoom(&mut self, _: f32) {}
    pub fn zoomx(&mut self, _: f32) {}
    pub fn zoomy(&mut self, _: f32) {}
    pub fn addzoomx(&mut self, _: f32) {}
    pub fn addzoomy(&mut self, _: f32) {}
    pub fn zoomto(&mut self, _: f32, _: f32) {}
    pub fn size(&mut self, _: f32, _: f32) {}
    pub fn cropleft(&mut self, _: f32) {}
    pub fn cropright(&mut self, _: f32) {}
    pub fn croptop(&mut self, _: f32) {}
    pub fn cropbottom(&mut self, _: f32) {}
    pub fn fadeleft(&mut self, _: f32) {}
    pub fn faderight(&mut self, _: f32) {}
    pub fn fadetop(&mut self, _: f32) {}
    pub fn fadebottom(&mut self, _: f32) {}
    pub fn visible(&mut self, _: bool) {}
    pub fn rotationx(&mut self, _: f32) {}
    pub fn rotationy(&mut self, _: f32) {}
    pub fn rotationz(&mut self, _: f32) {}
    pub fn addrotationx(&mut self, _: f32) {}
    pub fn addrotationy(&mut self, _: f32) {}
    pub fn addrotationz(&mut self, _: f32) {}
}
// PARITY COMMENT STANDARD:
// PARITY[<Source>]: <mirrored behavior>. Ref: <file/symbol> when known.
#[doc(hidden)]
#[inline(always)]
pub fn __dsl_parse_effect_clock(raw: &str) -> anim::EffectClock {
    let lower = raw
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .to_ascii_lowercase();
    match lower.as_str() {
        // ITGmania Actor::SetEffectClockString()
        "beat" | "beatnooffset" | "bgm" => anim::EffectClock::Beat,
        "timer" | "timerglobal" | "music" | "musicnooffset" | "time" | "seconds" => {
            anim::EffectClock::Time
        }
        _ if lower.contains("beat") => anim::EffectClock::Beat,
        _ => anim::EffectClock::Time,
    }
}

#[macro_export]
#[doc(hidden)]
macro_rules! __ui_textalign_from_ident {
    (left) => {
        $crate::actors::TextAlign::Left
    };
    (center) => {
        $crate::actors::TextAlign::Center
    };
    (right) => {
        $crate::actors::TextAlign::Right
    };
    ($other:ident) => {
        compile_error!(concat!(
            "horizalign expects left|center|right, got: ",
            stringify!($other)
        ));
    };
}

#[macro_export]
#[doc(hidden)]
macro_rules! __ui_halign_from_ident {
    (left) => {
        0.0f32
    };
    (center) => {
        0.5f32
    };
    (right) => {
        1.0f32
    };
    ($other:ident) => {
        compile_error!(concat!(
            "halign expects left|center|right, got: ",
            stringify!($other)
        ));
    };
}

#[macro_export]
#[doc(hidden)]
macro_rules! __ui_valign_from_ident {
    (top) => {
        0.0f32
    };
    (middle) => {
        0.5f32
    };
    (center) => {
        0.5f32
    };
    (bottom) => {
        1.0f32
    };
    ($other:ident) => {
        compile_error!(concat!(
            "valign expects top|middle|center|bottom, got: ",
            stringify!($other)
        ));
    };
}

#[macro_export]
#[doc(hidden)]
macro_rules! __dsl_apply {
    ( () $mods:ident $tw:ident $cur:ident $site:ident ) => { () };
    ( ($cmd:ident ( $($args:tt)* ) : $($rest:tt)* ) $mods:ident $tw:ident $cur:ident $site:ident ) => {{
        $crate::__dsl_apply_one!{ $cmd ( $($args)* ) $mods $tw $cur $site }
        $crate::__dsl_apply!( ($($rest)*) $mods $tw $cur $site );
    }};
    ( ($cmd:ident ( $($args:tt)* ) ) $mods:ident $tw:ident $cur:ident $site:ident ) => {{
        $crate::__dsl_apply_one!{ $cmd ( $($args)* ) $mods $tw $cur $site }
        $crate::__dsl_apply!( () $mods $tw $cur $site );
    }};
}

#[macro_export]
#[doc(hidden)]
macro_rules! __dsl_apply_split {
    ( () $mods:ident $tw:ident $cur:ident $in_tween:ident $has_tween:ident $tween_pass:tt ) => { () };
    ( ($cmd:ident ( $($args:tt)* ) : $($rest:tt)*) $mods:ident $tw:ident $cur:ident $in_tween:ident $has_tween:ident $tween_pass:tt ) => {{
        $crate::__dsl_apply_split_one!($cmd ($($args)*) $mods $tw $cur $in_tween $has_tween $tween_pass);
        $crate::__dsl_apply_split!(($($rest)*) $mods $tw $cur $in_tween $has_tween $tween_pass);
    }};
    ( ($cmd:ident ( $($args:tt)* )) $mods:ident $tw:ident $cur:ident $in_tween:ident $has_tween:ident $tween_pass:tt ) => {{
        $crate::__dsl_apply_split_one!($cmd ($($args)*) $mods $tw $cur $in_tween $has_tween $tween_pass);
        $crate::__dsl_apply_split!(() $mods $tw $cur $in_tween $has_tween $tween_pass);
    }};
}

#[macro_export]
#[doc(hidden)]
macro_rules! __dsl_apply_segment_control {
    ($cmd:ident ($($args:tt)*) $mods:ident $tw:ident $cur:ident $in_tween:ident $has_tween:ident false $next:expr) => {{
        $has_tween = true;
        $in_tween = $next;
    }};
    ($cmd:ident ($($args:tt)*) $mods:ident $tw:ident $cur:ident $in_tween:ident $has_tween:ident true $next:expr) => {{
        $has_tween = true;
        $crate::__dsl_apply_one!($cmd ($($args)*) $mods $tw $cur _dummy_site);
        $in_tween = $next;
    }};
}

#[macro_export]
#[doc(hidden)]
macro_rules! __dsl_apply_tweenable {
    ($cmd:ident ($($args:tt)*) $mods:ident $tw:ident $cur:ident $in_tween:ident $has_tween:ident false) => {{
        if !$in_tween {
            $crate::__dsl_apply_one!($cmd ($($args)*) $mods $tw $cur _dummy_site);
        }
    }};
    ($cmd:ident ($($args:tt)*) $mods:ident $tw:ident $cur:ident $in_tween:ident $has_tween:ident true) => {{
        if $in_tween {
            $crate::__dsl_apply_one!($cmd ($($args)*) $mods $tw $cur _dummy_site);
        }
    }};
}

#[macro_export]
#[doc(hidden)]
macro_rules! __dsl_apply_split_one {
    (linear ($($args:tt)*) $mods:ident $tw:ident $cur:ident $in_tween:ident $has_tween:ident $pass:tt) => { $crate::__dsl_apply_segment_control!(linear ($($args)*) $mods $tw $cur $in_tween $has_tween $pass true) };
    (accelerate ($($args:tt)*) $mods:ident $tw:ident $cur:ident $in_tween:ident $has_tween:ident $pass:tt) => { $crate::__dsl_apply_segment_control!(accelerate ($($args)*) $mods $tw $cur $in_tween $has_tween $pass true) };
    (decelerate ($($args:tt)*) $mods:ident $tw:ident $cur:ident $in_tween:ident $has_tween:ident $pass:tt) => { $crate::__dsl_apply_segment_control!(decelerate ($($args)*) $mods $tw $cur $in_tween $has_tween $pass true) };
    (ease ($($args:tt)*) $mods:ident $tw:ident $cur:ident $in_tween:ident $has_tween:ident $pass:tt) => { $crate::__dsl_apply_segment_control!(ease ($($args)*) $mods $tw $cur $in_tween $has_tween $pass true) };
    (smooth ($($args:tt)*) $mods:ident $tw:ident $cur:ident $in_tween:ident $has_tween:ident $pass:tt) => { $crate::__dsl_apply_segment_control!(smooth ($($args)*) $mods $tw $cur $in_tween $has_tween $pass true) };
    (sleep ($($args:tt)*) $mods:ident $tw:ident $cur:ident $in_tween:ident $has_tween:ident $pass:tt) => { $crate::__dsl_apply_segment_control!(sleep ($($args)*) $mods $tw $cur $in_tween $has_tween $pass false) };

    (xy ($($args:tt)*) $mods:ident $tw:ident $cur:ident $in_tween:ident $has_tween:ident $pass:tt) => { $crate::__dsl_apply_tweenable!(xy ($($args)*) $mods $tw $cur $in_tween $has_tween $pass) };
    (x ($($args:tt)*) $mods:ident $tw:ident $cur:ident $in_tween:ident $has_tween:ident $pass:tt) => { $crate::__dsl_apply_tweenable!(x ($($args)*) $mods $tw $cur $in_tween $has_tween $pass) };
    (y ($($args:tt)*) $mods:ident $tw:ident $cur:ident $in_tween:ident $has_tween:ident $pass:tt) => { $crate::__dsl_apply_tweenable!(y ($($args)*) $mods $tw $cur $in_tween $has_tween $pass) };
    (addx ($($args:tt)*) $mods:ident $tw:ident $cur:ident $in_tween:ident $has_tween:ident $pass:tt) => { $crate::__dsl_apply_tweenable!(addx ($($args)*) $mods $tw $cur $in_tween $has_tween $pass) };
    (addy ($($args:tt)*) $mods:ident $tw:ident $cur:ident $in_tween:ident $has_tween:ident $pass:tt) => { $crate::__dsl_apply_tweenable!(addy ($($args)*) $mods $tw $cur $in_tween $has_tween $pass) };
    (Center ($($args:tt)*) $mods:ident $tw:ident $cur:ident $in_tween:ident $has_tween:ident $pass:tt) => { $crate::__dsl_apply_tweenable!(Center ($($args)*) $mods $tw $cur $in_tween $has_tween $pass) };
    (CenterX ($($args:tt)*) $mods:ident $tw:ident $cur:ident $in_tween:ident $has_tween:ident $pass:tt) => { $crate::__dsl_apply_tweenable!(CenterX ($($args)*) $mods $tw $cur $in_tween $has_tween $pass) };
    (CenterY ($($args:tt)*) $mods:ident $tw:ident $cur:ident $in_tween:ident $has_tween:ident $pass:tt) => { $crate::__dsl_apply_tweenable!(CenterY ($($args)*) $mods $tw $cur $in_tween $has_tween $pass) };
    (center ($($args:tt)*) $mods:ident $tw:ident $cur:ident $in_tween:ident $has_tween:ident $pass:tt) => { $crate::__dsl_apply_tweenable!(center ($($args)*) $mods $tw $cur $in_tween $has_tween $pass) };
    (centerx ($($args:tt)*) $mods:ident $tw:ident $cur:ident $in_tween:ident $has_tween:ident $pass:tt) => { $crate::__dsl_apply_tweenable!(centerx ($($args)*) $mods $tw $cur $in_tween $has_tween $pass) };
    (centery ($($args:tt)*) $mods:ident $tw:ident $cur:ident $in_tween:ident $has_tween:ident $pass:tt) => { $crate::__dsl_apply_tweenable!(centery ($($args)*) $mods $tw $cur $in_tween $has_tween $pass) };
    (diffuse ($($args:tt)*) $mods:ident $tw:ident $cur:ident $in_tween:ident $has_tween:ident $pass:tt) => { $crate::__dsl_apply_tweenable!(diffuse ($($args)*) $mods $tw $cur $in_tween $has_tween $pass) };
    (alpha ($($args:tt)*) $mods:ident $tw:ident $cur:ident $in_tween:ident $has_tween:ident $pass:tt) => { $crate::__dsl_apply_tweenable!(alpha ($($args)*) $mods $tw $cur $in_tween $has_tween $pass) };
    (diffusealpha ($($args:tt)*) $mods:ident $tw:ident $cur:ident $in_tween:ident $has_tween:ident $pass:tt) => { $crate::__dsl_apply_tweenable!(diffusealpha ($($args)*) $mods $tw $cur $in_tween $has_tween $pass) };
    (glow ($($args:tt)*) $mods:ident $tw:ident $cur:ident $in_tween:ident $has_tween:ident $pass:tt) => { $crate::__dsl_apply_tweenable!(glow ($($args)*) $mods $tw $cur $in_tween $has_tween $pass) };
    (zoom ($($args:tt)*) $mods:ident $tw:ident $cur:ident $in_tween:ident $has_tween:ident $pass:tt) => { $crate::__dsl_apply_tweenable!(zoom ($($args)*) $mods $tw $cur $in_tween $has_tween $pass) };
    (zoomx ($($args:tt)*) $mods:ident $tw:ident $cur:ident $in_tween:ident $has_tween:ident $pass:tt) => { $crate::__dsl_apply_tweenable!(zoomx ($($args)*) $mods $tw $cur $in_tween $has_tween $pass) };
    (zoomy ($($args:tt)*) $mods:ident $tw:ident $cur:ident $in_tween:ident $has_tween:ident $pass:tt) => { $crate::__dsl_apply_tweenable!(zoomy ($($args)*) $mods $tw $cur $in_tween $has_tween $pass) };
    (addzoomx ($($args:tt)*) $mods:ident $tw:ident $cur:ident $in_tween:ident $has_tween:ident $pass:tt) => { $crate::__dsl_apply_tweenable!(addzoomx ($($args)*) $mods $tw $cur $in_tween $has_tween $pass) };
    (addzoomy ($($args:tt)*) $mods:ident $tw:ident $cur:ident $in_tween:ident $has_tween:ident $pass:tt) => { $crate::__dsl_apply_tweenable!(addzoomy ($($args)*) $mods $tw $cur $in_tween $has_tween $pass) };
    (zoomto ($($args:tt)*) $mods:ident $tw:ident $cur:ident $in_tween:ident $has_tween:ident $pass:tt) => { $crate::__dsl_apply_tweenable!(zoomto ($($args)*) $mods $tw $cur $in_tween $has_tween $pass) };
    (setsize ($($args:tt)*) $mods:ident $tw:ident $cur:ident $in_tween:ident $has_tween:ident $pass:tt) => { $crate::__dsl_apply_tweenable!(setsize ($($args)*) $mods $tw $cur $in_tween $has_tween $pass) };
    (cropleft ($($args:tt)*) $mods:ident $tw:ident $cur:ident $in_tween:ident $has_tween:ident $pass:tt) => { $crate::__dsl_apply_tweenable!(cropleft ($($args)*) $mods $tw $cur $in_tween $has_tween $pass) };
    (cropright ($($args:tt)*) $mods:ident $tw:ident $cur:ident $in_tween:ident $has_tween:ident $pass:tt) => { $crate::__dsl_apply_tweenable!(cropright ($($args)*) $mods $tw $cur $in_tween $has_tween $pass) };
    (croptop ($($args:tt)*) $mods:ident $tw:ident $cur:ident $in_tween:ident $has_tween:ident $pass:tt) => { $crate::__dsl_apply_tweenable!(croptop ($($args)*) $mods $tw $cur $in_tween $has_tween $pass) };
    (cropbottom ($($args:tt)*) $mods:ident $tw:ident $cur:ident $in_tween:ident $has_tween:ident $pass:tt) => { $crate::__dsl_apply_tweenable!(cropbottom ($($args)*) $mods $tw $cur $in_tween $has_tween $pass) };
    (fadeleft ($($args:tt)*) $mods:ident $tw:ident $cur:ident $in_tween:ident $has_tween:ident $pass:tt) => { $crate::__dsl_apply_tweenable!(fadeleft ($($args)*) $mods $tw $cur $in_tween $has_tween $pass) };
    (faderight ($($args:tt)*) $mods:ident $tw:ident $cur:ident $in_tween:ident $has_tween:ident $pass:tt) => { $crate::__dsl_apply_tweenable!(faderight ($($args)*) $mods $tw $cur $in_tween $has_tween $pass) };
    (fadetop ($($args:tt)*) $mods:ident $tw:ident $cur:ident $in_tween:ident $has_tween:ident $pass:tt) => { $crate::__dsl_apply_tweenable!(fadetop ($($args)*) $mods $tw $cur $in_tween $has_tween $pass) };
    (fadebottom ($($args:tt)*) $mods:ident $tw:ident $cur:ident $in_tween:ident $has_tween:ident $pass:tt) => { $crate::__dsl_apply_tweenable!(fadebottom ($($args)*) $mods $tw $cur $in_tween $has_tween $pass) };
    (visible ($($args:tt)*) $mods:ident $tw:ident $cur:ident $in_tween:ident $has_tween:ident $pass:tt) => { $crate::__dsl_apply_tweenable!(visible ($($args)*) $mods $tw $cur $in_tween $has_tween $pass) };
    (rotationx ($($args:tt)*) $mods:ident $tw:ident $cur:ident $in_tween:ident $has_tween:ident $pass:tt) => { $crate::__dsl_apply_tweenable!(rotationx ($($args)*) $mods $tw $cur $in_tween $has_tween $pass) };
    (rotationy ($($args:tt)*) $mods:ident $tw:ident $cur:ident $in_tween:ident $has_tween:ident $pass:tt) => { $crate::__dsl_apply_tweenable!(rotationy ($($args)*) $mods $tw $cur $in_tween $has_tween $pass) };
    (rotationz ($($args:tt)*) $mods:ident $tw:ident $cur:ident $in_tween:ident $has_tween:ident $pass:tt) => { $crate::__dsl_apply_tweenable!(rotationz ($($args)*) $mods $tw $cur $in_tween $has_tween $pass) };
    (addrotationx ($($args:tt)*) $mods:ident $tw:ident $cur:ident $in_tween:ident $has_tween:ident $pass:tt) => { $crate::__dsl_apply_tweenable!(addrotationx ($($args)*) $mods $tw $cur $in_tween $has_tween $pass) };
    (addrotationy ($($args:tt)*) $mods:ident $tw:ident $cur:ident $in_tween:ident $has_tween:ident $pass:tt) => { $crate::__dsl_apply_tweenable!(addrotationy ($($args)*) $mods $tw $cur $in_tween $has_tween $pass) };
    (addrotationz ($($args:tt)*) $mods:ident $tw:ident $cur:ident $in_tween:ident $has_tween:ident $pass:tt) => { $crate::__dsl_apply_tweenable!(addrotationz ($($args)*) $mods $tw $cur $in_tween $has_tween $pass) };

    ($cmd:ident ($($args:tt)*) $mods:ident $tw:ident $cur:ident $in_tween:ident $has_tween:ident false) => {
        $crate::__dsl_apply_one!($cmd ($($args)*) $mods $tw $cur _dummy_site)
    };
    ($cmd:ident ($($args:tt)*) $mods:ident $tw:ident $cur:ident $in_tween:ident $has_tween:ident true) => {};
}

#[macro_export]
#[doc(hidden)]
macro_rules! __dsl_apply_one {
    (tweensalt ($salt:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.tweensalt(($salt) as u64);
    }};

    // --- segment controls ---
    (linear ($d:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        if let ::core::option::Option::Some(seg)=$cur.take(){$tw.push(seg.build());}
        $cur = ::core::option::Option::Some($crate::anim::linear(($d) as f32));
    }};
    (accelerate ($d:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        if let ::core::option::Option::Some(seg)=$cur.take(){$tw.push(seg.build());}
        $cur = ::core::option::Option::Some($crate::anim::accelerate(($d) as f32));
    }};
    (decelerate ($d:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        if let ::core::option::Option::Some(seg)=$cur.take(){$tw.push(seg.build());}
        $cur = ::core::option::Option::Some($crate::anim::decelerate(($d) as f32));
    }};
    (ease ($d:expr, $f:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        if let ::core::option::Option::Some(seg) = $cur.take() { $tw.push(seg.build()); }
        $cur = ::core::option::Option::Some($crate::anim::ease(($d) as f32, ($f) as f32));
    }};
    (smooth ($d:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        if let ::core::option::Option::Some(seg) = $cur.take() { $tw.push(seg.build()); }
        $cur = ::core::option::Option::Some($crate::anim::smooth(($d) as f32));
    }};
    (sleep ($d:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        if let ::core::option::Option::Some(seg)=$cur.take(){$tw.push(seg.build());}
        $tw.push($crate::anim::sleep(($d) as f32));
    }};

    // --- tweenable props ---
    (xy ($x:expr, $y:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        if let ::core::option::Option::Some(mut seg)=$cur.take(){ seg=seg.x(($x) as f32).y(($y) as f32); $cur=::core::option::Option::Some(seg); }
        else { $mods.xy(($x) as f32, ($y) as f32); }
    }};
    (x ($x:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        if let ::core::option::Option::Some(mut seg)=$cur.take(){ seg=seg.x(($x) as f32); $cur=::core::option::Option::Some(seg); }
        else { $mods.x(($x) as f32); }
    }};
    (y ($y:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        if let ::core::option::Option::Some(mut seg)=$cur.take(){ seg=seg.y(($y) as f32); $cur=::core::option::Option::Some(seg); }
        else { $mods.y(($y) as f32); }
    }};
    (addx ($dx:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        if let ::core::option::Option::Some(mut seg)=$cur.take(){ seg=seg.addx(($dx) as f32); $cur=::core::option::Option::Some(seg); }
        else { $mods.addx(($dx) as f32); }
    }};
    (addy ($dy:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        if let ::core::option::Option::Some(mut seg)=$cur.take(){ seg=seg.addy(($dy) as f32); $cur=::core::option::Option::Some(seg); }
        else { $mods.addy(($dy) as f32); }
    }};

    // PARITY[StepMania Actor]: Center/CenterX/CenterY map to screen center globals.
    (Center () $mods:ident $tw:ident $cur:ident $site:ident) => {{
        let cx = $crate::space::screen_center_x();
        let cy = $crate::space::screen_center_y();
        if let ::core::option::Option::Some(mut seg) = $cur.take() {
            seg = seg.xy(cx, cy);
            $cur = ::core::option::Option::Some(seg);
        } else {
            $mods.xy(cx, cy);
        }
    }};
    (CenterX () $mods:ident $tw:ident $cur:ident $site:ident) => {{
        let cx = $crate::space::screen_center_x();
        if let ::core::option::Option::Some(mut seg) = $cur.take() {
            seg = seg.x(cx);
            $cur = ::core::option::Option::Some(seg);
        } else {
            $mods.x(cx);
        }
    }};
    (CenterY () $mods:ident $tw:ident $cur:ident $site:ident) => {{
        let cy = $crate::space::screen_center_y();
        if let ::core::option::Option::Some(mut seg) = $cur.take() {
            seg = seg.y(cy);
            $cur = ::core::option::Option::Some(seg);
        } else {
            $mods.y(cy);
        }
    }};

    // Lowercase aliases (so both Center() and center() work)
    (center ()  $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $crate::__dsl_apply_one!(Center() $mods $tw $cur $site)
    }};
    (centerx () $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $crate::__dsl_apply_one!(CenterX() $mods $tw $cur $site)
    }};
    (centery () $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $crate::__dsl_apply_one!(CenterY() $mods $tw $cur $site)
    }};

    // --- color (present both for sprite & text) ---
    (diffuse ($r:expr,$g:expr,$b:expr,$a:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        if let ::core::option::Option::Some(mut seg) = $cur.take() {
            seg = seg.diffuse(($r) as f32, ($g) as f32, ($b) as f32, ($a) as f32);
            $cur = ::core::option::Option::Some(seg);
        } else {
            $mods.diffuse([($r) as f32,($g) as f32,($b) as f32,($a) as f32]);
        }
    }};
    (alpha ($a:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        if let ::core::option::Option::Some(mut seg) = $cur.take() {
            seg = seg.alpha(($a) as f32);
            $cur = ::core::option::Option::Some(seg);
        } else {
            $mods.alpha(($a) as f32);
        }
    }};
    (diffusealpha ($a:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        if let ::core::option::Option::Some(mut seg) = $cur.take() {
            seg = seg.alpha(($a) as f32);
            $cur = ::core::option::Option::Some(seg);
        } else {
            $mods.alpha(($a) as f32);
        }
    }};

    // PARITY[StepMania/ITGmania Actor]: glow and stroke color commands.
    (glow ($r:expr,$g:expr,$b:expr,$a:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        if let ::core::option::Option::Some(mut seg) = $cur.take() {
            seg = seg.glow(($r) as f32, ($g) as f32, ($b) as f32, ($a) as f32);
            $cur = ::core::option::Option::Some(seg);
        } else {
            $mods.glow([($r) as f32,($g) as f32,($b) as f32,($a) as f32]);
        }
    }};
    (strokecolor ($r:expr,$g:expr,$b:expr,$a:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.strokecolor([($r) as f32,($g) as f32,($b) as f32,($a) as f32]);
    }};

    // PARITY[StepMania Actor]: shadowlength/shadowcolor command behavior.
    (shadowlength ($v:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.shadowlength(($v) as f32);
    }};
    (shadowlengthx ($v:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.shadowlengthx(($v) as f32);
    }};
    (shadowlengthy ($v:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.shadowlengthy(($v) as f32);
    }};
    (shadowcolor ($r:expr, $g:expr, $b:expr, $a:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.shadowcolor([($r) as f32, ($g) as f32, ($b) as f32, ($a) as f32]);
    }};

    // PARITY[StepMania/ITGmania Actor]: effect command behavior.
    (effectclock (beat) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.effectclock($crate::anim::EffectClock::Beat);
    }};
    (effectclock (time) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.effectclock($crate::anim::EffectClock::Time);
    }};
    (effectclock (music) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.effectclock($crate::anim::EffectClock::Time);
    }};
    (effectclock (seconds) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.effectclock($crate::anim::EffectClock::Time);
    }};
    (effectclock ($raw:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        let __clock_raw = ::std::format!("{}", $raw);
        let __clock = $crate::dsl::__dsl_parse_effect_clock(__clock_raw.as_str());
        $mods.effectclock(__clock);
    }};

    (diffuseramp () $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.effectmode($crate::anim::EffectMode::DiffuseRamp);
        $mods.effectperiod(1.0);
        $mods.effectcolor1([0.0, 0.0, 0.0, 1.0]);
        $mods.effectcolor2([1.0, 1.0, 1.0, 1.0]);
    }};
    (diffuseshift () $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.effectmode($crate::anim::EffectMode::DiffuseShift);
        $mods.effectperiod(1.0);
        $mods.effectcolor1([0.0, 0.0, 0.0, 1.0]);
        $mods.effectcolor2([1.0, 1.0, 1.0, 1.0]);
    }};
    (glowshift () $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.effectmode($crate::anim::EffectMode::GlowShift);
        $mods.effectperiod(1.0);
        $mods.effectcolor1([1.0, 1.0, 1.0, 0.2]);
        $mods.effectcolor2([1.0, 1.0, 1.0, 0.8]);
    }};
    (pulse () $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.effectmode($crate::anim::EffectMode::Pulse);
        $mods.effectperiod(2.0);
        $mods.effectmagnitude([0.5, 1.0, 0.0]);
    }};
    (spin () $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.effectmode($crate::anim::EffectMode::Spin);
        $mods.effectmagnitude([0.0, 0.0, 180.0]);
    }};
    (stopeffect () $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.effectmode($crate::anim::EffectMode::None);
    }};

    (effectcolor1 ($r:expr,$g:expr,$b:expr,$a:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.effectcolor1([($r) as f32, ($g) as f32, ($b) as f32, ($a) as f32]);
    }};
    (effectcolor2 ($r:expr,$g:expr,$b:expr,$a:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.effectcolor2([($r) as f32, ($g) as f32, ($b) as f32, ($a) as f32]);
    }};
    (effectperiod ($v:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.effectperiod(($v) as f32);
    }};
    (effectoffset ($v:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.effectoffset(($v) as f32);
    }};
    (effecttiming ($a:expr,$b:expr,$c:expr,$d:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        // ITGmania compatibility: 4-arg call is (ramp_to_half, hold_at_half, ramp_to_full, hold_at_zero).
        $mods.effecttiming([($a) as f32, ($b) as f32, ($c) as f32, 0.0, ($d) as f32]);
    }};
    (effecttiming ($a:expr,$b:expr,$c:expr,$d:expr,$e:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        // ITGmania compatibility: 5-arg call is (ramp_to_half, hold_at_half, ramp_to_full, hold_at_zero, hold_at_full).
        $mods.effecttiming([($a) as f32, ($b) as f32, ($c) as f32, ($e) as f32, ($d) as f32]);
    }};
    (effectmagnitude ($x:expr,$y:expr,$z:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.effectmagnitude([($x) as f32, ($y) as f32, ($z) as f32]);
    }};

    // PARITY[StepMania Actor]: zoom* commands mutate scale factors.
    (zoom ($f:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        let f=($f) as f32;
        if let ::core::option::Option::Some(mut seg)=$cur.take(){ seg=seg.zoom(f,f); $cur=::core::option::Option::Some(seg); }
        else { $mods.zoom(f); }
    }};
    (zoomx ($f:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        let f=($f) as f32;
        if let ::core::option::Option::Some(mut seg)=$cur.take(){ seg=seg.zoomx(f); $cur=::core::option::Option::Some(seg); }
        else { $mods.zoomx(f); }
    }};
    (zoomy ($f:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        let f=($f) as f32;
        if let ::core::option::Option::Some(mut seg)=$cur.take(){ seg=seg.zoomy(f); $cur=::core::option::Option::Some(seg); }
        else { $mods.zoomy(f); }
    }};
    (addzoomx ($df:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        let df=($df) as f32;
        if let ::core::option::Option::Some(mut seg)=$cur.take(){ seg=seg.addzoomx(df); $cur=::core::option::Option::Some(seg); }
        else { $mods.addzoomx(df); }
    }};
    (addzoomy ($df:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        let df=($df) as f32;
        if let ::core::option::Option::Some(mut seg)=$cur.take(){ seg=seg.addzoomy(df); $cur=::core::option::Option::Some(seg); }
        else { $mods.addzoomy(df); }
    }};

    // PARITY[StepMania Actor]: `zoomto` works from unzoomed size; `setsize` sets unzoomed size.
    (zoomto ($w:expr, $h:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        let ww = ($w) as f32;
        let hh = ($h) as f32;
        if let ::core::option::Option::Some(mut seg)=$cur.take(){ seg=seg.zoomto(ww, hh); $cur=::core::option::Option::Some(seg); }
        else { $mods.zoomto(ww, hh); }
    }};
    (setsize ($w:expr, $h:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        let ww = ($w) as f32;
        let hh = ($h) as f32;
        if let ::core::option::Option::Some(mut seg)=$cur.take(){ seg=seg.size(ww, hh); $cur=::core::option::Option::Some(seg); }
        else { $mods.size(ww, hh); }
    }};

    // --- absolute size helpers preserving aspect ---------------------
    (zoomtowidth ($w:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.zoomtowidth(($w) as f32);
    }};
    (zoomtoheight ($h:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.zoomtoheight(($h) as f32);
    }};
    (wrapwidthpixels ($w:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.wrapwidthpixels(($w) as f32);
    }};
    (vertspacing ($s:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.vertspacing(($s) as f32);
    }};
    // --- NEW: max constraints for text -------------------------------
    (maxwidth ($w:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.maxwidth(($w) as f32);
    }};
    (maxheight ($h:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.maxheight(($h) as f32);
    }};

    // static sprite bits / cropping / uv / blend ---------------------
    (align ($h:expr,$v:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.align(($h) as f32, ($v) as f32);
    }};
    (halign ($dir:ident) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.halign($crate::__ui_halign_from_ident!($dir));
    }};
    (halign ($v:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.halign(($v) as f32);
    }};
    (valign ($dir:ident) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.valign($crate::__ui_valign_from_ident!($dir));
    }};
    (valign ($v:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.valign(($v) as f32);
    }};

    (z ($v:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{ $mods.z(($v) as i16); }};
    (texcoordvelocity ($vx:expr,$vy:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.texcoordvelocity([($vx) as f32, ($vy) as f32]);
    }};
    (cropleft ($v:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        if let ::core::option::Option::Some(mut seg)=$cur.take(){ seg=seg.cropleft(($v) as f32); $cur=::core::option::Option::Some(seg); }
        else { $mods.cropleft(($v) as f32); }
    }};
    (cropright ($v:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        if let ::core::option::Option::Some(mut seg)=$cur.take(){ seg=seg.cropright(($v) as f32); $cur=::core::option::Option::Some(seg); }
        else { $mods.cropright(($v) as f32); }
    }};
    (croptop ($v:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        if let ::core::option::Option::Some(mut seg)=$cur.take(){ seg=seg.croptop(($v) as f32); $cur=::core::option::Option::Some(seg); }
        else { $mods.croptop(($v) as f32); }
    }};
    (cropbottom ($v:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        if let ::core::option::Option::Some(mut seg)=$cur.take(){ seg=seg.cropbottom(($v) as f32); $cur=::core::option::Option::Some(seg); }
        else { $mods.cropbottom(($v) as f32); }
    }};
    // edge fades (0..1 of visible width/height)
    (fadeleft ($v:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        let vv = ($v) as f32;
        if let ::core::option::Option::Some(mut seg)=$cur.take(){ seg=seg.fadeleft(vv); $cur=::core::option::Option::Some(seg); }
        else { $mods.fadeleft(vv); }
    }};
    (faderight ($v:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        let vv = ($v) as f32;
        if let ::core::option::Option::Some(mut seg)=$cur.take(){ seg=seg.faderight(vv); $cur=::core::option::Option::Some(seg); }
        else { $mods.faderight(vv); }
    }};
    (fadetop ($v:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        let vv = ($v) as f32;
        if let ::core::option::Option::Some(mut seg)=$cur.take(){ seg=seg.fadetop(vv); $cur=::core::option::Option::Some(seg); }
        else { $mods.fadetop(vv); }
    }};
    (fadebottom ($v:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        let vv = ($v) as f32;
        if let ::core::option::Option::Some(mut seg)=$cur.take(){ seg=seg.fadebottom(vv); $cur=::core::option::Option::Some(seg); }
        else { $mods.fadebottom(vv); }
    }};
    // PARITY[StepMania/ITGmania Sprite]: `setstate(i)` chooses row-major frame index.
    (setstate ($i:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.setstate(($i) as u32);
    }};
    // animation control
    (animate ($v:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.animate(($v) as bool);
    }};
    (setallstatedelays ($s:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.setallstatedelays(($s) as f32);
    }};
    // PARITY[StepMania/ITGmania Sprite]: `customtexturerect` uses normalized top-left UVs.
    (customtexturerect ($u0:expr, $v0:expr, $u1:expr, $v1:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.customtexturerect([($u0) as f32, ($v0) as f32, ($u1) as f32, ($v1) as f32]);
    }};

    // --- visibility (immediate or inside a tween) ---
    (visible ($v:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        if let ::core::option::Option::Some(mut seg)=$cur.take(){ seg=seg.set_visible(($v) as bool); $cur=::core::option::Option::Some(seg); }
        else { $mods.visible(($v) as bool); }
    }};

    // --- rotation (degrees) ---
    (rotationx ($deg:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        let d=($deg) as f32;
        if let ::core::option::Option::Some(mut seg)=$cur.take(){ seg=seg.rotationx(d); $cur=::core::option::Option::Some(seg); }
        else { $mods.rotationx(d); }
    }};

    (rotationy ($deg:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        let d=($deg) as f32;
        if let ::core::option::Option::Some(mut seg)=$cur.take(){ seg=seg.rotationy(d); $cur=::core::option::Option::Some(seg); }
        else { $mods.rotationy(d); }
    }};

    (rotationz ($deg:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        let d=($deg) as f32;
        if let ::core::option::Option::Some(mut seg)=$cur.take(){ seg=seg.rotationz(d); $cur=::core::option::Option::Some(seg); }
        else { $mods.rotationz(d); }
    }};

    (addrotationx ($ddeg:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        let dd=($ddeg) as f32;
        if let ::core::option::Option::Some(mut seg)=$cur.take(){ seg=seg.addrotationx(dd); $cur=::core::option::Option::Some(seg); }
        else { $mods.addrotationx(dd); }
    }};

    (addrotationy ($ddeg:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        let dd=($ddeg) as f32;
        if let ::core::option::Option::Some(mut seg)=$cur.take(){ seg=seg.addrotationy(dd); $cur=::core::option::Option::Some(seg); }
        else { $mods.addrotationy(dd); }
    }};

    (addrotationz ($ddeg:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        let dd=($ddeg) as f32;
        if let ::core::option::Option::Some(mut seg)=$cur.take(){ seg=seg.addrotationz(dd); $cur=::core::option::Option::Some(seg); }
        else { $mods.addrotationz(dd); }
    }};

    // blends: normal, add, multiply, subtract
    (blend (normal) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.blend($crate::render::BlendMode::Alpha);
    }};
    (blend (add) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.blend($crate::render::BlendMode::Add);
    }};
    (blend (multiply) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.blend($crate::render::BlendMode::Multiply);
    }};
    (blend (subtract) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.blend($crate::render::BlendMode::Subtract);
    }};
    (MaskSource () $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.mask_source();
    }};
    (masksource () $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.mask_source();
    }};
    (MaskDest () $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.mask_dest();
    }};
    (maskdest () $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.mask_dest();
    }};

    // Text properties (SM-compatible)
    (font ($n:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{ $mods.font($n); }};
    (settext ($s:literal) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.settext($crate::actors::TextContent::Static($s));
    }};
    (settext ($s:expr) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.settext($crate::actors::TextContent::from($s));
    }};
    (horizalign ($dir:ident) $mods:ident $tw:ident $cur:ident $site:ident) => {{
        $mods.horizalign($crate::__ui_textalign_from_ident!($dir));
    }};

    // unknown
    ($other:ident ( $($args:expr),* ) $mods:ident $tw:ident $cur:ident $site:ident) => {
        compile_error!(concat!("act!: unknown or removed command: ", stringify!($other)));
    };
}

#[macro_export]
#[doc(hidden)]
macro_rules! __act_from_builder {
    (($($tail:tt)+) $builder:expr) => {{
        let mut __mods = $builder;
        let mut __base_tw = ::std::vec::Vec::<$crate::anim::Step>::new();
        let mut __base_cur: ::core::option::Option<$crate::anim::SegmentBuilder> = None;
        let mut __in_tween = false;
        let mut __has_tween = false;
        $crate::__dsl_apply_split!(
            ($($tail)+)
            __mods __base_tw __base_cur __in_tween __has_tween false
        );
        const __SITE_BASE: u64 = $crate::runtime::site_base(file!(), line!(), column!());
        if __has_tween {
            __mods.build_tweened(__SITE_BASE, || {
                let mut __mods = $crate::dsl::TweenProgramTarget;
                let mut __tw = $crate::dsl::TweenSteps::new();
                let mut __cur: ::core::option::Option<$crate::anim::SegmentBuilder> = None;
                let mut __in_tween = false;
                let mut _has_tween = false;
                $crate::__dsl_apply_split!(
                    ($($tail)+)
                    __mods __tw __cur __in_tween _has_tween true
                );
                if let ::core::option::Option::Some(seg) = __cur.take() {
                    __tw.push(seg.build());
                }
                __tw
            })
        } else {
            __mods.build(__SITE_BASE)
        }
    }};
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actors::SpriteSource;
    use crate::texture::{TextureContext, TextureMeta};
    use std::cell::Cell;
    use std::mem::size_of;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct TestTextureContext;

    #[cfg(target_pointer_width = "64")]
    #[test]
    fn source_steps_keep_tween_builders_compact() {
        assert!(size_of::<anim::Step>() <= 464);
        assert!(size_of::<SpriteBuilder>() <= 328);
        assert!(size_of::<TextBuilder>() <= 280);
    }

    impl TextureContext for TestTextureContext {
        fn texture_registry_generation(&self) -> u64 {
            7
        }

        fn texture_dims(&self, key: &str) -> Option<TextureMeta> {
            match key {
                "banner" => Some(TextureMeta { w: 320, h: 120 }),
                "sheet" => Some(TextureMeta { w: 400, h: 200 }),
                "grid" => Some(TextureMeta { w: 500, h: 300 }),
                _ => None,
            }
        }

        fn sprite_sheet_dims(&self, key: &str) -> (u32, u32) {
            match key {
                "sheet" => (4, 2),
                _ => (1, 1),
            }
        }

        fn texture_handle(&self, _key: &str) -> deadlib_render::TextureHandle {
            99
        }
    }

    #[test]
    fn dsl_alignment_ident_macros_resolve_values() {
        assert_eq!(crate::__ui_halign_from_ident!(left), 0.0);
        assert_eq!(crate::__ui_halign_from_ident!(center), 0.5);
        assert_eq!(crate::__ui_halign_from_ident!(right), 1.0);
        assert_eq!(crate::__ui_valign_from_ident!(top), 0.0);
        assert_eq!(crate::__ui_valign_from_ident!(middle), 0.5);
        assert_eq!(crate::__ui_valign_from_ident!(center), 0.5);
        assert_eq!(crate::__ui_valign_from_ident!(bottom), 1.0);
        assert!(matches!(
            crate::__ui_textalign_from_ident!(left),
            TextAlign::Left
        ));
        assert!(matches!(
            crate::__ui_textalign_from_ident!(center),
            TextAlign::Center
        ));
        assert!(matches!(
            crate::__ui_textalign_from_ident!(right),
            TextAlign::Right
        ));
    }

    #[test]
    fn dsl_apply_macro_updates_sprite_builder() {
        crate::space::set_current_metrics(crate::space::Metrics {
            left: -100.0,
            right: 100.0,
            bottom: -50.0,
            top: 50.0,
        });

        let mut sprite = SpriteBuilder::solid();
        let steps = SmallVec::<[anim::Step; 4]>::new();
        let mut current: Option<anim::SegmentBuilder> = None;
        crate::__dsl_apply!(
            (Center(): diffuse(0.1, 0.2, 0.3, 0.4): halign(left): valign(bottom): blend(add))
            sprite
            steps
            current
            _site
        );

        assert!(current.is_none());
        assert!(steps.is_empty());
        let actor = sprite.build(0);
        let Actor::Sprite {
            align,
            offset,
            tint,
            blend,
            ..
        } = actor
        else {
            panic!("DSL should build a sprite");
        };
        assert_eq!(align, [0.0, 1.0]);
        assert_eq!(offset, [100.0, 50.0]);
        assert_eq!(tint, [0.1, 0.2, 0.3, 0.4]);
        assert_eq!(blend, deadlib_render::BlendMode::Add);

        crate::space::set_current_metrics(crate::space::metrics_for_window(854, 480));
    }

    #[test]
    fn dsl_apply_macro_updates_text_builder() {
        let mut text = TextBuilder::new();
        let steps = SmallVec::<[anim::Step; 4]>::new();
        let current: Option<anim::SegmentBuilder> = None;
        crate::__dsl_apply!(
            (font("common"): settext("Ready"): horizalign(center): blend(multiply))
            text
            steps
            current
            _site
        );

        assert!(current.is_none());
        assert!(steps.is_empty());
        let actor = text.build(0);
        let Actor::Text {
            font,
            content,
            align_text,
            blend,
            ..
        } = actor
        else {
            panic!("DSL should build text");
        };
        assert_eq!(font, "common");
        assert_eq!(content.as_str(), "Ready");
        assert_eq!(align_text, TextAlign::Center);
        assert_eq!(blend, deadlib_render::BlendMode::Multiply);
    }

    #[test]
    fn act_from_builder_macro_builds_actor() {
        let actor = crate::__act_from_builder!(
            (xy(12.0, 34.0): setsize(56.0, 78.0): diffuse(0.2, 0.3, 0.4, 0.5))
            SpriteBuilder::solid()
        );

        let Actor::Sprite {
            offset, size, tint, ..
        } = actor
        else {
            panic!("act builder macro should build a sprite");
        };
        assert_eq!(offset, [12.0, 34.0]);
        assert!(matches!(size[0], crate::actors::SizeSpec::Px(56.0)));
        assert!(matches!(size[1], crate::actors::SizeSpec::Px(78.0)));
        assert_eq!(tint, [0.2, 0.3, 0.4, 0.5]);
    }

    #[test]
    fn act_macro_builds_tween_program_only_on_cache_miss() {
        fn counted(calls: &Cell<u32>, value: f32) -> f32 {
            calls.set(calls.get() + 1);
            value
        }

        fn build(
            initial_calls: &Cell<u32>,
            tween_calls: &Cell<u32>,
            static_calls: &Cell<u32>,
        ) -> Actor {
            crate::__act_from_builder!(
                (x(counted(initial_calls, 4.0)):
                linear(counted(tween_calls, 1.0)):
                x(counted(tween_calls, 10.0)):
                z(counted(static_calls, 7.0)))
                SpriteBuilder::solid()
            )
        }

        runtime::clear_all();
        let initial_calls = Cell::new(0);
        let tween_calls = Cell::new(0);
        let static_calls = Cell::new(0);

        let _ = build(&initial_calls, &tween_calls, &static_calls);
        let _ = build(&initial_calls, &tween_calls, &static_calls);
        assert_eq!(initial_calls.get(), 2);
        assert_eq!(tween_calls.get(), 2);
        assert_eq!(static_calls.get(), 2);

        runtime::tick(0.5);
        let actor = build(&initial_calls, &tween_calls, &static_calls);
        let Actor::Sprite { offset, z, .. } = actor else {
            panic!("DSL should build a tweened sprite");
        };
        assert!((offset[0] - 7.0).abs() < 0.0001);
        assert_eq!(z, 7);
        assert_eq!(initial_calls.get(), 3);
        assert_eq!(tween_calls.get(), 2);
        assert_eq!(static_calls.get(), 3);
        runtime::clear_all();
    }

    #[test]
    fn sprite_native_dims_resolves_texture_dims() {
        let dims = sprite_native_dims(
            &SpriteSource::TextureStatic("banner"),
            None,
            None,
            None,
            &TestTextureContext,
        );

        assert_eq!(dims, [320.0, 120.0]);
    }

    #[test]
    fn sprite_native_dims_applies_uv_rect() {
        let dims = sprite_native_dims(
            &SpriteSource::TextureStatic("banner"),
            Some([0.25, 0.0, 0.75, 0.5]),
            None,
            None,
            &TestTextureContext,
        );

        assert_eq!(dims, [160.0, 60.0]);
    }

    #[test]
    fn sprite_native_dims_uses_sheet_hints() {
        let dims = sprite_native_dims(
            &SpriteSource::TextureStatic("sheet"),
            None,
            None,
            None,
            &TestTextureContext,
        );

        assert_eq!(dims, [100.0, 100.0]);
    }

    #[test]
    fn sprite_native_dims_uses_explicit_grid_for_cell() {
        let dims = sprite_native_dims(
            &SpriteSource::TextureStatic("grid"),
            None,
            Some((2, u32::MAX)),
            Some((5, 3)),
            &TestTextureContext,
        );

        assert_eq!(dims, [100.0, 100.0]);
    }

    #[test]
    fn sprite_native_dims_returns_unit_for_solid() {
        let dims = sprite_native_dims(&SpriteSource::Solid, None, None, None, &TestTextureContext);

        assert_eq!(dims, [1.0, 1.0]);
    }

    #[test]
    fn zoomto_with_native_dims_preserves_native_scale() {
        let mut sprite = SpriteBuilder::static_texture("banner");
        sprite.zoomto_with_native_dims(160.0, 30.0, [320.0, 120.0]);

        let actor = sprite.build(0);
        let Actor::Sprite { scale, .. } = actor else {
            panic!("sprite builder should produce a sprite actor");
        };

        assert_eq!(scale, [0.5, 0.25]);
    }

    #[test]
    fn zoomto_with_texture_context_uses_native_texture_dims() {
        let mut sprite = SpriteBuilder::static_texture("banner");
        sprite.zoomto_with_texture_context(160.0, 30.0, &TestTextureContext);

        let actor = sprite.build(0);
        let Actor::Sprite { scale, .. } = actor else {
            panic!("sprite builder should produce a sprite actor");
        };

        assert_eq!(scale, [0.5, 0.25]);
    }

    #[test]
    fn static_texture_cached_with_texture_context_builds_cached_source() {
        let cached_handle = AtomicU64::new(0);
        let cached_generation = AtomicU64::new(u64::MAX);
        let sprite = SpriteBuilder::static_texture_cached_with_texture_context(
            "banner",
            &cached_handle,
            &cached_generation,
            &TestTextureContext,
        );

        let actor = sprite.build(0);
        let Actor::Sprite { source, .. } = actor else {
            panic!("sprite builder should produce a sprite actor");
        };

        assert!(matches!(
            source,
            SpriteSource::TextureStaticHandle {
                key: "banner",
                handle: 99,
                generation: 7
            }
        ));
        assert_eq!(cached_handle.load(Ordering::Relaxed), 99);
        assert_eq!(cached_generation.load(Ordering::Relaxed), 7);
    }

    #[test]
    fn build_with_texture_context_seeds_tween_size_from_texture() {
        runtime::clear_all();
        let sprite = SpriteBuilder::static_texture("banner");
        let actor = sprite.build_tweened_with_texture_context(0, &TestTextureContext, || {
            let mut steps = TweenSteps::new();
            steps.push(anim::sleep(0.0));
            steps
        });
        let Actor::Sprite { size, scale, .. } = actor else {
            panic!("sprite builder should produce a sprite actor");
        };

        assert!(matches!(size[0], crate::actors::SizeSpec::Px(320.0)));
        assert!(matches!(size[1], crate::actors::SizeSpec::Px(120.0)));
        assert_eq!(scale, [1.0, 1.0]);
        runtime::clear_all();
    }
}

#[inline(always)]
pub fn sprite_native_dims<T: TextureContext + ?Sized>(
    source: &SpriteSource,
    uv: Option<[f32; 4]>,
    cell: Option<(u32, u32)>,
    grid: Option<(u32, u32)>,
    texture_ctx: &T,
) -> [f32; 2] {
    match source {
        SpriteSource::Solid => [1.0, 1.0],
        SpriteSource::TextureStatic(key) | SpriteSource::TextureStaticHandle { key, .. } => {
            texture_native_dims(key, uv, cell, grid, texture_ctx)
        }
        SpriteSource::Texture(key) | SpriteSource::TextureHandle { key, .. } => {
            texture_native_dims(key.as_ref(), uv, cell, grid, texture_ctx)
        }
        // Arena sources are final actor payloads whose dimensions were resolved
        // by their owning noteskin/widget before the texture key was replaced.
        SpriteSource::ArenaTextureHandle { .. } => [0.0, 0.0],
    }
}

#[inline(always)]
fn texture_native_dims<T: TextureContext + ?Sized>(
    key: &str,
    uv: Option<[f32; 4]>,
    cell: Option<(u32, u32)>,
    grid: Option<(u32, u32)>,
    texture_ctx: &T,
) -> [f32; 2] {
    let Some(meta) = texture_ctx.texture_dims(key) else {
        return [0.0, 0.0];
    };
    let (mut tw, mut th) = (meta.w as f32, meta.h as f32);

    if let Some([u0, v0, u1, v1]) = uv {
        tw *= (u1 - u0).abs().max(1e-6);
        th *= (v1 - v0).abs().max(1e-6);
        return [tw, th];
    }

    let effective_cell = if cell.is_some() {
        cell
    } else {
        let (gc, gr) = grid.unwrap_or_else(|| texture_ctx.sprite_sheet_dims(key));
        if gc.saturating_mul(gr) > 1 {
            Some((0, u32::MAX))
        } else {
            None
        }
    };

    if effective_cell.is_some() {
        let (gc, gr) = grid.unwrap_or_else(|| texture_ctx.sprite_sheet_dims(key));
        let cols = gc.max(1);
        let rows = gr.max(1);
        tw /= cols as f32;
        th /= rows as f32;
    }

    [tw, th]
}

/* ======================== SPRITE/QUAD CORE ======================== */

#[doc(hidden)]
pub struct SpriteBuilder {
    source: SpriteSource,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    hx: f32,
    vy: f32,
    tint: [f32; 4],
    glow: [f32; 4],
    z: i16,
    vis: bool,
    fx: bool,
    fy: bool,
    cl: f32,
    cr: f32,
    ct: f32,
    cb: f32,
    fl: f32,
    fr: f32,
    ft: f32,
    fb: f32,
    blend: BlendMode,
    mask_source: bool,
    mask_dest: bool,
    rot_x: f32,
    rot_y: f32,
    rot_z: f32,
    uv: Option<[f32; 4]>,
    cell: Option<(u32, u32)>,
    grid: Option<(u32, u32)>,
    texv: Option<[f32; 2]>,
    anim_enable: bool,
    state_delay: f32,
    effect: anim::EffectState,
    sx: f32,
    sy: f32,
    shx: f32,
    shy: f32,
    shc: [f32; 4],
    tween_salt: u64,
}

impl SpriteBuilder {
    #[inline(always)]
    pub fn with_source(source: SpriteSource) -> Self {
        Self {
            source,
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
            hx: 0.5,
            vy: 0.5,
            tint: [1.0, 1.0, 1.0, 1.0],
            glow: [1.0, 1.0, 1.0, 0.0],
            z: 0,
            vis: true,
            fx: false,
            fy: false,
            cl: 0.0,
            cr: 0.0,
            ct: 0.0,
            cb: 0.0,
            fl: 0.0,
            fr: 0.0,
            ft: 0.0,
            fb: 0.0,
            blend: BlendMode::Alpha,
            mask_source: false,
            mask_dest: false,
            rot_x: 0.0,
            rot_y: 0.0,
            rot_z: 0.0,
            uv: None,
            cell: None,
            grid: None,
            texv: None,
            anim_enable: false,
            state_delay: 0.1,
            effect: anim::EffectState::default(),
            sx: 1.0,
            sy: 1.0,
            shx: 0.0,
            shy: 0.0,
            shc: [0.0, 0.0, 0.0, 0.5],
            tween_salt: 0,
        }
    }

    #[inline(always)]
    pub fn texture<T: IntoTextureKey>(tex: T) -> Self {
        Self::with_source(tex.into_sprite_source())
    }

    #[inline(always)]
    pub fn static_texture(tex: &'static str) -> Self {
        Self::with_source(SpriteSource::TextureStatic(tex))
    }

    #[inline(always)]
    pub fn static_texture_cached_with_texture_context<T: TextureContext + ?Sized>(
        tex: &'static str,
        cached_handle: &AtomicU64,
        cached_generation: &AtomicU64,
        texture_ctx: &T,
    ) -> Self {
        Self::with_source(cached_static_texture_source(
            tex,
            cached_handle,
            cached_generation,
            texture_ctx,
        ))
    }

    #[inline(always)]
    pub fn solid() -> Self {
        Self::with_source(SpriteSource::Solid)
    }

    #[inline(always)]
    pub fn source(&self) -> &SpriteSource {
        &self.source
    }

    #[inline(always)]
    pub const fn uv_rect(&self) -> Option<[f32; 4]> {
        self.uv
    }

    #[inline(always)]
    pub const fn cell(&self) -> Option<(u32, u32)> {
        self.cell
    }

    #[inline(always)]
    pub const fn grid(&self) -> Option<(u32, u32)> {
        self.grid
    }

    #[inline(always)]
    pub fn tweensalt(&mut self, salt: u64) {
        self.tween_salt = salt;
    }

    #[inline(always)]
    pub fn xy(&mut self, x: f32, y: f32) {
        self.x = x;
        self.y = y;
    }

    #[inline(always)]
    pub fn x(&mut self, x: f32) {
        self.x = x;
    }

    #[inline(always)]
    pub fn y(&mut self, y: f32) {
        self.y = y;
    }

    #[inline(always)]
    pub fn addx(&mut self, dx: f32) {
        self.x += dx;
    }

    #[inline(always)]
    pub fn addy(&mut self, dy: f32) {
        self.y += dy;
    }

    #[inline(always)]
    pub fn align(&mut self, h: f32, v: f32) {
        self.hx = h;
        self.vy = v;
    }

    #[inline(always)]
    pub fn halign(&mut self, h: f32) {
        self.hx = h;
    }

    #[inline(always)]
    pub fn valign(&mut self, v: f32) {
        self.vy = v;
    }

    #[inline(always)]
    pub fn z(&mut self, z: i16) {
        self.z = z;
    }

    #[inline(always)]
    pub fn diffuse(&mut self, rgba: [f32; 4]) {
        self.tint = rgba;
    }

    #[inline(always)]
    pub fn alpha(&mut self, a: f32) {
        self.tint[3] = a;
    }

    #[inline(always)]
    pub fn glow(&mut self, rgba: [f32; 4]) {
        self.glow = rgba;
    }

    #[inline(always)]
    pub fn blend(&mut self, blend: BlendMode) {
        self.blend = blend;
    }

    #[inline(always)]
    pub fn size(&mut self, w: f32, h: f32) {
        self.w = w;
        self.h = h;
    }

    #[inline(always)]
    pub fn zoom(&mut self, f: f32) {
        self.sx = f;
        self.sy = f;
    }

    #[inline(always)]
    pub fn zoomx(&mut self, x: f32) {
        self.sx = x;
    }

    #[inline(always)]
    pub fn zoomy(&mut self, y: f32) {
        self.sy = y;
    }

    #[inline(always)]
    pub fn addzoomx(&mut self, dx: f32) {
        self.sx += dx;
    }

    #[inline(always)]
    pub fn addzoomy(&mut self, dy: f32) {
        self.sy += dy;
    }

    #[inline(always)]
    pub fn zoomto(&mut self, tw: f32, th: f32) {
        self.zoomto_with_native_dims(tw, th, [0.0, 0.0]);
    }

    #[inline(always)]
    pub fn zoomto_with_native_dims(&mut self, tw: f32, th: f32, native_dims: [f32; 2]) {
        let [nw, nh] = native_dims;
        let base_w = if self.w == 0.0 { nw } else { self.w };
        let base_h = if self.h == 0.0 { nh } else { self.h };
        self.sx = if base_w == 0.0 { 0.0 } else { tw / base_w };
        self.sy = if base_h == 0.0 { 0.0 } else { th / base_h };
    }

    #[inline(always)]
    pub fn zoomto_with_texture_context<T: TextureContext + ?Sized>(
        &mut self,
        tw: f32,
        th: f32,
        texture_ctx: &T,
    ) {
        let native_dims =
            sprite_native_dims(&self.source, self.uv, self.cell, self.grid, texture_ctx);
        self.zoomto_with_native_dims(tw, th, native_dims);
    }

    #[inline(always)]
    pub fn zoomtowidth(&mut self, new_w: f32) {
        if self.w > 0.0 && self.h > 0.0 {
            let aspect = self.h / self.w;
            self.w = new_w;
            self.h = self.w * aspect;
        } else {
            self.w = new_w;
        }
    }

    #[inline(always)]
    pub fn zoomtoheight(&mut self, new_h: f32) {
        if self.w > 0.0 && self.h > 0.0 {
            let aspect = self.w / self.h;
            self.h = new_h;
            self.w = self.h * aspect;
        } else {
            self.h = new_h;
        }
    }

    #[inline(always)]
    pub fn mask_source(&mut self) {
        self.mask_source = true;
    }

    #[inline(always)]
    pub fn mask_dest(&mut self) {
        self.mask_dest = true;
    }

    #[inline(always)]
    pub fn texcoordvelocity(&mut self, vel: [f32; 2]) {
        self.texv = Some(vel);
    }

    #[inline(always)]
    pub fn cropleft(&mut self, v: f32) {
        self.cl = v;
    }

    #[inline(always)]
    pub fn cropright(&mut self, v: f32) {
        self.cr = v;
    }

    #[inline(always)]
    pub fn croptop(&mut self, v: f32) {
        self.ct = v;
    }

    #[inline(always)]
    pub fn cropbottom(&mut self, v: f32) {
        self.cb = v;
    }

    #[inline(always)]
    pub fn fadeleft(&mut self, v: f32) {
        self.fl = v;
    }

    #[inline(always)]
    pub fn faderight(&mut self, v: f32) {
        self.fr = v;
    }

    #[inline(always)]
    pub fn fadetop(&mut self, v: f32) {
        self.ft = v;
    }

    #[inline(always)]
    pub fn fadebottom(&mut self, v: f32) {
        self.fb = v;
    }

    #[inline(always)]
    pub fn setstate(&mut self, i: u32) {
        self.cell = Some((i, u32::MAX));
        self.grid = None;
        self.uv = None;
    }

    #[inline(always)]
    pub fn animate(&mut self, v: bool) {
        self.anim_enable = v;
    }

    #[inline(always)]
    pub fn setallstatedelays(&mut self, s: f32) {
        self.state_delay = s.max(0.0);
    }

    #[inline(always)]
    pub fn customtexturerect(&mut self, uv: [f32; 4]) {
        self.uv = Some(uv);
        self.cell = None;
        self.grid = None;
    }

    #[inline(always)]
    pub fn visible(&mut self, v: bool) {
        self.vis = v;
    }

    #[inline(always)]
    pub fn rotationx(&mut self, d: f32) {
        self.rot_x = d;
    }

    #[inline(always)]
    pub fn rotationy(&mut self, d: f32) {
        self.rot_y = d;
    }

    #[inline(always)]
    pub fn rotationz(&mut self, d: f32) {
        self.rot_z = d;
    }

    #[inline(always)]
    pub fn addrotationx(&mut self, dd: f32) {
        self.rot_x += dd;
    }

    #[inline(always)]
    pub fn addrotationy(&mut self, dd: f32) {
        self.rot_y += dd;
    }

    #[inline(always)]
    pub fn addrotationz(&mut self, dd: f32) {
        self.rot_z += dd;
    }

    #[inline(always)]
    pub fn shadowlength(&mut self, v: f32) {
        self.shx = v;
        self.shy = -v;
    }

    #[inline(always)]
    pub fn shadowlengthx(&mut self, v: f32) {
        self.shx = v;
    }

    #[inline(always)]
    pub fn shadowlengthy(&mut self, v: f32) {
        self.shy = -v;
    }

    #[inline(always)]
    pub fn shadowcolor(&mut self, c: [f32; 4]) {
        self.shc = c;
    }

    #[inline(always)]
    pub fn effectclock(&mut self, clock: anim::EffectClock) {
        self.effect.clock = clock;
    }

    #[inline(always)]
    pub fn effectmode(&mut self, mode: anim::EffectMode) {
        self.effect.mode = mode;
    }

    #[inline(always)]
    pub fn effectcolor1(&mut self, color: [f32; 4]) {
        self.effect.color1 = color;
    }

    #[inline(always)]
    pub fn effectcolor2(&mut self, color: [f32; 4]) {
        self.effect.color2 = color;
    }

    #[inline(always)]
    pub fn effectperiod(&mut self, v: f32) {
        if v > 0.0 {
            self.effect.period = v;
            self.effect.timing = [v * 0.5, 0.0, v * 0.5, 0.0, 0.0];
        }
    }

    #[inline(always)]
    pub fn effectoffset(&mut self, v: f32) {
        self.effect.offset = v;
    }

    #[inline(always)]
    pub fn effecttiming(&mut self, v: [f32; 5]) {
        let timing = [
            v[0].max(0.0),
            v[1].max(0.0),
            v[2].max(0.0),
            v[3].max(0.0),
            v[4].max(0.0),
        ];
        let total = timing[0] + timing[1] + timing[2] + timing[3] + timing[4];
        if total > 0.0 {
            self.effect.timing = timing;
            self.effect.period = total;
        }
    }

    #[inline(always)]
    pub fn effectmagnitude(&mut self, v: [f32; 3]) {
        self.effect.magnitude = v;
    }

    #[inline(always)]
    pub fn strokecolor(&mut self, _rgba: [f32; 4]) {}

    #[inline(always)]
    pub fn font(&mut self, _font: &'static str) {}

    #[inline(always)]
    pub fn settext(&mut self, _content: TextContent) {}

    #[inline(always)]
    pub fn horizalign(&mut self, _align: TextAlign) {}

    #[inline(always)]
    pub fn wrapwidthpixels(&mut self, _w: f32) {}

    #[inline(always)]
    pub fn vertspacing(&mut self, _s: f32) {}

    #[inline(always)]
    pub fn maxwidth(&mut self, _w: f32) {}

    #[inline(always)]
    pub fn maxheight(&mut self, _h: f32) {}

    #[inline(always)]
    pub fn build_tweened(self, site_base: u64, build_steps: impl FnOnce() -> TweenSteps) -> Actor {
        self.build_tweened_with_native_dims(site_base, None, build_steps)
    }

    #[inline(always)]
    fn build_tweened_with_native_dims(
        mut self,
        site_base: u64,
        native_dims: Option<[f32; 2]>,
        build_steps: impl FnOnce() -> TweenSteps,
    ) -> Actor {
        if self.w == 0.0 && self.h == 0.0 {
            if let Some([nw, nh]) = native_dims {
                self.w = nw;
                self.h = nh;
            }
        }
        let mut init = anim::TweenState::default();
        init.x = self.x;
        init.y = self.y;
        init.w = self.w;
        init.h = self.h;
        init.hx = self.hx;
        init.vy = self.vy;
        init.tint = self.tint;
        init.glow = self.glow;
        init.visible = self.vis;
        init.flip_x = self.fx;
        init.flip_y = self.fy;
        init.rot_x = self.rot_x;
        init.rot_y = self.rot_y;
        init.rot_z = self.rot_z;
        init.fade_l = self.fl;
        init.fade_r = self.fr;
        init.fade_t = self.ft;
        init.fade_b = self.fb;
        init.crop_l = self.cl;
        init.crop_r = self.cr;
        init.crop_t = self.ct;
        init.crop_b = self.cb;
        init.scale = [self.sx, self.sy];

        let sid = runtime::site_id(site_base, self.tween_salt);
        let state = runtime::materialize_lazy(sid, init, build_steps);
        self.x = state.x;
        self.y = state.y;
        self.w = state.w;
        self.h = state.h;
        self.hx = state.hx;
        self.vy = state.vy;
        self.tint = state.tint;
        self.glow = state.glow;
        self.vis = state.visible;
        self.fx = state.flip_x;
        self.fy = state.flip_y;
        self.rot_x = state.rot_x;
        self.rot_y = state.rot_y;
        self.rot_z = state.rot_z;
        self.fl = state.fade_l;
        self.fr = state.fade_r;
        self.ft = state.fade_t;
        self.fb = state.fade_b;
        self.cl = state.crop_l;
        self.cr = state.crop_r;
        self.ct = state.crop_t;
        self.cb = state.crop_b;
        self.sx = state.scale[0];
        self.sy = state.scale[1];
        self.build_with_native_dims(site_base, None)
    }

    #[inline(always)]
    pub fn build(self, site_base: u64) -> Actor {
        self.build_with_native_dims(site_base, None)
    }

    #[inline(always)]
    pub fn build_with_native_dims(
        mut self,
        _site_base: u64,
        _native_dims: Option<[f32; 2]>,
    ) -> Actor {
        if self.sx < 0.0 {
            self.fx = !self.fx;
            self.sx = -self.sx;
        }
        if self.sy < 0.0 {
            self.fy = !self.fy;
            self.sy = -self.sy;
        }

        let scale_carry = if self.w != 0.0 || self.h != 0.0 {
            self.w *= self.sx;
            self.h *= self.sy;
            if self.w == 0.0 && self.h == 0.0 {
                [0.0, 0.0]
            } else {
                [1.0, 1.0]
            }
        } else {
            [self.sx, self.sy]
        };

        Actor::Sprite {
            align: [self.hx, self.vy],
            offset: [self.x, self.y],
            world_z: 0.0,
            size: [SizeSpec::Px(self.w), SizeSpec::Px(self.h)],
            source: self.source,
            tint: self.tint,
            glow: self.glow,
            z: self.z,
            cell: self.cell,
            grid: self.grid,
            uv_rect: self.uv,
            visible: self.vis,
            flip_x: self.fx,
            flip_y: self.fy,
            cropleft: self.cl,
            cropright: self.cr,
            croptop: self.ct,
            cropbottom: self.cb,
            fadeleft: self.fl,
            faderight: self.fr,
            fadetop: self.ft,
            fadebottom: self.fb,
            blend: self.blend,
            mask_source: self.mask_source,
            mask_dest: self.mask_dest,
            rot_x_deg: self.rot_x,
            rot_y_deg: self.rot_y,
            rot_z_deg: self.rot_z,
            local_offset: [0.0, 0.0],
            local_offset_rot_sin_cos: [0.0, 1.0],
            texcoordvelocity: self.texv,
            animate: self.anim_enable,
            state_delay: self.state_delay,
            scale: scale_carry,
            shadow_len: [self.shx, self.shy],
            shadow_color: self.shc,
            effect: self.effect,
        }
    }

    #[inline(always)]
    pub fn build_with_texture_context<T: TextureContext + ?Sized>(
        self,
        site_base: u64,
        texture_ctx: &T,
    ) -> Actor {
        let native_dims =
            sprite_native_dims(&self.source, self.uv, self.cell, self.grid, texture_ctx);
        self.build_with_native_dims(site_base, Some(native_dims))
    }

    #[inline(always)]
    pub fn build_tweened_with_texture_context<T: TextureContext + ?Sized>(
        self,
        site_base: u64,
        texture_ctx: &T,
        build_steps: impl FnOnce() -> TweenSteps,
    ) -> Actor {
        let native_dims =
            sprite_native_dims(&self.source, self.uv, self.cell, self.grid, texture_ctx);
        self.build_tweened_with_native_dims(site_base, Some(native_dims), build_steps)
    }
}

/* ============================== TEXT =============================== */

#[doc(hidden)]
pub struct TextBuilder {
    x: f32,
    y: f32,
    hx: f32,
    vy: f32,
    color: [f32; 4],
    glow: [f32; 4],
    stroke_color: Option<[f32; 4]>,
    font: &'static str,
    content: TextContent,
    talign: TextAlign,
    z: i16,
    sx: f32,
    sy: f32,
    fit_w: Option<f32>,
    fit_h: Option<f32>,
    wrap_width_pixels: Option<i32>,
    line_spacing: Option<i32>,
    max_w: Option<f32>,
    max_h: Option<f32>,
    saw_max_w: bool,
    saw_max_h: bool,
    max_w_pre_zoom: bool,
    max_h_pre_zoom: bool,
    blend: BlendMode,
    effect: anim::EffectState,
    shx: f32,
    shy: f32,
    shc: [f32; 4],
    tween_salt: u64,
}

impl TextBuilder {
    #[inline(always)]
    pub fn new() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            hx: 0.5,
            vy: 0.5,
            color: [1.0, 1.0, 1.0, 1.0],
            glow: [1.0, 1.0, 1.0, 0.0],
            stroke_color: None,
            font: "miso",
            content: TextContent::default(),
            talign: TextAlign::Left,
            z: 0,
            sx: 1.0,
            sy: 1.0,
            fit_w: None,
            fit_h: None,
            wrap_width_pixels: None,
            line_spacing: None,
            max_w: None,
            max_h: None,
            saw_max_w: false,
            saw_max_h: false,
            max_w_pre_zoom: false,
            max_h_pre_zoom: false,
            blend: BlendMode::Alpha,
            effect: anim::EffectState::default(),
            shx: 0.0,
            shy: 0.0,
            shc: [0.0, 0.0, 0.0, 0.5],
            tween_salt: 0,
        }
    }

    #[inline(always)]
    pub fn tweensalt(&mut self, salt: u64) {
        self.tween_salt = salt;
    }

    #[inline(always)]
    pub fn xy(&mut self, x: f32, y: f32) {
        self.x = x;
        self.y = y;
    }

    #[inline(always)]
    pub fn x(&mut self, x: f32) {
        self.x = x;
    }

    #[inline(always)]
    pub fn y(&mut self, y: f32) {
        self.y = y;
    }

    #[inline(always)]
    pub fn addx(&mut self, dx: f32) {
        self.x += dx;
    }

    #[inline(always)]
    pub fn addy(&mut self, dy: f32) {
        self.y += dy;
    }

    #[inline(always)]
    pub fn align(&mut self, h: f32, v: f32) {
        self.hx = h;
        self.vy = v;
    }

    #[inline(always)]
    pub fn halign(&mut self, h: f32) {
        self.hx = h;
    }

    #[inline(always)]
    pub fn valign(&mut self, v: f32) {
        self.vy = v;
    }

    #[inline(always)]
    pub fn diffuse(&mut self, rgba: [f32; 4]) {
        self.color = rgba;
    }

    #[inline(always)]
    pub fn alpha(&mut self, a: f32) {
        self.color[3] = a;
    }

    #[inline(always)]
    pub fn glow(&mut self, rgba: [f32; 4]) {
        self.glow = rgba;
    }

    #[inline(always)]
    pub fn strokecolor(&mut self, rgba: [f32; 4]) {
        self.stroke_color = Some(rgba);
    }

    #[inline(always)]
    pub fn font(&mut self, font: &'static str) {
        self.font = font;
    }

    #[inline(always)]
    pub fn settext(&mut self, content: TextContent) {
        self.content = content;
    }

    #[inline(always)]
    pub fn horizalign(&mut self, align: TextAlign) {
        self.talign = align;
    }

    #[inline(always)]
    pub fn z(&mut self, z: i16) {
        self.z = z;
    }

    #[inline(always)]
    pub fn zoom(&mut self, f: f32) {
        self.sx = f;
        self.sy = f;
        if self.saw_max_w {
            self.max_w_pre_zoom = true;
        }
        if self.saw_max_h {
            self.max_h_pre_zoom = true;
        }
    }

    #[inline(always)]
    pub fn zoomx(&mut self, x: f32) {
        self.sx = x;
        if self.saw_max_w {
            self.max_w_pre_zoom = true;
        }
    }

    #[inline(always)]
    pub fn zoomy(&mut self, y: f32) {
        self.sy = y;
        if self.saw_max_h {
            self.max_h_pre_zoom = true;
        }
    }

    #[inline(always)]
    pub fn addzoomx(&mut self, dx: f32) {
        self.sx += dx;
        if self.saw_max_w {
            self.max_w_pre_zoom = true;
        }
    }

    #[inline(always)]
    pub fn addzoomy(&mut self, dy: f32) {
        self.sy += dy;
        if self.saw_max_h {
            self.max_h_pre_zoom = true;
        }
    }

    #[inline(always)]
    pub fn zoomtowidth(&mut self, w: f32) {
        self.fit_w = Some(w);
    }

    #[inline(always)]
    pub fn zoomtoheight(&mut self, h: f32) {
        self.fit_h = Some(h);
    }

    #[inline(always)]
    pub fn wrapwidthpixels(&mut self, w: f32) {
        let wrap = w as i32;
        self.wrap_width_pixels = (wrap >= 0).then_some(wrap);
    }

    #[inline(always)]
    pub fn vertspacing(&mut self, spacing: f32) {
        // Mirrors SM5 `BitmapText:vertspacing(n)` — overrides the font's
        // default line spacing (i.e. the distance between successive lines).
        self.line_spacing = Some(spacing as i32);
    }

    #[inline(always)]
    pub fn maxwidth(&mut self, w: f32) {
        self.max_w = Some(w);
        self.saw_max_w = true;
        self.max_w_pre_zoom = false;
    }

    #[inline(always)]
    pub fn maxheight(&mut self, h: f32) {
        self.max_h = Some(h);
        self.saw_max_h = true;
        self.max_h_pre_zoom = false;
    }

    #[inline(always)]
    pub fn blend(&mut self, blend: BlendMode) {
        self.blend = blend;
    }

    #[inline(always)]
    pub fn shadowlength(&mut self, v: f32) {
        self.shx = v;
        self.shy = -v;
    }

    #[inline(always)]
    pub fn shadowlengthx(&mut self, v: f32) {
        self.shx = v;
    }

    #[inline(always)]
    pub fn shadowlengthy(&mut self, v: f32) {
        self.shy = -v;
    }

    #[inline(always)]
    pub fn shadowcolor(&mut self, c: [f32; 4]) {
        self.shc = c;
    }

    #[inline(always)]
    pub fn effectclock(&mut self, clock: anim::EffectClock) {
        self.effect.clock = clock;
    }

    #[inline(always)]
    pub fn effectmode(&mut self, mode: anim::EffectMode) {
        self.effect.mode = mode;
    }

    #[inline(always)]
    pub fn effectcolor1(&mut self, color: [f32; 4]) {
        self.effect.color1 = color;
    }

    #[inline(always)]
    pub fn effectcolor2(&mut self, color: [f32; 4]) {
        self.effect.color2 = color;
    }

    #[inline(always)]
    pub fn effectperiod(&mut self, v: f32) {
        if v > 0.0 {
            self.effect.period = v;
            self.effect.timing = [v * 0.5, 0.0, v * 0.5, 0.0, 0.0];
        }
    }

    #[inline(always)]
    pub fn effectoffset(&mut self, v: f32) {
        self.effect.offset = v;
    }

    #[inline(always)]
    pub fn effecttiming(&mut self, v: [f32; 5]) {
        let timing = [
            v[0].max(0.0),
            v[1].max(0.0),
            v[2].max(0.0),
            v[3].max(0.0),
            v[4].max(0.0),
        ];
        let total = timing[0] + timing[1] + timing[2] + timing[3] + timing[4];
        if total > 0.0 {
            self.effect.timing = timing;
            self.effect.period = total;
        }
    }

    #[inline(always)]
    pub fn effectmagnitude(&mut self, v: [f32; 3]) {
        self.effect.magnitude = v;
    }

    #[inline(always)]
    pub fn size(&mut self, _w: f32, _h: f32) {}

    #[inline(always)]
    pub fn zoomto(&mut self, _w: f32, _h: f32) {}

    #[inline(always)]
    pub fn mask_source(&mut self) {}

    #[inline(always)]
    pub fn mask_dest(&mut self) {}

    #[inline(always)]
    pub fn texcoordvelocity(&mut self, _vel: [f32; 2]) {}

    #[inline(always)]
    pub fn cropleft(&mut self, _v: f32) {}

    #[inline(always)]
    pub fn cropright(&mut self, _v: f32) {}

    #[inline(always)]
    pub fn croptop(&mut self, _v: f32) {}

    #[inline(always)]
    pub fn cropbottom(&mut self, _v: f32) {}

    #[inline(always)]
    pub fn fadeleft(&mut self, _v: f32) {}

    #[inline(always)]
    pub fn faderight(&mut self, _v: f32) {}

    #[inline(always)]
    pub fn fadetop(&mut self, _v: f32) {}

    #[inline(always)]
    pub fn fadebottom(&mut self, _v: f32) {}

    #[inline(always)]
    pub fn setstate(&mut self, _i: u32) {}

    #[inline(always)]
    pub fn animate(&mut self, _v: bool) {}

    #[inline(always)]
    pub fn setallstatedelays(&mut self, _s: f32) {}

    #[inline(always)]
    pub fn customtexturerect(&mut self, _uv: [f32; 4]) {}

    #[inline(always)]
    pub fn visible(&mut self, _v: bool) {}

    #[inline(always)]
    pub fn rotationx(&mut self, _d: f32) {}

    #[inline(always)]
    pub fn rotationy(&mut self, _d: f32) {}

    #[inline(always)]
    pub fn rotationz(&mut self, _d: f32) {}

    #[inline(always)]
    pub fn addrotationx(&mut self, _dd: f32) {}

    #[inline(always)]
    pub fn addrotationy(&mut self, _dd: f32) {}

    #[inline(always)]
    pub fn addrotationz(&mut self, _dd: f32) {}

    #[inline(always)]
    pub fn build_tweened(
        mut self,
        site_base: u64,
        build_steps: impl FnOnce() -> TweenSteps,
    ) -> Actor {
        let mut init = anim::TweenState::default();
        init.x = self.x;
        init.y = self.y;
        init.tint = self.color;
        init.glow = self.glow;
        init.scale = [self.sx, self.sy];

        let sid = runtime::site_id(site_base, self.tween_salt);
        let state = runtime::materialize_lazy(sid, init, build_steps);
        self.x = state.x;
        self.y = state.y;
        self.color = state.tint;
        self.glow = state.glow;
        self.sx = state.scale[0];
        self.sy = state.scale[1];
        self.build(site_base)
    }

    #[inline(always)]
    pub fn build(mut self, _site_base: u64) -> Actor {
        if self.content.as_str().as_bytes().contains(&b'&')
            && let std::borrow::Cow::Owned(s) = font::replace_markers(self.content.as_str())
        {
            self.content = TextContent::Owned(s);
        }

        Actor::Text {
            align: [self.hx, self.vy],
            offset: [self.x, self.y],
            local_transform: Matrix4::IDENTITY,
            color: self.color,
            stroke_color: self.stroke_color,
            glow: self.glow,
            font: self.font,
            content: self.content,
            attributes: Vec::new(),
            align_text: self.talign,
            z: self.z,
            scale: [self.sx, self.sy],
            fit_width: self.fit_w,
            fit_height: self.fit_h,
            line_spacing: self.line_spacing,
            wrap_width_pixels: self.wrap_width_pixels,
            max_width: self.max_w,
            max_height: self.max_h,
            max_w_pre_zoom: self.max_w_pre_zoom,
            max_h_pre_zoom: self.max_h_pre_zoom,
            jitter: false,
            distortion: 0.0,
            clip: None,
            mask_dest: false,
            blend: self.blend,
            shadow_len: [self.shx, self.shy],
            shadow_color: self.shc,
            effect: self.effect,
        }
    }
}
