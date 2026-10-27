/* Test-only libFLAC process. PCM protocol is always signed, right-aligned i32. */
#include <FLAC/stream_decoder.h>
#include <stdio.h>
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
int main(int argc,char **argv) {
    FLAC__StreamDecoder *d;
    FLAC__bool ok;
    if(argc!=2) {fprintf(stderr,"usage: deadlib_flac_oracle INPUT.flac\n");return 2;}
#ifdef _WIN32
    _setmode(_fileno(stdout),_O_BINARY);
#endif
    d=FLAC__stream_decoder_new();
    if(!d)return 2;
    FLAC__stream_decoder_set_md5_checking(d,true);
    if(FLAC__stream_decoder_init_file(d,argv[1],write_pcm,NULL,error_cb,NULL)!=FLAC__STREAM_DECODER_INIT_STATUS_OK)return 2;
    ok=FLAC__stream_decoder_process_until_end_of_stream(d);
    if(!FLAC__stream_decoder_finish(d))ok=false;
    FLAC__stream_decoder_delete(d);
    return ok && !failed && !ferror(stdout) ? 0 : 1;
}
