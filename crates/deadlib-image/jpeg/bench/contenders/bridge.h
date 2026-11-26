#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define EXPORT __declspec(dllexport)

typedef struct {
    uint8_t *data;
    size_t size;
    uint32_t width, height, channels, precision;
    char error[256];
} Output;

enum {
    PROGRESSIVE = 1, OPTIMIZE = 2, ARITHMETIC = 4, LOSSLESS = 8, NATIVE_PROFILE = 16
};

typedef struct {
    const void *data;
    uint32_t width, height, channels, precision, quality, horizontal, vertical, flags;
} Raster;

EXPORT void release_output(Output *output);
EXPORT void decode_jpeg(const uint8_t *bytes, size_t length, int fast, Output *output);
EXPORT void encode_jpeg(const Raster *input, Output *output);

#ifdef __cplusplus
}
#endif
