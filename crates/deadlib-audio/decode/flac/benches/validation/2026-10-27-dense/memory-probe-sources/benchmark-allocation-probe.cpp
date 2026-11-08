// Resident input. No PCM file writes, resampler or process-startup timing.
extern "C" {
#include <FLAC/stream_decoder.h>
#include <libavcodec/avcodec.h>
#include <libavutil/samplefmt.h>
}
#include <chrono>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <fstream>
#include <iterator>
#include <string>
#include <vector>
#include <algorithm>
using Clock=std::chrono::steady_clock;
extern "C" void deadlib_alloc_begin();
extern "C" void deadlib_alloc_end(size_t);
static void require(bool condition,const char* message){if(!condition){std::fprintf(stderr,"%s\n",message);std::exit(1);}}
static uint64_t word(const std::vector<uint8_t>& data,size_t& pos,size_t width){require(width<=data.size()-pos,"index bounds");uint64_t value=0;for(size_t i=0;i<width;i++)value|=uint64_t(data[pos++])<<(i*8);return value;}
static std::vector<uint8_t> load(const char* path){std::ifstream f(path,std::ios::binary);require(bool(f),"open input");return {std::istreambuf_iterator<char>(f),std::istreambuf_iterator<char>()};}
static void opaque(const void* pointer){asm volatile(""::"g"(pointer):"memory");}
struct Source{const std::vector<uint8_t>& data;size_t pos=0;uint64_t samples=0,sum=0;bool failed=false;};
static FLAC__StreamDecoderReadStatus read_flac(const FLAC__StreamDecoder*,FLAC__byte* out,size_t* bytes,void* user){auto& s=*static_cast<Source*>(user);*bytes=std::min(*bytes,s.data.size()-s.pos);std::memcpy(out,s.data.data()+s.pos,*bytes);s.pos+=*bytes;return *bytes?FLAC__STREAM_DECODER_READ_STATUS_CONTINUE:FLAC__STREAM_DECODER_READ_STATUS_END_OF_STREAM;}
static FLAC__StreamDecoderWriteStatus pcm_flac(const FLAC__StreamDecoder*,const FLAC__Frame* f,const FLAC__int32* const p[],void* user){auto& s=*static_cast<Source*>(user);for(unsigned c=0;c<f->header.channels;c++){opaque(p[c]);s.sum+=uint32_t(p[c][0]);s.sum+=uint32_t(p[c][f->header.blocksize-1]);}s.samples+=uint64_t(f->header.channels)*f->header.blocksize;return FLAC__STREAM_DECODER_WRITE_STATUS_CONTINUE;}
static void error_flac(const FLAC__StreamDecoder*,FLAC__StreamDecoderErrorStatus status,void* user){std::fprintf(stderr,"libFLAC: %s\n",FLAC__StreamDecoderErrorStatusString[status]);static_cast<Source*>(user)->failed=true;}
int main(int argc,char**argv){
    require(argc==5,"benchmark MODE INPUT INDEX ITERATIONS");std::string mode=argv[1];auto data=load(argv[2]);auto index=load(argv[3]);size_t iterations=std::strtoull(argv[4],nullptr,10);require(iterations>0,"iterations");
    require(index.size()>=46&&std::memcmp(index.data(),"DLFPERF1",8)==0,"index signature");size_t pos=42;auto count=word(index,pos,4);
    struct Location{uint64_t offset;uint32_t length,samples;};std::vector<Location> frames;
    for(size_t i=0;i<count;i++){Location f{word(index,pos,8),uint32_t(word(index,pos,4)),uint32_t(word(index,pos,4))};if(mode.find("ogg")==std::string::npos)require(f.offset+f.length<=data.size(),"frame bounds");frames.push_back(f);}
    uint64_t ns=0,units=0,sum=0;uint64_t expected_samples=0,expected_sum=0;
    if(mode.rfind("libflac",0)==0){
        Source source{data};FLAC__StreamDecoder* decoder=nullptr;
        bool md5=mode.find("md5")!=std::string::npos;bool raw=mode=="libflac-frames";bool startup=mode.find("startup")!=std::string::npos;
        bool fresh=mode.find("new")!=std::string::npos;bool ogg=mode.find("ogg")!=std::string::npos;
        auto init=[&](){
            if(!decoder){decoder=FLAC__stream_decoder_new();require(decoder,"libFLAC allocation");}
            require(FLAC__stream_decoder_set_md5_checking(decoder,md5),"libFLAC MD5 config");
            auto status=ogg?FLAC__stream_decoder_init_ogg_stream(decoder,read_flac,nullptr,nullptr,nullptr,nullptr,pcm_flac,nullptr,error_flac,&source):FLAC__stream_decoder_init_stream(decoder,read_flac,nullptr,nullptr,nullptr,nullptr,pcm_flac,nullptr,error_flac,&source);
            require(status==FLAC__STREAM_DECODER_INIT_STATUS_OK,"libFLAC init");
        };
        if(!fresh)init();
        for(size_t i=0;i<=iterations;i++){
            source.pos=0;source.samples=0;source.sum=0;source.failed=false;
            if(raw){require(FLAC__stream_decoder_reset(decoder),"libFLAC reset");require(FLAC__stream_decoder_process_until_end_of_metadata(decoder),"libFLAC metadata");}
            deadlib_alloc_begin();auto start=Clock::now();
            if(fresh)init();else if(!raw)require(FLAC__stream_decoder_reset(decoder),"libFLAC reset");
            require(bool(FLAC__stream_decoder_get_md5_checking(decoder))==md5,"libFLAC MD5 policy changed");
            if(startup){while(!source.samples&&!source.failed){require(FLAC__stream_decoder_process_single(decoder),"libFLAC first frame");require(FLAC__stream_decoder_get_state(decoder)!=FLAC__STREAM_DECODER_END_OF_STREAM,"empty startup");}}
            else require(FLAC__stream_decoder_process_until_end_of_stream(decoder),"libFLAC decode");
            if(md5||fresh)require(FLAC__stream_decoder_finish(decoder),"libFLAC digest/finish");
            if(fresh){FLAC__stream_decoder_delete(decoder);decoder=nullptr;}
            auto elapsed=std::chrono::duration_cast<std::chrono::nanoseconds>(Clock::now()-start).count();deadlib_alloc_end(i);require(!source.failed,"libFLAC decode error");
            if(i==0){expected_samples=source.samples;expected_sum=source.sum;}
            else{require(source.samples==expected_samples&&source.sum==expected_sum,"libFLAC repeat mismatch");ns+=elapsed;units+=source.samples;sum+=source.sum;}
            // The audit probe corrupts only the second pass's expected digest.
            // It detects loss of MD5 checking across finish/reinitialization.
            if(i==0&&std::getenv("DEADLIB_FLAC_BENCH_PROBE_MD5")){require(md5&&!ogg&&data.size()>26,"MD5 probe requires native MD5 mode");data[26]^=1;}
            if(md5&&!fresh&&i<iterations)init();
        }
        if(decoder)FLAC__stream_decoder_delete(decoder);
    }else if(mode=="ffmpeg"){
        av_log_set_level(AV_LOG_ERROR);const AVCodec* implementation=avcodec_find_decoder(AV_CODEC_ID_FLAC);auto* decoder=avcodec_alloc_context3(implementation);require(decoder,"FFmpeg allocation");
        decoder->extradata=static_cast<uint8_t*>(av_mallocz(34+AV_INPUT_BUFFER_PADDING_SIZE));decoder->extradata_size=34;std::memcpy(decoder->extradata,index.data()+8,34);decoder->thread_count=1;decoder->err_recognition=AV_EF_CRCCHECK|AV_EF_EXPLODE;
        require(avcodec_open2(decoder,implementation,nullptr)>=0,"FFmpeg init");std::vector<AVPacket*> packets;auto* frame=av_frame_alloc();require(frame,"FFmpeg frame");
        for(auto& f:frames){auto* p=av_packet_alloc();require(p&&av_new_packet(p,f.length)>=0,"FFmpeg packet");std::memcpy(p->data,data.data()+f.offset,f.length);packets.push_back(p);}
        for(size_t i=0;i<=iterations;i++){
            avcodec_flush_buffers(decoder);uint64_t samples=0,checksum=0;deadlib_alloc_begin();auto start=Clock::now();
            for(auto* packet:packets){require(avcodec_send_packet(decoder,packet)>=0,"FFmpeg send");int status;
                while((status=avcodec_receive_frame(decoder,frame))>=0){int bits=decoder->bits_per_raw_sample,channels=frame->ch_layout.nb_channels,bytes=av_get_bytes_per_sample(AVSampleFormat(frame->format));bool planar=av_sample_fmt_is_planar(AVSampleFormat(frame->format));require((bytes==2||bytes==4)&&bits<=bytes*8,"FFmpeg PCM format");
                    for(int c=0;c<channels;c++){auto* p=frame->extended_data[planar?c:0];opaque(p);for(int j:{0,frame->nb_samples-1}){size_t offset=(planar?j:j*channels+c)*bytes;int32_t value;if(bytes==2){int16_t v;std::memcpy(&v,p+offset,2);value=v;}else std::memcpy(&value,p+offset,4);checksum+=uint32_t(value>>(bytes*8-bits));}}
                    samples+=uint64_t(channels)*frame->nb_samples;av_frame_unref(frame);
                }require(status==AVERROR(EAGAIN),"FFmpeg receive");
            }
            auto elapsed=std::chrono::duration_cast<std::chrono::nanoseconds>(Clock::now()-start).count();deadlib_alloc_end(i);if(i==0){expected_samples=samples;expected_sum=checksum;}else{require(samples==expected_samples&&checksum==expected_sum,"FFmpeg repeat mismatch");ns+=elapsed;units+=samples;sum+=checksum;}
        }
        for(auto* p:packets)av_packet_free(&p);av_frame_free(&frame);avcodec_free_context(&decoder);
    }else require(false,"unknown mode");
    std::printf("%s\t%zu\t%llu\t%llu\t%llu\t%zu\t%zu\t0\t0\n",mode.c_str(),iterations,(unsigned long long)ns,(unsigned long long)units,(unsigned long long)sum,data.size(),frames.size());
}
