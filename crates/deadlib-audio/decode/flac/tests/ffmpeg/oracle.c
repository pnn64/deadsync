/* Test-only public libavformat/libavcodec API. No resampler or PCM encoder. */
#include <libavcodec/avcodec.h>
#include <libavformat/avformat.h>
#include <libavutil/samplefmt.h>
#include <libavutil/error.h>
#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <string.h>

static void u32le(unsigned value) {
    unsigned char b[4] = {value, value >> 8, value >> 16, value >> 24};
    if(fwrite(b,1,4,stdout)!=4) { perror("stdout"); exit(1); }
}
static int emit(AVCodecContext *codec, AVFrame *frame) {
    int channels=frame->ch_layout.nb_channels, bits=codec->bits_per_raw_sample;
    int format=av_get_packed_sample_fmt(frame->format);
    int planar=av_sample_fmt_is_planar(frame->format), bytes=av_get_bytes_per_sample(frame->format);
    if(channels<1||channels>8||bits<4||bits>bytes*8||!(format==AV_SAMPLE_FMT_S16||format==AV_SAMPLE_FMT_S32)) {
        fprintf(stderr,"unexpected FFmpeg PCM: %s channels=%d bits=%d\n",av_get_sample_fmt_name(frame->format),channels,bits); return 1;
    }
    u32le(frame->sample_rate);u32le(channels);u32le(bits);u32le(frame->nb_samples);
    for(int c=0;c<channels;c++) for(int i=0;i<frame->nb_samples;i++) {
        const unsigned char *p=frame->extended_data[planar?c:0]+(planar?i:i*channels+c)*bytes;
        int32_t value;
        if(bytes==2) { int16_t v;memcpy(&v,p,2);value=v; }
        else memcpy(&value,p,4);
        value >>= bytes*8-bits;
        u32le((uint32_t)value);
    }
    return 0;
}
static int drain(AVCodecContext *codec,AVFrame *frame,unsigned *emitted) {
    int status;
    while((status=avcodec_receive_frame(codec,frame))>=0) {
        if(emit(codec,frame))return AVERROR_INVALIDDATA;
        ++*emitted;
        av_frame_unref(frame);
    }
    return status==AVERROR(EAGAIN)||status==AVERROR_EOF?0:status;
}
static int decode_file(const char *filename) {
    AVFormatContext *format=NULL;
    AVCodecContext *codec=NULL;
    AVPacket *packet=av_packet_alloc();
    AVFrame *frame=av_frame_alloc();
    int status=0,stream=-1;
    unsigned emitted=0;
    if(!packet||!frame)return 2;
    av_log_set_level(AV_LOG_ERROR);
    if((status=avformat_open_input(&format,filename,NULL,NULL))<0)goto end;
    if((status=avformat_find_stream_info(format,NULL))<0)goto end;
    for(unsigned i=0;i<format->nb_streams;i++) if(format->streams[i]->codecpar->codec_id==AV_CODEC_ID_FLAC) {stream=i;break;}
    if(stream<0) {status=AVERROR_DECODER_NOT_FOUND;goto end;}
    codec=avcodec_alloc_context3(avcodec_find_decoder(AV_CODEC_ID_FLAC));
    if(!codec) {status=AVERROR(ENOMEM);goto end;}
    if((status=avcodec_parameters_to_context(codec,format->streams[stream]->codecpar))<0)goto end;
    codec->thread_count=1;
    codec->err_recognition=AV_EF_CRCCHECK|AV_EF_EXPLODE;
    if((status=avcodec_open2(codec,codec->codec,NULL))<0)goto end;
    while((status=av_read_frame(format,packet))>=0) {
        if(packet->stream_index==stream) {
            status=avcodec_send_packet(codec,packet);
            if(status>=0)status=drain(codec,frame,&emitted);
            /* Match the libFLAC oracle's initial LOST_SYNC recovery for the
             * official headerless multicast file with a garbage prefix. */
            if(status==AVERROR_INVALIDDATA&&!emitted&&!codec->extradata_size)status=0;
        }
        av_packet_unref(packet);
        if(status<0)goto end;
    }
    if(status!=AVERROR_EOF)goto end;
    status=avcodec_send_packet(codec,NULL);
    if(status>=0)status=drain(codec,frame,&emitted);
end:
    if(status<0) {char message[AV_ERROR_MAX_STRING_SIZE];av_strerror(status,message,sizeof(message));fprintf(stderr,"FFmpeg: %s\n",message);}
    av_frame_free(&frame);av_packet_free(&packet);avcodec_free_context(&codec);avformat_close_input(&format);
    return status<0||ferror(stdout)?1:0;
}
int main(int argc,char **argv) {
    if(argc==2&&strcmp(argv[1],"--version")==0) { puts(av_version_info()); return 0; }
    if(argc==2)return decode_file(argv[1]);
    if(argc==3&&strcmp(argv[1],"--batch")==0) {
        FILE *list=fopen(argv[2],"r");char filename[4096];unsigned index=0;int failed=0;
        if(!list)return 2;
        while(fgets(filename,sizeof(filename),list)) {
            filename[strcspn(filename,"\r\n")]=0;
            u32le(0xffffffff);u32le(index++);
            int status=decode_file(filename);failed|=status;
            u32le(0xfffffffe);u32le(status);
        }
        failed|=ferror(list);fclose(list);return failed?1:0;
    }
    return 2;
}
