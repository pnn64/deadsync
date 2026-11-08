/* Independent libpng encoder. Test-only input: ten big-endian u32 words
 * (NPG1, width, height, depth, color, interlace, filter index, zlib level,
 * palette entry count, tRNS byte count), followed by RGB palette bytes,
 * tRNS bytes and packed, big-endian native scanlines without filter bytes.
 * tRNS count 0xffffffff means absent. Output is a PNG, with no transforms. */
#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <png.h>
#ifdef _WIN32
#include <fcntl.h>
#include <io.h>
#endif

static uint32_t be32(const unsigned char *p) {
    return ((uint32_t)p[0]<<24)|((uint32_t)p[1]<<16)|((uint32_t)p[2]<<8)|p[3];
}
static unsigned int be16(const unsigned char *p) { return ((unsigned int)p[0]<<8)|p[1]; }

int main(int argc, char **argv) {
    unsigned char header[40], palette_bytes[768], alpha[256];
    uint32_t width,height,depth,color,interlace,filter,level,palette_size,trns_size;
    const int filters[6]={PNG_FILTER_NONE,PNG_FILTER_SUB,PNG_FILTER_UP,
                          PNG_FILTER_AVG,PNG_FILTER_PAETH,PNG_ALL_FILTERS};
    size_t stride,size;
    unsigned int channels,y,i;
    png_color palette[256];
    png_color_16 transparent={0};
    png_structp png;
    png_infop info;
    png_bytep data;
    png_bytepp rows;
    FILE *input;
    int status=0;
    if (argc!=2 || !(input=fopen(argv[1],"rb"))) return 2;
    if (fread(header,1,sizeof(header),input)!=sizeof(header) || be32(header)!=0x4e504731) { fclose(input);return 2; }
    width=be32(header+4);height=be32(header+8);depth=be32(header+12);color=be32(header+16);
    interlace=be32(header+20);filter=be32(header+24);level=be32(header+28);
    palette_size=be32(header+32);trns_size=be32(header+36);
    channels=color==0 || color==3 ? 1 : color==2 ? 3 : color==4 ? 2 : color==6 ? 4 : 0;
    if (!channels || !width || !height || width>4096 || height>4096 || interlace>1 || filter>5 || level>9 || palette_size>256 ||
        !(depth==1 || depth==2 || depth==4 || depth==8 || depth==16) ||
        (color==3 && depth==16) || ((color==2 || color==4 || color==6) && depth<8) ||
        ((color==0 || color==4) && palette_size!=0) || (color==3 && (palette_size==0 || palette_size>(1u<<depth))) ||
        (trns_size!=UINT32_MAX && (trns_size>256 || (color==0 && trns_size!=2) || (color==2 && trns_size!=6) ||
          (color==3 && trns_size>palette_size) || color==4 || color==6))) { fclose(input);return 2; }
    stride=((size_t)width*channels*depth+7)/8;
    size=stride*height;
    if (size>8*1024*1024 || fread(palette_bytes,3,palette_size,input)!=palette_size ||
        (trns_size!=UINT32_MAX && fread(alpha,1,trns_size,input)!=trns_size)) { fclose(input);return 2; }
    data=(png_bytep)malloc(size);
    rows=(png_bytepp)malloc(sizeof(*rows)*height);
    if (!data || !rows) { free(data);free(rows);fclose(input);return 3; }
    if (fread(data,1,size,input)!=size || fgetc(input)!=EOF || ferror(input)) { free(data);free(rows);fclose(input);return 2; }
    fclose(input);
    for (y=0;y<height;y++) rows[y]=data+y*stride;
    for (i=0;i<palette_size;i++) {
        palette[i].red=palette_bytes[3*i];palette[i].green=palette_bytes[3*i+1];palette[i].blue=palette_bytes[3*i+2];
    }
    png=png_create_write_struct(PNG_LIBPNG_VER_STRING,NULL,NULL,NULL);
    info=png ? png_create_info_struct(png) : NULL;
    if (!png || !info) { png_destroy_write_struct(&png,&info);free(data);free(rows);return 3; }
    if (setjmp(png_jmpbuf(png))) status=1;
    else {
#ifdef _WIN32
        _setmode(_fileno(stdout),_O_BINARY);
#endif
        png_init_io(png,stdout);
        png_set_IHDR(png,info,width,height,(int)depth,(int)color,(int)interlace,PNG_COMPRESSION_TYPE_BASE,PNG_FILTER_TYPE_BASE);
        if (palette_size) png_set_PLTE(png,info,palette,(int)palette_size);
        if (trns_size!=UINT32_MAX) {
            if (color==0) transparent.gray=(png_uint_16)be16(alpha);
            if (color==2) { transparent.red=(png_uint_16)be16(alpha);transparent.green=(png_uint_16)be16(alpha+2);transparent.blue=(png_uint_16)be16(alpha+4); }
            png_set_tRNS(png,info,color==3 ? alpha : NULL,color==3 ? (int)trns_size : 1,&transparent);
        }
        png_set_filter(png,PNG_FILTER_TYPE_BASE,filters[filter]);
        png_set_compression_level(png,(int)level);
        png_set_rows(png,info,rows);
        png_write_png(png,info,PNG_TRANSFORM_IDENTITY,NULL);
        if (fflush(stdout)!=0) status=3;
    }
    png_destroy_write_struct(&png,&info);
    free(data);free(rows);
    return status;
}
