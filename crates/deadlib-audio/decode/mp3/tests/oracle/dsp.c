/* Unmodified libmad public API, normalized hybrid and PCM validation traces. */
#include <mad.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
static void u32(FILE* out,uint32_t n){for(int i=0;i<4;i++)fputc((int)((n>>(8*i))&255),out);}
static void f64(FILE* out,double n){uint64_t v;memcpy(&v,&n,8);for(int i=0;i<8;i++)fputc((int)((v>>(8*i))&255),out);}
int main(int argc,char**argv){
    if(argc!=3)return 2;
    FILE* input=fopen(argv[1],"rb");if(!input)return 2;
    fseek(input,0,SEEK_END);long bytes=ftell(input);rewind(input);if(bytes<0||bytes%1044)return 2;
    unsigned char* data=calloc((size_t)bytes+MAD_BUFFER_GUARD,1);if(!data)return 2;
    if(fread(data,1,(size_t)bytes,input)!=(size_t)bytes)return 2;fclose(input);
    FILE* out=fopen(argv[2],"wb");if(!out)return 2;fwrite("DLMDOUT1",1,8,out);u32(out,(uint32_t)(bytes/1044*2));
    struct mad_stream stream;struct mad_frame frame;struct mad_synth synth;
    mad_stream_init(&stream);mad_frame_init(&frame);mad_synth_init(&synth);
    mad_stream_buffer(&stream,data,(size_t)bytes+MAD_BUFFER_GUARD);
    size_t index=0;
    while(stream.next_frame<data+bytes){
        if(mad_frame_decode(&frame,&stream)){fprintf(stderr,"frame %zu: %s\n",index,mad_stream_errorstr(&stream));return 1;}
        if(MAD_NCHANNELS(&frame.header)!=1||frame.header.flags&MAD_FLAG_LSF_EXT)return 2;
        mad_synth_frame(&synth,&frame);
        for(int g=0;g<2;g++){
            for(int t=0;t<18;t++)for(int sb=0;sb<32;sb++)f64(out,(double)frame.sbsample[0][18*g+t][sb]/(double)(1UL<<MAD_F_FRACBITS));
            for(int n=0;n<576;n++)f64(out,(double)synth.pcm.samples[0][576*g+n]/(double)(1UL<<MAD_F_FRACBITS));
        }
        index++;
    }
    fclose(out);mad_synth_finish(&synth);mad_frame_finish(&frame);mad_stream_finish(&stream);free(data);return 0;
}
