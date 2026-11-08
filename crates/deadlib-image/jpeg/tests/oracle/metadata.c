/* Test-only independent libjpeg marker/ICC reader. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <jpeglib.h>
static FILE *output(const char *prefix,const char *suffix) {
    size_t n=strlen(prefix)+strlen(suffix)+1;
    char *name=(char *)malloc(n);FILE *out;
    if(!name) exit(2);snprintf(name,n,"%s%s",prefix,suffix);out=fopen(name,"wb");free(name);if(!out) exit(2);return out;
}
static void word(FILE *out,unsigned n) {int i;for(i=0;i<4;i++) fputc((n>>(8*i))&255,out);}
int main(int argc,char **argv) {
    struct jpeg_decompress_struct c;struct jpeg_error_mgr e;
    unsigned char *profile=NULL;unsigned profile_len=0;int valid,i;FILE *in,*out;
    jpeg_saved_marker_ptr marker;
    if(argc!=3) return 2;
    in=fopen(argv[1],"rb");if(!in) return 2;
    c.err=jpeg_std_error(&e);jpeg_create_decompress(&c);
    for(i=0;i<16;i++) jpeg_save_markers(&c,JPEG_APP0+i,65535);
    jpeg_save_markers(&c,JPEG_COM,65535);
    jpeg_stdio_src(&c,in);jpeg_read_header(&c,TRUE);
    valid=jpeg_read_icc_profile(&c,&profile,&profile_len);
    if(valid) {out=output(argv[2],".icc");fwrite(profile,1,profile_len,out);fclose(out);free(profile);}
    out=output(argv[2],".markers");
    for(marker=c.marker_list;marker;marker=marker->next) {
        fputc(marker->marker,out);word(out,marker->data_length);fwrite(marker->data,1,marker->data_length,out);
    }
    fclose(out);out=output(argv[2],".json");
    fprintf(out,"{\"jfif\":%s,\"unit\":%u,\"x_density\":%u,\"y_density\":%u,\"icc\":%s,\"icc_length\":%u,\"adobe\":",
        c.saw_JFIF_marker?"true":"false",c.density_unit,c.X_density,c.Y_density,valid?"true":"false",valid?profile_len:0);
    if(c.saw_Adobe_marker) fprintf(out,"%u",c.Adobe_transform);else fprintf(out,"null");
    {
        const char *space="Unknown";
        switch(c.jpeg_color_space) {
            case JCS_GRAYSCALE:space="Grayscale";break;case JCS_RGB:space="Rgb";break;
            case JCS_YCbCr:space="YCbCr";break;case JCS_CMYK:space="Cmyk";break;
            case JCS_YCCK:space="Ycck";break;default:break;
        }
        fprintf(out,",\"color_space\":\"%s\"}\n",space);
    }
    fclose(out);jpeg_destroy_decompress(&c);fclose(in);return 0;
}
