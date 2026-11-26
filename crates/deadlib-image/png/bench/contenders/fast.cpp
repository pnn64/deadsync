#include "contenders.h"

#include <cstdlib>
#include <vector>
#include "fpng.h"
#include "fpnge.h"

using Bytes = std::vector<uint8_t>;

static void take(Bytes *bytes, Buffer *out) {
    out->data = bytes->data();
    out->size = bytes->size();
    out->owner = bytes;
    out->release = [](void *owner) { delete static_cast<Bytes *>(owner); };
}

void fast_decode(const uint8_t *data, size_t size, Buffer *out) {
    auto *bytes = new Bytes;
    uint32_t channels;
    if (fpng::fpng_decode_memory(data, (uint32_t)size, *bytes, out->width, out->height, channels, 4)) {
        delete bytes;
        out->error = "fpng could not decode this stream";
        return;
    }
    take(bytes, out);
}

void fast_encode(int codec, const Raster *input, int level, Buffer *out) {
    uint32_t channels = input->color == 0 ? 1 : input->color == 4 ? 2 : input->color == 2 ? 3 : 4;
    if (codec == 4) {
        fpng::fpng_init();
        auto *bytes = new Bytes;
        if (!fpng::fpng_encode_image_to_memory(input->pixels, input->width, input->height,
                                                channels, *bytes, level)) {
            delete bytes;
            out->error = "fpng could not encode this raster";
            return;
        }
        take(bytes, out);
    } else {
        FPNGEOptions options;
        FPNGEFillOptions(&options, level, FPNGE_CICP_NONE);
        size_t depth = input->depth / 8;
        size_t capacity = FPNGEOutputAllocSize(depth, channels, input->width, input->height);
        out->data = (uint8_t *)malloc(capacity);
        if (!out->data) {
            out->error = "FPNGE allocation failed";
            return;
        }
        out->size = FPNGEEncode(depth, channels, input->pixels, input->width, input->row_bytes,
                                input->height, out->data, &options);
        if (!out->size || out->size > capacity) out->error = "FPNGE could not encode this raster";
    }
}
