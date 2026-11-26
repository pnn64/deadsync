#include "bridge.h"

#include <climits>
#include <cstdio>
#include <cstdlib>

extern "C" {
#include <libavcodec/avcodec.h>
#include <libavutil/log.h>
#include <libswscale/swscale.h>
}

namespace {
void report(Output *out, int error) {
    av_strerror(error, out->error, sizeof(out->error));
}

void no_release(void *, uint8_t *) {}

void release_packet(void *owner) {
    auto packet = static_cast<AVPacket *>(owner);
    av_packet_free(&packet);
}

struct Work {
    AVCodecContext *context;
    AVFrame *frame = av_frame_alloc();
    AVPacket *packet = av_packet_alloc();
    SwsContext *scale = nullptr;

    explicit Work(const AVCodec *codec) : context(avcodec_alloc_context3(codec)) {
        if (context) context->thread_count = 1;
    }

    ~Work() {
        sws_freeContext(scale);
        av_packet_free(&packet);
        av_frame_free(&frame);
        avcodec_free_context(&context);
    }

    bool ready() const { return context && frame && packet; }
};
}

API int init_bmp() {
    av_log_set_level(AV_LOG_QUIET);
    return 1;
}

API void decode_bmp(unsigned, const uint8_t *data, size_t len, Output *out) {
    static_assert(AV_INPUT_BUFFER_PADDING_SIZE <= 64);
    const auto codec = avcodec_find_decoder(AV_CODEC_ID_BMP);
    Work work(codec);
    if (!work.ready() || len > INT_MAX) return report(out, AVERROR(ENOMEM));
    work.packet->buf = av_buffer_create(const_cast<uint8_t *>(data),
                                        len + AV_INPUT_BUFFER_PADDING_SIZE, no_release,
                                        nullptr, AV_BUFFER_FLAG_READONLY);
    if (!work.packet->buf) return report(out, AVERROR(ENOMEM));
    work.packet->data = work.packet->buf->data;
    work.packet->size = static_cast<int>(len);

    int status = avcodec_open2(work.context, codec, nullptr);
    if (status >= 0) status = avcodec_send_packet(work.context, work.packet);
    if (status >= 0) status = avcodec_receive_frame(work.context, work.frame);
    if (status < 0) return report(out, status);

    const int w = work.frame->width, h = work.frame->height;
    if (w <= 0 || h <= 0 || w > INT_MAX / 4) return report(out, AVERROR(EINVAL));
    const int stride = w * 4;
    const size_t size = static_cast<size_t>(stride) * h;
    auto pixels = static_cast<uint8_t *>(malloc(size + AV_INPUT_BUFFER_PADDING_SIZE));
    if (!pixels) return report(out, AVERROR(ENOMEM));
    const auto input_format = static_cast<AVPixelFormat>(work.frame->format);
    work.scale = sws_getContext(w, h, input_format, w, h, AV_PIX_FMT_RGBA, SWS_POINT,
                                nullptr, nullptr, nullptr);
    uint8_t *planes[4] = {pixels};
    const int strides[4] = {stride};
    if (!work.scale || sws_scale(work.scale, work.frame->data, work.frame->linesize,
                                 0, h, planes, strides) != h) {
        free(pixels);
        return report(out, AVERROR(EINVAL));
    }
    out->data = pixels;
    out->size = size;
    out->width = w;
    out->height = h;
    out->owner = pixels;
    out->release = free;
}

API void encode_bmp(unsigned, const uint8_t *data, unsigned w, unsigned h, unsigned channels,
                    Output *out) {
    const auto codec = avcodec_find_encoder(AV_CODEC_ID_BMP);
    Work work(codec);
    if (!work.ready()) return report(out, AVERROR(ENOMEM));
    work.context->width = work.frame->width = static_cast<int>(w);
    work.context->height = work.frame->height = static_cast<int>(h);
    work.context->pix_fmt = channels == 3 ? AV_PIX_FMT_BGR24 : AV_PIX_FMT_BGRA;
    work.context->time_base = {1, 1};
    work.frame->format = work.context->pix_fmt;
    int status = avcodec_open2(work.context, codec, nullptr);
    if (status >= 0) status = av_frame_get_buffer(work.frame, 32);
    if (status < 0) return report(out, status);

    const size_t source_stride = static_cast<size_t>(w) * channels;
    for (unsigned y = 0; y < h; ++y) {
        const uint8_t *src = data + y * source_stride;
        uint8_t *dst = work.frame->data[0] + static_cast<size_t>(y) * work.frame->linesize[0];
        for (unsigned x = 0; x < w; ++x, src += channels, dst += channels) {
            dst[0] = src[2];
            dst[1] = src[1];
            dst[2] = src[0];
            if (channels == 4) dst[3] = src[3];
        }
    }
    status = avcodec_send_frame(work.context, work.frame);
    if (status >= 0) status = avcodec_receive_packet(work.context, work.packet);
    if (status < 0) return report(out, status);
    out->data = work.packet->data;
    out->size = work.packet->size;
    out->owner = work.packet;
    out->release = release_packet;
    work.packet = nullptr;
}
