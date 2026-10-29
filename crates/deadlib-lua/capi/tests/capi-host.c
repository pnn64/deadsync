/* A C executable using Lua's public headers and the standalone shared library. */
#include "../../tests/native-default/native-owned.c"
#ifdef __linux__
#include <sys/resource.h>
#endif
static unsigned initializer_mask;
static void initializer_hook(lua_State *L,lua_Debug *ar) {
  lua_CFunction f;
  if(ar->event!=LUA_HOOKCALL)return;
  CHECK(lua_getinfo(L,"f",ar));f=lua_tocfunction(L,-1);lua_pop(L,1);
  if(f==luaopen_base)initializer_mask|=1;
  if(f==luaopen_package)initializer_mask|=2;
  if(f==luaopen_table)initializer_mask|=4;
  if(f==luaopen_io)initializer_mask|=8;
  if(f==luaopen_os)initializer_mask|=16;
  if(f==luaopen_string)initializer_mask|=32;
  if(f==luaopen_math)initializer_mask|=64;
  if(f==luaopen_debug)initializer_mask|=128;
}
static void abi_checks(const char *module) {
  lua_State *L=luaL_newstate();CHECK(L);
  lua_sethook(L,initializer_hook,LUA_MASKCALL,0);luaL_openlibs(L);lua_sethook(L,NULL,0,0);
  number("abi.initializers",initializer_mask);
  lua_pushcfunction(L,lua_error);
  number("abi.function.pointer",lua_tocfunction(L,-1)==lua_error);
  lua_pushliteral(L,"ABI error");
  number("abi.error.status",lua_pcall(L,1,0,0));
  string("abi.error.value",lua_tostring(L,-1));lua_settop(L,0);
  lua_pushfstring(L,"%s:%d:%f:%c:%%","fmt",42,1.5,'Z');
  string("abi.varargs",lua_tostring(L,-1));
  lua_settop(L,0);
  if(module) {
    lua_getglobal(L,"package");lua_getfield(L,-1,"loadlib");
    lua_pushstring(L,module);lua_pushliteral(L,"luaopen_capi_probe");
    CHECK(lua_pcall(L,2,1,0)==0 && lua_isfunction(L,-1));lua_call(L,0,1);
    lua_getfield(L,-1,"sum");lua_pushinteger(L,17);lua_pushinteger(L,25);lua_call(L,2,1);
    number("module.sum",lua_tointeger(L,-1));lua_pop(L,1);
    lua_getfield(L,-1,"fail");number("module.error.status",lua_pcall(L,0,0,0));
    string("module.error.value",lua_tostring(L,-1));
  }
  lua_close(L);
}
#ifdef CAPI_DIAGNOSTICS
#ifdef _WIN32
#include <windows.h>
#else
#include <dlfcn.h>
#endif
static void diagnostics(void) {
  typedef size_t (*Get)(size_t *,size_t);
  size_t values[8];Get get;
#ifdef _WIN32
  get=(Get)GetProcAddress(GetModuleHandleA("lua51.dll"),"deadlua_engine_statistics");
#else
  get=(Get)dlsym(RTLD_DEFAULT,"deadlua_engine_statistics");
#endif
  CHECK(get && get(values,8)==8);
  CHECK(values[0]==values[1] && values[2]==values[3] && values[4]==values[5] && values[6]==values[7]);
  fprintf(stderr,"CAPI operations=%lu states=%lu groups=%lu libraries=%lu\n",
    (unsigned long)values[4],(unsigned long)values[0],(unsigned long)values[2],(unsigned long)values[6]);
}
#endif
int main(int argc,char **argv) {
#ifdef __linux__
  /* Match the existing Rust runner's reserve for deep C callback recursion. */
  struct rlimit stack;
  CHECK(getrlimit(RLIMIT_STACK,&stack)==0);
  if(stack.rlim_cur<32*1024*1024) {
    stack.rlim_cur=32*1024*1024;
    CHECK(setrlimit(RLIMIT_STACK,&stack)==0);
  }
#endif
  /* The host and DLL can have distinct static CRT buffers on Windows. Keep
   * the host's observations visible when a DLL panic terminates the process. */
  setvbuf(stdout,NULL,_IONBF,0);
  if(argc>1) {
    if(strcmp(argv[1],"owned_default_panic")==0)return owned_default_panic();
    if(strcmp(argv[1],"owned_returning_panic")==0)return owned_returning_panic();
    CHECK(argc==3 && strcmp(argv[1],"--module")==0);
  }
  abi_checks(argc==3?argv[2]:NULL);
  {
    int first=owned_first(),second=owned_second();
    printf("program.first\t%d\nprogram.second\t%d\n",first,second);
  }
#ifdef CAPI_DIAGNOSTICS
  diagnostics();
#endif
  return 0;
}
