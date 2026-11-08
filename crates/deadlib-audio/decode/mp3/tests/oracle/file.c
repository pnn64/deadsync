/* Validation only: public libmpg123 gapless decoding and sample seeking. */
#include <mpg123.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
static void u32(FILE*out,uint32_t n){for(int i=0;i<4;i++)fputc((int)((n>>(8*i))&255),out);}
static void f32(FILE*out,float n){uint32_t v;memcpy(&v,&n,4);u32(out,v);}
int main(int argc,char**argv){
    if(argc!=3&&argc!=5)return 2;
    if(mpg123_init()!=MPG123_OK)return 2;
    int error;mpg123_handle*dec=mpg123_new(NULL,&error);if(!dec)return 2;
    if(mpg123_param(dec,MPG123_ADD_FLAGS,MPG123_GAPLESS,0)!=MPG123_OK)return 2;
    /* Explicit validation history, documented by MPG123_PREFRAMES. */
    if(mpg123_param(dec,MPG123_PREFRAMES,8,0)!=MPG123_OK)return 2;
    if(mpg123_format_none(dec)!=MPG123_OK)return 2;
    const long*rates;size_t count;mpg123_rates(&rates,&count);
    for(size_t i=0;i<count;i++)if(mpg123_format(dec,rates[i],MPG123_MONO|MPG123_STEREO,MPG123_ENC_FLOAT_32)!=MPG123_OK)return 2;
    if(mpg123_open(dec,argv[1])!=MPG123_OK)return 1;
    long rate;int channels,encoding;
    if(mpg123_getformat(dec,&rate,&channels,&encoding)!=MPG123_OK||encoding!=MPG123_ENC_FLOAT_32||channels<1||channels>2)return 2;
    uint64_t left=UINT64_MAX;
    if(argc==5){off_t sample=(off_t)strtoull(argv[3],NULL,10);left=strtoull(argv[4],NULL,10);if(mpg123_scan(dec)!=MPG123_OK||sample<0||mpg123_seek(dec,sample,SEEK_SET)!=sample)return 1;}
    FILE*out=fopen(argv[2],"wb");if(!out)return 2;fwrite("DLMP3FI1",1,8,out);
    unsigned char bytes[8192];
    while(left){
        size_t request=sizeof(bytes);if(left<request/(4*channels))request=(size_t)left*4*channels;
        size_t done;int result=mpg123_read(dec,bytes,request,&done);
        if(result==MPG123_NEW_FORMAT){if(done||mpg123_getformat(dec,&rate,&channels,&encoding)!=MPG123_OK||encoding!=MPG123_ENC_FLOAT_32||channels<1||channels>2)return 2;continue;}
        if(result!=MPG123_OK&&result!=MPG123_DONE){fprintf(stderr,"%s\n",mpg123_strerror(dec));return 1;}
        if(done%(4*channels))return 2;
        if(done){u32(out,(uint32_t)rate);u32(out,(uint32_t)channels);u32(out,(uint32_t)(done/(4*channels)));for(size_t n=0;n<done/4;n++){float sample;memcpy(&sample,bytes+4*n,4);f32(out,sample);}if(left!=UINT64_MAX)left-=done/(4*channels);}
        if(result==MPG123_DONE)break;
    }
    if(fclose(out))return 2;mpg123_close(dec);mpg123_delete(dec);mpg123_exit();return 0;
}
