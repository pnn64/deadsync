use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::{self as song, SongLuaCompileContext, SongRuntime};
use std::{fs, path::PathBuf};

#[test]
fn legacy_ini_layers_load_as_sprites_with_lowercased_commands() {
    let directory = tempfile::tempdir().unwrap();
    let dir = directory.path().join("rain");
    fs::create_dir(&dir).unwrap();
    fs::write(dir.join("Flare.png"), b"").unwrap();
    fs::write(dir.join("near.png"), b"").unwrap();
    fs::write(dir.join("Plate 4x4.png"), b"").unwrap();
    fs::write(
        dir.join("Sprite.sprite"),
        "[Sprite]\r\n\r\nTexture=Plate 4x4.png\r\n\r\nFrame0000=0;\r\nDelay0000=0.0375;\r\nFrame0001=1;\r\nDelay0001=0.25;\r\n",
    )
    .unwrap();
    fs::write(
        dir.join("BGAnimation.ini"),
        "[BGAnimation]\r\n\r\n[Layer10]\r\nFile=flare\r\nOnCommand=x,320;ZoomToWidth,640;diffusealpha,0;linear,1.000;diffusealpha,+0.5;blend,add;diffuse,#ff000080;\r\n\r\n[Layer2]\nFile=near\nType=1\nCommand=sleep,16\n[Layer3]\nFile=missing\n[Layer20]\nFile=Sprite\n",
    )
    .unwrap();
    let context = SongLuaCompileContext::new(directory.path(), "BGAnimation.ini regression");
    let mut rust = SongRuntime::new(&context).unwrap();
    // LoadActor on the folder resolves to its BGAnimation.ini.
    let path = PathBuf::from(song::file_path_string(&dir));
    let value = rust.load_actor(&path, &[]).unwrap()[0];
    rust.vm_mut().set_global(b"ani", value).unwrap();
    let observations = br#"
        assert(ani.Class=='ActorFrame' and #ani==3, 'three layers')
        local near, flare = ani[1][1], ani[2][1]
        assert(near.Class=='Sprite' and near.Texture:match('near.png$'))
        assert(flare.Class=='Sprite' and flare.Texture:match('Flare.png$'))
        local calls = {}
        local probe = setmetatable({}, {__index=function(_, name)
            return function(_, ...) calls[#calls+1] = name..'('..table.concat({...}, ',')..')' end
        end})
        flare.OnCommand(probe)
        near.OnCommand(probe)
        near.InitCommand(probe)
        local plate = ani[3][1]
        assert(plate.Class=='Sprite' and plate.Texture:match('Plate 4x4.png$'))
        assert(#plate.Frames==2 and plate.Frames[2].Frame==1 and plate.Frames[2].Delay==0.25)
        return table.concat(calls, ' ')
    "#;
    let result = rust
        .vm_mut()
        .execute(&compile_named(observations, b"=ini observations").unwrap())
        .unwrap()
        .to_vec();
    let Value::String(text) = result[0] else {
        panic!("expected a string");
    };
    let text = String::from_utf8_lossy(rust.vm_mut().string_bytes(text).unwrap()).into_owned();
    assert_eq!(
        text,
        "x(320) zoomtowidth(640) diffusealpha(0) linear(1) diffusealpha(0.5) blend(add) \
         diffuse(1,0,0,0.5019608) sleep(16) stretchto(0,0,640,480)"
    );
}
