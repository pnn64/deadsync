/* External test oracle. Private codebook APIs are deliberately confined here. */
#include <vorbis/codec.h>
#include "codebook.h"
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
int main(int argc, char **argv) {
    FILE *file; if (argc != 3) fail("usage: oracle headers|books input");
    file = fopen(argv[2], "rb"); if (!file) fail("open");
#ifdef _WIN32
    _setmode(_fileno(stdout), _O_BINARY);
#endif
    if (!strcmp(argv[1], "headers")) headers(file);
    else if (!strcmp(argv[1], "books")) books(file);
    else fail("unknown mode");
    fclose(file); if (fflush(stdout)) fail("flush"); return 0;
}
