"""Generate a test-only traced copy of the pinned vorbisfile implementation.

No checked-out reference source or ordinary oracle binary is modified. Hooks
only inspect existing fields and write integer records to stderr when
DEADLIB_SEEK_TRACE is set. stdout keeps the ordinary oracle protocol.
"""
from pathlib import Path
import argparse, hashlib, json, subprocess

parser = argparse.ArgumentParser()
parser.add_argument('--reference-root', type=Path, required=True)
parser.add_argument('--build', type=Path, required=True)
args = parser.parse_args()
root = args.reference_root.resolve()
build = args.build.resolve()
build.mkdir(parents=True, exist_ok=False)
source = root / 'vorbis/lib/vorbisfile.c'
original = source.read_text(encoding='utf8')
text = original
hooks = []

def replace(old, new, scope=None):
    global text
    start, end = (0, len(text)) if scope is None else (text.index(scope[0]), text.index(scope[1]))
    section = text[start:end]
    assert section.count(old) == 1, (scope, old, section.count(old))
    text = text[:start] + section.replace(old, new) + text[end:]
    hooks.append({'scope': scope, 'before': old, 'after': new})

definition = r'''
#include "codec_internal.h"
static void deadlib_seek_trace(const char *tag,OggVorbis_File *v,
                               ogg_int64_t a,ogg_int64_t b,ogg_int64_t c){
  vorbis_dsp_state *d=&v->vd;
  ogg_int64_t count=d->backend_state?((private_state*)d->backend_state)->sample_count:-1;
  if(!getenv("DEADLIB_SEEK_TRACE"))return;
  fprintf(stderr,"%s\t%lld\t%lld\t%d\t%lld\t%lld\t%lld\t%d\t%d\t%lld\t%lld\t%lld\t%lld\t%d\t%ld\t%ld\t%d\t%d\n",
    tag,(long long)v->offset,(long long)v->pcm_offset,v->current_link,
    (long long)d->sequence,(long long)d->granulepos,(long long)count,
    d->pcm_returned,d->pcm_current,(long long)v->os.packetno,
    (long long)a,(long long)b,(long long)c,d->centerW,d->W,d->lW,
    d->pcm_returned>=0&&d->pcm_current>d->pcm_returned?d->pcm_current-d->pcm_returned:0,d->eofflag!=0);
}
static void deadlib_seek_packet(const char *tag,OggVorbis_File *v,ogg_packet *op){
  long i;if(!getenv("DEADLIB_SEEK_TRACE"))return;
  deadlib_seek_trace(tag,v,op->packetno,op->granulepos,op->e_o_s);
  fprintf(stderr,"payload\t%ld\t",op->bytes);
  for(i=0;i<op->bytes;++i)fprintf(stderr,"%02x",(unsigned char)op->packet[i]);
  fputc('\n',stderr);
}
'''
replace('#include "misc.h"', '#include "misc.h"\n' + definition)
replace('vf->offset+=more;\n        return(ret);',
        'vf->offset+=more;\n        deadlib_seek_trace("page",vf,ret,ogg_page_granulepos(og),ogg_page_eos(og));\n        return(ret);')
assert text.count('vorbis_synthesis_restart(&vf->vd);') == 3
text=text.replace('vorbis_synthesis_restart(&vf->vd);','vorbis_synthesis_restart(&vf->vd);\n  deadlib_seek_trace("restart",vf,0,0,0);')
hooks.append({'scope':'all three restart calls','before':'vorbis_synthesis_restart(&vf->vd);','after':'same call followed by an integer state snapshot'})
replace('  vf->ready_state=INITSET;', '  vf->ready_state=INITSET;\n  deadlib_seek_trace("init",vf,0,0,0);')
page_scope = ('int ov_pcm_seek_page(', 'int ov_pcm_seek(')
replace('    ogg_page og;\n\n    /* if we have only one page',
        '    ogg_page og;\n    deadlib_seek_trace("bounds",vf,begin,end,target);\n\n    /* if we have only one page', page_scope)
replace('        vf->pcm_offset=total;',
        '        vf->pcm_offset=total;\n        deadlib_seek_trace("first-anchor",vf,begin,ogg_page_granulepos(&og),total);', page_scope)
replace('  /* verify result */',
        '  deadlib_seek_trace("page-ready",vf,pos,0,0);\n  /* verify result */', page_scope)
seek_scope = ('int ov_pcm_seek(', 'int ov_time_seek(')
replace('  int ret=ov_pcm_seek_page(vf,pos);',
        '  int ret;\n  deadlib_seek_trace("seek-start",vf,pos,0,0);\n  ret=ov_pcm_seek_page(vf,pos);', seek_scope)
replace('      thisblock=vorbis_packet_blocksize(vf->vi+vf->current_link,&op);',
        '      thisblock=vorbis_packet_blocksize(vf->vi+vf->current_link,&op);\n      deadlib_seek_trace("peek",vf,op.packetno,thisblock,op.granulepos);', seek_scope)
replace('vorbis_synthesis_blockin(&vf->vd,&vf->vb);\n\n      /* end of logical',
        'vorbis_synthesis_blockin(&vf->vd,&vf->vb);\n      deadlib_seek_packet("tracked",vf,&op);\n\n      /* end of logical', seek_scope)
replace('      lastblock=thisblock;',
        '      deadlib_seek_trace("track-cursor",vf,op.packetno,thisblock,op.granulepos);\n      lastblock=thisblock;', seek_scope)
replace('      vf->pcm_offset+=samples<<hs;',
        '      vf->pcm_offset+=samples<<hs;\n      deadlib_seek_trace("discard",vf,samples,target,pos);', seek_scope)
replace('  return 0;\n}',
        '  deadlib_seek_trace("seek-done",vf,pos,0,0);\n  return 0;\n}', seek_scope)
replace('              vorbis_synthesis_blockin(&vf->vd,&vf->vb);',
        '              vorbis_synthesis_blockin(&vf->vd,&vf->vb);\n              deadlib_seek_packet("decoded",vf,op_ptr);')
replace('              vf->pcm_offset=granulepos;',
        '              vf->pcm_offset=granulepos;\n              deadlib_seek_trace("fetch-cursor",vf,op_ptr->packetno,op_ptr->granulepos,samples);')
read_scope = ('long ov_read_float(', 'extern const float *vorbis_window(')
replace('        vf->pcm_offset+=samples<<hs;',
        '        vf->pcm_offset+=samples<<hs;\n        deadlib_seek_trace("read",vf,samples,length,0);', read_scope)
(build/'vorbisfile-traced.c').write_text(text, encoding='utf8', newline='\n')
oracle = Path(__file__).with_name('oracle.c').resolve()
(build/'oracle-snapshot.c').write_bytes(oracle.read_bytes())
cmake = f'''cmake_minimum_required(VERSION 3.16)
project(deadlib_vorbis_seek_trace C)
add_executable(deadlib_vorbis_seek_trace oracle-snapshot.c vorbisfile-traced.c)
target_include_directories(deadlib_vorbis_seek_trace PRIVATE "{root.as_posix()}/vorbis/include" "{root.as_posix()}/vorbis/lib" "{root.as_posix()}/ogg/include")
target_link_libraries(deadlib_vorbis_seek_trace PRIVATE "{root.as_posix()}/vorbis-oracle-build/vorbis/lib/Release/vorbis.lib" "{root.as_posix()}/vorbis-oracle-build/ogg/Release/ogg.lib")
'''
(build/'CMakeLists.txt').write_text(cmake, encoding='utf8', newline='\n')
commands = [['cmake','-S',str(build),'-B',str(build/'build'),'-A','x64'],
            ['cmake','--build',str(build/'build'),'--config','Release']]
for i, command in enumerate(commands):
    with (build/f'build-{i}.log').open('wb') as log:
        result = subprocess.run(command, stdout=log, stderr=subprocess.STDOUT)
    assert result.returncode == 0, (build/f'build-{i}.log').read_text(encoding='utf8', errors='replace')
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
binary = build/'build/Release/deadlib_vorbis_seek_trace.exe'
(build/'provenance.json').write_text(json.dumps({'original_source_sha256':sha(source),
    'patched_source_sha256':sha(build/'vorbisfile-traced.c'), 'oracle_source_sha256':sha(oracle),
    'script_sha256':sha(Path(__file__)), 'binary_sha256':sha(binary), 'commands':commands,
    'hooks':hooks}, indent=2)+'\n', encoding='utf8', newline='\n')
assert source.read_text(encoding='utf8') == original
print(binary)
