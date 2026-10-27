import hashlib, json, os, pathlib, subprocess, datetime
repo = pathlib.Path.cwd()
decode = repo / 'crates/deadlib-audio/decode'
root = repo / 'target/deadlib-audio-reference'
run = root / 'floor0-parameters-validation-verified-20261027'
run.mkdir(exist_ok=False)
def sha(path): return hashlib.sha256(path.read_bytes()).hexdigest()
def sources():
    files = []
    for part in ['ogg/src', 'vorbis/src', 'ogg/tests', 'vorbis/tests']:
        files += [p for p in (decode/part).rglob('*') if p.is_file() and p.suffix in ['.rs','.c','.h','.ogg','.libvorbis-pcm','.libvorbis-headers','.libvorbis']]
    files += [decode/p for p in ['ogg/Cargo.toml','vorbis/Cargo.toml','fuzz/src/lib.rs','scripts/verify_independent.ps1']]
    return {p.relative_to(decode).as_posix():sha(p) for p in sorted(files)}
up = root / 'corpus/upstream-3e064a007e6d4f5099a3ddb77c144955'
lines = (up/'assets.sha256').read_text().splitlines()
assert len(lines)==528
for line in lines:
    digest, name = line.split('  ',1)
    assert sha(up/name)==digest,name
external = json.loads((decode/'vorbis/validation/corpus.json').read_text())['files']
for entry in external:
    p=root/'corpus'/entry['path']
    assert p.stat().st_size==entry['bytes'] and sha(p)==entry['sha256']
env=os.environ.copy()
env.update({
 'DEADLIB_VORBIS_CONFIGURATION_INPUTS':str(run/'configuration-inputs'),
 'DEADLIB_VORBIS_BOOK_LIMIT_INPUTS':str(run/'book-inputs'),
 'DEADLIB_VORBIS_FLOOR0_INPUTS':str(run/'floor0-inputs'),
 'DEADLIB_LIBVORBIS_ORACLE':str(root/'vorbis-oracle-build/Release/deadlib_vorbis_oracle.exe'),
 'DEADLIB_LIBOGG_ORACLE':str(root/'ogg-oracle-build/Release/deadlib_ogg_oracle.exe'),
 'DEADLIB_VORBIS_CORPUS':str(repo/'assets'), 'DEADLIB_OGG_CORPUS':str(repo/'assets'),
 'DEADLIB_VORBIS_FLOOR0':str(root/'corpus/floor0/vorbis_floor_type_0.ogg'),
 'DEADLIB_VORBIS_EXTERNAL_CORPUS':str(root/'corpus'),
 'DEADLIB_OGG_EXTERNAL_CORPUS':str(root/'corpus'),
 'DEADLIB_VORBIS_HISTORICAL_REPORT':str(run/'historical.tsv'),
 'DEADLIB_VORBIS_LEGACY_REPORT':str(run/'legacy.tsv'),
 'DEADLIB_VORBIS_UPSTREAM_REPORT':str(run/'upstream.tsv'),
 'DEADLIB_VORBIS_UPSTREAM_CORPUS':str(up),
 'DEADLIB_VORBIS_ENCODE_REPORT':str(run/'matrix.tsv'),
})
manifest={'started_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),
 'git_head':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),
 'rustc':subprocess.check_output(['rustc','-Vv'],text=True),
 'source_sha256':sources(),
 'assets_input_sha256':{p.relative_to(repo).as_posix():sha(p) for p in sorted((repo/'assets').rglob('*.ogg'))},
 'external_inputs':{e['path']:e['sha256'] for e in external},
 'upstream_inputs':{'files':528,'all_hashes_verified':True,'manifest_sha256':sha(up/'assets.sha256')},
 'reference_binaries':{k:sha(pathlib.Path(env[k])) for k in ['DEADLIB_LIBVORBIS_ORACLE','DEADLIB_LIBOGG_ORACLE']},
 'reference_sources':{p:subprocess.check_output(['git','-C',str(root/p),'rev-parse','HEAD'],text=True).strip() for p in ['ogg','vorbis']},
 'commands':[]}
jobs=[('fmt',['cargo','fmt','-p','deadlib-audio-decode-ogg','-p','deadlib-audio-decode-vorbis','--check']),('release',['cargo','test','-p','deadlib-audio-decode-ogg','-p','deadlib-audio-decode-vorbis','--release','--all-features','--','--include-ignored','--skip','regenerate_surround_pcm_golden','--skip','regenerate_synthetic_setup_golden','--skip','regenerate_residue_geometry_golden','--skip','regenerate_degenerate_book_golden','--skip','regenerate_codebook_extent_golden','--skip','regenerate_floor0_reference_goldens','--skip','regenerate_floor0_pcm_golden','--nocapture']),
 ('clippy',['cargo','clippy','-p','deadlib-audio-decode-ogg','-p','deadlib-audio-decode-vorbis','--all-targets','--all-features','--','-D','warnings']),
 ('clippy-default',['cargo','clippy','-p','deadlib-audio-decode-ogg','-p','deadlib-audio-decode-vorbis','--all-targets','--','-D','warnings']),
 ('independent',['powershell','-NoProfile','-ExecutionPolicy','Bypass','-File',str(decode/'scripts/verify_independent.ps1'),'-ScratchRoot',str(root/'floor0-parameters-independent-verified-20261027')]),
 ('fuzz-tests',['cargo','test','--manifest-path',str(decode/'fuzz/Cargo.toml'),'--lib']),
 ('fuzz-clippy',['cargo','clippy','--manifest-path',str(decode/'fuzz/Cargo.toml'),'--all-targets','--features','seed','--','-D','warnings'])]
for name,args in jobs:
    print('running',name,flush=True)
    with (run/(name+'.log')).open('wb') as f:
        result=subprocess.run(args,env=env,stdout=f,stderr=subprocess.STDOUT)
    manifest['commands'].append({'name':name,'command':args,'exit_code':result.returncode})
    (run/'checks.json').write_bytes((json.dumps(manifest,indent=2)+'\n').encode())
    print('finished',name,result.returncode,flush=True)
    if result.returncode:
        print((run/(name+'.log')).read_text(errors='replace')[-10000:])
        raise SystemExit(result.returncode)
assert manifest['source_sha256']==sources(),'source changed during validation'
manifest['completed_utc']=datetime.datetime.now(datetime.timezone.utc).isoformat()
(run/'checks.json').write_bytes((json.dumps(manifest,indent=2)+'\n').encode())
