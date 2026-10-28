/* Test-only public libjpeg API client. Tables and images are separate files;
 * one codec object is deliberately reused across every stream in a sequence. */
#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <string.h>
#include <jpeglib.h>

static void u16(FILE *out,unsigned v) {fputc(v&255,out);fputc((v>>8)&255,out);}
static void u32(FILE *out,unsigned v) {u16(out,v);u16(out,v>>16);}
static FILE *output(const char *prefix,const char *suffix) {
    size_t n=strlen(prefix)+strlen(suffix)+1;
    char *path=(char *)malloc(n);FILE *file;
    if(!path) exit(2);
    snprintf(path,n,"%s%s",prefix,suffix);file=fopen(path,"wb");free(path);
    if(!file) exit(2);return file;
}
static int decode(int argc,char **argv) {
    struct jpeg_decompress_struct c;struct jpeg_error_mgr e;int item,images=0,tables=0;
    if(argc<4) return 2;
    c.err=jpeg_std_error(&e);jpeg_create_decompress(&c);
    for(item=3;item<argc;item++) {
        FILE *in=fopen(argv[item],"rb"),*out;char suffix[64];int header,ci,k;
        if(!in) return 2;
        jpeg_stdio_src(&c,in);
        for(;;) {
        header=jpeg_read_header(&c,FALSE);
        if(header==JPEG_HEADER_TABLES_ONLY) {tables++;}
        else {
        if(header!=JPEG_HEADER_OK) return 2;
        if(c.Ss>0 && c.Se==0) {
            JSAMPARRAY row;unsigned short *pixels;unsigned w,h,n,p,y,x;
            c.out_color_space=c.jpeg_color_space;jpeg_start_decompress(&c);
            w=c.output_width;h=c.output_height;n=c.output_components;p=c.data_precision;
            pixels=(unsigned short *)calloc((size_t)w*h*n,sizeof(*pixels));if(!pixels) return 2;
            row=(*c.mem->alloc_sarray)((j_common_ptr)&c,JPOOL_IMAGE,w*n*(p<=8?1:2),1);
            for(y=0;y<h;y++) {
                if(p<=8) jpeg_read_scanlines(&c,row,1);
                else if(p<=12) jpeg12_read_scanlines(&c,(J12SAMPARRAY)row,1);
                else jpeg16_read_scanlines(&c,(J16SAMPARRAY)row,1);
                for(x=0;x<w*n;x++) pixels[(size_t)y*w*n+x]=p<=8?row[0][x]:p<=12?((J12SAMPARRAY)row)[0][x]:((J16SAMPARRAY)row)[0][x];
            }
            snprintf(suffix,sizeof(suffix),".image-%d.samples",images);out=output(argv[2],suffix);
            fwrite("JLP1",1,4,out);u32(out,p);u32(out,w);u32(out,h);u32(out,n);
            for(ci=0;ci<(int)n;ci++) {
                jpeg_component_info *s=&c.comp_info[ci];
                u32(out,s->component_id);u32(out,s->h_samp_factor);u32(out,s->v_samp_factor);u32(out,w);u32(out,h);
                for(y=0;y<h;y++) for(x=0;x<w;x++) u16(out,pixels[((size_t)y*w+x)*n+ci]);
            }
            free(pixels);fclose(out);
        } else {
            jvirt_barray_ptr *arrays=jpeg_read_coefficients(&c);
            snprintf(suffix,sizeof(suffix),".image-%d.coef",images);out=output(argv[2],suffix);
            fwrite("JCO1",1,4,out);u32(out,c.data_precision);u32(out,c.image_width);u32(out,c.image_height);u32(out,c.num_components);
            for(ci=0;ci<c.num_components;ci++) {
                jpeg_component_info *s=&c.comp_info[ci];JDIMENSION y,x;
                u32(out,s->component_id);u32(out,s->h_samp_factor);u32(out,s->v_samp_factor);
                u32(out,s->downsampled_width);u32(out,s->downsampled_height);u32(out,s->width_in_blocks);u32(out,s->height_in_blocks);
                for(k=0;k<64;k++) u16(out,s->quant_table->quantval[k]);
                for(y=0;y<s->height_in_blocks;y++) {
                    JBLOCKARRAY row=(*c.mem->access_virt_barray)((j_common_ptr)&c,arrays[ci],y,1,FALSE);
                    for(x=0;x<s->width_in_blocks;x++) for(k=0;k<64;k++) u16(out,(unsigned short)row[0][x][k]);
                }
            }
            fclose(out);
        }
        jpeg_finish_decompress(&c);images++;
        }
        if(c.src->bytes_in_buffer==0) {int next=fgetc(in);if(next==EOF) break;ungetc(next,in);}
        }
        fclose(in);
    }
    jpeg_destroy_decompress(&c);
    printf("tables %d images %d\n",tables,images);return 0;
}
static J_COLOR_SPACE color(const char *name) {
    if(!strcmp(name,"gray")) return JCS_GRAYSCALE;
    if(!strcmp(name,"rgb")) return JCS_RGB;
    if(!strcmp(name,"ycbcr")) return JCS_YCbCr;
    if(!strcmp(name,"cmyk")) return JCS_CMYK;
    if(!strcmp(name,"ycck")) return JCS_YCCK;
    return JCS_UNKNOWN;
}
static int encode(int argc,char **argv,int reset) {
    struct jpeg_compress_struct c;struct jpeg_error_mgr e;FILE *out;
    int p,n,lossless,repeat,repeats,i;J_COLOR_SPACE space;char suffix[64];
    if(argc!=14) return 2;
    p=atoi(argv[3]);space=color(argv[4]);lossless=atoi(argv[8]);repeats=atoi(argv[13]);
    if(space==JCS_UNKNOWN || p<2 || p>16 || (!lossless && p!=8 && p!=12) || (lossless && atoi(argv[7]))) return 2;
    n=space==JCS_GRAYSCALE?1:(space==JCS_CMYK || space==JCS_YCCK)?4:3;
    c.err=jpeg_std_error(&e);jpeg_create_compress(&c);
    c.image_width=31;c.image_height=25;c.data_precision=p;c.input_components=n;c.in_color_space=space;
    /* Lossless start_compress reinitializes components. YCbCr/YCCK defaults
     * select Huffman table 1, while the lossless SOS writer emits selector 0.
     * Use generic native components (all table 0) and explicit Adobe tags. */
    if(lossless && (space==JCS_YCbCr || space==JCS_YCCK)) c.in_color_space=JCS_UNKNOWN;
    jpeg_set_defaults(&c);jpeg_set_colorspace(&c,space);
    jpeg_set_quality(&c,atoi(argv[5]),TRUE);c.dct_method=JDCT_FLOAT;
    c.arith_code=atoi(argv[7])?TRUE:FALSE;c.optimize_coding=p==12?TRUE:FALSE;
    for(i=0;i<n;i++) {c.comp_info[i].h_samp_factor=1;c.comp_info[i].v_samp_factor=1;}
    if(!lossless && n>1 && strcmp(argv[12],"444")) {
        int h=2,v=!strcmp(argv[12],"420")?2:1;
        c.comp_info[0].h_samp_factor=h;c.comp_info[0].v_samp_factor=v;
        if(space==JCS_YCCK) {c.comp_info[3].h_samp_factor=h;c.comp_info[3].v_samp_factor=v;}
    }
    if(lossless) {
        jpeg_enable_lossless(&c,atoi(argv[9]),atoi(argv[10]));c.optimize_coding=TRUE;
    }
    else if(atoi(argv[6])) jpeg_simple_progression(&c);
    c.restart_in_rows=atoi(argv[11]);
    out=output(argv[2],".tables.jpg");jpeg_stdio_dest(&c,out);jpeg_write_tables(&c);fclose(out);
    for(repeat=0;repeat<repeats;repeat++) {
        JSAMPARRAY row;unsigned maximum=(1u<<p)-1;char comment[64];
        if(reset) {
            if(!c.arith_code || lossless) return 2;
            for(i=0;i<NUM_ARITH_TBLS;i++) {
                c.arith_dc_L[i]=repeat==0?3:0;c.arith_dc_U[i]=repeat==0?8:1;c.arith_ac_K[i]=repeat==0?31:5;
            }
            c.restart_in_rows=repeat==0?1:0;c.restart_interval=0;
        }
        snprintf(suffix,sizeof(suffix),".image-%d.jpg",repeat);out=output(argv[2],suffix);jpeg_stdio_dest(&c,out);
        jpeg_start_compress(&c,FALSE);
        /* start_compress resets lossless colorspace/component defaults.
         * T.81 B.2.2 requires Tq=0; set it after that reset and before the
         * first scanline triggers frame-header emission. No quantizer is
         * used by the predictive lossless entropy or sample path. */
        if(lossless) for(i=0;i<n;i++) c.comp_info[i].quant_tbl_no=0;
        if(lossless && (space==JCS_YCbCr || space==JCS_YCCK)) {
            unsigned char adobe[12]={'A','d','o','b','e',0,100,0,0,0,0,0};
            adobe[11]=space==JCS_YCCK?2:1;
            jpeg_write_marker(&c,JPEG_APP0+14,adobe,12);
        }
        snprintf(comment,sizeof(comment),"abbreviated image %d",repeat);
        jpeg_write_marker(&c,JPEG_COM,(const JOCTET *)comment,(unsigned)strlen(comment));
        row=(*c.mem->alloc_sarray)((j_common_ptr)&c,JPOOL_IMAGE,c.image_width*n*(p<=8?1:2),1);
        while(c.next_scanline<c.image_height) {
            unsigned x,y=c.next_scanline,channel;
            for(x=0;x<c.image_width;x++) for(channel=0;channel<(unsigned)n;channel++) {
                unsigned value=(x*193+y*317+channel*911+x*y*53+repeat*977+repeat*x*y*7)&maximum;
                unsigned j=x*n+channel;
                if(p<=8) row[0][j]=(JSAMPLE)value;
                else if(p<=12) ((J12SAMPARRAY)row)[0][j]=(J12SAMPLE)value;
                else ((J16SAMPARRAY)row)[0][j]=(J16SAMPLE)value;
            }
            if(p<=8) jpeg_write_scanlines(&c,row,1);
            else if(p<=12) jpeg12_write_scanlines(&c,(J12SAMPARRAY)row,1);
            else jpeg16_write_scanlines(&c,(J16SAMPARRAY)row,1);
        }
        jpeg_finish_compress(&c);fclose(out);
    }
    jpeg_destroy_compress(&c);return 0;
}
int main(int argc,char **argv) {
    if(argc<2) return 2;
    if(!strcmp(argv[1],"decode")) return decode(argc,argv);
    if(!strcmp(argv[1],"decode-list")) {
        char **args;char line[2048];int count=3,result,i;FILE *list;
        if(argc!=4) return 2;
        list=fopen(argv[3],"rb");if(!list) return 2;
        args=(char **)calloc(8192,sizeof(*args));if(!args) return 2;
        args[0]=argv[0];args[1]="decode";args[2]=argv[2];
        while(fgets(line,sizeof(line),list)) {
            size_t length=strcspn(line,"\r\n");line[length]=0;
            if(!length) continue;
            if(count==8192) return 2;
            args[count]=(char *)malloc(length+1);if(!args[count]) return 2;
            memcpy(args[count++],line,length+1);
        }
        fclose(list);result=decode(count,args);
        for(i=3;i<count;i++) free(args[i]);free(args);return result;
    }
    if(!strcmp(argv[1],"encode")) return encode(argc,argv,0);
    if(!strcmp(argv[1],"encode-reset")) return encode(argc,argv,1);
    return 2;
}
