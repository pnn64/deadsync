/* Test-only process boundary to unmodified libogg; never linked to Rust. */
#include <ogg/ogg.h>
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
static void put64(uint64_t value) { int i; for (i = 0; i < 8; ++i) put8((unsigned char)(value >> (i * 8))); }
static void put32(uint32_t value) { int i; for (i = 0; i < 4; ++i) put8((unsigned char)(value >> (i * 8))); }
static uint64_t get64(FILE *file) {
    uint64_t out = 0; int i;
    for (i = 0; i < 8; ++i) { int c = fgetc(file); if (c == EOF) fail("truncated protocol"); out |= (uint64_t)c << (i * 8); }
    return out;
}
static uint32_t get32(FILE *file) {
    uint32_t out = 0; int i;
    for (i = 0; i < 4; ++i) { int c = fgetc(file); if (c == EOF) fail("truncated protocol"); out |= (uint32_t)c << (i * 8); }
    return out;
}
static void putbytes(const void *bytes, size_t len) { if (fwrite(bytes, 1, len, stdout) != len) fail("write"); }

typedef struct { uint32_t serial; ogg_stream_state state; } logical;
static void decode(FILE *file) {
    ogg_sync_state sync;
    logical *streams = (logical *)calloc(4096, sizeof(logical));
    size_t count = 0, i;
    if (!streams || ogg_sync_init(&sync)) fail("init");
    for (;;) {
        ogg_page page; int result;
        char *buffer = ogg_sync_buffer(&sync, 4096);
        size_t n = fread(buffer, 1, 4096, file);
        if (ferror(file)) fail("read");
        if (ogg_sync_wrote(&sync, (long)n)) fail("sync write");
        while ((result = ogg_sync_pageout(&sync, &page)) != 0) {
            uint32_t serial; ogg_packet packet; int packet_result;
            if (result < 0) fail("page sync/CRC error");
            serial = (uint32_t)ogg_page_serialno(&page);
            put8('G'); put32(serial); put32((uint32_t)ogg_page_pageno(&page));
            put64((uint64_t)ogg_page_granulepos(&page)); put8(page.header[5]);
            put32((uint32_t)(page.header_len + page.body_len));
            for (i = 0; i < count && streams[i].serial != serial; ++i) {}
            if (i == count) {
                if (count == 4096) fail("too many streams");
                streams[i].serial = serial;
                if (ogg_stream_init(&streams[i].state, (int)serial)) fail("stream init");
                ++count;
            }
            if (ogg_stream_pagein(&streams[i].state, &page)) fail("pagein");
            while ((packet_result = ogg_stream_packetout(&streams[i].state, &packet)) != 0) {
                if (packet_result < 0) fail("packet hole");
                put8('P'); put32(serial); put64((uint64_t)packet.granulepos);
                put8(packet.b_o_s != 0); put8(packet.e_o_s != 0);
                put64((uint64_t)packet.bytes); putbytes(packet.packet, (size_t)packet.bytes);
            }
        }
        if (n == 0) break;
    }
    for (i = 0; i < count; ++i) ogg_stream_clear(&streams[i].state);
    free(streams); ogg_sync_clear(&sync);
}
static void output_page(ogg_page *page) {
    putbytes(page->header, (size_t)page->header_len);
    putbytes(page->body, (size_t)page->body_len);
}
static void encode(FILE *file) {
    ogg_stream_state stream; ogg_int64_t number = 0; ogg_page page;
    uint32_t serial = get32(file);
    if (ogg_stream_init(&stream, (int)serial)) fail("stream init");
    while (fgetc(file) != EOF) {
        uint64_t len = get64(file), granule = get64(file);
        int eos = fgetc(file); ogg_packet packet;
        unsigned char *data;
        if (eos == EOF || len > (64u << 20)) fail("input packet");
        data = (unsigned char *)malloc((size_t)len + 1);
        if (!data || fread(data, 1, (size_t)len, file) != len) fail("packet read");
        packet.packet = data; packet.bytes = (long)len; packet.b_o_s = number == 0;
        packet.e_o_s = eos; packet.granulepos = (ogg_int64_t)granule; packet.packetno = number;
        if (ogg_stream_packetin(&stream, &packet)) fail("packetin");
        free(data);
        if (number == 0) { while (ogg_stream_flush(&stream, &page)) output_page(&page); }
        else { while (ogg_stream_pageout(&stream, &page)) output_page(&page); }
        ++number;
    }
    while (ogg_stream_flush(&stream, &page)) output_page(&page);
    ogg_stream_clear(&stream);
}
int main(int argc, char **argv) {
    FILE *file;
    if (argc != 3) fail("usage: oracle decode|encode input");
    file = fopen(argv[2], "rb"); if (!file) fail("open");
#ifdef _WIN32
    _setmode(_fileno(stdout), _O_BINARY);
#endif
    if (!strcmp(argv[1], "decode")) decode(file);
    else if (!strcmp(argv[1], "encode")) encode(file);
    else fail("unknown mode");
    fclose(file); if (fflush(stdout)) fail("flush"); return 0;
}
