/* Public IJG coefficient API. No pixel conversion or Rust codec linkage. */
#include <stdio.h>
#include <stdlib.h>
#include <jpeglib.h>
static void u16(FILE *out,unsigned v){fputc(v&255,out);fputc((v>>8)&255,out);}
static void u32(FILE *out,unsigned v){u16(out,v);u16(out,v>>16);}
int main(int argc,char **argv) {
    struct jpeg_decompress_struct c;struct jpeg_error_mgr e;FILE *in,*out;
    jvirt_barray_ptr *arrays;int ci,k;
    if(argc!=3&&argc!=4)return 2;
    c.err=jpeg_std_error(&e);jpeg_create_decompress(&c);
    if(argc==4) {
        in=fopen(argv[1],"rb");if(!in)return 2;
        jpeg_stdio_src(&c,in);
        if(jpeg_read_header(&c,FALSE)!=JPEG_HEADER_TABLES_ONLY)return 2;
        fclose(in);
    }
    in=fopen(argv[argc-2],"rb");out=fopen(argv[argc-1],"wb");if(!in||!out)return 2;
    jpeg_stdio_src(&c,in);jpeg_read_header(&c,TRUE);
    arrays=jpeg_read_coefficients(&c);fwrite("JCO1",1,4,out);
    u32(out,c.data_precision);u32(out,c.image_width);u32(out,c.image_height);u32(out,c.num_components);
    for(ci=0;ci<c.num_components;ci++) {
        jpeg_component_info *s=&c.comp_info[ci];JDIMENSION y,x;
        u32(out,s->component_id);u32(out,s->h_samp_factor);u32(out,s->v_samp_factor);
        u32(out,s->downsampled_width);u32(out,s->downsampled_height);u32(out,s->width_in_blocks);u32(out,s->height_in_blocks);
        for(k=0;k<64;k++)u16(out,s->quant_table->quantval[k]);
        for(y=0;y<s->height_in_blocks;y++) {
            JBLOCKARRAY row=(*c.mem->access_virt_barray)((j_common_ptr)&c,arrays[ci],y,1,FALSE);
            for(x=0;x<s->width_in_blocks;x++)for(k=0;k<64;k++)u16(out,(unsigned short)row[0][x][k]);
        }
    }
    fclose(out);jpeg_finish_decompress(&c);jpeg_destroy_decompress(&c);fclose(in);return 0;
}
