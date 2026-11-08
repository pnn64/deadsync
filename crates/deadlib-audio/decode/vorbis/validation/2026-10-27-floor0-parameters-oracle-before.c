/* External test oracle. Private codebook APIs are deliberately confined here. */
#include <vorbis/codec.h>
#include <vorbis/vorbisfile.h>
#include "codebook.h"
#include "mdct.h"
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
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
static void bookheaders(FILE *file) {
    uint64_t count = get64(file), i;
    if (count > (1u << 20)) fail("codebook header count");
    put64(count);
    for (i = 0; i < count; ++i) {
        long size; unsigned char *data = packet(file, &size);
        oggpack_buffer bits; static_codebook *book;
        oggpack_readinit(&bits, data, size);
        book = vorbis_staticbook_unpack(&bits);
        put8(book != NULL); put64((uint64_t)oggpack_bits(&bits));
        if (book) vorbis_staticbook_destroy(book);
        free(data);
    }
    if (fgetc(file) != EOF) fail("trailing codebook headers");
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
    FILE *file; if (argc < 3 || (!strcmp(argv[1], "oggseek") ? argc != 4 : argc != 3)) fail("usage: oracle headers|bookheaders|books|pcm|pcm-cases|mdct|mdctforward|oggpcm|oggpcmraw input; oggseek input requests");
    file = fopen(argv[2], "rb"); if (!file) fail("open");
#ifdef _WIN32
    _setmode(_fileno(stdout), _O_BINARY);
#endif
    if (!strcmp(argv[1], "headers")) headers(file);
    else if (!strcmp(argv[1], "books")) books(file);
    else if (!strcmp(argv[1], "bookheaders")) bookheaders(file);
    else if (!strcmp(argv[1], "pcm")) pcm(file, 0);
    else if (!strcmp(argv[1], "pcm-cases")) pcm(file, 1);
    else if (!strcmp(argv[1], "mdct")) transform(file, 0);
    else if (!strcmp(argv[1], "mdctforward")) transform(file, 1);
    else if (!strcmp(argv[1], "oggpcm")) ogg_pcm(file, 0);
    else if (!strcmp(argv[1], "oggpcmraw")) ogg_pcm(file, 1);
    else if (!strcmp(argv[1], "oggseek")) ogg_seek(file, argv[3]);
    else fail("unknown mode");
    fclose(file); if (fflush(stdout)) fail("flush"); return 0;
}
