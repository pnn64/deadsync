/* Standard library entrypoints, using only the official public Lua 5.1 API. */
#include "lua.h"
#include "lauxlib.h"
#include "lualib.h"
#include <string.h>
#include <stdlib.h>
#ifdef _WIN32
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#endif

static lua_CFunction library_initializer(lua_State *L,int index) {
  const char *name=luaL_checkstring(L,index);
  if(!strcmp(name,"base"))return luaopen_base;
  if(!strcmp(name,"math"))return luaopen_math;
  if(!strcmp(name,"string"))return luaopen_string;
  if(!strcmp(name,"table"))return luaopen_table;
  if(!strcmp(name,"os"))return luaopen_os;
  if(!strcmp(name,"debug"))return luaopen_debug;
  if(!strcmp(name,"package"))return luaopen_package;
  if(!strcmp(name,"io"))return luaopen_io;
  luaL_error(L,"unknown standard library");return NULL;
}
static int library_snapshot(lua_State *L,int results) {
  int top=lua_gettop(L),i;
  lua_pushinteger(L,results);lua_createtable(L,top,1);
  lua_pushinteger(L,top);lua_setfield(L,-2,"n");
  for(i=1;i<=top;i++){lua_pushvalue(L,i);lua_rawseti(L,-2,i);}
  for(i=top-results+1;i<=top;i++)lua_pushvalue(L,i);
  return results+2;
}
static int library_open(lua_State *L) {
  lua_CFunction f=library_initializer(L,1);int results;
  lua_remove(L,1);results=f(L);return library_snapshot(L,results);
}
static int library_all(lua_State *L) {luaL_openlibs(L);return library_snapshot(L,0);}
static int library_standard(lua_State *L) {
  FILE **file=(FILE **)luaL_checkudata(L,1,"FILE*");
  lua_pushboolean(L,lua_objlen(L,1)==sizeof(FILE *));
  lua_pushboolean(L,*file!=NULL);return 2;
}
static int library_registry(lua_State *L) {
  int count=0;lua_pushnil(L);
  while(lua_next(L,LUA_REGISTRYINDEX)){count++;lua_pop(L,1);}
  lua_pushvalue(L,LUA_REGISTRYINDEX);lua_pushinteger(L,count);return 2;
}
static int library_metatable(lua_State *L) {return lua_getmetatable(L,1);}
static int library_environment(lua_State *L) {
  lua_pushstring(L,getenv(luaL_checkstring(L,1)));return 1;
}
static int library_program_directory(lua_State *L) {
#ifdef _WIN32
  char buffer[MAX_PATH+1],*separator;
  DWORD size=sizeof(buffer),length=GetModuleFileNameA(NULL,buffer,size);
  if(!length||length==size||(separator=strrchr(buffer,'\\'))==NULL)
    return luaL_error(L,"cannot inspect program directory");
  *separator='\0';lua_pushstring(L,buffer);
#else
  lua_pushnil(L);
#endif
  return 1;
}
static int library_rawcall(lua_State *L) {
  lua_CFunction f=lua_tocfunction(L,1);int results;
  luaL_argcheck(L,f,1,"C function expected");lua_remove(L,1);
  results=f(L);return library_snapshot(L,results);
}
static int library_open_thread(lua_State *L) {
  lua_State *T=lua_tothread(L,1);lua_CFunction f=library_initializer(L,2);
  int old,top,results,i,count=lua_gettop(L)-2;
  luaL_argcheck(L,T,1,"thread expected");old=lua_gettop(T);
  luaL_checkstack(T,count+16,"initializer arguments");
  for(i=3;i<=count+2;i++){lua_pushvalue(L,i);lua_xmove(L,T,1);}
  results=f(T);top=lua_gettop(T);
  lua_pushinteger(L,results);lua_createtable(L,top-old,1);
  lua_pushinteger(L,top-old);lua_setfield(L,-2,"n");
  for(i=old+1;i<=top;i++){
    lua_pushvalue(T,i);lua_xmove(T,L,1);lua_rawseti(L,-2,i-old);
  }
  for(i=top-results+1;i<=top;i++){lua_pushvalue(T,i);lua_xmove(T,L,1);}
  lua_settop(T,old);return results+2;
}
#ifdef _WIN32
#define LIBRARY_EXPORT __declspec(dllexport)
#else
#define LIBRARY_EXPORT __attribute__((visibility("default")))
#endif
static int library_identity(lua_State *L) {return lua_gettop(L);}
LIBRARY_EXPORT int luaopen_package_probe(lua_State *L) {
  lua_createtable(L,0,3);
  lua_pushvalue(L,1);lua_setfield(L,-2,"name");
  lua_pushvalue(L,LUA_ENVIRONINDEX);lua_setfield(L,-2,"environment");
  lua_pushcfunction(L,library_identity);lua_setfield(L,-2,"identity");return 1;
}
LIBRARY_EXPORT int luaopen_package_probe_child(lua_State *L) {return luaopen_package_probe(L);}
LIBRARY_EXPORT int luaopen_library_initializers(lua_State *L) {
  static const luaL_Reg helpers[]={
    {"library_open",library_open},{"library_rawcall",library_rawcall},{"library_all",library_all},
    {"library_registry",library_registry},{"library_metatable",library_metatable},
    {"library_environment",library_environment},{"library_program_directory",library_program_directory},
    {"library_open_thread",library_open_thread},{"library_standard",library_standard},{NULL,NULL}
  };
  const luaL_Reg *entry;
  for(entry=helpers;entry->name;entry++){
    lua_pushcfunction(L,entry->func);lua_setglobal(L,entry->name);
  }
  return 0;
}

#ifdef LIBRARY_INITIALIZERS_MAIN
#include <stdio.h>
#include <stdlib.h>
#ifdef _WIN32
#include <fcntl.h>
#include <io.h>
#endif
int main(int argc,char **argv) {
  lua_State *L=luaL_newstate();size_t length;const char *text;
  if(!L||argc!=3)return 2;
#ifdef _WIN32
  _setmode(_fileno(stdout),_O_BINARY);
#endif
  luaopen_library_initializers(L);
  lua_pushinteger(L,atoi(argv[2]));lua_setglobal(L,"library_order");
  if(luaL_loadfile(L,argv[1])||lua_pcall(L,0,1,0)){
    fprintf(stderr,"%s\n",lua_tostring(L,-1));lua_close(L);return 1;
  }
  text=lua_tolstring(L,-1,&length);
  if(!text){lua_close(L);return 3;}
  fwrite(text,1,length,stdout);lua_close(L);return 0;
}
#endif
