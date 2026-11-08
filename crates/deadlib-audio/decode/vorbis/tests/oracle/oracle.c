/* External test oracle. Private codebook APIs are deliberately confined here. */
#include <vorbis/codec.h>
#include <vorbis/vorbisfile.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "codebook.h"
#include "mdct.h"
#include "codec_internal.h"
#include "registry.h"
#ifdef _WIN32
#include <fcntl.h>
#include <io.h>
#endif
static void fail(const char *reason) { fprintf(stderr, "%s\n", reason); exit(1); }
static void put8(unsigned char value) { if (fputc(value, stdout) == EOF) fail("write"); }
static void put32(uint32_t value) { int i; for (i = 0; i < 4; ++i) put8((unsigned char)(value >> (i * 8))); }
static void put64(uint64_t value) { int i; for (i = 0; i < 8; ++i) put8((unsigned char)(value >> (i * 8))); }
static void putbytes(const void *data, size_t len) { if (fwrite(data, 1, len, stdout) != len) fail("write"); }
static uint64_t get64(FILE *file) {
    uint64_t value = 0; int i;
    for (i = 0; i < 8; ++i) { int c = fgetc(file); if (c == EOF) fail("truncated protocol"); value |= (uint64_t)c << (i * 8); }
    return value;
}
static unsigned char *packet(FILE *file, long *size) {
    uint64_t len = get64(file); unsigned char *data;
    if (len > (64u << 20)) fail("packet size");
    data = (unsigned char *)malloc((size_t)len + 1);
    if (!data || fread(data, 1, (size_t)len, file) != len) fail("packet read");
    *size = (long)len; return data;
}
static void vector(const void *data, uint32_t len) { put32(len); putbytes(data, len); }
static void headers(FILE *file) {
    vorbis_info info; vorbis_comment comments; int i;
    vorbis_info_init(&info); vorbis_comment_init(&comments);
    for (i = 0; i < 3; ++i) {
        ogg_packet op; memset(&op, 0, sizeof(op));
        op.packet = packet(file, &op.bytes); op.b_o_s = i == 0; op.packetno = i;
        if (vorbis_synthesis_headerin(&info, &comments, &op)) fail("header rejected by libvorbis");
        free(op.packet);
    }
    put32((uint32_t)info.channels); put32((uint32_t)info.rate);
    put32((uint32_t)info.bitrate_upper); put32((uint32_t)info.bitrate_nominal); put32((uint32_t)info.bitrate_lower);
    put32((uint32_t)vorbis_info_blocksize(&info, 0)); put32((uint32_t)vorbis_info_blocksize(&info, 1));
    vector(comments.vendor, (uint32_t)strlen(comments.vendor)); put32((uint32_t)comments.comments);
    for (i = 0; i < comments.comments; ++i) vector(comments.user_comments[i], (uint32_t)comments.comment_lengths[i]);
    vorbis_comment_clear(&comments); vorbis_info_clear(&info);
}
/* Header-only unpacking: never initialize or evaluate potentially degenerate VQ
   books. Report the reference bit reader's position even on an early rejection. */
static void bookheaders(FILE *file, int offsets) {
    uint64_t count = get64(file), i;
    if (count > (1u << 20)) fail("codebook header count");
    put64(count);
    for (i = 0; i < count; ++i) {
        uint64_t prefix = offsets ? get64(file) : 0;
        long size; unsigned char *data = packet(file, &size);
        oggpack_buffer bits; static_codebook *book;
        if (prefix > 7) fail("codebook prefix");
        oggpack_readinit(&bits, data, size); oggpack_adv(&bits, (int)prefix);
        book = vorbis_staticbook_unpack(&bits);
        put8(book != NULL); put64((uint64_t)oggpack_bits(&bits));
        if (offsets) {
            int initialized = 0;
            if (book) {
                codebook decoded;
                initialized = vorbis_book_init_decode(&decoded, book) == 0;
                vorbis_book_clear(&decoded);
            }
            put8((unsigned char)initialized);
        }
        if (book) vorbis_staticbook_destroy(book);
        free(data);
    }
    if (fgetc(file) != EOF) fail("trailing codebook headers");
}
/* The prefix layout is pinned to floor0.c. Copying its object representation
   avoids aliasing the opaque look through an incompatible structure type. */
typedef struct { int ln, m; int **linearmap; int n[2]; } floor0_look_prefix;
static void floor0cases(FILE *file) {
    vorbis_info info; vorbis_comment comments; vorbis_dsp_state dsp;
    codec_setup_info *ci; vorbis_look_floor *look; const vorbis_func_floor *floor;
    uint64_t count, c; int i;
    vorbis_info_init(&info); vorbis_comment_init(&comments);
    for (i = 0; i < 3; ++i) {
        ogg_packet op; memset(&op, 0, sizeof(op));
        op.packet = packet(file, &op.bytes); op.b_o_s = i == 0; op.packetno = i;
        if (vorbis_synthesis_headerin(&info, &comments, &op)) fail("floor0 header");
        free(op.packet);
    }
    ci = (codec_setup_info *)info.codec_setup;
    if (ci->floor_type[0] != 0 || vorbis_synthesis_init(&dsp, &info)) fail("floor0 init");
    floor = _floor_P[0]; look = floor->look(&dsp, ci->floor_param[0]);
    count = get64(file); if (count > (1u << 20)) fail("floor0 case count");
    put64(count);
    for (c = 0; c < count; ++c) {
        uint64_t w = get64(file), prefix = get64(file);
        long size; unsigned char *data = packet(file, &size); vorbis_block block;
        floor0_look_prefix view; void *memo; float *curve; uint32_t raw; int n;
        if (w > 1 || prefix > 7 || vorbis_block_init(&dsp, &block)) fail("floor0 case");
        block.W = (long)w; block.pcmend = ci->blocksizes[w]; n = block.pcmend / 2;
        oggpack_readinit(&block.opb, data, size); oggpack_adv(&block.opb, (int)prefix);
        memo = floor->inverse1(&block, look);
        put8(memo != NULL); put64((uint64_t)oggpack_bits(&block.opb)); put8(block.opb.ptr == NULL);
        put32((uint32_t)n);
        if (memo) memcpy(&raw, (float *)memo + ((vorbis_info_floor0 *)ci->floor_param[0])->order, 4);
        else raw = 0;
        put32(raw);
        curve = (float *)malloc((size_t)n * sizeof(*curve)); if (!curve) fail("floor0 curve");
        for (i = 0; i < n; ++i) curve[i] = 1.f;
        floor->inverse2(&block, look, memo, curve);
        memcpy(&view, look, sizeof(view));
        if (view.n[w] != n || !view.linearmap[w] || view.linearmap[w][n] != -1) fail("floor0 look layout");
        for (i = 0; i < n; ++i) put32((uint32_t)view.linearmap[w][i]);
        for (i = 0; i < n; ++i) { memcpy(&raw, curve + i, 4); put32(raw); }
        free(curve); free(data); vorbis_block_clear(&block);
    }
    if (fgetc(file) != EOF) fail("trailing floor0 cases");
    floor->free_look(look); vorbis_dsp_clear(&dsp); vorbis_comment_clear(&comments); vorbis_info_clear(&info);
}
static void floor1cases(FILE *file) {
    vorbis_info info; vorbis_comment comments; vorbis_dsp_state dsp;
    codec_setup_info *ci; vorbis_look_floor *look; const vorbis_func_floor *floor;
    uint64_t count, c; int i;
    vorbis_info_init(&info); vorbis_comment_init(&comments);
    for (i = 0; i < 3; ++i) {
        ogg_packet op; memset(&op, 0, sizeof(op));
        op.packet = packet(file, &op.bytes); op.b_o_s = i == 0; op.packetno = i;
        if (vorbis_synthesis_headerin(&info, &comments, &op)) fail("floor1 header");
        free(op.packet);
    }
    ci = (codec_setup_info *)info.codec_setup;
    if (ci->floor_type[0] != 1 || vorbis_synthesis_init(&dsp, &info)) fail("floor1 init");
    floor = _floor_P[1]; look = floor->look(&dsp, ci->floor_param[0]);
    count = get64(file); if (count > (1u << 20)) fail("floor1 count"); put64(count);
    for (c = 0; c < count; ++c) {
        uint64_t w = get64(file), prefix = get64(file), exhausted = get64(file);
        long size; unsigned char *data = packet(file, &size); vorbis_block block;
        void *memo; float *curve; int n; uint32_t raw;
        if (w > 1 || prefix > 7 || exhausted > 1 || vorbis_block_init(&dsp, &block)) fail("floor1 case");
        block.W = (long)w; block.pcmend = ci->blocksizes[w]; n = block.pcmend / 2;
        oggpack_readinit(&block.opb, data, size); oggpack_adv(&block.opb, (int)prefix);
        if (exhausted) oggpack_read(&block.opb, 33);
        memo = floor->inverse1(&block, look);
        put8(memo != NULL); put64((uint64_t)oggpack_bits(&block.opb)); put8(block.opb.ptr == NULL); put32((uint32_t)n);
        curve = (float *)malloc((size_t)(n + 16) * sizeof(float)); if (!curve) fail("floor1 curve");
        for (i = 0; i < n + 16; ++i) curve[i] = -1234.25f;
        for (i = 0; i < n; ++i) curve[8+i] = 1.f;
        floor->inverse2(&block, look, memo, curve+8);
        for (i = 0; i < 8; ++i) if (curve[i] != -1234.25f || curve[n+8+i] != -1234.25f) fail("floor1 guard");
        for (i = 0; i < n; ++i) {memcpy(&raw, curve+8+i, 4); put32(raw);}
        free(curve); free(data); vorbis_block_clear(&block);
    }
    if (fgetc(file) != EOF) fail("trailing floor1 cases");
    floor->free_look(look); vorbis_dsp_clear(&dsp); vorbis_comment_clear(&comments); vorbis_info_clear(&info);
}
/* Direct unmodified vector-add callbacks, including nonmultiple n and tails. */
static void vqaddcases(FILE *file) {
    long booksize; unsigned char *bookdata = packet(file, &booksize);
    oggpack_buffer packed; static_codebook *book; codebook decoded;
    uint64_t count, c;
    oggpack_readinit(&packed, bookdata, booksize); book = vorbis_staticbook_unpack(&packed);
    if (!book || book->maptype == 0 || vorbis_book_init_decode(&decoded, book)) fail("VQ add book");
    if (decoded.used_entries && decoded.dim == 0) fail("active zero-dimensional VQ add");
    count = get64(file); if (count > (1u << 20)) fail("VQ add count"); put64(count);
    for (c = 0; c < count; ++c) {
        uint64_t kind = get64(file), prefix = get64(file), exhausted = get64(file);
        uint64_t start = get64(file), size = get64(file), channels = get64(file), bins = get64(file);
        long bytes, result; unsigned char *data = packet(file, &bytes); oggpack_buffer bits;
        float *storage, **planes; size_t stride, ch, i; uint32_t raw;
        if (kind > 2 || prefix > 7 || exhausted > 1 || channels < 1 || channels > 16 || bins < 1 || bins > 256) fail("VQ add shape");
        if (start + size > bins * (kind == 2 ? channels : 1)) fail("VQ add range");
        stride = (size_t)bins + 16;
        storage = (float *)malloc((size_t)channels * stride * sizeof(float));
        planes = (float **)malloc((size_t)channels * sizeof(float *));
        if (!storage || !planes) fail("VQ add allocation");
        for (ch = 0; ch < channels; ++ch) {
            planes[ch] = storage + ch * stride + 8;
            for (i = 0; i < stride; ++i) storage[ch * stride + i] = -1234.25f;
            for (i = 0; i < bins; ++i) planes[ch][i] = .125f + (float)ch * .0625f + (float)i * .00390625f;
        }
        oggpack_readinit(&bits, data, bytes); oggpack_adv(&bits, (int)prefix);
        if (exhausted) oggpack_read(&bits, 33);
        if (kind == 0) result = vorbis_book_decodevs_add(&decoded, planes[0] + start, &bits, (int)size);
        else if (kind == 1) result = vorbis_book_decodev_add(&decoded, planes[0] + start, &bits, (int)size);
        else result = vorbis_book_decodevv_add(&decoded, planes, (long)start, (int)channels, &bits, (int)size);
        put8(result == 0); put64((uint64_t)oggpack_bits(&bits)); put8(bits.ptr == NULL);
        for (ch = 0; ch < channels; ++ch) {
            for (i = 0; i < 8; ++i) if (storage[ch * stride + i] != -1234.25f || planes[ch][bins + i] != -1234.25f) fail("VQ add guard changed");
            for (i = 0; i < bins; ++i) { memcpy(&raw, planes[ch] + i, 4); put32(raw); }
        }
        free(storage); free(planes); free(data);
    }
    if (fgetc(file) != EOF) fail("trailing VQ add cases");
    vorbis_book_clear(&decoded); vorbis_staticbook_destroy(book); free(bookdata);
}
static void floatcases(FILE *file) {
    uint64_t count = get64(file), i;
    if (count > (1u << 20)) fail("float count");
    put64(count);
    for (i = 0; i < count; ++i) {
        uint64_t word = get64(file); uint32_t raw; float value;
        if (word > UINT32_MAX) fail("float word");
        value = _float32_unpack((long)word); memcpy(&raw, &value, 4); put32(raw);
    }
    if (fgetc(file) != EOF) fail("trailing floats");
}
static void books(FILE *file) {
    long size; unsigned char *data = packet(file, &size); oggpack_buffer bits;
    int count, i;
    if (size < 8 || memcmp(data, "\005vorbis", 7)) fail("setup signature");
    oggpack_readinit(&bits, data + 7, size - 7); count = (int)oggpack_read(&bits, 8) + 1;
    put32((uint32_t)count);
    for (i = 0; i < count; ++i) {
        static_codebook *book = vorbis_staticbook_unpack(&bits); codebook decoded, encoded;
        oggpack_buffer wire; long entry, dimension; float *values;
        if (!book || vorbis_book_init_decode(&decoded, book)) fail("codebook rejected by libvorbis");
        put32((uint32_t)book->dim); put32((uint32_t)book->entries); put8((unsigned char)book->maptype);
        putbytes(book->lengthlist, (size_t)book->entries);
        values = book->maptype ? _book_unquantize(book, (int)book->entries, NULL) : NULL;
        if (book->maptype && !values) fail("unquantize");
        for (entry = 0; entry < book->entries; ++entry) {
            if (book->lengthlist[entry] == 0) continue;
            if (values) for (dimension = 0; dimension < book->dim; ++dimension) {
                uint32_t raw; memcpy(&raw, &values[entry * book->dim + dimension], sizeof(raw)); put32(raw);
            }
        }
        free(values);
        if (vorbis_book_init_encode(&encoded, book)) fail("encode book init");
        oggpack_writeinit(&wire);
        for (entry = 0; entry < book->entries; ++entry) {
            if (book->lengthlist[entry]) vorbis_book_encode(&encoded, (int)entry, &wire);
        }
        put64((uint64_t)oggpack_bits(&wire)); vector(oggpack_get_buffer(&wire), (uint32_t)oggpack_bytes(&wire));
        oggpack_writeclear(&wire); vorbis_book_clear(&encoded); vorbis_book_clear(&decoded); vorbis_staticbook_destroy(book);
    }
    free(data);
}
static void pcm(FILE *file, int reset_cases) {
    vorbis_info info; vorbis_comment comments; vorbis_dsp_state dsp; vorbis_block block;
    ogg_int64_t number = 0; int i;
    vorbis_info_init(&info); vorbis_comment_init(&comments);
    for (i = 0; i < 3; ++i) {
        ogg_packet op; memset(&op, 0, sizeof(op)); op.packet = packet(file, &op.bytes);
        op.b_o_s = i == 0; op.packetno = number++;
        if (vorbis_synthesis_headerin(&info, &comments, &op)) fail("PCM header"); free(op.packet);
    }
    if (vorbis_synthesis_init(&dsp, &info) || vorbis_block_init(&dsp, &block)) fail("PCM init");
    put32((uint32_t)info.channels);
    {
    uint64_t cases = reset_cases ? get64(file) : 1, c;
    if (cases > (1u << 20)) fail("PCM case count");
    if (reset_cases) put64(cases);
    for (c = 0; c < cases; ++c) {
      uint64_t remaining = reset_cases ? get64(file) : UINT64_MAX;
      if (reset_cases) {
        if (remaining > (1u << 20) || vorbis_synthesis_restart(&dsp)) fail("PCM case restart/count");
        number = 3;
      }
      for (; remaining; --remaining) {
        ogg_packet op; int marker = fgetc(file), result, frames, ch, sample; float **samples;
        if (marker == EOF) { if (reset_cases) fail("truncated PCM case"); break; }
        if (ungetc(marker, file) == EOF) fail("ungetc");
        memset(&op, 0, sizeof(op)); op.packet = packet(file, &op.bytes); op.packetno = number++;
        /* No container timing or EOS trim: test pure packet synthesis/overlap. */
        op.granulepos = -1; result = vorbis_synthesis(&block, &op);
        free(op.packet);
        if (result != 0) { put8('S'); continue; }
        if (vorbis_synthesis_blockin(&dsp, &block)) fail("blockin");
        frames = vorbis_synthesis_pcmout(&dsp, &samples); if (frames < 0) fail("pcmout");
        put8('P'); put32((uint32_t)block.pcmend); put32((uint32_t)frames);
        for (ch = 0; ch < info.channels; ++ch) for (sample = 0; sample < frames; ++sample) {
            uint32_t raw; memcpy(&raw, &samples[ch][sample], sizeof(raw)); put32(raw);
        }
        if (vorbis_synthesis_read(&dsp, frames)) fail("synthesis read");
      }
    }
    if (reset_cases && fgetc(file) != EOF) fail("trailing PCM cases");
    }
    vorbis_block_clear(&block); vorbis_dsp_clear(&dsp); vorbis_comment_clear(&comments); vorbis_info_clear(&info);
}
static void pcm_timed_cases(FILE *file) {
    vorbis_info info; vorbis_comment comments; vorbis_dsp_state dsp; vorbis_block block;
    uint64_t cases, c; int i;
    vorbis_info_init(&info); vorbis_comment_init(&comments);
    for (i = 0; i < 3; ++i) {
        ogg_packet op; memset(&op, 0, sizeof(op)); op.packet = packet(file, &op.bytes); op.b_o_s = i == 0; op.packetno = i;
        if (vorbis_synthesis_headerin(&info, &comments, &op)) fail("timed PCM header"); free(op.packet);
    }
    if (vorbis_synthesis_init(&dsp, &info) || vorbis_block_init(&dsp, &block)) fail("timed PCM init");
    cases = get64(file); if (cases > (1u << 20)) fail("timed PCM cases"); put32((uint32_t)info.channels); put64(cases);
    for (c = 0; c < cases; ++c) {
        uint64_t count = get64(file), j;
        if (count > (1u << 20) || vorbis_synthesis_restart(&dsp)) fail("timed PCM restart");
        for (j = 0; j < count; ++j) {
            ogg_packet op; uint64_t gp = get64(file), eos = get64(file); int frames, ch, sample; float **pcm;
            if (eos > 1) fail("timed PCM EOS");
            memset(&op, 0, sizeof(op)); op.granulepos = (ogg_int64_t)gp; op.e_o_s = (long)eos; op.packetno = (ogg_int64_t)j + 3; op.packet = packet(file, &op.bytes);
            i = vorbis_synthesis(&block, &op); free(op.packet);
            if (i) {put8('S'); continue;}
            if (vorbis_synthesis_blockin(&dsp, &block)) fail("timed PCM blockin");
            frames = vorbis_synthesis_pcmout(&dsp, &pcm); if (frames < 0) fail("timed PCM out");
            put8('P'); put32((uint32_t)block.pcmend); put32((uint32_t)frames); put64((uint64_t)dsp.granulepos);
            for (ch = 0; ch < info.channels; ++ch) for (sample = 0; sample < frames; ++sample) {uint32_t raw; memcpy(&raw, pcm[ch]+sample, 4); put32(raw);}
            if (vorbis_synthesis_read(&dsp, frames)) fail("timed PCM read");
        }
    }
    if (fgetc(file) != EOF) fail("trailing timed PCM cases");
    vorbis_block_clear(&block); vorbis_dsp_clear(&dsp); vorbis_comment_clear(&comments); vorbis_info_clear(&info);
}
/* Inject only counters after real overlap priming; blockin remains unmodified.
   This covers multi-hour and host-long narrowing boundaries without huge PCM. */
static void eos_state_cases(FILE *file) {
    vorbis_info info; vorbis_comment comments; vorbis_dsp_state dsp; vorbis_block block;
    unsigned char *audio[3]; long sizes[3]; uint64_t count, c; int i;
    vorbis_info_init(&info); vorbis_comment_init(&comments);
    for (i=0;i<3;++i) {
        ogg_packet op; memset(&op,0,sizeof(op)); op.packet=packet(file,&op.bytes); op.packetno=i; op.b_o_s=i==0;
        if(vorbis_synthesis_headerin(&info,&comments,&op)) fail("EOS state header"); free(op.packet);
    }
    for(i=0;i<3;++i) audio[i]=packet(file,&sizes[i]);
    if(vorbis_synthesis_init(&dsp,&info)||vorbis_block_init(&dsp,&block)) fail("EOS state init");
    count=get64(file); if(count>(1u<<20)) fail("EOS state count"); put32(sizeof(long)); put32(info.channels); put64(count);
    for(c=0;c<count;++c) {
        uint64_t known=get64(file), state=get64(file), gp=get64(file); float **pcm; int n,ch,j;
        if(known>1||vorbis_synthesis_restart(&dsp)) fail("EOS state restart");
        for(i=0;i<3;++i) {
            ogg_packet op; memset(&op,0,sizeof(op)); op.packet=audio[i]; op.bytes=sizes[i]; op.packetno=i+3; op.granulepos=-1;
            if(i==2) {dsp.granulepos=known?(ogg_int64_t)state:-1; ((private_state*)dsp.backend_state)->sample_count=(ogg_int64_t)state; op.granulepos=(ogg_int64_t)gp; op.e_o_s=1;}
            if(vorbis_synthesis(&block,&op)||vorbis_synthesis_blockin(&dsp,&block)) fail("EOS state synthesis");
            n=vorbis_synthesis_pcmout(&dsp,&pcm); if(n<0) fail("EOS state pcmout");
            if(i==2) {
                put32(block.pcmend); put32(n); put64((uint64_t)dsp.granulepos); put64((uint64_t)((private_state*)dsp.backend_state)->sample_count);
                for(ch=0;ch<info.channels;++ch) for(j=0;j<n;++j) {uint32_t raw; memcpy(&raw,pcm[ch]+j,4);put32(raw);}
            }
            if(vorbis_synthesis_read(&dsp,n)) fail("EOS state read");
        }
    }
    if(fgetc(file)!=EOF) fail("trailing EOS states");
    for(i=0;i<3;++i) free(audio[i]);
    vorbis_block_clear(&block); vorbis_dsp_clear(&dsp); vorbis_comment_clear(&comments); vorbis_info_clear(&info);
}
/* Arbitrary decode/track/read/restart schedules. All queue/clock decisions and
   the entire retained PCM buffer are observable, including stale overlap. */
static void synthesis_ops(FILE *file) {
    vorbis_info info; vorbis_comment comments; vorbis_dsp_state dsp; vorbis_block block;
    uint64_t cases,c;int i;vorbis_info_init(&info);vorbis_comment_init(&comments);
    for(i=0;i<3;++i) {ogg_packet op;memset(&op,0,sizeof(op));op.packet=packet(file,&op.bytes);op.b_o_s=i==0;op.packetno=i;if(vorbis_synthesis_headerin(&info,&comments,&op))fail("synthesis ops header");free(op.packet);}
    if(vorbis_synthesis_init(&dsp,&info)||vorbis_block_init(&dsp,&block))fail("synthesis ops init");
    cases=get64(file);if(cases>65536)fail("synthesis ops count");put32(info.channels);put64(cases);
    for(c=0;c<cases;++c) {
        uint64_t n=get64(file),j;if(n>65536||vorbis_synthesis_restart(&dsp))fail("synthesis ops restart");
        for(j=0;j<n;++j) {
            uint64_t kind=get64(file);int decoded=0,result=0,frames,ch,k;float **pcm;
            if(kind==0) result=vorbis_synthesis_restart(&dsp);
            else if(kind==1||kind==2) {
                ogg_packet op;memset(&op,0,sizeof(op));op.packetno=(ogg_int64_t)get64(file);op.granulepos=(ogg_int64_t)get64(file);op.e_o_s=(long)get64(file);op.packet=packet(file,&op.bytes);
                decoded=kind==1?vorbis_synthesis(&block,&op):vorbis_synthesis_trackonly(&block,&op);free(op.packet);
                if(!decoded) result=vorbis_synthesis_blockin(&dsp,&block);
            }else if(kind==3) {uint64_t count=get64(file);if(count>INT32_MAX)fail("synthesis ops read count");result=vorbis_synthesis_read(&dsp,(int)count);}
            else if(kind==4) {dsp.granulepos=(ogg_int64_t)get64(file);((private_state*)dsp.backend_state)->sample_count=(ogg_int64_t)get64(file);dsp.sequence=(ogg_int64_t)get64(file);}
            else if(kind==5) result=vorbis_synthesis_read(&dsp,vorbis_synthesis_pcmout(&dsp,NULL));
            else if(kind==7) {vorbis_block_clear(&block);vorbis_dsp_clear(&dsp);if(vorbis_synthesis_init(&dsp,&info)||vorbis_block_init(&dsp,&block))fail("synthesis ops reinitialize");}
            else if(kind==6) {
                uint64_t large=get64(file);float **raw;int ch,k,n;
                if(large>1)fail("synthesis ops direct block");n=(int)vorbis_info_blocksize(&info,(int)large);
                block.W=(long)large;block.sequence=(ogg_int64_t)get64(file);block.granulepos=(ogg_int64_t)get64(file);block.eofflag=(long)get64(file);block.pcmend=n;
                raw=(float**)malloc(info.channels*sizeof(*raw));if(!raw)fail("synthesis ops direct channels");
                for(ch=0;ch<info.channels;++ch){raw[ch]=(float*)malloc(n*sizeof(float));if(!raw[ch])fail("synthesis ops direct data");for(k=0;k<n;++k){uint64_t bits=get64(file);uint32_t word=(uint32_t)bits;memcpy(raw[ch]+k,&word,4);}}
                block.pcm=raw;result=vorbis_synthesis_blockin(&dsp,&block);block.pcm=NULL;block.pcmend=0;
                for(ch=0;ch<info.channels;++ch)free(raw[ch]);free(raw);
            }
            else fail("synthesis ops kind");
            frames=vorbis_synthesis_pcmout(&dsp,&pcm);if(frames<0)fail("synthesis ops pcmout");
            put32((uint32_t)decoded);put32((uint32_t)result);put64((uint64_t)dsp.sequence);put64((uint64_t)dsp.granulepos);put64((uint64_t)((private_state*)dsp.backend_state)->sample_count);
            put32((uint32_t)vorbis_info_blocksize(&info,(int)dsp.W));put32((uint32_t)dsp.centerW);put32((uint32_t)dsp.pcm_returned);put32((uint32_t)dsp.pcm_current);put32((uint32_t)frames);put8(dsp.eofflag!=0);
            /* Check full backing storage, not only the reported queue. */
            for(ch=0;ch<info.channels;++ch)for(k=0;k<dsp.pcm_storage;++k){uint32_t raw;memcpy(&raw,dsp.pcm[ch]+k,4);put32(raw);}
        }
    }
    if(fgetc(file)!=EOF)fail("trailing synthesis ops");vorbis_block_clear(&block);vorbis_dsp_clear(&dsp);vorbis_comment_clear(&comments);vorbis_info_clear(&info);
}
static void transform(FILE *file, int forward) {
    long size; unsigned char *data = packet(file, &size); int n, k, in_count, out_count; float *input, *output; mdct_lookup lookup;
    if (size < 4) fail("MDCT protocol");
    n = (int)((uint32_t)data[0] | (uint32_t)data[1] << 8 | (uint32_t)data[2] << 16 | (uint32_t)data[3] << 24);
    in_count = forward ? n : n / 2; out_count = forward ? n / 2 : n;
    if (n < 64 || n > 8192 || (n & (n - 1)) || size != 4 + in_count * 4) fail("MDCT size");
    input = (float *)malloc((size_t)in_count * sizeof(float)); output = (float *)malloc((size_t)out_count * sizeof(float));
    if (!input || !output) fail("MDCT allocation");
    for (k = 0; k < in_count; ++k) {
        unsigned char *p = data + 4 + k * 4;
        uint32_t raw = (uint32_t)p[0] | (uint32_t)p[1] << 8 | (uint32_t)p[2] << 16 | (uint32_t)p[3] << 24;
        memcpy(&input[k], &raw, sizeof(raw));
    }
    mdct_init(&lookup, n);
    if (forward) mdct_forward(&lookup, input, output); else mdct_backward(&lookup, input, output);
    for (k = 0; k < out_count; ++k) { uint32_t raw; memcpy(&raw, &output[k], sizeof(raw)); put32(raw); }
    mdct_clear(&lookup); free(input); free(output); free(data);
}
static void ogg_pcm(FILE *file, int raw_start) {
    OggVorbis_File reader; int links, i, link;
    if (ov_open_callbacks(file, &reader, NULL, 0, OV_CALLBACKS_NOCLOSE)) fail("vorbisfile open");
    links = (int)ov_streams(&reader); put32((uint32_t)links);
    for (i = 0; i < links; ++i) {
        vorbis_info *info = ov_info(&reader, i); ogg_int64_t total = ov_pcm_total(&reader, i);
        if (!info || total < 0) fail("vorbisfile stream information");
        put32((uint32_t)ov_serialnumber(&reader, i)); put32((uint32_t)info->channels); put32((uint32_t)info->rate); put64((uint64_t)total);
    }
    /* No PCM seek target exists in an empty stream. The reported count is zero. */
    if (ov_pcm_total(&reader, -1) == 0) { ov_clear(&reader); return; }
    /* Establish the first link's cursor explicitly after the metadata scan. */
    if (raw_start ? ov_raw_seek(&reader, 0) : ov_pcm_seek(&reader, 0)) fail("vorbisfile initial seek");
    for (;;) {
        float **samples; long frames = ov_read_float(&reader, &samples, 4096, &link); int ch; long frame;
        if (frames == 0) break; if (frames < 0) fail("vorbisfile decode/hole");
        put32((uint32_t)link); put32((uint32_t)frames);
        for (ch = 0; ch < ov_info(&reader, link)->channels; ++ch) for (frame = 0; frame < frames; ++frame) {
            uint32_t raw; memcpy(&raw, &samples[ch][frame], sizeof(raw)); put32(raw);
        }
    }
    ov_clear(&reader);
}
/* Unseekable public vorbisfile API: no metadata scan, initial seek or zero-total shortcut. */
static void ogg_stream(FILE *file) {
    OggVorbis_File reader; vorbis_info *info; int link;
    if (ov_open_callbacks(file, &reader, NULL, 0, OV_CALLBACKS_STREAMONLY_NOCLOSE)) fail("stream open");
    info = ov_info(&reader, -1); if (!info) fail("stream info"); put32((uint32_t)info->channels); put32((uint32_t)info->rate);
    for (;;) {
        float **pcm; long n = ov_read_float(&reader, &pcm, 4096, &link); long i; int ch;
        if (n < 0) fail("stream decode/hole"); if (!n) break;
        info = ov_info(&reader, -1); if (!info) fail("stream current info");
        put32((uint32_t)n); put32((uint32_t)link); put32((uint32_t)info->channels); put32((uint32_t)info->rate);
        for (ch = 0; ch < info->channels; ++ch) for (i = 0; i < n; ++i) {uint32_t raw; memcpy(&raw, pcm[ch]+i, 4); put32(raw);}
    }
    put32(0); ov_clear(&reader);
}
/* Report real seek status/tell, including zero declared totals; retain errors as data. */
static void ogg_seek_states(FILE *file, const char *path) {
    OggVorbis_File reader; FILE *requests; uint64_t count, j; int links, i;
    if (ov_open_callbacks(file, &reader, NULL, 0, OV_CALLBACKS_NOCLOSE)) fail("seek states open");
    links = (int)ov_streams(&reader); put32((uint32_t)links);
    for (i = 0; i < links; ++i) {
        vorbis_info *info = ov_info(&reader, i); ogg_int64_t n = ov_pcm_total(&reader, i);
        if (!info || n < 0) fail("seek states info"); put32((uint32_t)ov_serialnumber(&reader,i)); put32((uint32_t)info->channels); put32((uint32_t)info->rate); put64((uint64_t)n);
    }
    requests = fopen(path,"rb"); if (!requests) fail("seek states requests"); count = get64(requests); if (count > 1000000) fail("seek states count");
    for (j = 0; j < count; ++j) {
        uint64_t target = get64(requests), remaining = get64(requests); int status;
        if (target > INT64_MAX || remaining > 100000000) fail("seek states size"); status = ov_pcm_seek(&reader,(ogg_int64_t)target); put32((uint32_t)status); put64((uint64_t)ov_pcm_tell(&reader));
        if (!status) while (remaining) {
            float **pcm; int link, ch; long sample; long n = ov_read_float(&reader,&pcm,remaining < 4096 ? (int)remaining : 4096,&link);
            if (n < 0) {put32(UINT32_MAX); put32((uint32_t)n); break;} if (!n) break;
            put32((uint32_t)n); put32((uint32_t)link);
            for (ch = 0; ch < ov_info(&reader,link)->channels; ++ch) for (sample = 0; sample < n; ++sample) {uint32_t raw; memcpy(&raw,pcm[ch]+sample,4); put32(raw);}
            remaining -= (uint64_t)n;
        }
        put32(0);
    }
    if (fgetc(requests) != EOF) fail("trailing seek states requests"); fclose(requests); ov_clear(&reader);
}
static void ogg_seek(FILE *file, const char *requests_path) {
    OggVorbis_File reader; FILE *requests; uint64_t count, request; int links, i;
    if (ov_open_callbacks(file, &reader, NULL, 0, OV_CALLBACKS_NOCLOSE)) fail("vorbisfile seek open");
    links = (int)ov_streams(&reader); put32((uint32_t)links);
    for (i = 0; i < links; ++i) {
        vorbis_info *info = ov_info(&reader, i); ogg_int64_t total = ov_pcm_total(&reader, i);
        if (!info || total < 0) fail("vorbisfile seek information");
        put32((uint32_t)ov_serialnumber(&reader, i)); put32((uint32_t)info->channels);
        put32((uint32_t)info->rate); put64((uint64_t)total);
    }
    requests = fopen(requests_path, "rb"); if (!requests) fail("seek requests open");
    count = get64(requests); if (count > 1000000) fail("seek request count");
    for (request = 0; request < count; ++request) {
        uint64_t target = get64(requests), remaining = get64(requests);
        if (target > INT64_MAX || remaining > 100000000) fail("seek request size");
        if (ov_pcm_total(&reader, -1) == 0 && target == 0) { put64(0); put32(0); continue; }
        if (ov_pcm_seek(&reader, (ogg_int64_t)target)) fail("vorbisfile PCM seek");
        put64((uint64_t)ov_pcm_tell(&reader));
        while (remaining) {
            float **samples; int link, ch; long frame;
            long frames = ov_read_float(&reader, &samples, remaining < 4096 ? (int)remaining : 4096, &link);
            if (frames < 0) fail("vorbisfile seek decode/hole");
            if (frames == 0) break;
            put32((uint32_t)frames); put32((uint32_t)link);
            for (ch = 0; ch < ov_info(&reader, link)->channels; ++ch) for (frame = 0; frame < frames; ++frame) {
                uint32_t raw; memcpy(&raw, &samples[ch][frame], sizeof(raw)); put32(raw);
            }
            remaining -= (uint64_t)frames;
        }
        put32(0);
    }
    fclose(requests); ov_clear(&reader);
}
int main(int argc, char **argv) {
    FILE *file; if (argc < 3 || ((!strcmp(argv[1], "oggseek") || !strcmp(argv[1], "oggseekstates")) ? argc != 4 : argc != 3)) fail("usage: oracle headers|bookheaders|bookcases|floor0cases|floor1cases|floatcases|vqaddcases|books|pcm|pcm-cases|pcm-timed-cases|eos-state-cases|synthesis-ops|mdct-cases|mdct|mdctforward|oggpcm|oggpcmraw|oggstream input; oggseek|oggseekstates input requests");
    file = fopen(argv[2], "rb"); if (!file) fail("open");
#ifdef _WIN32
    _setmode(_fileno(stdout), _O_BINARY);
#endif
    if (!strcmp(argv[1], "headers")) headers(file);
    else if (!strcmp(argv[1], "floor1cases")) floor1cases(file);
    else if (!strcmp(argv[1], "floatcases")) floatcases(file);
    else if (!strcmp(argv[1], "vqaddcases")) vqaddcases(file);
    else if (!strcmp(argv[1], "books")) books(file);
    else if (!strcmp(argv[1], "bookheaders")) bookheaders(file, 0);
    else if (!strcmp(argv[1], "bookcases")) bookheaders(file, 1);
    else if (!strcmp(argv[1], "floor0cases")) floor0cases(file);
    else if (!strcmp(argv[1], "pcm")) pcm(file, 0);
    else if (!strcmp(argv[1], "pcm-cases")) pcm(file, 1);
    else if (!strcmp(argv[1], "pcm-timed-cases")) pcm_timed_cases(file);
    else if (!strcmp(argv[1], "eos-state-cases")) eos_state_cases(file);
    else if (!strcmp(argv[1], "synthesis-ops")) synthesis_ops(file);
    else if (!strcmp(argv[1], "mdct-cases")) {
        uint64_t count = get64(file), i;
        if (count > (1u << 20)) fail("MDCT case count");
        for (i = 0; i < count; ++i) transform(file, 0);
        if (fgetc(file) != EOF) fail("trailing MDCT cases");
    }
    else if (!strcmp(argv[1], "mdct")) transform(file, 0);
    else if (!strcmp(argv[1], "mdctforward")) transform(file, 1);
    else if (!strcmp(argv[1], "oggpcm")) ogg_pcm(file, 0);
    else if (!strcmp(argv[1], "oggpcmraw")) ogg_pcm(file, 1);
    else if (!strcmp(argv[1], "oggstream")) ogg_stream(file);
    else if (!strcmp(argv[1], "oggseekstates")) ogg_seek_states(file, argv[3]);
    else if (!strcmp(argv[1], "oggseek")) ogg_seek(file, argv[3]);
    else fail("unknown mode");
    fclose(file); if (fflush(stdout)) fail("flush"); return 0;
}
