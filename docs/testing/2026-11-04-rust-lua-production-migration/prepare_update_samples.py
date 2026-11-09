"""Copy the existing pure frame-window finalizer into the Rust driver."""
from pathlib import Path
import base64,json,sys
base=Path(__file__).resolve().parent;root=base.parents[2]
saved=json.loads((base/'song-update-driver-edit-baseline.json').read_text(encoding='utf-8-sig'))
source=base64.b64decode(saved['crates/deadsync-song-lua/src/perframe.rs']['base64']).decode().replace('\r\n','\n')
start=source.index('    let mut eases = Vec::new();\n    let mut column_transforms = Vec::new();',source.index('pub fn compile_update_functions'))
end=source.index('    // Replay captured every rendered frame;',start)
body=source[start:end]
# Borrowed generic inputs retain the native algorithm; enumerate the fixed
# player array and omit redundant references required only by native owned locals.
body=body.replace('for player in 0..LUA_PLAYERS {','for (player, mods) in from_mods.iter().enumerate() {').replace('if from_mods[player].get(key).is_none()','if mods.get(key).is_none()')
body=body.replace('&baseline_mods,','baseline_mods,').replace('&baseline_players,','baseline_players,')

header='''
#[allow(clippy::too_many_arguments)]
fn finish_samples<M: crate::perframe::ModState>(
    context: &SongLuaCompileContext, end: f32, origin: Option<f32>, use_mod_clock: bool,
    sample_beats: &[f32], sample_seconds: &[f32], player_samples: &[[Player; LUA_PLAYERS]],
    mod_samples: &[[M; LUA_PLAYERS]], mod_speed_samples: &[[M; LUA_PLAYERS]],
    column_samples: &[Vec<crate::SongLuaColumnTransformSample>],
    baseline_players: &[Player; LUA_PLAYERS], baseline_mods: &[M; LUA_PLAYERS],
) -> (Vec<SongLuaEaseWindow>, Vec<SongLuaColumnOffsetWindow>) {
    use crate::{append_column_transform_windows_from_samples, push_perframe_player_targets};
    use crate::perframe::push_update_mod_targets_with_key;
'''
result=header+body+'    (eases, column_transforms)\n}\n'
path=root/'crates/deadsync-song-lua/src/rust_engine/song_updates.rs'
if sys.argv[1:]==['--apply']:
    current=path.read_text();assert 'fn finish_samples<' not in current
    path.write_text(current+result)
else:
    import subprocess
    fmt=lambda s:subprocess.run(['rustfmt','--edition','2024','--emit','stdout'],input=s,text=True,capture_output=True,check=True).stdout
    actual=path.read_text();actual=actual[actual.index('#[allow(clippy::too_many_arguments)]\nfn finish_samples<'):]
    assert fmt(actual)==fmt(result),'Pure finalizer differs from saved native implementation'
    print('Pure frame-window finalizer matches the saved native implementation')
