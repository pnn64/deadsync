#define WIN32_LEAN_AND_MEAN
#define _WIN32_WINNT 0x0600
#include <windows.h>
#include <errno.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <libavcodec/avcodec.h>
#include <libavutil/imgutils.h>
#ifdef BENCH_PACKET_INPUT
#include <libavutil/intreadwrite.h>
#endif
#include <libavutil/sha.h>

#ifndef BENCH_DECODER_NAME
#define BENCH_DECODER_NAME "h264"
#endif

typedef struct {
    uint64_t frames;
    uint64_t luma_pixels;
} Output;

typedef struct {
    uint64_t allocations;
    uint64_t reallocations;
    uint64_t requested_bytes;
} Counts;

typedef struct {
    unsigned mode;
    struct AVSHA *sha;
    Output *output;
} Consumer;

static volatile unsigned output_sample;

static int consume(Consumer *sink, const AVFrame *frame) {
    if (frame->format != AV_PIX_FMT_YUV420P || frame->width <= 0 || frame->height <= 0 ||
        (frame->width & 1) || (frame->height & 1))
        return AVERROR(EINVAL);
    sink->output->frames++;
    sink->output->luma_pixels += (uint64_t)frame->width * frame->height;
    if (sink->mode == 0) {
        output_sample = frame->data[0][0];
    } else if (sink->mode == 1) {
        int bytes = av_image_get_buffer_size(frame->format, frame->width, frame->height, 1);
        if (bytes < 0) return bytes;
        uint8_t *packed = av_malloc(bytes);
        if (!packed) return AVERROR(ENOMEM);
        int ret = av_image_copy_to_buffer(packed, bytes, (const uint8_t * const *)frame->data,
                                          frame->linesize, frame->format, frame->width, frame->height, 1);
        if (ret >= 0) output_sample = packed[0];
        av_free(packed);
        if (ret < 0) return ret;
    } else {
        for (int plane = 0; plane < 3; plane++) {
            int shift = plane != 0;
            for (int y = 0; y < (frame->height >> shift); y++)
                av_sha_update(sink->sha, frame->data[plane] + (ptrdiff_t)y * frame->linesize[plane],
                              frame->width >> shift);
        }
    }
    return 0;
}

static int send_and_drain(AVCodecContext *dec, AVPacket *packet, AVFrame *frame, Consumer *sink) {
    int ret = avcodec_send_packet(dec, packet);
    if (ret < 0) return ret;
    while ((ret = avcodec_receive_frame(dec, frame)) >= 0) {
        ret = consume(sink, frame);
        av_frame_unref(frame);
        if (ret < 0) return ret;
    }
    return ret == AVERROR(EAGAIN) || ret == AVERROR_EOF ? 0 : ret;
}

#ifdef BENCH_PACKET_INPUT
static void keep_input(void *opaque, uint8_t *data) {
    (void)opaque;
    (void)data;
}
#endif

int ff_decode(const uint8_t *data, int size, unsigned mode, struct AVSHA *sha, Output *output) {
    const AVCodec *codec = avcodec_find_decoder_by_name(BENCH_DECODER_NAME);
    AVCodecContext *dec = avcodec_alloc_context3(codec);
    AVFrame *frame = av_frame_alloc();
    AVPacket *packet = av_packet_alloc();
    Consumer sink = {mode, sha, output};
    int ret = AVERROR(ENOMEM);
#ifdef BENCH_PACKET_INPUT
    AVBufferRef *input = NULL;
    if (!codec || !dec || !frame || !packet) goto done;
    ret = AVERROR_INVALIDDATA;
    if (size < 8 || AV_RL32(data) > INT_MAX || AV_RL32(data + 4) > INT_MAX) goto done;
    dec->width = AV_RL32(data);
    dec->height = AV_RL32(data + 4);
    input = av_buffer_create((uint8_t *)data, size, keep_input, NULL, AV_BUFFER_FLAG_READONLY);
    ret = AVERROR(ENOMEM);
    if (!input) goto done;
    data += 8;
    size -= 8;
#else
    AVCodecParserContext *parser = codec ? av_parser_init(codec->id) : NULL;
    if (!codec || !dec || !parser || !frame || !packet) goto done;
#endif
    dec->thread_count = 1;
    dec->thread_type = 0;
#ifdef BENCH_IDCT
    dec->idct_algo = BENCH_IDCT;
#endif
    ret = avcodec_open2(dec, codec, NULL);
    if (ret < 0) goto done;
#ifdef BENCH_PACKET_INPUT
    while (size) {
        uint32_t length;
        ret = AVERROR_INVALIDDATA;
        if (size <= 4 + AV_INPUT_BUFFER_PADDING_SIZE) goto done;
        length = AV_RL32(data);
        data += 4;
        size -= 4;
        if (!length || length > (uint32_t)(size - AV_INPUT_BUFFER_PADDING_SIZE)) goto done;
        packet->buf = input;
        packet->data = (uint8_t *)data;
        packet->size = (int)length;
        ret = send_and_drain(dec, packet, frame, &sink);
        packet->buf = NULL;
        packet->data = NULL;
        packet->size = 0;
        if (ret < 0) goto done;
        data += length + AV_INPUT_BUFFER_PADDING_SIZE;
        size -= length + AV_INPUT_BUFFER_PADDING_SIZE;
    }
#else
    for (;;) {
        uint8_t *payload = NULL;
        int payload_size = 0;
        int consumed = av_parser_parse2(parser, dec, &payload, &payload_size, size ? data : NULL, size,
                                        AV_NOPTS_VALUE, AV_NOPTS_VALUE, -1);
        if (consumed < 0) { ret = consumed; goto done; }
        if (!consumed && !payload_size) {
            if (size) { ret = AVERROR_INVALIDDATA; goto done; }
            break;
        }
        data += consumed;
        size -= consumed;
        if (payload_size) {
            packet->data = payload;
            packet->size = payload_size;
            ret = send_and_drain(dec, packet, frame, &sink);
            packet->data = NULL;
            packet->size = 0;
            if (ret < 0) goto done;
        }
    }
#endif
    ret = send_and_drain(dec, NULL, frame, &sink);
done:
#ifdef BENCH_PACKET_INPUT
    av_buffer_unref(&input);
#else
    if (parser) av_parser_close(parser);
#endif
    av_packet_free(&packet);
    av_frame_free(&frame);
    avcodec_free_context(&dec);
    return ret;
}

static Counts counts;
static int tracking;
static void *(*crt_malloc)(size_t);
static void *(*crt_calloc)(size_t, size_t);
static void *(*crt_realloc)(void *, size_t);
static void *(*crt_aligned_malloc)(size_t, size_t);
static void *(*crt_aligned_realloc)(void *, size_t, size_t);

static void count(void *result, size_t size, int reallocating) {
    if (!tracking || !result || !size) return;
    if (reallocating) counts.reallocations++;
    else counts.allocations++;
    counts.requested_bytes += size;
}

static void *count_malloc(size_t size) {
    void *result = crt_malloc(size);
    count(result, size, 0);
    return result;
}

static void *count_calloc(size_t n, size_t size) {
    void *result = crt_calloc(n, size);
    count(result, n * size, 0);
    return result;
}

static void *count_realloc(void *old, size_t size) {
    int reallocating = old != NULL;
    void *result = crt_realloc(old, size);
    count(result, size, reallocating);
    return result;
}

static void *count_aligned_malloc(size_t size, size_t alignment) {
    void *result = crt_aligned_malloc(size, alignment);
    count(result, size, 0);
    return result;
}

static void *count_aligned_realloc(void *old, size_t size, size_t alignment) {
    int reallocating = old != NULL;
    void *result = crt_aligned_realloc(old, size, alignment);
    count(result, size, reallocating);
    return result;
}

static int hook_allocators(HMODULE dll) {
    if (!dll) return -1;
    uint8_t *base = (uint8_t *)dll;
    IMAGE_DOS_HEADER *dos = (IMAGE_DOS_HEADER *)base;
    IMAGE_NT_HEADERS64 *pe = (IMAGE_NT_HEADERS64 *)(base + dos->e_lfanew);
    IMAGE_IMPORT_DESCRIPTOR *desc = (IMAGE_IMPORT_DESCRIPTOR *)
        (base + pe->OptionalHeader.DataDirectory[IMAGE_DIRECTORY_ENTRY_IMPORT].VirtualAddress);
    int hooked = 0;
    for (; desc->Name; desc++) {
        if (_stricmp((char *)(base + desc->Name), "msvcrt.dll")) continue;
        IMAGE_THUNK_DATA64 *names = (IMAGE_THUNK_DATA64 *)(base + desc->OriginalFirstThunk);
        IMAGE_THUNK_DATA64 *slots = (IMAGE_THUNK_DATA64 *)(base + desc->FirstThunk);
        for (; names->u1.AddressOfData; names++, slots++) {
            if (IMAGE_SNAP_BY_ORDINAL64(names->u1.Ordinal)) continue;
            const char *name = ((IMAGE_IMPORT_BY_NAME *)(base + names->u1.AddressOfData))->Name;
            uintptr_t replacement = 0;
            if (!strcmp(name, "malloc")) replacement = (uintptr_t)count_malloc;
            if (!strcmp(name, "calloc")) replacement = (uintptr_t)count_calloc;
            if (!strcmp(name, "realloc")) replacement = (uintptr_t)count_realloc;
            if (!strcmp(name, "_aligned_malloc")) replacement = (uintptr_t)count_aligned_malloc;
            if (!strcmp(name, "_aligned_realloc")) replacement = (uintptr_t)count_aligned_realloc;
            if (!replacement) continue;
            DWORD old_protection, ignored;
            if (!VirtualProtect(&slots->u1.Function, sizeof(slots->u1.Function), PAGE_READWRITE, &old_protection))
                return -1;
            slots->u1.Function = replacement;
            if (!VirtualProtect(&slots->u1.Function, sizeof(slots->u1.Function), old_protection, &ignored))
                return -1;
            hooked++;
        }
    }
    return hooked;
}

void ff_alloc_start(void) {
    counts = (Counts){0};
    tracking = 1;
}

void ff_alloc_stop(Counts *out) {
    tracking = 0;
    *out = counts;
}

int ff_alloc_install(void) {
    HMODULE crt = GetModuleHandleA("msvcrt.dll");
    if (!crt) return 0;
    crt_malloc = (void *(*)(size_t))GetProcAddress(crt, "malloc");
    crt_calloc = (void *(*)(size_t, size_t))GetProcAddress(crt, "calloc");
    crt_realloc = (void *(*)(void *, size_t))GetProcAddress(crt, "realloc");
    crt_aligned_malloc = (void *(*)(size_t, size_t))GetProcAddress(crt, "_aligned_malloc");
    crt_aligned_realloc = (void *(*)(void *, size_t, size_t))GetProcAddress(crt, "_aligned_realloc");
    if (!crt_malloc || !crt_calloc || !crt_realloc || !crt_aligned_malloc || !crt_aligned_realloc) return 0;
    if (hook_allocators(GetModuleHandleA("avcodec-62.dll")) != 3 ||
        hook_allocators(GetModuleHandleA("avutil-60.dll")) != 5)
        return 0;

    ff_alloc_start();
    void *a = av_malloc(1234);
    void *b = av_realloc(a, 2345);
    Counts calibration;
    ff_alloc_stop(&calibration);
    av_free(b ? b : a);
    return a && b && calibration.allocations == 1 && calibration.reallocations == 1 &&
           calibration.requested_bytes == 3579;
}
