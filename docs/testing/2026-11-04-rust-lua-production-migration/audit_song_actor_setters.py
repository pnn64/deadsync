"""Accept the setter boundary only after inspecting final-source check results."""
import hashlib,json,re,subprocess,sys,tomllib
from pathlib import Path
d=Path(__file__).resolve().parent;r=d.parents[2]
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
read=lambda p:json.loads(p.read_text(encoding='utf-8-sig'))
manifest=tomllib.loads((r/'crates/deadsync-song-lua/Cargo.toml').read_text())
targets=[item['name'] for item in manifest['test'] if item['name'].startswith('rust_')]
assert len(targets)==len(set(targets))==73
stages={key:'song-actor-setters-'+suffix for key,suffix in {
    'focus':'focus-v6-song-linux-debug','lint':'lint-v3-song-linux-debug',
    'library':'library-v3-song-linux-debug','shipping':'shipping-cfg-v3',
    'regressions':'regressions-v3-song-linux-debug'}.items()}
results={key:read(d/stage/'result.json') for key,stage in stages.items()}
frozen=read(d/stages['focus']/'sources.json')
for key,item in results.items():
    stage=d/stages[key]
    assert item['sources_unchanged'] and sha(stage/'run.log')==item['log_sha256'],key
    sources=item['sources'] if key=='shipping' else read(stage/'sources.json')
    assert sources==frozen,key
    assert item['exit_code']==(101 if key=='lint' else 0),key
for name,digest in frozen.items():assert sha(r/name)==digest,name
for key,count in [('focus',10),('library',1)]:
    assert results[key]['tests_passed']==count and results[key]['tests_failed']==0,key
    assert len(results[key]['test_results'])==1,key
assert '849 filtered out' in results['library']['test_results'][0]
reg=results['regressions'];command=reg['command']
observed=[command[index+1] for index,value in enumerate(command) if value=='--test']
assert observed==targets and len(reg['test_results'])==len(targets)
assert reg['tests_failed']==0 and reg['tests_passed']>=694
for line in reg['test_results']:
    assert re.fullmatch(r'test result: ok\. \d+ passed; 0 failed; 0 ignored; \d+ measured; 0 filtered out; finished in .+',line),line
assert results['lint']['log_sha256']=='0e77776cc4b21cb0c25c0793fd785511697c6ec2c7e05747b206cf7066f5ce70'
baseline=read(d/'song-live-visuals-final-state.json')
for name in ['docs/lua-architecture.md','docs/testing/2026-11-04-rust-lua-production-migration/README.md']:
    assert sha(r/name)==baseline['documents'][name],name
objective='86cbb91ab372fad5ac5326b3c3b04fb3db8cab9d4f3a4cc8e71cb64dba48ae34'
assert sha(d/'objective.txt')==objective
assert sha(Path('/mnt/c/Users/PerfectTaste/.codex/attachments/bcf567e6-d9b3-4fcf-b693-47949c76632c/pasted-text-1.txt'))==objective
review=subprocess.run([sys.executable,str(d/'review_song_actor_setters.py')],check=True,capture_output=True,text=True)
boundary=json.loads(review.stdout)
assert boundary['sources']==len(frozen)==1300
output_names=['song-actor-setters-reviewed-sources.json','song-actor-setters-review.json','song-actor-setters-final-state.json']
for name in output_names:assert not (d/name).exists(),name
report={'goal_complete':False,'production_cutover':False,'snapshot_factories_migrated':False,
        'objective_sha256':objective,'boundary':boundary,'results':results,
        'previous_goal_turn':'progress: original Rust setter bindings and 46 focused checks; verification was running',
        'current_goal_turn':'progress: preserve Sound scalar setters, verify all Rust adapter targets, stage snapshot-owner drafts',
        'limitations':['Normal production song routing still uses legacy.',
                       'Existing input-only/combined snapshot factories still require migration; staged drafts are not production source.',
                       'Reference/eligibility/coverage and retained-handle/session audits remain, followed by routing/fallback/cache cutover and legacy isolation.',
                       'Final interpreter/downstream/content checks, normal application build and rendered smoke, graph/link proof and matched performance/memory measurements remain required.']}
(d/output_names[0]).write_text(json.dumps(frozen,indent=2)+'\n')
(d/output_names[1]).write_text(json.dumps(report,indent=2)+'\n')
architecture=f'''
## Rust actor setters for existing input snapshots (2026-11-08)

The Rust host now installs original per-actor `visible`, `x` and `y` bindings.
Copied setters retain and return their captured target; constructor attributes,
fresh clones, authored overrides, argument slots, class fallback and assignment
hooks follow the existing controls. Lookup and assignment run in Rust host
functions, with private weak identity records. Scene retirement retains the
method table with its actor; Sound initialization preserves the shared bindings.

Final-source checks pass **{reg['tests_passed']} tests across 73 Rust targets**,
including 10 setter cases, plus 1 filtered identity/GC unit test. Formatting and
the build without test support pass. Clippy exactly matches its pre-existing
diagnostics, including the immutable-loop failure. The interpreter, native
implementations and pre-existing test files are unchanged.
See [source review](testing/2026-11-04-rust-lua-production-migration/song-actor-setters-review.json).

This is groundwork for the existing input snapshot factories; their migration
remains pending. Production routing, remaining reference/session work,
fallback/cache cutover, legacy isolation and final application/content/performance
validation remain required. The migration objective is unchanged; the completed
interpreter and C API-removal milestones remain closed.
'''
ledger=f'''
## Rust actor setter identities and lifetimes (2026-11-08)

The three existing snapshot-eligible setters now have authenticated Rust
forwarders and per-actor bound functions. Copying a bound setter preserves its
target and chained return. Declaration attributes are replaced during native
installation, fresh template clones receive their own setters, and public/raw
overrides, class removal, assignment hooks and second-argument rules are tested
against the existing native controls in both clock modes. Identity records are
private weak Rust data. Retired scenes retain method tables through their traced
Lua owner, including weak actors; saved setters keep their target alive until
released. Sound initialization merges its methods into the existing table.

- **{reg['tests_passed']} tests passed across all 73 existing Rust adapter targets**,
  with no failures, ignored cases or filtered cases. The focused setter target
  passes 10 tests. The identity/GC library test passes with 849 other tests
  explicitly filtered; this does not claim a full library run.
- Formatting and the shipping configuration without test support pass. Clippy
  retains exactly the prior 166 warnings and src/cmd.rs:349 immutable-loop error;
  its complete log hash is unchanged. These validation durations are not matched
  application performance measurements.
- [Source review](song-actor-setters-review.json) checks all 1300 watched sources,
  five archived existing-file boundaries and two new source files. The interpreter,
  native implementations and pre-existing test bodies are byte-identical. Earlier
  failures and interrupted checks remain preserved: missing preparation fields,
  weak retirement ownership, constructor attributes and non-setter fallback were
  corrected without dropping tests. Source review additionally identified Sound's
  table replacement; its focused differential case now passes.

Input snapshot runtime-owner and inspection/compiler drafts are staged as `.txt`
artifacts outside watched production source. They have not been compiled or wired
into the scene. The input-only and combined factories, frozen-plan eligibility and
VM-free teardown controls are the next required migration path.

The [fixed objective](objective.txt) remains active. Normal production song routing
still uses legacy. Remaining reference/eligibility/coverage and saved-handle/session
audits, unconditional routing, fallback/cache cutover and legacy isolation remain,
followed by all final downstream/content, application/rendering, dependency/link
and matched preparation/live/memory gates. The interpreter milestone stays closed.
'''
documents=[r/'docs/lua-architecture.md',d/'README.md']
for path,addition in zip(documents,[architecture,ledger]):
    path.write_bytes(path.read_bytes()+addition.replace('\n','\r\n').encode())
state={'goal_status':'active','goal_complete':False,'production_cutover':False,
       'source_count':len(frozen),'tests_passed':reg['tests_passed'],'test_targets':len(targets),
       'focused_tests':10,'filtered_library_tests':1,'old_library_tests_filtered':849,
       'interpreter_unchanged':True,'existing_tests_unchanged':True,'snapshot_factories_migrated':False,
       'objective_sha256':objective,'documents':{p.relative_to(r).as_posix():sha(p)
        for p in [*documents,d/'review_song_actor_setters.py',d/output_names[1],Path(__file__)]}}
(d/output_names[2]).write_text(json.dumps(state,indent=2)+'\n')
print(json.dumps(state),flush=True)
