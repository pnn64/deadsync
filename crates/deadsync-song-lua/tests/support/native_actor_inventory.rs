//! Recreate the original native-only inventory phase in a fresh Rust host.
//! The full host installs these documented fallback helpers immediately; the
//! frozen inventories predate that phase. Every other actual class key remains.
use deadlib_lua::compile_named;
use deadsync_song_lua::{SongLuaCompileContext, rust_engine::RustSongRuntime};

pub fn runtime() -> RustSongRuntime {
    let mut host =
        RustSongRuntime::new(&SongLuaCompileContext::new(".", "native method inventory")).unwrap();
    execute(
        &mut host,
        r#"
local fallback = {
 Actor={'ease','bouncebegin','bounceend','smooth','compound','hide_if','player','align',
  'FullScreen','scale_or_crop_background_no_move','scale_or_crop_background',
  'CenterX','CenterY','Center','bezier','Real','RealInverse','MaskSource','MaskDest',
  'thump','heartbeat','SetSize','drop','hidden','LyricCommand'},
 ActorFrame={'propagatecommand'},
 Sprite={'LoadFromSongBanner','LoadFromSongBackground','LoadFromCurrentSongBackground',
  'position','loop','rate','cropto','LinearFrames'},
 BitmapText={'PixelFont','Stroke','NoStroke','settextf','DiffuseAndStroke'},
}
for class,names in pairs(fallback) do
 for _,name in ipairs(names) do rawset(_G[class],name,nil) end
end
"#,
    );
    host
}

pub fn execute(host: &mut RustSongRuntime, source: impl AsRef<[u8]>) {
    let code = compile_named(source.as_ref(), b"=frozen native class inventory").unwrap();
    host.vm_mut().execute(&code).unwrap();
}
