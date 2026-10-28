/* Linked into each API image so FILE ownership uses that image's CRT. */
#ifndef _WIN32
#define _LARGEFILE64_SOURCE
#define _POSIX_C_SOURCE 200809L
#endif
#include "lua.h"
#include "lauxlib.h"
#include <stdio.h>
#include <string.h>
#ifndef _WIN32
#include <unistd.h>
#endif
#ifdef _WIN32
#define FILE_EXPORT __declspec(dllexport)
#else
#define FILE_EXPORT __attribute__((visibility("default")))
#endif
static FILE **file_arg(lua_State *L) {return (FILE **)luaL_checkudata(L,1,"FILE*");}
static FILE **new_file_environment(lua_State *L,const char *entry) {
  FILE **file=(FILE **)lua_newuserdata(L,sizeof(FILE *));int index=lua_gettop(L);*file=NULL;
  luaL_getmetatable(L,"FILE*");lua_setmetatable(L,index);
  lua_getglobal(L,"io");lua_getfield(L,-1,entry);lua_getfenv(L,-1);lua_setfenv(L,index);lua_pop(L,2);return file;
}
static FILE **new_file(lua_State *L) {return new_file_environment(L,"tmpfile");}
static FILE *open_data(lua_State *L,int index) {
  size_t size;const char *bytes=luaL_checklstring(L,index,&size);FILE *file=tmpfile();
  if(!file)luaL_error(L,"cannot create native file");
  if(fwrite(bytes,1,size,file)!=size||fseek(file,0,SEEK_SET)){
    fclose(file);luaL_error(L,"cannot prepare native file");
  }
  return file;
}
static int file_new(lua_State *L) {FILE **file=new_file(L);*file=open_data(L,1);return 1;}
static int file_closed(lua_State *L) {new_file(L);return 1;}
static int file_open(lua_State *L) {
  const char *path=luaL_checkstring(L,1),*mode=luaL_checkstring(L,2);FILE **file=new_file(L);
  *file=fopen(path,mode);if(!*file)return luaL_error(L,"native fopen failed");return 1;
}
static int file_pipe(lua_State *L) {
  const char *command=luaL_checkstring(L,1);FILE **file=new_file_environment(L,"popen");
#ifdef _WIN32
  *file=_popen(command,"r");
#else
  *file=popen(command,"r");
#endif
  if(!*file)return luaL_error(L,"native popen failed");return 1;
}
static int file_large(lua_State *L) {
#ifdef _WIN32
  lua_pushnil(L);return 1;
#else
  FILE **file=new_file(L);*file=tmpfile64();
  if(!*file||ftruncate64(fileno(*file),(off64_t)2147483664LL))return luaL_error(L,"native sparse file failed");
  return 1;
#endif
}
static int file_read(lua_State *L) {
  char buffer[4096];FILE *file=*file_arg(L);int count=luaL_checkint(L,2);size_t read;
  luaL_argcheck(L,count>=0&&count<=(int)sizeof(buffer),2,"invalid read size");
  luaL_argcheck(L,file!=NULL,1,"closed file");read=fread(buffer,1,(size_t)count,file);
  lua_pushlstring(L,buffer,read);return 1;
}
static int file_write(lua_State *L) {
  FILE *file=*file_arg(L);size_t size;const char *bytes=luaL_checklstring(L,2,&size);
  luaL_argcheck(L,file!=NULL,1,"closed file");lua_pushboolean(L,fwrite(bytes,1,size,file)==size);return 1;
}
static int file_seek(lua_State *L) {
  FILE *file=*file_arg(L);long offset=(long)luaL_checkinteger(L,2);int mode=luaL_optint(L,3,SEEK_SET);
  luaL_argcheck(L,file!=NULL,1,"closed file");
  if(fseek(file,offset,mode))return luaL_error(L,"native seek failed");
  lua_pushnumber(L,(lua_Number)ftell(file));return 1;
}
static int file_close(lua_State *L) {
  FILE **file=file_arg(L);int result;
  luaL_argcheck(L,*file!=NULL,1,"closed file");result=fclose(*file);*file=NULL;
  lua_pushboolean(L,result==0);return 1;
}
static int file_replace(lua_State *L) {
  FILE **file=file_arg(L);FILE *fresh=open_data(L,2);
  if(*file)fclose(*file);*file=fresh;return 0;
}
static int file_take(lua_State *L) {FILE **file=file_arg(L);lua_pushlightuserdata(L,*file);*file=NULL;return 1;}
static int file_wrap(lua_State *L) {FILE *pointer=(FILE *)lua_touserdata(L,1);*new_file(L)=pointer;return 1;}
static int file_state(lua_State *L) {
  FILE *file=*file_arg(L);lua_pushboolean(L,file!=NULL);
  if(file)lua_pushfstring(L,"file (%p)",file);else lua_pushliteral(L,"file (closed)");return 2;
}
FILE_EXPORT int luaopen_native_files(lua_State *L) {
  static const luaL_Reg methods[]={
    {"new",file_new},{"closed",file_closed},{"open",file_open},{"read",file_read},{"write",file_write},
    {"pipe",file_pipe},{"large",file_large},
    {"seek",file_seek},{"close",file_close},{"replace",file_replace},
    {"take",file_take},{"wrap",file_wrap},{"state",file_state},{NULL,NULL}
  };
  lua_newtable(L);luaL_register(L,NULL,methods);return 1;
}
#ifdef NATIVE_FILES_BOOTSTRAP
#include "lualib.h"
FILE_EXPORT int luaopen_library_initializers(lua_State *L) {
  luaL_openlibs(L);luaopen_native_files(L);lua_setglobal(L,"native_files");return 0;
}
#endif
#ifdef NATIVE_FILES_MAIN
#include "lualib.h"
#ifdef _WIN32
#include <fcntl.h>
#include <io.h>
#endif
int main(int argc,char **argv) {
  lua_State *L=luaL_newstate();const char *text;size_t length;
  if(!L||argc!=2)return 2;
#ifdef _WIN32
  _setmode(_fileno(stdout),_O_BINARY);
#endif
  luaL_openlibs(L);luaopen_native_files(L);lua_setglobal(L,"native_files");
  lua_pushstring(L,argv[1]);lua_setglobal(L,"native_files_source");
  if(luaL_loadfile(L,argv[1])||lua_pcall(L,0,1,0)){
    fprintf(stderr,"%s\n",lua_tostring(L,-1));lua_close(L);return 1;
  }
  text=lua_tolstring(L,-1,&length);if(!text){lua_close(L);return 3;}
  fwrite(text,1,length,stdout);lua_close(L);return 0;
}
#endif
