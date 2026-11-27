#if defined(BENCH_HEVC)
#define BENCH_DECODER_NAME "hevc"
#define BENCH_PACKET_INPUT
#define BENCH_PACKET_CONFIG
#elif defined(BENCH_VP9)
#define BENCH_DECODER_NAME "vp9"
#define BENCH_PACKET_INPUT
#elif defined(BENCH_MP42)
#define BENCH_DECODER_NAME "msmpeg4v2"
#define BENCH_PACKET_INPUT
#elif defined(BENCH_MPEG4)
#define BENCH_DECODER_NAME "mpeg4"
#elif defined(BENCH_MPEG1)
#define BENCH_DECODER_NAME "mpeg1video"
#else
#define BENCH_DECODER_NAME "mpeg2video"
#endif
#if !defined(BENCH_VP9) && !defined(BENCH_HEVC)
#define BENCH_IDCT FF_IDCT_SIMPLE
#endif
#include "../../deadlib-video-h264/bench/ffmpeg.c"
