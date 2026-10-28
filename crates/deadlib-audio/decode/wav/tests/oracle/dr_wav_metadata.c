#define DR_WAV_IMPLEMENTATION
#include "dr_wav.h"
#include <stdio.h>
#include <string.h>
static void hex(const void *data, size_t size) {
    const unsigned char *p=(const unsigned char *)data;
    size_t i;
    if (!size) {printf("-");return;}
    for (i=0;i<size;++i) printf("%02x",p[i]);
}
static void strhex(const char *p) {hex(p,p?strlen(p):0);}
int main(int argc,char **argv) {
    drwav wav; drwav_uint32 i,j;
    if (argc!=2||!drwav_init_file_with_metadata(&wav,argv[1],0,NULL)) return 2;
    for (i=0;i<wav.metadataCount;++i) {
        drwav_metadata *m=&wav.pMetadata[i];
        if (m->type==drwav_metadata_type_bext) {
            drwav_bext *b=&m->data.bext;
            printf("bext %u %llu ",b->version,(unsigned long long)b->timeReference);
            strhex(b->pDescription);printf(" ");strhex(b->pOriginatorName);printf(" ");strhex(b->pOriginatorReference);printf(" ");hex(b->pOriginationDate,10);printf(" ");hex(b->pOriginationTime,8);printf(" ");
            if (b->version>=1) hex(b->pUMID,64);else printf("-");printf(" ");
            if (b->version>=2) printf("%d,%d,%d,%d,%d",(drwav_int16)b->loudnessValue,(drwav_int16)b->loudnessRange,(drwav_int16)b->maxTruePeakLevel,(drwav_int16)b->maxMomentaryLoudness,(drwav_int16)b->maxShortTermLoudness);else printf("-");
            printf(" ");hex(b->pCodingHistory,b->codingHistorySize);printf("\n");
        } else if (m->type==drwav_metadata_type_cue) {
            for (j=0;j<m->data.cue.cuePointCount;++j) {drwav_cue_point *p=&m->data.cue.pCuePoints[j];printf("cue %u %u ",p->id,p->playOrderPosition);hex(p->dataChunkId,4);printf(" %u %u %u\n",p->chunkStart,p->blockStart,p->sampleOffset);}
        } else if (m->type==drwav_metadata_type_smpl) {
            drwav_smpl *s=&m->data.smpl;
            printf("smpl %u %u %u %u %u %u %u %u ",s->manufacturerId,s->productId,s->samplePeriodNanoseconds,s->midiUnityNote,s->midiPitchFraction,s->smpteFormat,s->smpteOffset,s->sampleLoopCount);hex(s->pSamplerSpecificData,s->samplerSpecificDataSizeInBytes);printf("\n");
            for (j=0;j<s->sampleLoopCount;++j) {drwav_smpl_loop *p=&s->pLoops[j];printf("loop %u %u %u %u %u %u\n",p->cuePointId,p->type,p->firstSampleOffset,p->lastSampleOffset,p->sampleFraction,p->playCount);}
        } else if (m->type==drwav_metadata_type_list_info_title||m->type==drwav_metadata_type_list_info_artist) {
            printf("info %s ",m->type==drwav_metadata_type_list_info_title?"INAM":"IART");hex(m->data.infoText.pString,m->data.infoText.stringLength);printf("\n");
        } else if (m->type==drwav_metadata_type_list_label||m->type==drwav_metadata_type_list_note) {
            printf("%s %u ",m->type==drwav_metadata_type_list_label?"labl":"note",m->data.labelOrNote.cuePointId);hex(m->data.labelOrNote.pString,m->data.labelOrNote.stringLength);printf("\n");
        } else if (m->type==drwav_metadata_type_list_labelled_cue_region) {
            drwav_list_labelled_cue_region *p=&m->data.labelledCueRegion;
            printf("ltxt %u %u ",p->cuePointId,p->sampleLength);hex(p->purposeId,4);printf(" %u %u %u %u ",p->country,p->language,p->dialect,p->codePage);hex(p->pString,p->stringLength);printf("\n");
        }
    }
    drwav_uninit(&wav);return 0;
}
