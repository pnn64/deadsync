/* Test-only native-sample libpng oracle. No sample transformations are enabled.
 * Emits width/height/depth/color (all u32 BE), then packed, big-endian scanlines.
 * libpng's setjmp error boundary stays entirely within this C process. */
#include <png.h>
#include <zlib.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#ifdef _WIN32
#include <fcntl.h>
#include <io.h>
#endif
static void word(unsigned int n) {
    unsigned char bytes[4]={(unsigned char)(n>>24),(unsigned char)(n>>16),(unsigned char)(n>>8),(unsigned char)n};
    if (fwrite(bytes,1,4,stdout)!=4) exit(3);
}
static void blob(const void *bytes,size_t size) {
    if (size && fwrite(bytes,1,size,stdout)!=size) exit(3);
}
static void named_header(const char *kind,const char *name,size_t size) {
    size_t length=strlen(name);
    blob(kind,4);word((unsigned int)length);blob(name,length);word((unsigned int)size);
}
/* Version 1, followed by kind/count/u32-BE-value records until EOF.  Values
 * come from libpng's integer getters, with no color or sample transforms. */
static void scalar_header(const char *kind,unsigned int count) { blob(kind,4);word(count); }
static void scalar_metadata(png_structp png,png_infop info) {
    png_colorp palette=NULL;
    png_bytep alpha=NULL;
    png_color_16p transparent=NULL,background=NULL;
    png_color_8p significant=NULL;
    png_uint_16p histogram=NULL;
    png_timep stamp=NULL;
    png_fixed_point gamma=0,wx=0,wy=0,rx=0,ry=0,gx=0,gy=0,bx=0,by=0;
    png_uint_32 x=0,y=0;
    int n=0,transparency=0,intent=0,unit=0,i,color=png_get_color_type(png,info);
    word(1);
    if (png_get_PLTE(png,info,&palette,&n)) {
        scalar_header("PLTE",3*(unsigned int)n);
        for (i=0;i<n;i++) {word(palette[i].red);word(palette[i].green);word(palette[i].blue);}
    }
    if (png_get_tRNS(png,info,&alpha,&transparency,&transparent)) {
        if (color==PNG_COLOR_TYPE_PALETTE) {scalar_header("tRNS",(unsigned int)transparency);for(i=0;i<transparency;i++)word(alpha[i]);}
        else if (color==PNG_COLOR_TYPE_GRAY) {scalar_header("tRNS",1);word(transparent->gray);}
        else {scalar_header("tRNS",3);word(transparent->red);word(transparent->green);word(transparent->blue);}
    }
    if (png_get_gAMA_fixed(png,info,&gamma)) {scalar_header("gAMA",1);word((unsigned int)gamma);}
    if (png_get_cHRM_fixed(png,info,&wx,&wy,&rx,&ry,&gx,&gy,&bx,&by)) {
        scalar_header("cHRM",8);word((unsigned int)wx);word((unsigned int)wy);word((unsigned int)rx);word((unsigned int)ry);
        word((unsigned int)gx);word((unsigned int)gy);word((unsigned int)bx);word((unsigned int)by);
    }
    if (png_get_sBIT(png,info,&significant)) {
        int gray=color==PNG_COLOR_TYPE_GRAY || color==PNG_COLOR_TYPE_GRAY_ALPHA;
        scalar_header("sBIT",(gray ? 1u : 3u)+((color&PNG_COLOR_MASK_ALPHA) ? 1u : 0u));
        if(gray)word(significant->gray);else {word(significant->red);word(significant->green);word(significant->blue);}
        if(color&PNG_COLOR_MASK_ALPHA)word(significant->alpha);
    }
    if (png_get_sRGB(png,info,&intent)) {scalar_header("sRGB",1);word((unsigned int)intent);}
    if (png_get_bKGD(png,info,&background)) {
        if(color==PNG_COLOR_TYPE_PALETTE) {scalar_header("bKGD",1);word(background->index);}
        else if(color==PNG_COLOR_TYPE_GRAY || color==PNG_COLOR_TYPE_GRAY_ALPHA) {scalar_header("bKGD",1);word(background->gray);}
        else {scalar_header("bKGD",3);word(background->red);word(background->green);word(background->blue);}
    }
    if (png_get_hIST(png,info,&histogram)) {scalar_header("hIST",(unsigned int)n);for(i=0;i<n;i++)word(histogram[i]);}
    if (png_get_pHYs(png,info,&x,&y,&unit)) {scalar_header("pHYs",3);word(x);word(y);word((unsigned int)unit);}
    if (png_get_tIME(png,info,&stamp)) {scalar_header("tIME",6);word(stamp->year);word(stamp->month);word(stamp->day);word(stamp->hour);word(stamp->minute);word(stamp->second);}
}
/* Version 1, presence bits cICP/mDCV/cLLI, then present fixed-point fields.
 * Chromaticities are libpng's integers scaled by 100,000, in R,G,B,white order.
 * Luminance values use the getter's exact 10,000 scale; no floating point. */
static void hdr_metadata(png_structp png,png_infop info) {
    png_byte primaries=0,transfer=0,matrix=0,range=0;
    png_fixed_point wx=0,wy=0,rx=0,ry=0,gx=0,gy=0,bx=0,by=0;
    png_uint_32 maxDL=0,minDL=0,maxCLL=0,maxFALL=0;
    unsigned int flags=0;
    if (png_get_cICP(png,info,&primaries,&transfer,&matrix,&range)) flags|=1;
    if (png_get_mDCV_fixed(png,info,&wx,&wy,&rx,&ry,&gx,&gy,&bx,&by,&maxDL,&minDL)) flags|=2;
    if (png_get_cLLI_fixed(png,info,&maxCLL,&maxFALL)) flags|=4;
    word(1);word(flags);
    if (flags&1) { word(primaries);word(transfer);word(matrix);word(range); }
    if (flags&2) {
        word((unsigned int)rx);word((unsigned int)ry);
        word((unsigned int)gx);word((unsigned int)gy);
        word((unsigned int)bx);word((unsigned int)by);
        word((unsigned int)wx);word((unsigned int)wy);word(maxDL);word(minDL);
    }
    if (flags&4) { word(maxCLL);word(maxFALL); }
}
/* Version 1: record count, then kind/name-length/name/value-length/value.
 * Text and ICC values are decompressed; sPLT is depth then five u16 BE fields
 * per entry, independent of its original eight-/sixteen-bit sample encoding. */
static void named_metadata(png_structp png,png_infop info) {
    png_textp text=NULL;
    png_sPLT_tp palettes=NULL;
    png_charp name=NULL;
    png_bytep profile=NULL;
    png_uint_32 profile_size=0;
    int compression=0,text_count=0,i,j,n;
    (void)png_get_text(png,info,&text,&text_count);
    n=png_get_sPLT(png,info,&palettes);
    (void)png_get_iCCP(png,info,&name,&compression,&profile,&profile_size);
    word(1);word((unsigned int)(text_count+n+(profile!=NULL)));
    for (i=0;i<text_count;i++) {
        int international=text[i].compression==PNG_ITXT_COMPRESSION_NONE ||
            text[i].compression==PNG_ITXT_COMPRESSION_zTXt;
        const char *kind=international ? "iTXt" :
            (text[i].compression==PNG_TEXT_COMPRESSION_zTXt ? "zTXt" : "tEXt");
        size_t size=international ? text[i].itxt_length : text[i].text_length;
        named_header(kind,text[i].key,size);blob(text[i].text,size);
    }
    if (profile!=NULL) {
        named_header("iCCP",name,profile_size);blob(profile,profile_size);
    }
    for (i=0;i<n;i++) {
        unsigned char depth=palettes[i].depth;
        named_header("sPLT",palettes[i].name,1+10*(size_t)palettes[i].nentries);
        blob(&depth,1);
        for (j=0;j<palettes[i].nentries;j++) {
            png_sPLT_entry *entry=palettes[i].entries+j;
            unsigned int values[5]={entry->red,entry->green,entry->blue,entry->alpha,entry->frequency};
            int k;
            for (k=0;k<5;k++) {
                unsigned char bytes[2]={(unsigned char)(values[k]>>8),(unsigned char)values[k]};
                blob(bytes,2);
            }
        }
    }
}
/* Batch protocol: input length, output capacity, compressed bytes. Emit signed
 * zlib status, successful output length, then successful output bytes (u32 BE).
 * A failed stream is a result record, not a failure of the batch transport. */
static int zlib_batch(FILE *file) {
    unsigned char fields[8];
    size_t count;
    while ((count=fread(fields,1,8,file))!=0) {
        unsigned int input_size,capacity;
        unsigned char *input,*output;
        uLong source_size;
        uLongf output_size;
        int status;
        if (count!=8) return 2;
        input_size=((unsigned int)fields[0]<<24)|((unsigned int)fields[1]<<16)|
            ((unsigned int)fields[2]<<8)|fields[3];
        capacity=((unsigned int)fields[4]<<24)|((unsigned int)fields[5]<<16)|
            ((unsigned int)fields[6]<<8)|fields[7];
        if (input_size>16777216 || capacity==0 || capacity>1048576) return 2;
        input=(unsigned char*)malloc(input_size ? input_size : 1);
        output=(unsigned char*)malloc(capacity);
        if (!input || !output) { free(input);free(output);return 3; }
        if (fread(input,1,input_size,file)!=input_size) { free(input);free(output);return 2; }
        source_size=input_size;output_size=capacity;
        status=uncompress2(output,&output_size,input,&source_size);
        if (status==Z_OK && source_size!=input_size) status=Z_DATA_ERROR;
        word((unsigned int)status);word(status==Z_OK ? (unsigned int)output_size : 0);
        if (status==Z_OK && fwrite(output,1,output_size,stdout)!=output_size) {
            free(input);free(output);return 3;
        }
        free(input);free(output);
    }
    return ferror(file) ? 2 : 0;
}
int main(int argc,char **argv) {
    FILE *file;
    png_structp png;
    png_infop info;
    png_bytep data;
    png_bytepp rows;
    png_uint_32 width,height,y;
    size_t stride;
    png_color_8p significant=NULL;
    png_byte original_bits[4]={0,0,0,0};
    unsigned int original_count=0;
    int metadata=argc==3 && strcmp(argv[1],"--metadata")==0;
    int names=argc==3 && strcmp(argv[1],"--named-metadata")==0;
    int hdr=argc==3 && strcmp(argv[1],"--hdr-metadata")==0;
    int scalar=argc==3 && strcmp(argv[1],"--scalar-metadata")==0;
    int batch=argc==3 && strcmp(argv[1],"--zlib-batch")==0;
    int reference=argc==3 && strcmp(argv[1],"--reference-samples")==0;
    const char *path=argc==2 ? argv[1] : ((metadata || names || hdr || scalar || batch || reference) ? argv[2] : NULL);
    if (path==NULL || !(file=fopen(path,"rb"))) return 2;
#ifdef _WIN32
    _setmode(_fileno(stdout),_O_BINARY);
#endif
    if (batch) { int result=zlib_batch(file);fclose(file);return result; }
    png=png_create_read_struct(PNG_LIBPNG_VER_STRING,NULL,NULL,NULL);
    info=png_create_info_struct(png);
    if (!png || !info) return 3;
    if (setjmp(png_jmpbuf(png))) { png_destroy_read_struct(&png,&info,NULL); fclose(file); return 1; }
    png_init_io(png,file);
    png_set_crc_action(png,PNG_CRC_ERROR_QUIT,PNG_CRC_ERROR_QUIT);
    png_set_user_limits(png,100000,100000);
    png_read_info(png,info);
    width=png_get_image_width(png,info); height=png_get_image_height(png,info);
    if (reference && png_get_sBIT(png,info,&significant)) {
        int color=png_get_color_type(png,info);
        if (color==PNG_COLOR_TYPE_GRAY || color==PNG_COLOR_TYPE_GRAY_ALPHA) {
            original_bits[original_count++]=significant->gray;
        } else {
            original_bits[original_count++]=significant->red;
            original_bits[original_count++]=significant->green;
            original_bits[original_count++]=significant->blue;
        }
        if (color & PNG_COLOR_MASK_ALPHA) original_bits[original_count++]=significant->alpha;
        png_set_shift(png,significant);
    }
    png_set_interlace_handling(png);
    png_read_update_info(png,info);
    stride=png_get_rowbytes(png,info);
    if (height>0 && stride>536870912/height) return 3;
    data=(png_bytep)calloc(height,stride);
    rows=(png_bytepp)malloc(sizeof(*rows)*height);
    if (!data || !rows) return 3;
    for (y=0;y<height;y++) rows[y]=data+y*stride;
    png_read_image(png,rows); png_read_end(png,info);
    if (scalar) {
        scalar_metadata(png,info);
    } else if (hdr) {
        hdr_metadata(png,info);
    } else if (names) {
        named_metadata(png,info);
    } else if (metadata) {
        png_charp name=NULL;
        int compression=0;
        png_bytep icc=NULL,exif=NULL;
        png_uint_32 icc_size=0,exif_size=0;
        (void)png_get_iCCP(png,info,&name,&compression,&icc,&icc_size);
        (void)png_get_eXIf_1(png,info,&exif_size,&exif);
        word(icc_size);word(exif_size);
        if (icc_size && fwrite(icc,1,icc_size,stdout)!=icc_size) return 3;
        if (exif_size && fwrite(exif,1,exif_size,stdout)!=exif_size) return 3;
    } else {
        word(width);word(height);word(png_get_bit_depth(png,info));word(png_get_color_type(png,info));
        if (reference) { word(original_count);blob(original_bits,original_count); }
        if (fwrite(data,stride,height,stdout)!=height) return 3;
    }
    free(rows);free(data);png_destroy_read_struct(&png,&info,NULL);fclose(file);return 0;
}
