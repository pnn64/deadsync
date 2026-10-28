/* Public libjpeg API producer; generic component planes, test-only linkage. */
#include <stdio.h>
#include <stdlib.h>
#include <jpeglib.h>
int main(int argc,char **argv) {
    struct jpeg_compress_struct c;struct jpeg_error_mgr e;FILE *out;
    jpeg_scan_info scans[64]={{0}};JSAMPARRAY row;
    int n,p,layout,progressive,reversed,ci,s=0,start,end,blocks,level,al;
    if(argc!=10) return 2;
    n=atoi(argv[2]);p=atoi(argv[3]);layout=atoi(argv[4]);progressive=atoi(argv[8]);reversed=atoi(argv[9]);
    if(n<1||n>MAX_COMPONENTS||(p!=8&&p!=12)||(progressive&&n>4)||layout==1) return 2;
    out=fopen(argv[1],"wb");if(!out) return 2;
    c.err=jpeg_std_error(&e);jpeg_create_compress(&c);jpeg_stdio_dest(&c,out);
    c.image_width=17;c.image_height=19;c.data_precision=p;c.input_components=n;c.in_color_space=JCS_UNKNOWN;
    jpeg_set_defaults(&c);jpeg_set_colorspace(&c,JCS_UNKNOWN);
    jpeg_set_quality(&c,atoi(argv[7]),FALSE);c.dct_method=JDCT_FLOAT;c.optimize_coding=TRUE;
    c.arith_code=atoi(argv[5])?TRUE:FALSE;c.restart_in_rows=atoi(argv[6]);
    for(ci=0;ci<n;ci++) {
        c.comp_info[ci].component_id=(ci*73+251)&255;
        c.comp_info[ci].h_samp_factor=c.comp_info[ci].v_samp_factor=layout==3?4:layout==2&&ci%3==0?2:1;
        c.comp_info[ci].quant_tbl_no=ci%4;
    }
    for(ci=1;ci<4;ci++) {unsigned q[64];int k;for(k=0;k<64;k++)q[k]=1+(ci*31+k*7)%255;jpeg_add_quant_table(&c,ci,q,100,FALSE);}
    for(level=0;level<(progressive?4:1);level++) {
        al=progressive?3-level:0;start=0;
        while(start<n) {
            end=start+1;blocks=c.comp_info[start].h_samp_factor*c.comp_info[start].v_samp_factor;
            while(!reversed&&end<n&&end-start<4) {
                int next=c.comp_info[end].h_samp_factor*c.comp_info[end].v_samp_factor;
                if(blocks+next>10) break;blocks+=next;end++;
            }
            scans[s].comps_in_scan=end-start;
            for(ci=start;ci<end;ci++)scans[s].component_index[ci-start]=reversed?n-1-ci:ci;
            scans[s].Ss=0;scans[s].Se=progressive?0:63;scans[s].Ah=level?al+1:0;scans[s].Al=al;s++;start=end;
        }
        if(progressive)for(ci=0;ci<n;ci++) {
            scans[s].comps_in_scan=1;scans[s].component_index[0]=reversed?n-1-ci:ci;
            scans[s].Ss=1;scans[s].Se=63;scans[s].Ah=level?al+1:0;scans[s].Al=al;s++;
        }
    }
    c.scan_info=scans;c.num_scans=s;jpeg_start_compress(&c,TRUE);
    row=(*c.mem->alloc_sarray)((j_common_ptr)&c,JPOOL_IMAGE,c.image_width*n*(p<=8?1:2),1);
    while(c.next_scanline<c.image_height) {
        unsigned x,y=c.next_scanline,maximum=(1u<<p)-1;
        for(x=0;x<c.image_width*(unsigned)n;x++) {
            unsigned value=((x/n)*193+y*317+(x%n)*911+(x/n)*y*53)&maximum;
            if(p==8)row[0][x]=(JSAMPLE)value;else((J12SAMPARRAY)row)[0][x]=(J12SAMPLE)value;
        }
        if(p==8)jpeg_write_scanlines(&c,row,1);else jpeg12_write_scanlines(&c,(J12SAMPARRAY)row,1);
    }
    jpeg_finish_compress(&c);jpeg_destroy_compress(&c);fclose(out);return 0;
}
