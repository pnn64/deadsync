/* Test-only libFLAC process. PCM protocol is always signed, right-aligned i32. */
#include <FLAC/stream_decoder.h>
#include <FLAC/stream_encoder.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#ifdef _WIN32
#include <fcntl.h>
#include <io.h>
#endif

static int failed = 0;
static void u32le(FLAC__uint32 x) {
    unsigned char b[4] = {(unsigned char)x,(unsigned char)(x>>8),(unsigned char)(x>>16),(unsigned char)(x>>24)};
    if (fwrite(b,1,4,stdout)!=4) failed=1;
}
static FLAC__StreamDecoderWriteStatus write_pcm(const FLAC__StreamDecoder *d,
    const FLAC__Frame *frame,const FLAC__int32 * const planes[],void *user) {
    unsigned c,i;
    (void)d;(void)user;
    u32le(frame->header.sample_rate);u32le(frame->header.channels);
    u32le(frame->header.bits_per_sample);u32le(frame->header.blocksize);
    for(c=0;c<frame->header.channels;c++)
        for(i=0;i<frame->header.blocksize;i++) u32le((FLAC__uint32)planes[c][i]);
    return failed ? FLAC__STREAM_DECODER_WRITE_STATUS_ABORT : FLAC__STREAM_DECODER_WRITE_STATUS_CONTINUE;
}
static void error_cb(const FLAC__StreamDecoder *d,FLAC__StreamDecoderErrorStatus status,void *user) {
    (void)d;(void)user;
    /* LOST_SYNC is expected for headerless multicast test vectors. */
    if(status!=FLAC__STREAM_DECODER_ERROR_STATUS_LOST_SYNC) {
        fprintf(stderr,"libFLAC: %s\n",FLAC__StreamDecoderErrorStatusString[status]); failed=1;
    }
}
static int encode_file(int argc,char **argv) {
    FLAC__StreamEncoder *encoder;
    FLAC__bool ok=true;
    FILE *input;
    unsigned rate,channels,bits,level,block;
    unsigned char raw[4096*8*4];
    FLAC__int32 pcm[4096*8];
    size_t n,i;
    if(argc!=9)return 2;
    rate=(unsigned)strtoul(argv[4],NULL,10);channels=(unsigned)strtoul(argv[5],NULL,10);
    bits=(unsigned)strtoul(argv[6],NULL,10);level=(unsigned)strtoul(argv[7],NULL,10);block=(unsigned)strtoul(argv[8],NULL,10);
    if(channels<1||channels>8)return 2;
    input=fopen(argv[2],"rb");if(!input)return 2;
    encoder=FLAC__stream_encoder_new();if(!encoder){fclose(input);return 2;}
    ok=FLAC__stream_encoder_set_verify(encoder,true)
        && FLAC__stream_encoder_set_streamable_subset(encoder,false)
        && FLAC__stream_encoder_set_channels(encoder,channels)
        && FLAC__stream_encoder_set_bits_per_sample(encoder,bits)
        && FLAC__stream_encoder_set_sample_rate(encoder,rate)
        && FLAC__stream_encoder_set_compression_level(encoder,level)
        && FLAC__stream_encoder_set_blocksize(encoder,block);
    if(ok)ok=(strcmp(argv[1],"encode-ogg")==0
        ? FLAC__stream_encoder_init_ogg_file(encoder,argv[3],NULL,NULL)
        : FLAC__stream_encoder_init_file(encoder,argv[3],NULL,NULL))==FLAC__STREAM_ENCODER_INIT_STATUS_OK;
    while(ok&&(n=fread(raw,1,4096*channels*4,input))!=0) {
        if(n%(channels*4)!=0){ok=false;break;}
        for(i=0;i<n/4;i++)pcm[i]=(FLAC__int32)((FLAC__uint32)raw[i*4]|((FLAC__uint32)raw[i*4+1]<<8)|((FLAC__uint32)raw[i*4+2]<<16)|((FLAC__uint32)raw[i*4+3]<<24));
        ok=FLAC__stream_encoder_process_interleaved(encoder,pcm,(unsigned)(n/(channels*4)));
    }
    if(ferror(input))ok=false;
    fclose(input);
    if(!FLAC__stream_encoder_finish(encoder))ok=false;
    if(!ok)fprintf(stderr,"libFLAC encoder failed\n");
    FLAC__stream_encoder_delete(encoder);
    return ok?0:1;
}
int main(int argc,char **argv) {
    FLAC__StreamDecoder *d;
    FLAC__bool ok;
    int ogg=argc==3&&strcmp(argv[1],"ogg")==0;
    if(argc>1&&(strcmp(argv[1],"encode")==0||strcmp(argv[1],"encode-ogg")==0))return encode_file(argc,argv);
    if(argc!=2&&!ogg) {fprintf(stderr,"usage: deadlib_flac_oracle [ogg] INPUT.flac\n       deadlib_flac_oracle encode[-ogg] INPUT.i32le OUTPUT.flac RATE CHANNELS BITS LEVEL BLOCK\n");return 2;}
#ifdef _WIN32
    _setmode(_fileno(stdout),_O_BINARY);
#endif
    d=FLAC__stream_decoder_new();
    if(!d)return 2;
    FLAC__stream_decoder_set_md5_checking(d,true);
    if((ogg ? FLAC__stream_decoder_init_ogg_file(d,argv[2],write_pcm,NULL,error_cb,NULL)
        : FLAC__stream_decoder_init_file(d,argv[1],write_pcm,NULL,error_cb,NULL))!=FLAC__STREAM_DECODER_INIT_STATUS_OK)return 2;
    ok=FLAC__stream_decoder_process_until_end_of_stream(d);
    if(!FLAC__stream_decoder_finish(d))ok=false;
    FLAC__stream_decoder_delete(d);
    return ok && !failed && !ferror(stdout) ? 0 : 1;
}
