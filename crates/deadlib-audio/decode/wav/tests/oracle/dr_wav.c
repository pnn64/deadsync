#define DR_WAV_IMPLEMENTATION
#include "dr_wav.h"
#include <stdio.h>
#ifdef _WIN32
#include <io.h>
#include <fcntl.h>
#endif
int main(int argc, char **argv) {
    drwav wav;
    drwav_int16 samples[16384];
    unsigned char bytes[32768];
    if (argc != 2 || !drwav_init_file(&wav, argv[1], NULL)) return 2;
    if (wav.channels > 16384) return 3;
#ifdef _WIN32
    _setmode(_fileno(stdout), _O_BINARY);
#endif
    for (;;) {
        drwav_uint64 n = drwav_read_pcm_frames_s16(&wav, 16384 / wav.channels, samples);
        drwav_uint64 i;
        if (!n) break;
        for (i = 0; i < n * wav.channels; ++i) {
            bytes[2*i] = (unsigned char)samples[i];
            bytes[2*i+1] = (unsigned char)((unsigned short)samples[i] >> 8);
        }
        if (fwrite(bytes, 2 * wav.channels, (size_t)n, stdout) != n) return 4;
    }
    drwav_uninit(&wav);
    return fflush(stdout) ? 5 : 0;
}
