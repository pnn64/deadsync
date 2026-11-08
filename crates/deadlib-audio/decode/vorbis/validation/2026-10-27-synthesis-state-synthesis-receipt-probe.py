from pathlib import Path
import hashlib,json,re,struct,numpy as np
repo=Path.cwd();root=repo/'target/deadlib-audio-reference';run=root/'seek-reference-20261027';gates=root/'synthesis-state-validation-final-20261027'
sha=lambda b:hashlib.sha256(b).hexdigest()
raw=(gates/'release.log').read_text(encoding='utf8')
# The test harness writes stdout while eprintln writes stderr. Remove only
# recognized progress tokens and allow whitespace within one diagnostic record.
clean=re.sub(r'test [a-zA-Z_][a-zA-Z_0-9:]* \.\.\. ok','',raw)
pattern=r'(synthesis-[^:\r\n]+):\s+(\d+) ops, (\d+) accepted, (\d+) discarded, (\d+) busy, (\d+) reads, (\d+) read errors, (\d+) finite values, (\d+) nonfinite classes, peak ([^,]+), RMS ([^,]+), signal ([^\r\n]+)'
rows=re.findall(pattern,clean);logs={r[0]:list(map(int,r[1:9]))for r in rows};assert len(logs)==len(rows)==1342
items={p.stem:(p,p.with_suffix('.out'))for p in(gates/'synthesis-inputs').glob('*.bin')};assert len(items)==1248
for folder in sorted((run/'captured-final').iterdir()):
 if folder.is_dir():items['synthesis-seek-trace-'+folder.name]=(folder/'schedule.bin',folder/'schedule.out')
for i in range(6):items[f'synthesis-state-golden-{i}']=(run/'golden-inputs'/f'synthesis-golden-{i}.bin',run/'golden-inputs'/f'synthesis-golden-{i}.out')
assert set(items)==set(logs),(set(items)-set(logs),set(logs)-set(items))
class R:
 def __init__(self,b):self.b=b;self.at=0
 def take(self,n):b=self.b[self.at:self.at+n];self.at+=n;assert len(b)==n;return b
 def num(self):return int.from_bytes(self.take(8),'little')
 def blob(self):return self.take(self.num())
 def done(self):assert self.at==len(self.b)
receipts=[];total=[0]*8
for index,(label,(p,o))in enumerate(sorted(items.items())):
 q=p.read_bytes();response=o.read_bytes();r=R(q);headers=[r.blob()for _ in range(3)];ch=headers[0][11];blocks=[1<<(headers[0][28]&15),1<<(headers[0][28]>>4)];cases=r.num();assert struct.unpack_from('<IQ',response)==(ch,cases)
 stride=53+ch*blocks[1]*4;assert(len(response)-12)%stride==0;operations=(len(response)-12)//stride;at=12;counts=[0]*8
 for _ in range(cases):
  for _ in range(r.num()):
   kind=r.num()
   if kind in[1,2]:r.take(24);r.blob()
   elif kind==3:r.take(8)
   elif kind==4:r.take(24)
   elif kind==6:
    large=r.num();assert large<=1;r.take(24+ch*blocks[large]*8)
   else:assert kind in[0,5,7]
   decoded,result=struct.unpack_from('<ii',response,at);at+=stride;counts[0]+=1
   if kind in[1,2,6]:counts[1 if decoded==0 else 2]+=1
   if kind in[3,5]:
    counts[4]+=1;counts[5]+=int(result!=0)
   elif result:counts[3]+=1
 r.done();assert at==len(response)and counts[0]==operations
 words=np.ndarray(shape=(operations,ch*blocks[1]),dtype='<u4',buffer=response,offset=65,strides=(stride,4))
 nonfinite=int(np.count_nonzero((words&0x7f800000)==0x7f800000));counts[6]=operations*ch*blocks[1]-nonfinite;counts[7]=nonfinite
 assert counts==logs[label],(label,counts,logs[label])
 total=[a+b for a,b in zip(total,counts)]
 receipts.append({'label':label,'request':p.relative_to(repo).as_posix(),'response':o.relative_to(repo).as_posix(),'request_sha256':sha(q),'response_sha256':sha(response),'request_bytes':len(q),'response_bytes':len(response),'counts':counts})
 if index%200==0:print(index,'complete C request/response counts verified',flush=True)
summary={'scope':'Every saved synthesis request and complete C response is hash-verified and independently parsed. Operation/acceptance/discard/busy/read/error counts and every finite/nonfinite buffer-word class agree with normalized native comparison logs. Raw logs are preserved; one stdout/stderr-interleaved diagnostic is recovered by removing recognized test progress tokens. No PCM bounds are changed.','release_log_sha256':sha((gates/'release.log').read_bytes()),'numpy':np.__version__,'columns':['operations','accepted_inputs','discarded_prefixes','busy_insertions','read_attempts','read_errors','finite_values','nonfinite_classes'],'comparisons':len(receipts),'procedural_requests':1248,'captured_requests':88,'golden_requests':6,'totals':total,'receipts':receipts}
(run/'synthesis-receipts.json').write_text(json.dumps(summary,indent=2)+'\n',encoding='utf8',newline='\n');print('Independent totals',len(receipts),total)
