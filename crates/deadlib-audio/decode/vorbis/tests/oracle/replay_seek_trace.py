"""Replay actual vorbisfile DSP schedules through the unmodified raw oracle.

Checks that the trace does not change file output, that direct synthesis has
identical queue/clock transitions, and that every returned PCM chunk has exactly
the same bytes. The resulting synthesis request/response also tests native Rust.
"""
from pathlib import Path
import argparse, hashlib, json, os, struct, subprocess

p=argparse.ArgumentParser()
p.add_argument('--input',type=Path,required=True)
p.add_argument('--requests',type=Path,required=True)
p.add_argument('--oracle',type=Path,required=True)
p.add_argument('--trace',type=Path,required=True)
p.add_argument('--output',type=Path,required=True)
p.add_argument('--floor0',action='store_true')
args=p.parse_args();out=args.output.resolve();out.mkdir(parents=True,exist_ok=False)
sha=lambda b:hashlib.sha256(b).hexdigest()
def call(exe,mode,path,requests=None,trace=False):
 env=os.environ.copy();env.pop('DEADLIB_SEEK_TRACE',None)
 if trace:env['DEADLIB_SEEK_TRACE']='1'
 cmd=[str(exe.resolve()),mode,str(path.resolve())]
 if requests:cmd.append(str(requests.resolve()))
 r=subprocess.run(cmd,env=env,capture_output=True,timeout=60)
 assert r.returncode==0,(cmd,r.stderr.decode(errors='replace'))
 return r
source=args.input.read_bytes();q=args.requests.read_bytes();normal=call(args.oracle,'oggseekstates',args.input,args.requests);traced=call(args.trace,'oggseekstates',args.input,args.requests,True)
assert normal.stdout==traced.stdout,'trace changes reference output'
(out/'input.ogg').write_bytes(source);(out/'requests.bin').write_bytes(q);(out/'file.out').write_bytes(normal.stdout);(out/'trace.tsv').write_bytes(traced.stderr)
headers=[];pending=bytearray();at=0
while len(headers)<3:
 assert source[at:at+4]==b'OggS';n=source[at+26];laces=source[at+27:at+27+n];body=at+27+n
 for size in laces:
  pending.extend(source[body:body+size]);body+=size
  if size<255:headers.append(bytes(pending));pending.clear()
  if len(headers)==3:break
 at=body
assert headers[0][:7]==b'\x01vorbis';channels=headers[0][11];blocks=[1<<(headers[0][28]&15),1<<(headers[0][28]>>4)];size=blocks[1]
blob=lambda b:struct.pack('<Q',len(b))+b
rows=traced.stderr.decode().splitlines();ops=[];expected=[];read_requests=[];request=-1;ready=False;i=0
while i<len(rows):
 fields=rows[i].split('\t');tag=fields[0];i+=1
 if tag=='payload':raise AssertionError('orphan payload')
 v=list(map(int,fields[1:]));assert len(v)==17,(tag,len(v))
 if tag=='seek-start':request+=1;read_requests.append([])
 if tag=='init':wire=struct.pack('<Q',7);ready=True
 elif tag=='restart':
  if v[12]==0:ready=False;continue
  if not ready:continue
  wire=struct.pack('<Q',0)
 elif tag in ['decoded','tracked']:
  payload=rows[i].split('\t');i+=1;assert payload[0]=='payload';data=bytes.fromhex(payload[2]);assert len(data)==int(payload[1]);assert ready
  wire=struct.pack('<QQQQ',1 if tag=='decoded'else 2,v[9]&((1<<64)-1),v[10]&((1<<64)-1),int(v[11]!=0))+blob(data)
 elif tag in ['discard','read']:
  assert ready and v[9]>=0,('negative reference read',tag,v[9])
  wire=struct.pack('<QQ',3,v[9])
 else:continue
 ops.append(wire);expected.append({'tag':tag,'state':v,'request':request})
 if tag=='read':read_requests[request].append(len(ops)-1)
wire=b''.join(map(blob,headers))+struct.pack('<QQ',1,len(ops))+b''.join(ops);request_path=out/'schedule.bin';request_path.write_bytes(wire)
raw=call(args.oracle,'synthesis-ops',request_path).stdout;(out/'schedule.out').write_bytes(raw);(out/'bounds.txt').write_text('floor0\n'if args.floor0 else'normal\n',encoding='utf8',newline='\n')
class Reader:
 def __init__(self,b):self.b=b;self.at=0
 def take(self,n):v=self.b[self.at:self.at+n];self.at+=n;assert len(v)==n;return v
 def num(self,n=8,signed=False):return int.from_bytes(self.take(n),'little',signed=signed)
 def done(self):assert self.at==len(self.b)
r=Reader(raw);assert r.num(4)==channels and r.num()==1;chunks={};accepted=busy=0
for index,item in enumerate(expected):
 decoded=r.num(4,True);insert=r.num(4,True);seq=r.num(signed=True);gp=r.num(signed=True);samples=r.num(signed=True);block=r.num(4);center=r.num(4);returned=r.num(4,True);current=r.num(4,True);frames=r.num(4);eos=r.num(1)
 v=item['state'];actual=(seq,gp,samples,block,center,returned,current,frames,eos);wanted=(v[3],v[4],v[5],blocks[v[13]],v[12],v[6],v[7],v[15],v[16]);assert actual==wanted,(index,item['tag'],actual,wanted)
 if item['tag']in['decoded','tracked']:assert decoded==0,(index,decoded);accepted+=1
 if insert:busy+=1
 backing=[r.take(size*4)for _ in range(channels)]
 if item['tag']=='read':
  n=v[9];start=returned-n;assert start>=0 and start+n<=size
  chunks[index]=(n,[p[start*4:(start+n)*4]for p in backing])
r.done();file=Reader(normal.stdout);assert file.num(4)==1;file.take(20);requests=int.from_bytes(q[:8],'little');assert request+1==requests
reads=0
for number in range(requests):
 status=file.num(4,True);tell=file.num(signed=True);records=[]
 while True:
  n=file.num(4)
  if not n:break
  assert n!=0xffffffff,'reference read failure';assert file.num(4)==0;planes=[file.take(n*4)for _ in range(channels)];records.append((n,planes))
 assert records==[chunks[i]for i in read_requests[number]],(number,status,tell)
 reads+=len(records)
file.done()
manifest={'scope':'Reference file output is byte-identical under tracing. Every direct-synthesis clock/queue transition matches its actual vorbisfile trace. All returned PCM chunks are byte-identical. Single logical stream only.','input_sha256':sha(source),'requests_sha256':sha(q),'file_response_sha256':sha(normal.stdout),'trace_sha256':sha(traced.stderr),'synthesis_request_sha256':sha(wire),'synthesis_response_sha256':sha(raw),'reference_binary_sha256':sha(args.oracle.read_bytes()),'trace_binary_sha256':sha(args.trace.read_bytes()),'operations':len(ops),'accepted_packets':accepted,'busy':busy,'read_chunks':reads,'requests':requests,'floor0':args.floor0}
(out/'checks.json').write_text(json.dumps(manifest,indent=2)+'\n',encoding='utf8',newline='\n');print(out.name,len(ops),'DSP operations,',reads,'exact PCM chunks')
