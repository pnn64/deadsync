/* Validation-only libmad public API: hybrid samples, before polyphase synthesis.
   The generator places isolated coefficients in subband zero, then clears the
   overlap with a zero second granule. No decoder internals are patched. */
#include <mad.h>
#include <stdio.h>
#include <stdlib.h>
int main(int argc,char**argv) {
    if(argc!=2)return 2;
    FILE* file=fopen(argv[1],"rb");if(!file)return 2;
    fseek(file,0,SEEK_END);long size=ftell(file);rewind(file);if(size<0)return 2;
    unsigned char* data=calloc((size_t)size+MAD_BUFFER_GUARD,1);if(!data)return 2;
    if(fread(data,1,(size_t)size,file)!=(size_t)size)return 2;fclose(file);
    struct mad_stream stream;struct mad_frame frame;
    mad_stream_init(&stream);mad_frame_init(&frame);mad_stream_buffer(&stream,data,(size_t)size+MAD_BUFFER_GUARD);
    size_t index=0;
    while(stream.next_frame<data+size) {
        if(mad_frame_decode(&frame,&stream)) {fprintf(stderr,"frame %zu: %s\n",index,mad_stream_errorstr(&stream));return 1;}
        printf("%zu",index++);
        for(int n=0;n<18;n++)printf("\t%.17g",(double)frame.sbsample[0][n][0]/(double)(1UL<<MAD_F_FRACBITS));
        putchar('\n');
    }
    mad_frame_finish(&frame);mad_stream_finish(&stream);free(data);return 0;
}
