/* Native files retain C stdio buffering; Rust never decodes a FILE layout. */
#include <errno.h>
#ifndef _WIN32
extern int pclose(FILE *);
extern FILE *popen(const char *,const char *);
#endif
#define STDIO_ERROR (errno?errno:-1)
API int boundary_stdio(int op,void *pointer,BoundaryValue *value) {
  FILE *file=(FILE *)pointer;int result=0;
  switch(op) {
    case 0: case 1:
      value->integer=getc(file);
      if(value->integer!=EOF&&op==1)ungetc(value->integer,file);
      return ferror(file)?STDIO_ERROR:0;
    case 2:
      value->length=fread((void *)value->bytes,1,value->length,file);
      return ferror(file)?STDIO_ERROR:0;
    case 3:
      return fwrite(value->bytes,1,value->length,file)==value->length?0:STDIO_ERROR;
    case 4: return fflush(file)==0?0:STDIO_ERROR;
    case 5:
      result=fseek(file,(long)value->wide_integer,value->integer==0?SEEK_SET:value->integer==1?SEEK_CUR:SEEK_END);
      if(result)return STDIO_ERROR;value->wide_integer=(ptrdiff_t)ftell(file);return 0;
    case 6:
      return setvbuf(file,NULL,value->integer==0?_IONBF:value->integer==1?_IOFBF:_IOLBF,value->length)==0?0:STDIO_ERROR;
    case 7: return fclose(file)==0?0:STDIO_ERROR;
    case 8:
#ifdef _WIN32
      result=_pclose(file);
#else
      result=pclose(file);
#endif
      return result!=-1?0:STDIO_ERROR;
    case 9: clearerr(file);return 0;
    case 10:
      value->integer=fscanf(file,"%lf",&value->number)==1;return ferror(file)?STDIO_ERROR:0;
    case 11:
      value->length=(size_t)snprintf((char *)value->bytes,value->length,"file (%p)",pointer);return 0;
    case 12:
      value->integer=fgets((char *)value->bytes,(int)value->length,file)!=NULL;
      value->length=value->integer?strlen(value->bytes):0;
      if(value->length&&value->bytes[value->length-1]=='\n'){value->integer|=2;value->length--;}
      value->wide_integer=ferror(file)?STDIO_ERROR:0;return 0;
    case 13:
      if(!value->bytes)file=tmpfile();
      else if(value->integer) {
#ifdef _WIN32
        file=_popen(value->bytes,(const char *)value->userdata);
#else
        file=popen(value->bytes,(const char *)value->userdata);
#endif
      }
      else file=fopen(value->bytes,(const char *)value->userdata);
      value->userdata=file;return file?0:STDIO_ERROR;
    case 14:
      value->userdata=value->integer==0?stdin:value->integer==1?stdout:stderr;return 0;
    default: return EINVAL;
  }
}
#undef STDIO_ERROR
