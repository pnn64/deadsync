/* A public-header embedding program. The phases run directly on base_ci,
 * rather than as lua_CFunction activations. */
#include "lua.h"
#include "lauxlib.h"
#include "lualib.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#if defined(_WIN32)
#define EXPORT __declspec(dllexport)
#else
#define EXPORT __attribute__((visibility("default")))
#endif

#define CHECK(e) do { if (!(e)) { fprintf(stderr,"ROOT_CHECK %d: %s\n",__LINE__,#e); exit(87); } } while (0)
static lua_State *saved;
static int finalized, hook_calls, hook_bad, recursive_calls;
static char transcript[8192];
static size_t transcript_length;
static void string(const char *name, const char *value) {
  int count=snprintf(transcript+transcript_length,sizeof(transcript)-transcript_length,
                     "%s\t%s\n",name,value);
  CHECK(count>0 && (size_t)count<sizeof(transcript)-transcript_length);
  transcript_length+=(size_t)count;
  printf("%s\t%s\n",name,value);
}
static void number(const char *name, int value) {
  char bytes[32];snprintf(bytes,sizeof(bytes),"%d",value);string(name,bytes);
}
static int callback(lua_State *L) {
  lua_Debug ar;
  CHECK(lua_getstack(L,0,&ar));
  CHECK(!lua_getstack(L,1,&ar));
  CHECK(lua_type(L,lua_upvalueindex(1))==LUA_TTABLE);
  lua_getfield(L,lua_upvalueindex(1),"kept");
  return 1;
}
static int finalizer(lua_State *L) {
  lua_Debug ar;
  CHECK(lua_getstack(L,0,&ar));
  CHECK(!lua_getstack(L,1,&ar));
  finalized++;
  return 0;
}
static void hook(lua_State *L, lua_Debug *ar) {
  int top=lua_gettop(L);
  lua_Debug parent;
  hook_calls++;
  if (ar->event!=LUA_HOOKLINE || !lua_getinfo(L,"Sl",ar) ||
      lua_getstack(L,1,&parent)) hook_bad++;
  lua_pushliteral(L,"hook temporary");
  lua_settop(L,top);
}
typedef struct { const char *text; int next; } Reader;
static const char *reader(lua_State *L, void *ud, size_t *length) {
  Reader *r=(Reader *)ud;
  lua_Debug ar;
  CHECK(!lua_getstack(L,0,&ar));
  if (r->next++) { *length=0; return NULL; }
  *length=strlen(r->text);
  return r->text;
}
typedef struct { char *data; size_t length; int calls; } Buffer;
static int writer(lua_State *L, const void *data, size_t length, void *ud) {
  Buffer *b=(Buffer *)ud;
  lua_Debug ar;
  CHECK(!lua_getstack(L,0,&ar));
  b->data=(char *)realloc(b->data,b->length+length);
  CHECK(b->data!=NULL);
  memcpy(b->data+b->length,data,length);
  b->length+=length;
  b->calls++;
  return 0;
}
static int protected_callback(lua_State *L) {
  lua_Debug ar;
  CHECK(lua_getstack(L,0,&ar));
  CHECK(!lua_getstack(L,1,&ar));
  lua_pushliteral(L,"protected marker");
  return lua_error(L);
}
static int recursive(lua_State *L) {
  recursive_calls++;
  lua_pushcfunction(L,recursive);
  lua_call(L,0,0);
  return 0;
}

EXPORT int root_first(lua_State *L) {
  lua_Debug ar;
  lua_CFunction abs_body;
  Reader r={"assert(debug.getinfo(2)==nil);return 17,nil,false",0};
  Buffer b={NULL,0,0};
  lua_State *child;
  int depth;
  saved=L;
  number("initial.top",lua_gettop(L));
  number("initial.frame",lua_getstack(L,0,&ar));
  number("initial.thread",lua_pushthread(L)); lua_pop(L,1);
  luaL_openlibs(L);
  number("openlibs.top",lua_gettop(L));
  lua_newtable(L);
  lua_pushliteral(L,"rooted value");lua_setfield(L,1,"kept");
  lua_pushvalue(L,1);lua_pushcclosure(L,callback,1);
  lua_pushvalue(L,1);lua_setfield(L,LUA_REGISTRYINDEX,"root-table");
  number("persistent.top",lua_gettop(L));
  CHECK(lua_load(L,reader,&r,"=root-reader")==0);
  number("reader.top",lua_gettop(L));
  number("dump.status",lua_dump(L,writer,&b));
  number("dump.nonempty",b.length>0 && b.calls>0);
  number("call.status",lua_pcall(L,0,LUA_MULTRET,0));
  number("call.top",lua_gettop(L));
  number("call.number",lua_tointeger(L,3));
  string("call.nil",lua_typename(L,lua_type(L,4)));
  number("call.false",lua_toboolean(L,5));
  lua_settop(L,2);
  CHECK(luaL_loadbuffer(L,b.data,b.length,"=root-binary")==0);
  free(b.data);
  lua_call(L,0,1);number("binary.result",lua_tointeger(L,-1));lua_pop(L,1);
  CHECK(luaL_loadstring(L,"local =")!=0);
  number("syntax.string",lua_isstring(L,-1));lua_pop(L,1);
  for (depth=195;depth<=201;depth++) {
    char text[512],label[64];
    memcpy(text,"return ",7);
    memset(text+7,'(',depth);text[7+depth]='1';
    memset(text+8+depth,')',depth);
    snprintf(label,sizeof(label),"parser.depth.%d",depth);
    number(label,luaL_loadbuffer(L,text,8+2*depth,"=root-depth"));lua_pop(L,1);
  }
  lua_pushcfunction(L,protected_callback);
  number("protected.status",lua_pcall(L,0,0,0));
  string("protected.value",lua_tostring(L,-1));lua_pop(L,1);
  lua_pushcfunction(L,recursive);
  number("recursive.status",lua_pcall(L,0,0,0));
  number("recursive.calls",recursive_calls);
  string("recursive.value",lua_tostring(L,-1));lua_pop(L,1);
  lua_getglobal(L,"math");lua_getfield(L,-1,"abs");
  abs_body=lua_tocfunction(L,-1);CHECK(abs_body!=NULL);lua_pop(L,2);
  /* A raw builtin body uses the root window as its arguments. */
  lua_settop(L,0);lua_pushinteger(L,-42);
  number("raw.abs.count",abs_body(L));
  number("raw.abs.value",lua_tointeger(L,-1));
  lua_settop(L,0);
  lua_getfield(L,LUA_REGISTRYINDEX,"root-table");
  lua_pushvalue(L,1);lua_pushcclosure(L,callback,1);
  CHECK(luaL_loadstring(L,"local n=17\nn=n+25\nreturn n") == 0);
  CHECK(lua_sethook(L,hook,LUA_MASKLINE,0));
  lua_call(L,0,1);
  lua_sethook(L,NULL,0,0);
  number("hook.calls",hook_calls);number("hook.bad",hook_bad);
  number("hook.result",lua_tointeger(L,-1));lua_pop(L,1);
  child=lua_newthread(L);
  CHECK(luaL_loadstring(child,"coroutine.yield(17);return 42")==0);
  number("child.first.status",lua_resume(child,0));
  number("child.first.top",lua_gettop(child));
  number("child.first.value",lua_tointeger(child,1));
  lua_setfield(L,LUA_REGISTRYINDEX,"root-child");
  lua_newuserdata(L,1);lua_newtable(L);
  lua_pushcfunction(L,finalizer);lua_setfield(L,-2,"__gc");
  lua_setmetatable(L,-2);lua_pop(L,1);
  number("gc.status",lua_gc(L,LUA_GCCOLLECT,0));
  number("gc.finalized",finalized);
  number("first.top",lua_gettop(L));
  number("first.frame",lua_getstack(L,0,&ar));
  fflush(stdout);
  return 517;
}

EXPORT int root_second(lua_State *L) {
  lua_Debug ar;
  lua_State *child;
  number("second.same.state",L==saved);
  number("second.top",lua_gettop(L));
  number("second.frame",lua_getstack(L,0,&ar));
  lua_getfield(L,1,"kept");string("second.table",lua_tostring(L,-1));lua_pop(L,1);
  lua_pushvalue(L,2);lua_call(L,0,1);
  string("second.callback",lua_tostring(L,-1));lua_pop(L,1);
  lua_getfield(L,LUA_REGISTRYINDEX,"root-table");
  number("second.registry",lua_rawequal(L,1,-1));lua_pop(L,1);
  lua_getfield(L,LUA_REGISTRYINDEX,"root-child");child=lua_tothread(L,-1);CHECK(child!=NULL);
  lua_settop(child,0);
  lua_setlevel(L,child);
  number("child.second.status",lua_resume(child,0));
  number("child.second.top",lua_gettop(child));
  number("child.second.value",lua_tointeger(child,1));lua_pop(L,1);
  lua_pushliteral(L,"a");lua_pushinteger(L,42);lua_concat(L,2);
  string("second.concat",lua_tostring(L,-1));lua_pop(L,1);
  CHECK(luaL_loadstring(L,"return debug.getinfo(2)==nil,coroutine.running()==nil") == 0);
  lua_call(L,0,2);
  number("second.lua.no.parent",lua_toboolean(L,-2));
  number("second.lua.main",lua_toboolean(L,-1));lua_pop(L,2);
  number("second.final.top",lua_gettop(L));
  fflush(stdout);
  return -37;
}

EXPORT int root_snapshot(lua_State *L) {
  lua_pushlstring(L,transcript,transcript_length);
  return 0;
}

EXPORT int root_error(lua_State *L) {
  lua_pushliteral(L,"unprotected root error");
  return lua_error(L);
}

#ifdef NATIVE_ROOT_MAIN
int main(void) {
  lua_State *L=luaL_newstate();
  CHECK(L!=NULL);
  number("first.return",root_first(L));
  lua_gc(L,LUA_GCCOLLECT,0);
  number("second.return",root_second(L));
  lua_close(L);
  return 0;
}
#endif
