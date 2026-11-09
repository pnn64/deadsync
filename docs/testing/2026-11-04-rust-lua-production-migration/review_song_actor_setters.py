"""Check the actor-setter edit boundary without changing previous evidence."""
import base64, hashlib, json, re, subprocess, tomllib
from pathlib import Path

d=Path(__file__).resolve().parent; r=d.parents[2]
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
before=json.loads((d/'song-actor-setters-before-sources.json').read_text())
saved=json.loads((d/'song-actor-setters-edit-baseline.json').read_text())
saved.update(json.loads((d/'song-actor-setters-sound-edit-baseline.json').read_text()))
prefix='crates/deadsync-song-lua/'
new={prefix+'src/rust_engine/actors/scalar_methods.rs',prefix+'tests/rust_actor_setters.rs'}
names=subprocess.check_output(['git','-c',f'safe.directory={r.as_posix()}','ls-files','--cached','--others','--exclude-standard','-z','--',*[f'crates/{c}' for c in ['deadlib-lua','deadsync-song-lua','deadsync-noteskin','deadsync-theme-simply-love','deadsync-assets','deadsync-shell','deadsync-notefield']]],cwd=r).decode().split('\0')
paths={r/n for n in names if n and Path(n).suffix in {'.rs','.lua','.toml'}}
paths.update(r/n for n in ['Cargo.toml','Cargo.lock','.cargo/config.toml'])
current={p.relative_to(r).as_posix():sha(p) for p in sorted(paths) if p.is_file()}
assert current.keys()-before.keys()==new
assert not before.keys()-current.keys()
changed={p for p in before if before[p]!=current[p]}
assert changed==saved.keys(),changed
for p,item in saved.items():
    assert hashlib.sha256(base64.b64decode(item['base64'])).hexdigest()==item['sha256']==before[p]
old=lambda name:base64.b64decode(saved[prefix+name]['base64']).decode().replace('\r\n','\n')
now=lambda name:(r/prefix/name).read_text()
fmt=lambda value:subprocess.check_output(['rustfmt','--edition','2024','--config','skip_children=true','--emit','stdout'],input=value.encode()).decode()
def once(value,needle,replacement):
    assert value.count(needle)==1,needle
    return value.replace(needle,replacement,1)
actors=once(old('src/rust_engine/actors.rs'),'mod initial_state;\n','mod initial_state;\nmod scalar_methods;\n')
actors=once(actors,'    update_plans: HashMap<TableId, Arc<lifecycle::Plan>>,\n','    update_plans: HashMap<TableId, Arc<lifecycle::Plan>>,\n    scalar_methods: scalar_methods::Methods,\n')
actors=once(actors,'        super::put(vm, methods, "ActorFrameTexture", Value::Table(framebuffer))?;\n','        super::put(vm, methods, "ActorFrameTexture", Value::Table(framebuffer))?;\n        scalar_methods::install_metatable(vm, methods, &store)?;\n')
actors=once(actors,'            install_method(vm, actor, name, Op::Set(name), &store, &captures)?;', '''            if let Some(setter) = scalar_methods::Setter::from_name(name) {
                scalar_methods::install(vm, actor, setter, &store, &captures)?;
            } else {
                install_method(vm, actor, name, Op::Set(name), &store, &captures)?;
            }''')
assert fmt(actors)==now('src/rust_engine/actors.rs')
lua=old('src/rust_engine/actors.lua')
start=lua.index('    local lookups = {}\n');end=lua.index('    local function copy_text_state',start)
lua=lua[:start]+'    local lookup, assign = methods.Lookup, methods.Assign\n'+lua[end:]
lua=once(lua,'{__index=lookups[class], __concat=concat_template}',
         '{__index=lookup, __newindex=assign, __concat=concat_template}')
assert lua==now('src/rust_engine/actors.lua')
retirement=once(old('src/rust_engine/actors/retirement.rs'),
    '    Ok(())\n}\n\npub(super) fn retire',
    '''    if !store.lock().unwrap().actors.contains_key(&actor) {
        scalar_methods::bind(call, store, actor)?;
    }
    Ok(())
}

pub(super) fn retire''')
retirement=once(retirement,'''        for value in references {
            call.raw_table_set(owner, Value::Table(value), Value::Boolean(true))?;
        }
''','''        for value in references {
            call.raw_table_set(owner, Value::Table(value), Value::Boolean(true))?;
        }
        // Native methods are part of the retained actor, even when an authored
        // weak metatable removes the instance's ordinary strong value edges.
        for &actor in &exclusive {
            if let Value::Table(bound) = get(call, actor, b"__songlua_bound_methods")? {
                call.raw_table_set(owner, Value::Table(bound), Value::Boolean(true))?;
            }
        }
''')
assert fmt(retirement)==now('src/rust_engine/actors/retirement.rs')
sound=once(old('src/rust_engine/actors/sound.rs'),'    let methods = call.try_new_table()?;',
    '''    let methods = match get(call, actor, b"__songlua_bound_methods")? {
        Value::Nil => call.try_new_table()?,
        value => table(call, value)?,
    };''')
assert fmt(sound)==now('src/rust_engine/actors/sound.rs')
previous=tomllib.loads(old('Cargo.toml'));manifest=tomllib.loads(now('Cargo.toml'))
assert manifest.pop('test')==previous.pop('test')+[{'name':'rust_actor_setters','required-features':['rust-engine','test-support']}]
assert previous==manifest
# Existing interpreter, oracle and regression bodies remain byte-identical.
assert all(before[p]==current[p] for p in before if p.startswith(('crates/deadlib-lua/',prefix+'tests/')))
subprocess.run(['rustfmt','--check','--edition','2024','--config','skip_children=true',*[str(r/p) for p in sorted(changed|new) if p.endswith('.rs')]],check=True,capture_output=True)
assert '#[ignore]' not in now('tests/rust_actor_setters.rs')
archived=json.loads((d/'song-actor-setters-v1-source.json').read_text())
first=base64.b64decode(archived[prefix+'tests/rust_actor_setters.rs']['base64']).decode()
def test_bodies(text):
    result={}
    for chunk in re.split(r'(?m)(?=^#\[test\]\n)',text.replace('\r\n','\n')):
        name=re.match(r'#\[test\]\nfn (\w+)\(',chunk)
        if name:result[name[1]]=chunk.strip()+'\n'
    return result
prior=test_bodies(first);present=test_bodies(now('tests/rust_actor_setters.rs'))
assert len(prior)==6 and len(present)==10 and prior.keys()<=present.keys()
for name,body in prior.items():
    if name=='custom_assignment_hooks_observe_writes_to_the_captured_target':
        # Normalize VM-specific pointer text while strengthening structural
        # assertions for the previously missing preparation publication.
        body=once(body,"writes[#writes+1]=key..'='..tostring(value)",
                  "writes[#writes+1]=key..'='..(type(value)=='table' and type(value) or tostring(value))")
        body=once(body,'        return table.concat(writes,\'|\')',
            '''        assert(b.__songlua_capture_block.visible==false)
        assert(b.__songlua_capture_block.__songlua_has_changes==true)
        assert(b.__songlua_state_visible==false)
        return table.concat(writes,'|')''')
    assert fmt(body).strip()==present[name].strip(),name
v3=test_bodies((d/'song-actor-setters-v3-tests.rs.txt').read_text())
assert len(v3)==8 and v3.keys()<=present.keys()
assert all(present[name]==body for name,body in v3.items())
assert sha(d/'objective.txt')=='86cbb91ab372fad5ac5326b3c3b04fb3db8cab9d4f3a4cc8e71cb64dba48ae34'
assert sha(d/'checks.py')=='05439883fa88dd9c8f4c4440eed91f484d0aea1648559de28348d92720deb59d'
print(json.dumps({'sources':len(current),'changed':sorted(changed),'added':sorted(new),
                  'interpreter_unchanged':True,'existing_tests_unchanged':True}),flush=True)
