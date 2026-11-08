from pathlib import Path
import hashlib,json,shutil,subprocess
repo=Path.cwd();root=repo/'target/deadlib-audio-reference';source=root/'residue-tails-20261027';run=root/'residue-tails-before-after-verified-20261027';run.mkdir(exist_ok=False)
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
base=r'''use codec::*;
fn number(wire:&mut &[u8])->usize{let(v,r)=wire.split_at(8);*wire=r;u64::from_le_bytes(v.try_into().unwrap()) as usize}
fn packet<'a>(wire:&mut &'a[u8])->&'a[u8]{let n=number(wire);let(v,r)=wire.split_at(n);*wire=r;v}
fn main(){for name in std::env::args().skip(1){let data=std::fs::read(&name).unwrap();let mut wire=data.as_slice();let id=Identification::parse(packet(&mut wire)).unwrap();packet(&mut wire);let setup=packet(&mut wire);assert!(wire.is_empty());match Setup::parse_with_options(setup,&id,OPTIONS){Ok(s)=>{let r=&s.residues()[0];println!("{}\taccepted\t{}\t{}\t{}",std::path::Path::new(&name).file_name().unwrap().to_string_lossy(),r.begin,r.end,r.partition_size)},Err(e)=>println!("{}\trejected\t{e:?}",std::path::Path::new(&name).file_name().unwrap().to_string_lossy())}}}
'''
inputs=sorted((source/'baseline-inputs').glob('init-*.bin'));assert len(inputs)==9;results=[]
for variant in ['before','after']:
 folder=run/variant;(folder/'src').mkdir(parents=True)
 if variant=='before':
  for package in ['ogg','vorbis']:
   target=folder/package;(target/'src').mkdir(parents=True)
   manifest=(repo/f'crates/deadlib-audio/decode/{package}/Cargo.toml').read_text(encoding='utf8').split('\n[[bench]]')[0];(target/'Cargo.toml').write_text(manifest,encoding='utf8',newline='\n')
   for p in (repo/f'crates/deadlib-audio/decode/{package}/src').glob('*.rs'):
    q=source/'baseline-source'/p.relative_to(repo) if package=='vorbis' else p
    if package=='vorbis' and not q.exists():continue
    shutil.copyfile(q,target/'src'/p.name)
  dependency=folder/'vorbis';options='ParseOptions::default()'
 else:dependency=repo/'crates/deadlib-audio/decode/vorbis';options='ParseOptions {residue_behavior:ResidueBehavior::Libvorbis,..ParseOptions::default()}'
 (folder/'src/main.rs').write_text(base.replace('OPTIONS',options),encoding='utf8',newline='\n')
 cargo=f'[package]\nname="deadlib-residue-{variant}"\nversion="0.0.0"\nedition="2024"\n[workspace]\n[dependencies]\ncodec={{package="deadlib-audio-decode-vorbis",path={json.dumps(dependency.as_posix())}}}\n';(folder/'Cargo.toml').write_text(cargo,encoding='utf8',newline='\n')
 args=['cargo','build','--offline','--release','--manifest-path',str(folder/'Cargo.toml')]
 with (run/(variant+'-build.log')).open('wb') as f:r=subprocess.run(args,stdout=f,stderr=subprocess.STDOUT)
 assert r.returncode==0,(run/(variant+'-build.log')).read_text(encoding='utf8')
 binary=folder/f'target/release/deadlib-residue-{variant}.exe';r=subprocess.run([str(binary)]+[str(p) for p in inputs],capture_output=True);assert r.returncode==0,r.stderr;(run/(variant+'.out')).write_bytes(r.stdout)
 rows=[line.split('\t') for line in r.stdout.decode().splitlines()];assert len(rows)==9
 assert [row[1] for row in rows]==(['rejected','rejected','accepted']*3 if variant=='before' else ['accepted']*9)
 results.append({'variant':variant,'command':args,'driver_sha256':sha(folder/'src/main.rs'),'manifest_sha256':sha(folder/'Cargo.toml'),'binary_sha256':sha(binary),'output_sha256':sha(run/(variant+'.out')),'rows':rows})
summary={'scope':'Freshly compiled pre-milestone source rejects six reference-accepted setups; explicit residue behavior accepts all nine. Published defaults retain the guards. No timing run.','before_source_commit':'e8d4fca809e6bd8946f11c139f06c0336e9f3db5','inputs':{p.name:sha(p) for p in inputs},'before_sources_sha256':{p.relative_to(run/'before').as_posix():sha(p) for p in (run/'before').rglob('*.rs') if 'target' not in p.relative_to(run/'before').parts},'after_sources_sha256':{p.relative_to(repo).as_posix():sha(p) for package in ['ogg','vorbis'] for p in (repo/f'crates/deadlib-audio/decode/{package}/src').glob('*.rs')},'results':results}
(run/'checks.json').write_text(json.dumps(summary,indent=2)+'\n',encoding='utf8',newline='\n');print('Fresh before/after: three accepted -> nine accepted; six reference-only witnesses corrected.')
