// Test-only process boundary to pinned, unmodified libogg.
#include <ogg/ogg.h>
#include <algorithm>
#include <chrono>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <fstream>
#include <iterator>
#include <string>
#include <vector>
#ifdef _WIN32
#include <fcntl.h>
#include <io.h>
#endif
static void require(bool ok) { if(!ok) {std::fputs("invalid input or worker state\n",stderr);std::exit(1);} }
struct Stats {uint64_t pages=0,packets=0,payload=0,reads=0,bytes=0,checksum=0;
  void add(const Stats& s) {pages+=s.pages;packets+=s.packets;payload+=s.payload;reads+=s.reads;bytes+=s.bytes;checksum+=s.checksum;}};
struct Logical {uint32_t serial;ogg_stream_state state;};
static void put(uint64_t v,int bytes) {for(int i=0;i<bytes;++i)require(std::putchar((int)((v>>(i*8))&255))!=EOF);}
static Stats run(const std::string& phase,const std::vector<unsigned char>& input,bool verify=false) {
  ogg_sync_state sync;require(ogg_sync_init(&sync)==0);std::vector<Logical> streams;Stats out;size_t position=0;
  bool startup=phase=="startup",page_only=phase=="pages";size_t quantum=phase=="stream4096"?4096:8192;
  for(;;) {
    char* buffer=ogg_sync_buffer(&sync,(long)quantum);require(buffer!=nullptr);
    size_t n=std::min(quantum,input.size()-position);if(n)std::memcpy(buffer,input.data()+position,n);
    ++out.reads;position+=n;out.bytes+=n;require(ogg_sync_wrote(&sync,(long)n)==0);
    ogg_page page;int result;
    while((result=ogg_sync_pageout(&sync,&page))!=0) {
      require(result==1);++out.pages;uint32_t serial=(uint32_t)ogg_page_serialno(&page);
      if(verify) {put('G',1);put(serial,4);put((uint32_t)ogg_page_pageno(&page),4);put((uint64_t)ogg_page_granulepos(&page),8);put(page.header[5],1);put((uint32_t)(page.header_len+page.body_len),4);}
      if(!page_only) {
        size_t index=0;while(index<streams.size()&&streams[index].serial!=serial)++index;
        if(index==streams.size()) {require(streams.size()<4096);streams.push_back(Logical{});streams.back().serial=serial;require(ogg_stream_init(&streams.back().state,(int)serial)==0);}
        require(ogg_stream_pagein(&streams[index].state,&page)==0);ogg_packet packet;int packet_result;
        while((packet_result=ogg_stream_packetout(&streams[index].state,&packet))!=0) {
          require(packet_result==1);++out.packets;out.payload+=(uint64_t)packet.bytes;
          if(verify) {put('P',1);put(serial,4);put((uint64_t)packet.granulepos,8);put(packet.b_o_s!=0,1);put(packet.e_o_s!=0,1);put((uint64_t)packet.bytes,8);require(std::fwrite(packet.packet,1,(size_t)packet.bytes,stdout)==(size_t)packet.bytes);}
        }
      }
      if(startup&&out.packets)goto finished;
    }
    if(!n) {require(sync.fill==sync.returned);break;}
  }
finished:
  for(auto& stream:streams)require(ogg_stream_clear(&stream.state)==0);
  require(ogg_sync_clear(&sync)==0);return out;
}
static std::vector<unsigned char> load(const char* path) {
  std::ifstream input(path,std::ios::binary);require(input.good());
  return std::vector<unsigned char>(std::istreambuf_iterator<char>(input),{});
}
int main(int argc,char** argv) {
  require(argc>=3);std::string phase=argv[1];
  if(phase=="verify") {
#ifdef _WIN32
    _setmode(_fileno(stdout),_O_BINARY);
#endif
    run("demux",load(argv[2]),true);require(std::fflush(stdout)==0);return 0;
  }
  require(argc==4);size_t iterations=(size_t)std::stoull(argv[2]);require(iterations>0);Stats stats;uint64_t elapsed;
  if(phase=="crc") {
    size_t n=(size_t)std::stoull(argv[3]);require(n>=27&&n<=(1u<<24));std::vector<unsigned char> data(n);
    for(size_t i=0;i<n;++i)data[i]=(unsigned char)(i*37+(i>>8)*17+11);
    ogg_page page;page.header=data.data();page.header_len=27;page.body=data.data()+27;page.body_len=(long)n-27;
    ogg_page_checksum_set(&page);uint64_t expected=(uint32_t)data[22]|((uint32_t)data[23]<<8)|((uint32_t)data[24]<<16)|((uint32_t)data[25]<<24);
    auto begin=std::chrono::steady_clock::now();
    for(size_t i=0;i<iterations;++i) {ogg_page_checksum_set(&page);stats.checksum+=(uint32_t)data[22]|((uint32_t)data[23]<<8)|((uint32_t)data[24]<<16)|((uint32_t)data[25]<<24);}
    elapsed=(uint64_t)std::chrono::duration_cast<std::chrono::nanoseconds>(std::chrono::steady_clock::now()-begin).count();
    require(stats.checksum==expected*iterations);stats.bytes=n*iterations;
  } else {
    require(phase=="pages"||phase=="demux"||phase=="stream4096"||phase=="startup");auto input=load(argv[3]);run(phase,input);
    auto begin=std::chrono::steady_clock::now();for(size_t i=0;i<iterations;++i)stats.add(run(phase,input));
    elapsed=(uint64_t)std::chrono::duration_cast<std::chrono::nanoseconds>(std::chrono::steady_clock::now()-begin).count();
  }
  std::printf("libogg\t%s\t%llu\t%llu\t%llu\t%llu\t%llu\t%llu\t%llu\t%llu\n",phase.c_str(),(unsigned long long)iterations,(unsigned long long)elapsed,(unsigned long long)stats.pages,(unsigned long long)stats.packets,(unsigned long long)stats.payload,(unsigned long long)stats.reads,(unsigned long long)stats.bytes,(unsigned long long)stats.checksum);
}
