from pathlib import Path
import subprocess,json,hashlib
repo=Path.cwd();root=repo/'target/deadlib-audio-reference';run=root/'codebook-extents-before-after-20261027';run.mkdir(exist_ok=False)
commit='4b46420d6';files={}
for package in ['ogg','vorbis']:
 prefix=f'crates/deadlib-audio/decode/{package}/'
 names=subprocess.check_output(['git','ls-tree','-r','--name-only',commit,'--',prefix],text=True).splitlines()
 for name in names:
  rel=name[len(prefix):]
  if rel!='Cargo.toml' and not (rel.startswith(('src/','benches/')) and rel.endswith('.rs')):continue
  data=subprocess.check_output(['git','show',commit+':'+name]);p=run/'before'/package/rel;p.parent.mkdir(parents=True,exist_ok=True);p.write_bytes(data);files[name]=hashlib.sha256(data).hexdigest()
source='''use codec::{bitpack::BitReader,codebook::Codebook,Limits};
fn main() {
 let p=std::env::args().nth(1).unwrap();let data=std::fs::read(p).unwrap();let mut bits=BitReader::new(&data);
 let book=Codebook::read(&mut bits,&Limits::default());
 println!("accepted={} consumed={} exhausted={}",book.is_ok(),bits.position(),bits.is_exhausted());
 assert_eq!(book.is_ok(),EXPECTED);assert_eq!(bits.position(),POSITION);assert!(!bits.is_exhausted());
}
'''
records=[]
for name,expected,position in [('before','true',331),('after','false',64)]:
 work=run/name;probe=work/'probe';(probe/'src').mkdir(parents=True,exist_ok=False)
 if name=='before':dep='../vorbis';members=['probe','ogg','vorbis']
 else:dep=(repo/'crates/deadlib-audio/decode/vorbis').as_posix();members=['probe']
 (work/'Cargo.toml').write_text('[workspace]\nmembers='+json.dumps(members)+'\nresolver="3"\n',encoding='utf8',newline='\n')
 (probe/'Cargo.toml').write_text('[package]\nname="extent-'+name+'-probe"\nversion="0.0.0"\nedition="2024"\n[dependencies]\ncodec={package="deadlib-audio-decode-vorbis",path='+json.dumps(dep)+'}\n',encoding='utf8',newline='\n')
 driver=source.replace('EXPECTED',expected).replace('POSITION',str(position));(probe/'src/main.rs').write_text(driver,encoding='utf8',newline='\n')
 args=['cargo','run','--offline','--manifest-path',str(probe/'Cargo.toml'),'--',str(root/'codebook-extents-20261027/default-rejection.bin')]
 with (run/(name+'.log')).open('wb') as f:r=subprocess.run(args,stdout=f,stderr=subprocess.STDOUT)
 records.append({'variant':name,'command':args,'exit_code':r.returncode,'accepted':expected=='true','consumed_bits':position,'source_sha256':hashlib.sha256(driver.encode()).hexdigest()})
 assert r.returncode==0,(name,(run/(name+'.log')).read_text(encoding='utf8'))
report={'scope':'Same complete 46-byte scalar book under ordinary default Limits before and after the extent guard. Preceding committed source accepts at bit 331; current source rejects at bit 64 without exhausting the reader. No performance measurement.','before_source_commit':subprocess.check_output(['git','rev-parse',commit],text=True).strip(),'before_source_sha256':files,'input_sha256':hashlib.sha256((root/'codebook-extents-20261027/default-rejection.bin').read_bytes()).hexdigest(),'results':records}
(run/'checks.json').write_text(json.dumps(report,indent=2)+'\n',encoding='utf8',newline='\n')
print('Default-limit regression reproduced:',[(r['variant'],r['accepted'],r['consumed_bits']) for r in records])
