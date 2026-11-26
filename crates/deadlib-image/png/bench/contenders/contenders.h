#include <stddef.h>
#include <stdint.h>

#ifdef _WIN32
#define EXPORT __declspec(dllexport)
#else
#define EXPORT __attribute__((visibility("default")))
#endif

#ifdef __cplusplus
extern "C" {
#endif

typedef struct {
    uint8_t *data;
    size_t size;
    uint32_t width, height;
    const char *error;
    void *owner;
    void (*release)(void *);
} Buffer;

typedef struct {
    const uint8_t *pixels;
    size_t row_bytes;
    uint32_t width, height;
    uint8_t color, depth, interlaced;
    const uint8_t *palette;
    size_t palette_len;
} Raster;

EXPORT void codec_release(Buffer *buffer);
EXPORT void codec_decode(int codec, const uint8_t *data, size_t size, int depth, Buffer *out);
EXPORT void codec_encode(int codec, const Raster *input, int level, Buffer *out);
void fast_decode(const uint8_t *data, size_t size, Buffer *out);
void fast_encode(int codec, const Raster *input, int level, Buffer *out);

#ifdef __cplusplus
}
#endif
