"""Independent direct-equation 8 kHz mixed-block oracle, no native source reads."""
from pathlib import Path
import argparse,hashlib,json,re,subprocess,sys
import numpy as np
tests=Path(__file__).resolve().parent;sys.path.insert(0,str(tests))
from reference_huffman import Bits
from reference_pcm import frames
p=argparse.ArgumentParser()
p.add_argument('--native',required=True);p.add_argument('--oracle',required=True)
for name in ['dewindow','output']:p.add_argument('--'+name,type=Path,required=True)
p.add_argument('--wsl-distro',help='invoke Linux binaries from Windows, e.g. Debian')
args=p.parse_args();out=args.output;out.mkdir(parents=True,exist_ok=False)
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
def toolpath(path):
 path=path.resolve()
 if args.wsl_distro:
  assert path.drive and str(path)[1]==':'
  return '/mnt/'+path.drive[0].lower()+'/'+path.as_posix()[3:]
 return str(path)
def command(binary,*arguments):
 return (['wsl','-d',args.wsl_distro,'--'] if args.wsl_distro else [])+[binary,*arguments]
bands=json.loads((tests/'data/bands.json').read_text())['rates']['8000'];long,short=bands
order=list(range(72))+[3*n+w for b in range(3,13) for w in range(3) for n in range(short[b],short[b+1])]
assert len(order)==576 and sorted(order)==list(range(576))
def frame(line):
 main=Bits();coded=order.index(line) if line is not None else None;big=0 if coded is None else coded//2+1
 for pair in range(big):
  x=int(2*pair==coded);y=int(2*pair+1==coded);main.put(1,1 if not x and not y else 2 if x else 3)
  if x or y:main.put(line%3==1,1)
 side=Bits()
 for value,width in [(0,8),(0,1),(len(main.bits),12),(big,9),(206,8),(0,9),(1,1),(2,2),(1,1),(1,5),(1,5),(0,3),(0,3),(0,3),(0,1),(0,1)]:side.put(value,width)
 assert len(side.bits)==72
 data=bytes.fromhex('ffe3e8c0')+side.data()+main.data();return data+bytes(1440-len(data))
source=out/'mixed8.mp3';source.write_bytes(b''.join(frame(line)+frame(None)*2 for line in range(576)))
assert sha(args.dewindow)==json.loads((tests/'data/dsp.json').read_text())['dewindow_sha256']
window=np.array([float(v) for _,v in re.findall(r'D\[\s*(\d+)\s*\]\s*=\s*([-+0-9.]+)',args.dewindow.read_text())])
assert len(window)==512
pi=np.pi
# Direct matrix equations with shifted 1024-element synthesis history, separate
# from the native ring buffer, generated matrices and transform implementation.
ml=np.cos(pi/72*(2*np.arange(36)[:,None]+19)*(2*np.arange(18)[None,:]+1))
ms=np.cos(pi/24*(2*np.arange(12)[:,None]+7)*(2*np.arange(6)[None,:]+1))
wl=np.sin(pi/36*(np.arange(36)+.5));ws=np.sin(pi/12*(np.arange(12)+.5))
matrix=np.cos(pi/64*(16+np.arange(64)[:,None])*(2*np.arange(32)[None,:]+1))
ci=np.array([-.6,-.535,-.33,-.185,-.095,-.041,-.0142,-.0037]);cs=1/np.sqrt(1+ci*ci);ca=ci*cs
indices=np.array([128*(n//64)+(n%64 if n%64<32 else n%64+64) for n in range(512)])
history=np.zeros(1024);overlap=np.zeros((32,18));expected=[]
for line in range(576):
 for active in [True,False,False]:
  spectrum=np.zeros(576)
  if active:spectrum[line]=-.5 if line%3==1 else .5
  for boundary in [18,36,54]:
   a=boundary-1-np.arange(8);b=boundary+np.arange(8);up=spectrum[a].copy();down=spectrum[b].copy()
   spectrum[a]=up*cs-down*ca;spectrum[b]=down*cs+up*ca
  spectrum=spectrum.reshape(32,18);transformed=np.zeros((32,36))
  transformed[:4]=(spectrum[:4]@ml.T)*wl
  for w in range(3):transformed[4:,6+6*w:18+6*w]+=(spectrum[4:,w::3]@ms.T)*ws
  hybrid=transformed[:,:18]+overlap;overlap=transformed[:,18:].copy();hybrid[1::2,1::2]*=-1
  pcm=[]
  for t in range(18):
   history[64:]=history[:-64].copy();history[:64]=matrix@hybrid[:,t]
   pcm.extend((history[indices]*window).reshape(16,32).sum(axis=0))
  expected.extend(pcm)
expected=np.array(expected);report={'method':__doc__,'limits':{'rms':2e-6,'max':2e-5},'source_sha256':sha(source),'script_sha256':sha(Path(__file__)),'dewindow_sha256':sha(args.dewindow),'bands_sha256':sha(tests/'data/bands.json'),'numpy_version':np.__version__,'comparisons':{}}
for mode in ['native','mad','mpg123','symphonia']:
 binary=args.native if mode in ['native','symphonia'] else args.oracle
 target=out/(mode+'.bin');cmd=command(binary,mode,toolpath(source),toolpath(target));run=subprocess.run(cmd,capture_output=True,text=True);assert not run.returncode,(cmd,run.stderr)
 actual=np.array([v for _,pcm in frames(target) for v in pcm]);assert len(actual)==len(expected)
 digest=subprocess.check_output(command('sha256sum',binary),text=True).split()[0] if args.wsl_distro else sha(Path(binary))
 delta=actual-expected;record={'rms':float(np.sqrt(np.mean(delta*delta))),'max':float(np.max(np.abs(delta))),'samples':len(actual),'command':cmd,'binary_sha256':digest,'pcm_sha256':sha(target)}
 per=np.max(np.abs(delta).reshape(576,1728),axis=1);record['disagreeing_frequencies']=np.flatnonzero(per>2e-5).tolist()
 report['comparisons'][mode]=record;print(mode,record['rms'],record['max'],len(record['disagreeing_frequencies']),flush=True)
report['passed']=report['comparisons']['native']['rms']<2e-6 and report['comparisons']['native']['max']<2e-5
(out/'results.json').write_text(json.dumps(report,indent=2)+'\n');assert report['passed']
