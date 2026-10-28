"""Experimental single-link/small-search controller, outside production.

Derives decisions from input bytes/headers and requests. Uses only native Rust
synthesis for runtime DSP. Retained C traces/output are consulted afterwards to
check decisions and PCM. Continuation/raw-seek and large bisection are pending.
"""
from pathlib import Path
import struct,subprocess,json,hashlib,math
repo=Path.cwd();root=repo/'target/deadlib-audio-reference';home=root/'seek-reference-model-20261027';captures=root/'seek-reference-20261027/captured-final'
sha=lambda b:hashlib.sha256(b).hexdigest();blob=lambda b:struct.pack('<Q',len(b))+b;u64=lambda n:struct.pack('<Q',n&((1<<64)-1))
class R:
 def __init__(self,b):self.b=b;self.at=0
 def take(self,n):b=self.b[self.at:self.at+n];self.at+=n;assert len(b)==n;return b
 def num(self,n=8,signed=False):return int.from_bytes(self.take(n),'little',signed=signed)
 def done(self):assert self.at==len(self.b)
class Dsp:
 def __init__(self,source,ch):
  self.p=subprocess.Popen([str(home/'target/release/deadlib-seek-reference-model.exe'),str(source)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE);self.ch=ch;self.modes=list(self.read(self.num(4)));self.frames=0;self.planes=[b'']*ch;self.ops=[]
 def read(self,n):
  b=self.p.stdout.read(n);assert len(b)==n,(n,self.p.poll(),self.p.stderr.read());return b
 def num(self,n):return int.from_bytes(self.read(n),'little')
 def call(self,kind,payload=b''):
  self.p.stdin.write(bytes([kind])+payload);self.p.stdin.flush();status=self.num(1);self.frames=self.num(4);self.planes=[self.read(self.frames*4)for _ in range(self.ch)];assert status!=2,(kind,payload);return status
 def reset(self):self.call(0);self.ops.append(u64(0))
 def packet(self,p,number,track):
  timing=u64(number)+u64(p['gp'])+u64(p['eos']);wire=timing+blob(p['data']);accepted=self.call(2 if track else 1,wire)
  if accepted:self.ops.append(u64(2 if track else 1)+wire)
  return accepted
 def consume(self,n):self.call(3,u64(n));self.ops.append(u64(3)+u64(n))
 def close(self):self.p.stdin.close();assert self.p.wait(timeout=10)==0,self.p.stderr.read()
def pages(b):
 at=0;pending=bytearray();start=0;out=[];all_packets=[]
 while at<len(b):
  offset=at;assert b[at:at+4]==b'OggS';flags=b[at+5];gp=int.from_bytes(b[at+6:at+14],'little',signed=True);serial=int.from_bytes(b[at+14:at+18],'little');n=b[at+26];laces=b[at+27:at+27+n];at+=27+n;packets=[]
  for size in laces:
   if not pending:start=offset
   pending.extend(b[at:at+size]);at+=size
   if size<255:
    packet={'data':bytes(pending),'gp':-1,'eos':False,'start':start,'end':offset};packets.append(packet);all_packets.append(packet);pending.clear()
  if packets:packets[-1]['gp']=gp;packets[-1]['eos']=bool(flags&4)
  out.append({'offset':offset,'end':at,'flags':flags,'gp':gp,'serial':serial,'packets':packets})
 assert not pending
 return out,all_packets
def run(folder):
 source=(folder/'input.ogg').read_bytes();ps,packets=pages(source);headers=[p['data']for p in packets[:3]];ch=headers[0][11];blocks=[1<<(headers[0][28]&15),1<<(headers[0][28]>>4)];d=Dsp(folder/'schedule.bin',ch);width=(len(d.modes)-1).bit_length()
 def declared(p):
  b=p['data']
  if not b or b[0]&1:return -1
  mode=(b[0]>>1)&((1<<width)-1)
  return blocks[d.modes[mode]]if mode<len(d.modes)else -1
 data_start=next(p['end']for p in ps if packets[2]in p['packets']);begin=next(i for i,p in enumerate(ps)if p['offset']>=data_start);serial=ps[0]['serial'];assert all(p['serial']==serial for p in ps)
 previous=-1;accumulated=0;origin=0
 for page in ps[begin:]:
  for p in page['packets']:
   block=declared(p)
   if block>=0:
    if previous>=0:accumulated+=(previous+block)//4
    previous=block
  if page['gp']!=-1:origin=max(0,page['gp']-accumulated);break
 end_offset=ps[-1]['offset'];duration=max(0,ps[-1]['gp']-origin);assert end_offset-data_start<65536
 q=R((folder/'requests.bin').read_bytes());response=R((folder/'file.out').read_bytes());assert response.num(4)==1
 assert response.num(4)==serial and response.num(4)==ch and response.num(4)==int.from_bytes(headers[0][12:16],'little') and response.num()==duration
 initialized=False;position=0;next_page=begin;queue=[];packet_number=0;base_offset=0;count=0;chunks=0;values=0;peak=energy=signal=0.
 def load():
  nonlocal next_page,queue
  if next_page==len(ps):return False
  page=ps[next_page];next_page+=1;queue.extend(p for p in page['packets']if p['start']>=base_offset);return True
 def peek():
  while not queue:
   if not load():return None
  return queue[0]
 def pop():
  nonlocal packet_number
  p=queue.pop(0);n=packet_number;packet_number+=1;return p,n
 def fetch():
  nonlocal position
  while peek()is not None:
   p,n=pop()
   if d.packet(p,n,False):
    if p['gp']!=-1 and not p['eos']:position=max(0,p['gp'])-d.frames
    return True
  return False
 for _ in range(q.num()):
  target=q.num();remaining=q.num();assert 0<=target<=duration
  # Exact small-range branch from ov_pcm_seek_page: candidate granules are
  # strictly below target. The final-link end fence is final-page start.
  best=None;first=None
  for i in range(begin,len(ps)):
   page=ps[i]
   if page['offset']>=end_offset and data_start!=end_offset:break
   if first is None:first=i
   if page['gp']==-1:continue
   if page['gp']<target+origin:best=i
   else:break
  anchor=first if best is None else best;assert anchor is not None
  next_page=anchor;queue=[];packet_number=0;base_offset=ps[anchor]['offset'];load()
  if initialized:d.reset()
  else:initialized=True;d.ops.append(u64(7))
  if best is None:position=0
  else:
   assert queue,'continued/raw-seek fallback not modeled'
   while queue[0]['gp']==-1:pop()
   position=max(0,queue[0]['gp']-origin)
  lastblock=0
  while (p:=peek())is not None:
   block=declared(p)
   if block<0:pop();continue
   if lastblock:position+=(lastblock+block)//4
   if position+(block+blocks[1])//4>=target:break
   p,n=pop();assert d.packet(p,n,True),'rejected track prefix not modeled'
   if p['gp']>-1:position=max(0,p['gp']-origin)
   lastblock=block
  while position<target:
   want=target-position;n=min(d.frames,want);d.consume(n);position+=n
   if n<want and not fetch():position=duration
  status=response.num(4,signed=True);tell=response.num(signed=True);assert status==0 and tell==position,(folder.name,target,status,tell,position)
  while remaining:
   while not d.frames:
    if not fetch():break
   if not d.frames:break
   n=min(d.frames,remaining,4096);expected=response.num(4);link=response.num(4);assert(expected,link)==(n,0),(folder.name,target,expected,n,link)
   for a in d.planes:
    b=response.take(n*4)
    for(x,),(y,)in zip(struct.iter_unpack('<f',a[:n*4]),struct.iter_unpack('<f',b)):
     if math.isfinite(y):assert math.isfinite(x);e=x-y;peak=max(peak,abs(e));energy+=e*e;signal=max(signal,abs(y));values+=1
     elif math.isnan(y):assert math.isnan(x)
     else:assert x==y
   d.consume(n);position+=n;remaining-=n;chunks+=1
  assert response.num(4)==0;count+=1
 q.done();response.done();d.close();rms=math.sqrt(energy/max(1,values));assert peak<=3e-6*max(1,signal) and rms<=5e-7*max(1,signal)
 derived=b''.join(map(blob,headers))+u64(1)+u64(len(d.ops))+b''.join(d.ops);expected=(folder/'schedule.bin').read_bytes();assert derived==expected,(folder.name,'derived DSP schedule differs')
 return {'label':folder.name,'input_sha256':sha(source),'derived_schedule_sha256':sha(derived),'file_response_sha256':sha((folder/'file.out').read_bytes()),'requests':count,'chunks':chunks,'values':values,'peak_error':peak,'rms_error':rms,'reference_peak':signal}
rows=[]
for folder in sorted(p for p in captures.iterdir()if p.is_dir()):
 row=run(folder);rows.append(row);print(row['label'],row['requests'],'requests;',row['chunks'],'chunks; exact derived schedule',flush=True)
(home/'checks.json').write_text(json.dumps({'scope':__doc__,'production_seek_differences_still_open':172,'controller_sha256':sha(Path(__file__).read_bytes()),'native_service_source_sha256':sha((home/'src/main.rs').read_bytes()),'native_service_binary_sha256':sha((home/'target/release/deadlib-seek-reference-model.exe').read_bytes()),'results':rows},indent=2)+'\n',encoding='utf8',newline='\n')
