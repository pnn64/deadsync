/* Test-only native-sample libpng oracle. No sample transformations are enabled.
 * Emits width/height/depth/color (all u32 BE), then packed, big-endian scanlines.
 * libpng's setjmp error boundary stays entirely within this C process. */
#include <png.h>
#include <stdio.h>
#include <stdlib.h>
#ifdef _WIN32
#include <fcntl.h>
#include <io.h>
#endif
static void word(unsigned int n) {
    unsigned char bytes[4]={(unsigned char)(n>>24),(unsigned char)(n>>16),(unsigned char)(n>>8),(unsigned char)n};
    if (fwrite(bytes,1,4,stdout)!=4) exit(3);
}
int main(int argc,char **argv) {
    FILE *file;
    png_structp png;
    png_infop info;
    png_bytep data;
    png_bytepp rows;
    png_uint_32 width,height,y;
    size_t stride;
    if (argc!=2 || !(file=fopen(argv[1],"rb"))) return 2;
#ifdef _WIN32
    _setmode(_fileno(stdout),_O_BINARY);
#endif
    png=png_create_read_struct(PNG_LIBPNG_VER_STRING,NULL,NULL,NULL);
    info=png_create_info_struct(png);
    if (!png || !info) return 3;
    if (setjmp(png_jmpbuf(png))) { png_destroy_read_struct(&png,&info,NULL); fclose(file); return 1; }
    png_init_io(png,file);
    png_set_crc_action(png,PNG_CRC_ERROR_QUIT,PNG_CRC_ERROR_QUIT);
    png_set_user_limits(png,100000,100000);
    png_read_info(png,info);
    width=png_get_image_width(png,info); height=png_get_image_height(png,info);
    png_set_interlace_handling(png);
    png_read_update_info(png,info);
    stride=png_get_rowbytes(png,info);
    if (height>0 && stride>536870912/height) return 3;
    data=(png_bytep)calloc(height,stride);
    rows=(png_bytepp)malloc(sizeof(*rows)*height);
    if (!data || !rows) return 3;
    for (y=0;y<height;y++) rows[y]=data+y*stride;
    png_read_image(png,rows); png_read_end(png,info);
    word(width);word(height);word(png_get_bit_depth(png,info));word(png_get_color_type(png,info));
    if (fwrite(data,stride,height,stdout)!=height) return 3;
    free(rows);free(data);png_destroy_read_struct(&png,&info,NULL);fclose(file);return 0;
}
