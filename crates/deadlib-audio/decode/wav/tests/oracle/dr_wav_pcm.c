#define DR_WAV_IMPLEMENTATION
#include "dr_wav.h"
#include <stdio.h>
#ifdef _WIN32
#include <io.h>
#include <fcntl.h>
#endif
/* Integer protocol: signed i32 LE, container values left-aligned to bit 31.
 * Floating protocol: exact native-width IEEE bits LE, without conversion. */
int main(int argc,char **argv) {
    drwav wav;drwav_int32 samples[16384];unsigned char bytes[65536];drwav_uint64 frames,i;size_t width;
    if(argc!=2||!drwav_init_file(&wav,argv[1],NULL))return 2;
    if(!wav.channels||wav.channels>16384)return 3;
    fprintf(stderr,"{\"channels\":%u,\"sample_rate\":%u,\"bits\":%u,\"valid_bits\":%u,\"mask\":%u,\"tag\":%u,\"frames\":%llu}\n",wav.channels,wav.sampleRate,wav.bitsPerSample,wav.fmt.validBitsPerSample,wav.fmt.channelMask,wav.translatedFormatTag,(unsigned long long)wav.totalPCMFrameCount);
#ifdef _WIN32
    _setmode(_fileno(stdout),_O_BINARY);
#endif
    if(wav.translatedFormatTag==DR_WAVE_FORMAT_PCM) {
        for(;;) {
            frames=drwav_read_pcm_frames_s32(&wav,16384/wav.channels,samples);if(!frames)break;
            for(i=0;i<frames*wav.channels;++i){drwav_uint32 s=(drwav_uint32)samples[i];bytes[4*i]=(unsigned char)s;bytes[4*i+1]=(unsigned char)(s>>8);bytes[4*i+2]=(unsigned char)(s>>16);bytes[4*i+3]=(unsigned char)(s>>24);}
            if(fwrite(bytes,4*wav.channels,(size_t)frames,stdout)!=(size_t)frames)return 4;
        }
    } else if(wav.translatedFormatTag==DR_WAVE_FORMAT_IEEE_FLOAT&&(wav.bitsPerSample==32||wav.bitsPerSample==64)) {
        width=wav.bitsPerSample/8;
        for(;;) {
            frames=drwav_read_pcm_frames(&wav,65536/(width*wav.channels),bytes);if(!frames)break;
            if(!drwav__is_little_endian())for(i=0;i<frames*wav.channels;++i){size_t j;for(j=0;j<width/2;++j){unsigned char b=bytes[i*width+j];bytes[i*width+j]=bytes[i*width+width-1-j];bytes[i*width+width-1-j]=b;}}
            if(fwrite(bytes,width*wav.channels,(size_t)frames,stdout)!=(size_t)frames)return 4;
        }
    } else return 6;
    drwav_uninit(&wav);return fflush(stdout)?5:0;
}
