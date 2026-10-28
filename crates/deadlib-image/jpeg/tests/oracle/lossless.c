/* Test-only public libjpeg lossless sample API client. No production linkage. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <jpeglib.h>
static void u32(FILE *f,unsigned v) { int i;for(i=0;i<4;i++) fputc((v>>(8*i))&255,f); }
static void u16(FILE *f,unsigned v) { fputc(v&255,f);fputc((v>>8)&255,f); }
static int encode(int argc,char **argv) {
    struct jpeg_compress_struct c;struct jpeg_error_mgr e;FILE *in,*out;
    jpeg_scan_info scans[MAX_COMPONENTS]={{0}};JSAMPARRAY row;int i,p,n,psv,pt,layout;
    if(argc!=12) return 2;
    in=fopen(argv[2],"rb");out=fopen(argv[3],"wb");if(!in || !out) return 2;
    p=atoi(argv[6]);n=atoi(argv[7]);psv=atoi(argv[8]);pt=atoi(argv[9]);layout=atoi(argv[11]);
    if(n<1 || n>MAX_COMPONENTS) return 2;
    c.err=jpeg_std_error(&e);jpeg_create_compress(&c);jpeg_stdio_dest(&c,out);
    c.image_width=atoi(argv[4]);c.image_height=atoi(argv[5]);c.data_precision=p;c.input_components=n;
    c.in_color_space=n==1?JCS_GRAYSCALE:n==3?JCS_RGB:n==4?JCS_CMYK:JCS_UNKNOWN;
    jpeg_set_defaults(&c);jpeg_set_colorspace(&c,c.in_color_space);
    jpeg_enable_lossless(&c,psv,pt);c.optimize_coding=TRUE;c.restart_interval=c.image_width*atoi(argv[10]);
    c.write_JFIF_header=FALSE;
    if(layout) {
        for(i=0;i<n;i++) {
            scans[i].comps_in_scan=1;scans[i].component_index[0]=i;
            scans[i].Ss=layout==2?(psv-1+i)%7+1:psv;scans[i].Al=layout==2?(pt+i)%p:pt;
        }
        c.scan_info=scans;c.num_scans=n;
    }
    jpeg_start_compress(&c,TRUE);
    row=(*c.mem->alloc_sarray)((j_common_ptr)&c,JPOOL_IMAGE,c.image_width*n*(p<=8?1:2),1);
    while(c.next_scanline<c.image_height) {
        unsigned j;
        for(j=0;j<c.image_width*(unsigned)n;j++) {
            int lo=fgetc(in),hi=fgetc(in);unsigned v;if(lo==EOF || hi==EOF) return 2;
            v=(unsigned)lo|((unsigned)hi<<8);
            if(p<=8) row[0][j]=(JSAMPLE)v;
            else if(p<=12) ((J12SAMPARRAY)row)[0][j]=(J12SAMPLE)v;
            else ((J16SAMPARRAY)row)[0][j]=(J16SAMPLE)v;
        }
        if(p<=8) jpeg_write_scanlines(&c,row,1);
        else if(p<=12) jpeg12_write_scanlines(&c,(J12SAMPARRAY)row,1);
        else jpeg16_write_scanlines(&c,(J16SAMPARRAY)row,1);
    }
    jpeg_finish_compress(&c);jpeg_destroy_compress(&c);fclose(in);fclose(out);return 0;
}
static int decode(int argc,char **argv) {
    struct jpeg_decompress_struct c;struct jpeg_error_mgr e;FILE *in,*out;
    JSAMPARRAY row;unsigned short *pixels;unsigned y,x,ci,w,h,n,p;
    if(argc!=4) return 2;
    in=fopen(argv[2],"rb");out=fopen(argv[3],"wb");if(!in || !out) return 2;
    c.err=jpeg_std_error(&e);jpeg_create_decompress(&c);jpeg_stdio_src(&c,in);jpeg_read_header(&c,TRUE);
    c.out_color_space=c.jpeg_color_space;jpeg_start_decompress(&c);
    w=c.output_width;h=c.output_height;n=c.output_components;p=c.data_precision;
    pixels=(unsigned short *)calloc((size_t)w*h*n,sizeof(unsigned short));if(!pixels) return 2;
    row=(*c.mem->alloc_sarray)((j_common_ptr)&c,JPOOL_IMAGE,w*n*(p<=8?1:2),1);
    for(y=0;y<h;y++) {
        if(p<=8) jpeg_read_scanlines(&c,row,1);
        else if(p<=12) jpeg12_read_scanlines(&c,(J12SAMPARRAY)row,1);
        else jpeg16_read_scanlines(&c,(J16SAMPARRAY)row,1);
        for(x=0;x<w*n;x++) pixels[((size_t)y*w*n)+x]=p<=8?row[0][x]:p<=12?((J12SAMPARRAY)row)[0][x]:((J16SAMPARRAY)row)[0][x];
    }
    fwrite("JLP1",1,4,out);u32(out,p);u32(out,w);u32(out,h);u32(out,n);
    for(ci=0;ci<n;ci++) {
        jpeg_component_info *component=&c.comp_info[ci];
        u32(out,component->component_id);u32(out,component->h_samp_factor);u32(out,component->v_samp_factor);
        u32(out,w);u32(out,h);
        for(y=0;y<h;y++) for(x=0;x<w;x++) u16(out,pixels[((size_t)y*w+x)*n+ci]);
    }
    free(pixels);jpeg_finish_decompress(&c);jpeg_destroy_decompress(&c);fclose(in);fclose(out);return 0;
}
int main(int argc,char **argv) {
    if(argc<2) return 2;
    if(!strcmp(argv[1],"encode")) return encode(argc,argv);
    if(!strcmp(argv[1],"decode")) return decode(argc,argv);
    return 2;
}
