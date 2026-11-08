/* Independent test-only libjpeg API client. No reference codec is linked into
 * the Rust crate. The coefficient stream excludes edge MCU padding blocks. */
#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <string.h>
#include <jpeglib.h>

static void u16(FILE *out, unsigned v) { fputc(v & 255,out); fputc((v>>8)&255,out); }
static void u32(FILE *out, unsigned v) {u16(out,v); u16(out,v>>16);}
static FILE *output(const char *prefix,const char *suffix) {
    size_t n=strlen(prefix)+strlen(suffix)+1;
    char *path=(char *)malloc(n);
    FILE *out;
    if(!path) exit(2);
    snprintf(path,n,"%s%s",prefix,suffix);
    out=fopen(path,"wb"); free(path);
    if(!out) exit(2);
    return out;
}
static FILE *open_jpeg(struct jpeg_decompress_struct *c,struct jpeg_error_mgr *e,const char *path) {
    FILE *in=fopen(path,"rb");
    if(!in) exit(2);
    c->err=jpeg_std_error(e); jpeg_create_decompress(c);
    jpeg_stdio_src(c,in); jpeg_read_header(c,TRUE); return in;
}
int main(int argc,char **argv) {
    struct jpeg_decompress_struct c;
    struct jpeg_error_mgr e;
    FILE *in,*out;
    jvirt_barray_ptr *arrays;
    int ci,k;
    if(argc!=3 && !(argc==4 && strcmp(argv[3],"fancy")==0)) return 2;
    in=open_jpeg(&c,&e,argv[1]);
    arrays=jpeg_read_coefficients(&c);
    out=output(argv[2],".coef"); fwrite("JCO1",1,4,out);
    u32(out,c.data_precision);u32(out,c.image_width);u32(out,c.image_height);u32(out,c.num_components);
    for(ci=0;ci<c.num_components;ci++) {
        jpeg_component_info *p=&c.comp_info[ci];
        JDIMENSION y,x;
        u32(out,p->component_id);u32(out,p->h_samp_factor);u32(out,p->v_samp_factor);
        u32(out,p->downsampled_width);u32(out,p->downsampled_height);
        u32(out,p->width_in_blocks);u32(out,p->height_in_blocks);
        for(k=0;k<64;k++) u16(out,p->quant_table->quantval[k]);
        for(y=0;y<p->height_in_blocks;y++) {
            JBLOCKARRAY row=(*c.mem->access_virt_barray)((j_common_ptr)&c,arrays[ci],y,1,FALSE);
            for(x=0;x<p->width_in_blocks;x++) for(k=0;k<64;k++) u16(out,(unsigned short)row[0][x][k]);
        }
    }
    fclose(out);jpeg_finish_decompress(&c);jpeg_destroy_decompress(&c);fclose(in);
    /* Raw component IDCT: no upsampling, no conversion. Float is intentionally
     * independent from the Rust Q20 integer matrix. */
    in=open_jpeg(&c,&e,argv[1]);
    c.raw_data_out=TRUE;c.dct_method=JDCT_FLOAT;
    jpeg_start_decompress(&c);
    {
        unsigned short **planes=(unsigned short **)calloc(c.num_components,sizeof(*planes));
        JSAMPARRAY rows8[MAX_COMPONENTS];
        J12SAMPARRAY rows12[MAX_COMPONENTS];
        JDIMENSION base=0;
        for(ci=0;ci<c.num_components;ci++) {
            jpeg_component_info *p=&c.comp_info[ci];
            planes[ci]=(unsigned short *)calloc((size_t)p->downsampled_width*p->downsampled_height,sizeof(unsigned short));
            if(!planes[ci]) exit(2);
            if(c.data_precision==8) rows8[ci]=(*c.mem->alloc_sarray)((j_common_ptr)&c,JPOOL_IMAGE,p->width_in_blocks*8,p->v_samp_factor*8);
            else rows12[ci]=(J12SAMPARRAY)(*c.mem->alloc_sarray)((j_common_ptr)&c,JPOOL_IMAGE,p->width_in_blocks*8*2,p->v_samp_factor*8);
        }
        while(c.output_scanline<c.output_height) {
            if(c.data_precision==8) jpeg_read_raw_data(&c,rows8,c.max_v_samp_factor*8);
            else jpeg12_read_raw_data(&c,rows12,c.max_v_samp_factor*8);
            for(ci=0;ci<c.num_components;ci++) {
                jpeg_component_info *p=&c.comp_info[ci];
                JDIMENSION y,x,start=base*p->v_samp_factor*8;
                for(y=0;y<(JDIMENSION)p->v_samp_factor*8 && start+y<p->downsampled_height;y++)
                    for(x=0;x<p->downsampled_width;x++) planes[ci][(size_t)(start+y)*p->downsampled_width+x]=c.data_precision==8?rows8[ci][y][x]:rows12[ci][y][x];
            }
            base++;
        }
        out=output(argv[2],".planes");
        for(ci=0;ci<c.num_components;ci++) {
            size_t i,n=(size_t)c.comp_info[ci].downsampled_width*c.comp_info[ci].downsampled_height;
            for(i=0;i<n;i++) u16(out,planes[ci][i]);
            free(planes[ci]);
        }
        free(planes);fclose(out);
    }
    jpeg_finish_decompress(&c);jpeg_destroy_decompress(&c);fclose(in);
    /* Full-resolution output, native precision in LE u16, nearest upsampling.
     * The raw/coefficient gate also covers fractional ratios unsupported by
     * libjpeg's color upsampler; those cases intentionally have no .color file. */
    in=open_jpeg(&c,&e,argv[1]);
    {
        int maxh=1,maxv=1;
        for(ci=0;ci<c.num_components;ci++) {
            if(c.comp_info[ci].h_samp_factor>maxh) maxh=c.comp_info[ci].h_samp_factor;
            if(c.comp_info[ci].v_samp_factor>maxv) maxv=c.comp_info[ci].v_samp_factor;
        }
        for(ci=0;ci<c.num_components;ci++) {
            if(maxh%c.comp_info[ci].h_samp_factor || maxv%c.comp_info[ci].v_samp_factor) {
                jpeg_destroy_decompress(&c);fclose(in);return 0;
            }
        }
    }
    c.dct_method=JDCT_FLOAT;c.do_fancy_upsampling=argc==4?TRUE:FALSE;
    jpeg_start_decompress(&c);
    out=output(argv[2],".color");
    {
        JDIMENSION count=c.output_width*c.output_components,i;
        JSAMPARRAY row=(*c.mem->alloc_sarray)((j_common_ptr)&c,JPOOL_IMAGE,count*(c.data_precision==8?1:2),1);
        while(c.output_scanline<c.output_height) {
            if(c.data_precision==8) jpeg_read_scanlines(&c,row,1);
            else jpeg12_read_scanlines(&c,(J12SAMPARRAY)row,1);
            for(i=0;i<count;i++) u16(out,c.data_precision==8?row[0][i]:((J12SAMPARRAY)row)[0][i]);
        }
    }
    fclose(out);jpeg_finish_decompress(&c);jpeg_destroy_decompress(&c);fclose(in);
    return 0;
}
