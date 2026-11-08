/* Validation-only public libmad and mpg123 raw PCM. No production codec use. */
#include <mad.h>
#include <mpg123.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
static void u32(FILE* out,uint32_t n){for(int i=0;i<4;i++)fputc((int)((n>>(8*i))&255),out);}
static void f32(FILE* out,float n){uint32_t v;memcpy(&v,&n,4);u32(out,v);}
static int mad_decode(const char* path,FILE* out){
    FILE* input=fopen(path,"rb");if(!input)return 2;fseek(input,0,SEEK_END);long size=ftell(input);rewind(input);if(size<0)return 2;
    unsigned char* data=calloc((size_t)size+MAD_BUFFER_GUARD,1);if(!data)return 2;if(fread(data,1,(size_t)size,input)!=(size_t)size)return 2;fclose(input);
    size_t begin=0,end=(size_t)size;
    while(begin+10<=end&&!memcmp(data+begin,"ID3",3)){size_t n=0;for(int i=6;i<10;i++)n=n*128+(data[begin+i]&127);n+=10;if(data[begin+3]==4&&(data[begin+5]&16))n+=10;if(n>end-begin)return 2;begin+=n;}
    if(end>=128&&!memcmp(data+end-128,"TAG",3))end-=128;memset(data+end,0,MAD_BUFFER_GUARD);
    struct mad_stream stream;struct mad_frame frame;struct mad_synth synth;
    mad_stream_init(&stream);mad_frame_init(&frame);mad_synth_init(&synth);mad_stream_buffer(&stream,data+begin,end-begin+MAD_BUFFER_GUARD);
    while(stream.next_frame<data+end){
        if(mad_frame_decode(&frame,&stream)){fprintf(stderr,"libmad at %zu: %s\n",(size_t)(stream.this_frame-data),mad_stream_errorstr(&stream));return 1;}
        mad_synth_frame(&synth,&frame);u32(out,synth.pcm.samplerate);u32(out,synth.pcm.channels);u32(out,synth.pcm.length);
        for(unsigned n=0;n<synth.pcm.length;n++)for(unsigned ch=0;ch<synth.pcm.channels;ch++)f32(out,(float)((double)synth.pcm.samples[ch][n]/(double)(1UL<<MAD_F_FRACBITS)));
    }
    mad_synth_finish(&synth);mad_frame_finish(&frame);mad_stream_finish(&stream);free(data);return 0;
}
static int mpg_decode(const char* path,FILE* out){
    int error=0;mpg123_init();mpg123_handle* decoder=mpg123_new(NULL,&error);if(!decoder)return 2;
    if(mpg123_param(decoder,MPG123_ADD_FLAGS,MPG123_IGNORE_INFOFRAME,0)!=MPG123_OK||mpg123_param(decoder,MPG123_REMOVE_FLAGS,MPG123_GAPLESS,0)!=MPG123_OK)return 2;
    if(mpg123_format_none(decoder)!=MPG123_OK)return 2;const long* rates;size_t count;mpg123_rates(&rates,&count);
    for(size_t i=0;i<count;i++)if(mpg123_format(decoder,rates[i],MPG123_MONO|MPG123_STEREO,MPG123_ENC_FLOAT_32)!=MPG123_OK)return 2;
    if(mpg123_open(decoder,path)!=MPG123_OK){fprintf(stderr,"mpg123: %s\n",mpg123_strerror(decoder));return 1;}
    for(;;){off_t number;unsigned char* pcm;size_t bytes;
        int result=mpg123_decode_frame(decoder,&number,&pcm,&bytes);
        if(result==MPG123_NEW_FORMAT)continue;if(result==MPG123_DONE)break;
        if(result!=MPG123_OK){fprintf(stderr,"mpg123: %s\n",mpg123_strerror(decoder));return 1;}
        long rate;int channels,encoding;if(mpg123_getformat(decoder,&rate,&channels,&encoding)!=MPG123_OK)return 2;if(encoding!=MPG123_ENC_FLOAT_32||channels<1||channels>2||bytes%(channels*4))return 2;
        if(!bytes)continue;u32(out,(uint32_t)rate);u32(out,(uint32_t)channels);u32(out,(uint32_t)(bytes/(channels*4)));
        for(size_t i=0;i<bytes/4;i++){float sample;memcpy(&sample,pcm+4*i,4);f32(out,sample);}
    }
    mpg123_close(decoder);mpg123_delete(decoder);mpg123_exit();return 0;
}
int main(int argc,char**argv){if(argc!=4)return 2;FILE* out=fopen(argv[3],"wb");if(!out)return 2;fwrite("DLMP3PC1",1,8,out);int status;
    if(!strcmp(argv[1],"mad"))status=mad_decode(argv[2],out);else if(!strcmp(argv[1],"mpg123"))status=mpg_decode(argv[2],out);else return 2;fclose(out);return status;}
