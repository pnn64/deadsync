"""Transfer the existing scalar algorithms with host access replacements only."""
from pathlib import Path
import re,subprocess,sys
r=Path(__file__).resolve().parents[3];p=r/'crates/deadsync-song-lua/src'
s=(p/'host.rs').read_text()
def section(a,b):return s[s.index(a):s.index(b,s.index(a))]
def convert(body):
 body=re.sub(r'\bvalue\(', 'n(call, state, ',body)
 body=re.sub(r'\boption\(', 'n(call, state, ',body)
 body=re.sub(r'\bflag\(', 'flag(call, state, ',body)
 body=re.sub(r'\bnumber\(', 'strict(call, args, ',body)
 body=body.replace('current_style_name(lua)','style_name(call)').replace('arrow_effects_field_zoom','field_zoom')
 body=body.replace('args.get(1).cloned().and_then(read_f32).unwrap_or(1.0)','loose(call, args, 1, 1.0)')
 body=body.replace('arrow_effects_query_time(lua, state.as_ref(),','query_time(call, b, state,')
 body=body.replace('lua.app_data_ref::<NativeArrowEffects>()\n            .map_or([0.0, 0.0], |state| state.bounds[player][column])','b.state.cache.lock().unwrap().bounds[player][column]')
 for key in ['invert','beat_x']:
  index='[player][column]' if key=='invert' else '[player]'
  body=body.replace(f'lua.app_data_ref::<NativeArrowEffects>().map_or(0.0, |state| state.{key}{index})',f'b.state.cache.lock().unwrap().{key}{index}')
 body=body.replace('let cosecant = state.as_ref().map(|state| state.raw_get::<Option<bool>>("cosecant")).transpose()?.flatten().unwrap_or(false);','let cosecant = flag(call, state, "cosecant")?;')
 return body.replace('screen_height','b.height')
out='//! The established native float algorithms, using rooted Rust host values.\nuse super::*;\n'
for name,next_name,start in [
 ('x_pos','field_zoom','    let number ='),('zoom','alpha','    let number ='),('alpha','rotation_z','    let number =')]:
 text=section('fn arrow_effects_'+name+'(','fn arrow_effects_'+next_name+'(')
 if name in ['x_pos','zoom']:
  text=text[text.index('    let column = number(1') if name=='x_pos' else text.index('    let y = number(1'):]
 else:text=text[text.index('    let column = number(1'):]
 text=text[:text.rfind('\n}')]
 text=convert(text)
 if name=='x_pos':text='    use std::f32::consts::PI;\n    let player = player_for(call, options)?;\n'+text
 if name=='alpha':
  a=text.index('    let runtime = lua.globals()');b=text.index('    let mut y = offset;',a)
  text=text[:a]+'    let (beat, time) = alpha_clock(call, state)?;\n'+text[b:]
  text=text.replace('default_draw_distance','b.height * 1.5')
 text=text.replace('song_lua_style_column_x(', 'crate::song_lua_style_column_x(')
 out+='pub(super) fn '+name+'(call: &mut HostCall<\'_>, b: &Data, args: &[Value]) -> Result<f32, Error> {\nwith_options(call,args,|call, options, state| {\n'+text+'\n})\n}\n'
text=section('fn arrow_effects_field_zoom(', 'fn arrow_effects_zoom(').replace('arrow_effects_field_zoom','field_zoom')
out+=text
text=section('fn arrow_effects_y_pos(', 'fn arrow_effects_x_pos(')
text=text[text.index('    let style ='):text.rfind('\n}')]
a=text.index('    let player = options.as_ref()');b=text.index('    let reverse =',a)
text=text[:a]+'    let player = player_for(call, options)?;\n'+text[b:]
text=text.replace('    let cached = lua.app_data_ref::<NativeArrowEffects>().map(|state| *state);','    let cached = *b.state.cache.lock().unwrap();')
text=re.sub(r'cached.map_or\(0.0, \|state\| state\.([a-z_]+)(\[player\](?:\[column\])?)\)',r'cached.\1\2',text)
text=text.replace('arrow_effects_reverse_percent(lua, args)','reverse_percent(call, args)').replace('args.get(3).cloned().and_then(read_f32)\n        .unwrap_or(THEME_RECEPTOR_Y_REV - THEME_RECEPTOR_Y_STD)','loose(call,args,3,crate::THEME_RECEPTOR_Y_REV - crate::THEME_RECEPTOR_Y_STD)')
text=convert(text)
out+='pub(super) fn y_pos(call: &mut HostCall<\'_>, b: &Data, args: &[Value]) -> Result<f32, Error> {\nlet y_offset=loose(call,args,2,0.);\nwith_options(call,args,|call,options,state|{\n'+text+'\n})\n}\n'
text=section('fn arrow_effects_rotation_z(', 'pub(crate) const fn arrow_effects_rotation_x(')
text=text[text.index('    let runtime:'):text.rfind('\n}')]
a=text.index('    let runtime:');b=text.index('    let column =',a)
text=text[:a]+'''    let player = player_for(call, options)?;
    let t = runtime(call, true)?;
    let beat = call.with_roots(&[Value::Table(t)], |call| {
        let v = raw(call, t, PLAYERS)?;let positions=optional_table(call,v)?;
        position_beat(call,t,positions,player)
    })?;
'''+text[b:]
text=text.replace('args.get(3).cloned().and_then(read_f32).unwrap_or(1.0)','loose(call,args,3,1.0)').replace('args.get(1).cloned().and_then(read_f32).unwrap_or(0.0)','loose(call,args,1,0.0)')
text=text.replace('let dizzy_holds = state.as_ref().map(|state| state.raw_get::<Option<bool>>("dizzyholds"))\n        .transpose()?.flatten().unwrap_or(false);','let dizzy_holds = flag(call,state,"dizzyholds")?;')
text=convert(text)
out+='pub(super) fn rotation_z(call:&mut HostCall<\'_>, _b:&Data, args:&[Value])->Result<f32,Error>{\nuse std::f32::consts::PI;\nwith_options(call,args,|call,options,state|{\n'+text+'\n})\n}\n'
out=out.replace('THEME_RECEPTOR_Y_REV - THEME_RECEPTOR_Y_STD','crate::THEME_RECEPTOR_Y_REV - crate::THEME_RECEPTOR_Y_STD')
out=out.replace('crate::crate::','crate::')
out=out.replace('pub(super) fn alpha(', '// Keep the captured native float expression for the far-edge fade.\n#[allow(clippy::neg_multiply)]\npub(super) fn alpha(')
out=re.sub(r'mlua::Error::runtime\("([^"]*)"\)',r'call.error(b"\1")',out)
out=out.replace('flag(call, state, call,state,','flag(call,state,')
out=out.replace('fn zoom(call: &mut HostCall<\'_>, b: &Data,','fn zoom(call: &mut HostCall<\'_>, _b: &Data,')
for name in ['zoom','alpha']:
 a=out.index('fn '+name+'(');b=out.index('\n}\n',a)+3
 chunk=out[a:b].replace('|call, options, state|','|call, _options, state|');out=out[:a]+chunk+out[b:]
for line in out.splitlines():
 if any(x in line for x in ['lua.','mlua','read_f32']):print(line)
assert 'lua.' not in out and 'mlua' not in out and 'read_f32' not in out
target=p/'rust_engine/arrows/math.rs'
if sys.argv[1:]==['--check']:
 expected=subprocess.check_output(['rustfmt','--edition','2024','--config','skip_children=true','--emit','stdout'],input=out.encode()).decode()
 assert target.read_text()==expected,'math differs from the native expression transfer'
 print('Native expression transfer verified.');sys.exit(0)
assert not target.exists();target.parent.mkdir(exist_ok=True);target.write_text(out)
print('Existing x/y/zoom/alpha/rotation float expressions transferred to Rust host reads.')
