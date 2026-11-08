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

typedef struct { uint32_t serial; ogg_stream_state state; int emitted_packet; } logical;
static void decode(FILE *file, int capture) {
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
                if (packet_result < 0) {
                    /* A seek into the middle starts with an intentional hole.
                       Only capture mode discards it before the first output. */
                    if (capture && !streams[i].emitted_packet) continue;
                    fail("packet hole");
                }
                streams[i].emitted_packet = 1;
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
/* Direct, stateful synchronization protocols; all parsing is unmodified libogg. */
static void sync_ops(FILE *file) {
    ogg_sync_state sync; uint64_t count=get64(file),i;
    if(count>1000000||ogg_sync_init(&sync))fail("sync ops init");put64(count);
    for(i=0;i<count;++i){
        uint64_t kind=get64(file);long result=0;ogg_page page;int have=0;
        if(kind==0)result=ogg_sync_reset(&sync);
        else if(kind==1){
            uint64_t n=get64(file);char *b;if(n>(32u<<20))fail("sync ops bytes");
            b=ogg_sync_buffer(&sync,(long)n);if(n&&(!b||fread(b,1,(size_t)n,file)!=(size_t)n))fail("sync ops append");
            result=ogg_sync_wrote(&sync,(long)n);
        }else if(kind==2){result=ogg_sync_pageseek(&sync,&page);have=result>0;}
        else if(kind==3){result=ogg_sync_pageout(&sync,&page);have=result>0;}
        else fail("sync ops kind");
        put64((uint64_t)(int64_t)result);put64((uint64_t)(sync.fill-sync.returned));put8(sync.unsynced!=0);
        put64(have?(uint64_t)(page.header_len+page.body_len):0);
        if(have)output_page(&page);
    }
    if(fgetc(file)!=EOF)fail("sync ops trailing bytes");ogg_sync_clear(&sync);
}
typedef struct {
    unsigned char *bytes;uint64_t length,at,read_size,reads,seeks;
    int fail_read,fail_seek;
} sync_source;
/* Boundary/seek driver adapted from pinned vorbisfile's transport helpers.
   Xiph.Org Foundation BSD license: see VORBIS-COPYING. No codec is linked. */
static ogg_int64_t sync_next(ogg_sync_state *sync,sync_source *source,
                              ogg_int64_t *offset,ogg_int64_t boundary,ogg_page *page){
    if(boundary>0)boundary+=*offset;
    for(;;){
        long more;if(boundary>0&&*offset>=boundary)return -1;
        more=ogg_sync_pageseek(sync,page);
        if(more<0)*offset-=more;
        else if(!more){
            uint64_t n;char *b;
            if(!boundary)return -1;
            ++source->reads;
            if(source->fail_read){source->fail_read=0;return -128;}
            n=source->at<source->length?source->length-source->at:0;
            if(n>source->read_size)n=source->read_size;
            b=ogg_sync_buffer(sync,(long)source->read_size);if(!b)fail("sync read buffer");
            if(n)memcpy(b,source->bytes+(size_t)source->at,(size_t)n);
            source->at+=n;if(ogg_sync_wrote(sync,(long)n))fail("sync read append");
            if(!n)return -2;
        }else{ogg_int64_t result=*offset;*offset+=more;return result;}
    }
}
static void sync_read_ops(FILE *file){
    ogg_sync_state sync;sync_source source;uint64_t count,i;ogg_int64_t offset=0;
    memset(&source,0,sizeof(source));source.length=get64(file);if(source.length>(32u<<20))fail("sync read source size");
    source.bytes=(unsigned char*)malloc((size_t)source.length+1);
    if(!source.bytes||fread(source.bytes,1,(size_t)source.length,file)!=(size_t)source.length)fail("sync read source");
    source.read_size=get64(file);count=get64(file);
    if(!source.read_size||source.read_size>(1u<<20)||count>1000000||ogg_sync_init(&sync))fail("sync read init");put64(count);
    for(i=0;i<count;++i){
        uint64_t kind=get64(file);ogg_int64_t result=0;ogg_page page;int have=0;
        if(kind==0){result=sync_next(&sync,&source,&offset,(ogg_int64_t)get64(file),&page);have=result>=0;}
        else if(kind==1){
            uint64_t at=get64(file);
            if(at!=(uint64_t)offset){
                ++source.seeks;
                if(source.fail_seek){source.fail_seek=0;result=-128;}
                else{source.at=at;offset=(ogg_int64_t)at;ogg_sync_reset(&sync);}
            }
        }else if(kind==2)source.fail_read=1;
        else if(kind==3)source.fail_seek=1;
        else fail("sync read kind");
        put64((uint64_t)result);put64((uint64_t)offset);put64(source.at);
        put64(source.reads);put64(source.seeks);put64((uint64_t)(sync.fill-sync.returned));put8(sync.unsynced!=0);
        put64(have?(uint64_t)(page.header_len+page.body_len):0);if(have)output_page(&page);
    }
    if(fgetc(file)!=EOF)fail("sync read trailing bytes");ogg_sync_clear(&sync);free(source.bytes);
}
int main(int argc, char **argv) {
    FILE *file;
    if (argc != 3) fail("usage: oracle decode|capture|encode|sync-ops|sync-read-ops input");
    file = fopen(argv[2], "rb"); if (!file) fail("open");
#ifdef _WIN32
    _setmode(_fileno(stdout), _O_BINARY);
#endif
    if (!strcmp(argv[1], "decode")) decode(file, 0);
    else if (!strcmp(argv[1], "capture")) decode(file, 1);
    else if (!strcmp(argv[1], "encode")) encode(file);
    else if (!strcmp(argv[1], "sync-ops")) sync_ops(file);
    else if (!strcmp(argv[1], "sync-read-ops")) sync_read_ops(file);
    else fail("unknown mode");
    fclose(file); if (fflush(stdout)) fail("flush"); return 0;
}
