/* Test-only libjpeg CMYK/YCCK encoder. Input is complemented CMYK, LE u16. */
#include <stdio.h>
#include <stdlib.h>
#include <jpeglib.h>
int main(int argc,char **argv) {
    struct jpeg_compress_struct c;struct jpeg_error_mgr e;
    FILE *in,*out;int i,precision,ycck,progressive;JSAMPARRAY row;
    if(argc!=17) return 2;
    precision=atoi(argv[5]);ycck=atoi(argv[7]);progressive=atoi(argv[8]);
    in=fopen(argv[1],"rb");out=fopen(argv[2],"wb");if(!in || !out) return 2;
    c.err=jpeg_std_error(&e);jpeg_create_compress(&c);jpeg_stdio_dest(&c,out);
    c.image_width=atoi(argv[3]);c.image_height=atoi(argv[4]);c.input_components=4;
    c.in_color_space=JCS_CMYK;c.data_precision=precision;jpeg_set_defaults(&c);
    jpeg_set_colorspace(&c,ycck?JCS_YCCK:JCS_CMYK);jpeg_set_quality(&c,atoi(argv[6]),TRUE);
    c.dct_method=JDCT_FLOAT;c.optimize_coding=TRUE;c.restart_interval=3;
    for(i=0;i<4;i++) {c.comp_info[i].h_samp_factor=atoi(argv[9+2*i]);c.comp_info[i].v_samp_factor=atoi(argv[10+2*i]);}
    if(progressive) jpeg_simple_progression(&c);
    jpeg_start_compress(&c,TRUE);
    row=(*c.mem->alloc_sarray)((j_common_ptr)&c,JPOOL_IMAGE,c.image_width*4*(precision==8?1:2),1);
    while(c.next_scanline<c.image_height) {
        unsigned j;
        for(j=0;j<c.image_width*4;j++) {
            int lo=fgetc(in),hi=fgetc(in);unsigned value;
            if(lo==EOF || hi==EOF) return 2;value=(unsigned)lo|((unsigned)hi<<8);
            if(precision==8) row[0][j]=(JSAMPLE)value;else ((J12SAMPARRAY)row)[0][j]=(J12SAMPLE)value;
        }
        if(precision==8) jpeg_write_scanlines(&c,row,1);else jpeg12_write_scanlines(&c,(J12SAMPARRAY)row,1);
    }
    jpeg_finish_compress(&c);jpeg_destroy_compress(&c);fclose(in);fclose(out);return 0;
}
