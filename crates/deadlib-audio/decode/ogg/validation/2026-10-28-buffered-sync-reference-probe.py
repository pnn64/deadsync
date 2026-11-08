from pathlib import Path
import hashlib,json,struct,subprocess
repo=Path.cwd();root=repo/'target/deadlib-audio-reference';run=root/'reference-seek-20261027';gates=root/'buffered-sync-validation-20261028';decode=repo/'crates/deadlib-audio/decode';sha=lambda b:hashlib.sha256(b).hexdigest()
base=json.loads((run/'baseline.json').read_text());unchanged={}
for name,h in base['source_sha256'].items():
 p=repo/name;before=(run/'baseline-source'/name).read_bytes();assert sha(before)==h
 assert before.replace(b'\r\n',b'\n')==subprocess.check_output(['git','show',base['own_commit']+':'+name]).replace(b'\r\n',b'\n')
 current=p.read_bytes();a=before.decode('utf8').replace('\r\n','\n');b=current.decode('utf8').replace('\r\n','\n')
 if name.endswith('/ogg/src/lib.rs'):
  b=b.replace('mod sync;\n','');start=b.index('pub use sync::{');end=b.index('};',start)+3;b=b[:start]+b[end:]
 elif name.endswith('/ogg/src/page.rs'):
  b=b.replace('''    /// Only call on an exact slice with verified lacing length and CRC.
    /// Strict callers also validate version/flags; SyncedPage keeps that policy
    /// separate and never exposes an unconverted Page publicly.''','''    /// Only call on an exact slice previously validated by `parse`.''')
 assert a==b,name
 unchanged[name]={'baseline_sha256':h,'current_sha256':sha(current),'only_new_exports_or_comments':current!=before}
old=run/'ogg-oracle-before.exe';new=root/'ogg-oracle-build/Release/deadlib_ogg_oracle.exe';rows=[]
def compare(mode,p,label):
 results=[]
 for exe in[old,new]:
  r=subprocess.run([str(exe),mode,str(p)],capture_output=True);results.append((r.returncode,r.stdout,r.stderr))
 assert results[0]==results[1],(mode,label)
 rows.append({'mode':mode,'label':label,'input_sha256':sha(p.read_bytes()),'exit_code':results[0][0],'stdout_sha256':sha(results[0][1]),'stdout_bytes':len(results[0][1]),'stderr_sha256':sha(results[0][2])})
 return results[0][1]
q=struct.pack('<I',0xfefdfcfb)
for i,n in enumerate([12,0,1,254,255,256,65024,65025,130050,1,0]):
 b=bytes((j*17+i*21)&255 for j in range(n));q+=b'\0'+struct.pack('<QQB',n,i,int(i==10))+b
p=run/'old-encode-input.bin';p.write_bytes(q);wire=compare('encode',p,'generated packet sizes');p=run/'old-encoded.ogg';p.write_bytes(wire);compare('decode',p,'generated packet sizes');compare('capture',p,'generated packet sizes')
at=0;offsets=[]
while at<len(wire):
 offsets.append(at);n=wire[at+26];at+=27+n+sum(wire[at+27:at+27+n])
assert at==len(wire)
for i,at in enumerate(offsets):
 p=run/'capture-suffix.ogg';p.write_bytes(wire[at:]);compare('capture',p,f'generated page {i}')
for p in sorted((repo/'assets').rglob('*.ogg')):compare('decode',p,p.relative_to(repo).as_posix())
class R:
 def __init__(self,b):self.b=b;self.at=0
 def take(self,n):b=self.b[self.at:self.at+n];self.at+=n;assert len(b)==n;return b
 def num(self,n=8,signed=False):return int.from_bytes(self.take(n),'little',signed=signed)
 def blob(self):return self.take(self.num())
 def done(self):assert self.at==len(self.b)
receipts=[];counts={'operations':0,'appends':0,'resets':0,'pageseek_pages':0,'pageseek_skips':0,'pageout_pages':0,'holes':0,'reader_pages':0,'boundaries':0,'eof':0,'io_errors':0,'returned_page_bytes':0}
for p in sorted((gates/'sync-inputs').glob('*.bin')):
 q=p.read_bytes();out=p.with_suffix('.out').read_bytes();r=R(q);o=R(out);reader=p.stem.startswith('sync-reader-')
 if reader:r.blob();r.num()
 n=r.num();assert o.num()==n;counts['operations']+=n
 for _ in range(n):
  kind=r.num()
  if reader:
   if kind in[0,1]:r.num()
  else:
   if kind==1:r.blob();counts['appends']+=1
   if kind==0:counts['resets']+=1
  code=o.num(signed=True)
  if reader:
   offset=o.num();source=o.num();reads=o.num();seeks=o.num();unread=o.num()
  else:unread=o.num()
  unsynced=o.num(1);assert unsynced in[0,1];b=o.blob();counts['returned_page_bytes']+=len(b)
  if b:
   assert b[:4]==b'OggS'and len(b)==27+b[26]+sum(b[27:27+b[26]])
   if reader:assert kind==0 and code>=0 and offset==code+len(b)and source>=offset;counts['reader_pages']+=1
   elif kind==2:assert code==len(b);counts['pageseek_pages']+=1
   else:assert kind==3 and code==1;counts['pageout_pages']+=1
  elif reader:
   if kind==0:assert code in[-1,-2,-128];counts[{-1:'boundaries',-2:'eof',-128:'io_errors'}[code]]+=1
   elif kind==1:assert code in[0,-128];counts['io_errors']+=int(code==-128)
   else:assert code==0 and kind in[2,3]
  elif kind==2:
   assert code<=0;counts['pageseek_skips']+=int(code<0)
  elif kind==3:
   assert code in[-1,0];counts['holes']+=int(code==-1)
  else:assert code==0
 r.done();o.done();receipts.append({'name':p.stem,'mode':'sync-read-ops'if reader else'sync-ops','request_sha256':sha(q),'response_sha256':sha(out),'operations':n,'request_bytes':len(q),'response_bytes':len(out)})
assert len(receipts)==879 and counts['operations']==242352
gold=(decode/'ogg/tests/fixtures/sync-state-v1.libogg').read_bytes();r=R(gold);assert r.take(8)==b'DSYOGG01'and r.num(4)==4;goldens=[]
for i in range(4):
 kind=r.num(1);q=r.blob();expected=r.blob();p=run/'golden-recapture.bin';p.write_bytes(q);mode='sync-read-ops'if kind else'sync-ops';ret=subprocess.run([str(new),mode,str(p)],capture_output=True);assert ret.returncode==0 and ret.stdout==expected
 goldens.append({'batch':i,'mode':mode,'request_sha256':sha(q),'response_sha256':sha(expected)})
r.done()
report={'scope':'Every original Ogg/Vorbis production method remains byte-identical after removing only the new Ogg exports and a private documentation comment. Old Ogg helper encode/decode/capture responses match bit for bit. Every saved new synchronization request/response is hash-verified and completely parsed; fresh offline goldens match C. Original PCM tolerances and the 172 seek inventory are unchanged.','before_commit':base['own_commit'],'existing_source_proof':unchanged,'old_binary_sha256':sha(old.read_bytes()),'new_binary_sha256':sha(new.read_bytes()),'old_protocols':rows,'new_protocol_receipts':receipts,'counts':counts,'fresh_goldens':goldens,'golden_sha256':sha(gold)}
(run/'sync-reference-checks.json').write_text(json.dumps(report,indent=2)+'\n',encoding='utf8',newline='\n');print(len(rows),'old protocol responses;',len(receipts),'new full receipts;',counts['operations'],'C operations;',len(goldens),'fresh goldens')
