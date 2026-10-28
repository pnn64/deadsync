/* Test-only arithmetic coefficient transcoder using the public libjpeg API. */
#include <stdio.h>
#include <stdlib.h>
#include <jpeglib.h>
int main(int argc,char **argv) {
    struct jpeg_decompress_struct d;struct jpeg_compress_struct c;
    struct jpeg_error_mgr de,ce;FILE *in,*out;jvirt_barray_ptr *blocks;int i,layout;
    jpeg_scan_info scans[128]={{0}};
    if(argc!=9) return 2;
    in=fopen(argv[1],"rb");out=fopen(argv[2],"wb");if(!in || !out) return 2;
    d.err=jpeg_std_error(&de);jpeg_create_decompress(&d);jpeg_stdio_src(&d,in);
    jpeg_read_header(&d,TRUE);blocks=jpeg_read_coefficients(&d);
    c.err=jpeg_std_error(&ce);jpeg_create_compress(&c);jpeg_stdio_dest(&c,out);
    jpeg_copy_critical_parameters(&d,&c);c.arith_code=TRUE;
    c.restart_interval=atoi(argv[3]);layout=atoi(argv[7]);
    for(i=0;i<NUM_ARITH_TBLS;i++) {
        c.arith_dc_L[i]=atoi(argv[4]);c.arith_dc_U[i]=atoi(argv[5]);
        c.arith_ac_K[i]=atoi(argv[6]);
    }
    /* Exercise all four legal table destinations, shared and independent. */
    for(i=0;i<c.num_components;i++) {
        c.comp_info[i].dc_tbl_no=layout==0?0:(i+layout)%4;
        c.comp_info[i].ac_tbl_no=layout==0?0:(3-i+layout)%4;
    }
    if(atoi(argv[8])==1) jpeg_simple_progression(&c);
    if(atoi(argv[8])==2) {
        if(c.num_components>4) return 2;
        for(i=0;i<c.num_components;i++) {
            scans[i].comps_in_scan=1;scans[i].component_index[0]=i;scans[i].Se=63;
        }
        c.scan_info=scans;c.num_scans=c.num_components;
    }
    if(atoi(argv[8])==3) {
        int al,component,band,count=0,maximum=c.data_precision==12?13:10;
        if(c.num_components>4) return 2;
        for(al=maximum;al>=0;al--) {
            jpeg_scan_info *scan=&scans[count++];
            scan->comps_in_scan=c.num_components;
            for(i=0;i<c.num_components;i++) scan->component_index[i]=i;
            scan->Ah=al==maximum?0:al+1;scan->Al=al;
            for(component=c.num_components-1;component>=0;component--) {
                for(band=0;band<2;band++) {
                    scan=&scans[count++];scan->comps_in_scan=1;scan->component_index[0]=component;
                    scan->Ss=band==0?6:1;scan->Se=band==0?63:5;
                    scan->Ah=al==maximum?0:al+1;scan->Al=al;
                }
            }
        }
        c.scan_info=scans;c.num_scans=count;
    }
    jpeg_write_coefficients(&c,blocks);jpeg_finish_compress(&c);
    jpeg_finish_decompress(&d);jpeg_destroy_compress(&c);jpeg_destroy_decompress(&d);
    fclose(in);fclose(out);return 0;
}
