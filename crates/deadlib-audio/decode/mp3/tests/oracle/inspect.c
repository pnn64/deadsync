/* Validation-only libmad public API. No codec implementation is included. */
#include <mad.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
int main(int argc,char**argv) {
    if(argc!=2)return 2;
    FILE* input=fopen(argv[1],"rb");if(!input)return 2;
    fseek(input,0,SEEK_END);long size=ftell(input);rewind(input);
    if(size<0)return 2;
    unsigned char* data=calloc((size_t)size+MAD_BUFFER_GUARD,1);if(!data)return 2;
    if(fread(data,1,(size_t)size,input)!=(size_t)size)return 2;fclose(input);
    size_t begin=0,end=(size_t)size;
    while(begin+10<=end&&!memcmp(data+begin,"ID3",3)) {
        size_t n=0;for(int i=6;i<10;i++)n=n*128+(data[begin+i]&127);
        n+=10;if(data[begin+3]==4&&(data[begin+5]&16))n+=10;
        if(n>end-begin)return 2;begin+=n;
    }
    if(end>=128&&!memcmp(data+end-128,"TAG",3))end-=128;
    memset(data+end,0,MAD_BUFFER_GUARD);
    struct mad_stream stream;struct mad_frame frame;
    mad_stream_init(&stream);mad_frame_init(&frame);
    mad_stream_buffer(&stream,data+begin,end-begin+MAD_BUFFER_GUARD);
    while(stream.next_frame<data+end) {
        if(mad_frame_decode(&frame,&stream)) {
            fprintf(stderr,"libmad at %zu: %s\n",(size_t)(stream.this_frame-data),mad_stream_errorstr(&stream));
            return 1;
        }
        printf("frame\t%zu\t%lu\t%u\t%d\t%d\t%zu\n",(size_t)(stream.this_frame-data),frame.header.bitrate,frame.header.samplerate,MAD_NCHANNELS(&frame.header),frame.header.flags&MAD_FLAG_LSF_EXT?576:1152,(size_t)(stream.next_frame-stream.this_frame));
    }
    mad_frame_finish(&frame);mad_stream_finish(&stream);free(data);return 0;
}
