"""Inventory the frozen production Lua surface and its existing regressions."""
from pathlib import Path
import collections,json,re
root=Path(__file__).resolve().parents[3];out=Path(__file__).resolve().parent
metadata=json.loads((out/'metadata.stdout').read_text());baseline=json.loads((out/'baseline-sources.json').read_text())
packages={p['name']:p for p in metadata['packages']}
direct=[]
for name,p in packages.items():
    for d in p['dependencies']:
        if d['name']=='deadlib-lua-legacy' and d['kind'] in [None,'build']:
            direct.append(dict(package=name,manifest=str(Path(p['manifest_path']).relative_to(root)),alias=d.get('rename'),kind=d['kind'] or 'normal',features=d['features']))
assert {x['package'] for x in direct}=={'deadsync-song-lua','deadsync-noteskin','deadsync-theme-simply-love'}
dependents=set()
for row in direct:dependents.add(row['package'])
while True:
    more={name for name,p in packages.items() if any(d['name'] in dependents and d['kind'] in [None,'build'] for d in p['dependencies'])}
    if more.issubset(dependents):break
    dependents|=more
refs=[];raw=[];retained=[];surfaces=[]
for row in direct:
    base=root/'crates'/row['package']/'src'
    for path in sorted(base.rglob('*.rs')):
        rel=path.relative_to(root).as_posix();text=path.read_text(encoding='utf-8')
        matches=[]
        for number,line in enumerate(text.splitlines(),1):
            if re.search(r'\bmlua\b|\b(?:Lua|Table|Function|MultiValue|RegistryKey|WeakLua|AnyUserData)\b',line):
                matches.append(number)
            for symbol in re.findall(r'\bffi::(lua\w+|LUA_\w+)\b',line):
                raw.append(dict(file=rel,line=number,symbol=symbol,source=line.strip()))
            if re.search(r'\bexec_raw_lua\b|\b(?:push_ref|push_value|pop_value|to_pointer)\(',line):
                raw.append(dict(file=rel,line=number,symbol='raw/identity access',source=line.strip()))
            if re.search(r'^\s*(?:pub(?:\([^)]*\))?\s+)?\w+\s*:\s*.*\b(?:Lua|Table|Function|Value|RegistryKey|WeakLua|AnyUserData)\b',line):
                retained.append(dict(file=rel,line=number,source=line.strip()))
            if re.search(r'\b(?:set_memory_limit|set_hook|set_instruction_limit|used_memory|gc_collect|gc_stop|gc_restart|set_app_data|app_data_ref|app_data_mut|remove_app_data)\b',line):
                surfaces.append(dict(file=rel,line=number,source=line.strip()))
        if matches:refs.append(dict(file=rel,sha256=baseline[rel],lines=matches))
fixture_manifest=out/'fixture-sources.json'
fixture_sources=baseline| (json.loads(fixture_manifest.read_text()) if fixture_manifest.exists() else {})
fixtures={}
for name in ['tests/fixtures/song_lua','tests/fixtures/full_song_lua','tests/fixtures/itgmania-song-lua-micro','assets/noteskins']:
    fixtures[name]={n:h for n,h in fixture_sources.items() if n.startswith(name+'/')}
fixtures['step_stats_gifs']={n:h for n,h in fixture_sources.items() if 'step_stats_gifs' in n and not n.endswith('.rs')}
tests={}
for name in ['deadlib-lua','deadsync-song-lua','deadsync-noteskin','deadsync-theme-simply-love','deadsync-assets','deadsync-shell','deadsync-profile-gameplay']:
    p=packages[name]
    tests[name]=[dict(name=t['name'],kind=t['kind'],source=str(Path(t['src_path']).relative_to(root)),required_features=t.get('required-features',[])) for t in p['targets'] if any(k in t['kind'] for k in ['test','lib','bin','example','bench'])]
report=dict(direct_legacy_dependencies=direct,normal_downstream_packages=sorted(dependents),source_references=refs,raw_c_api_and_identity_references=raw,handle_field_and_parameter_candidates=retained,state_limit_and_gc_references=surfaces,fixture_hashes=fixtures,existing_targets=tests,classification_note='Source candidates conservatively include inline unit tests and source-side performance controls. The three normal manifest edges establish production consumers; inspect conditional code before migrating/deleting each reference. Lua/Value handles and callback captures must be traced/rooted even without an explicit RegistryKey.')
(out/'inventory.json').write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8')
print(json.dumps(dict(direct_consumers=[r['package'] for r in direct],downstream=sorted(dependents),source_files=len(refs),raw_reference_sites=len(raw),handle_candidate_sites=len(retained),fixture_files={k:len(v) for k,v in fixtures.items()})),flush=True)
