/* Recoverable allocation errors, using only Lua 5.1's public C API. */
static int memory_handler_calls;
static int memory_oom(lua_State *L) {
  lua_newuserdata(L,(size_t)-256);return 1;
}
static int memory_waiting(lua_State *L) {
  lua_State *parent;
  lua_getglobal(L,"memory_parent");parent=lua_tothread(L,-1);CHECK(parent);
  lua_newuserdata(parent,(size_t)-256);return 0;
}
/* Reproduce the default public-header userdata layout to probe both sides of
 * PUC's overflow guard without depending on private Lua headers. */
typedef union MemoryHeader {
  LUAI_USER_ALIGNMENT_T alignment;
  struct { void *next;unsigned char tag,mark;void *metatable,*env;size_t length; } fields;
} MemoryHeader;
static int memory_edge(lua_State *L) {
  lua_newuserdata(L,*(size_t *)lua_touserdata(L,1));return 1;
}
static int memory_too_big(lua_State *L) {
  lua_newuserdata(L,(size_t)-1);return 1;
}
static int memory_handler(lua_State *L) {
  ++memory_handler_calls;lua_pushliteral(L,"handled");return 1;
}
static int memory_raise(lua_State *L) {
  lua_pushliteral(L,"not enough memory");return lua_error(L);
}
static int memory_call(lua_State *L) {
  lua_pushcfunction(L,memory_oom);lua_call(L,0,0);return 0;
}
static int memory_nested(lua_State *L) {
  lua_pushcfunction(L,memory_oom);
  number("memory.nested.inner",lua_pcall(L,0,0,0));
  lua_settop(L,0);lua_pushinteger(L,42);return 1;
}
static const char *memory_reader(lua_State *L,void *data,size_t *length) {
  (void)data;(void)length;memory_oom(L);return NULL;
}
static int memory_load(lua_State *L) {
  int status=lua_load(L,memory_reader,NULL,"=memory-reader");
  lua_pushinteger(L,status);lua_insert(L,-2);return 2;
}
static void memory_result(lua_State *L,const char *label,int status) {
  char name[100];
  snprintf(name,sizeof(name),"%s.status",label);number(name,status);
  snprintf(name,sizeof(name),"%s.top",label);number(name,lua_gettop(L));
  snprintf(name,sizeof(name),"%s.value",label);string(name,lua_tostring(L,-1));
}
static void memory_checks(void) {
  lua_State *L=luaL_newstate(),*child;int status;CHECK(L);luaL_openlibs(L);
  lua_pushcfunction(L,memory_oom);lua_setglobal(L,"memory_oom");
  lua_pushcfunction(L,memory_load);lua_setglobal(L,"memory_load");
  lua_pushcfunction(L,memory_waiting);lua_setglobal(L,"memory_waiting");
  lua_pushthread(L);lua_setglobal(L,"memory_parent");
  lua_pushcfunction(L,memory_oom);status=lua_pcall(L,0,0,0);
  memory_result(L,"memory.pcall",status);lua_settop(L,0);
  lua_pushcfunction(L,memory_handler);lua_pushcfunction(L,memory_oom);
  status=lua_pcall(L,0,0,1);memory_result(L,"memory.handler",status);
  number("memory.handler.calls",memory_handler_calls);lua_settop(L,0);
  lua_pushinteger(L,17);lua_pushcfunction(L,memory_oom);
  status=lua_pcall(L,0,0,1);memory_result(L,"memory.invalid-handler",status);lua_settop(L,0);
  lua_pushcfunction(L,memory_oom);lua_pushcfunction(L,memory_raise);
  status=lua_pcall(L,0,0,1);memory_result(L,"memory.handler-oom",status);lua_settop(L,0);
  lua_pushcfunction(L,memory_handler);lua_pushcfunction(L,memory_raise);
  status=lua_pcall(L,0,0,1);memory_result(L,"memory.ordinary-string",status);
  number("memory.ordinary-string.calls",memory_handler_calls);lua_settop(L,0);
  lua_pushcfunction(L,memory_too_big);status=lua_pcall(L,0,0,0);
  memory_result(L,"memory.too-big",status);lua_settop(L,0);
  {
    size_t edge=(size_t)-3-sizeof(MemoryHeader);
    lua_pushcfunction(L,memory_edge);lua_pushlightuserdata(L,&edge);
    status=lua_pcall(L,1,0,0);memory_result(L,"memory.largest-block",status);lua_settop(L,0);
    ++edge;lua_pushcfunction(L,memory_edge);lua_pushlightuserdata(L,&edge);
    status=lua_pcall(L,1,0,0);memory_result(L,"memory.first-overflow",status);lua_settop(L,0);
  }
  lua_pushcfunction(L,memory_call);status=lua_pcall(L,0,0,0);
  memory_result(L,"memory.call",status);lua_settop(L,0);
  lua_pushcfunction(L,memory_nested);status=lua_pcall(L,0,1,0);
  memory_result(L,"memory.nested",status);lua_settop(L,0);
  status=lua_cpcall(L,memory_oom,NULL);memory_result(L,"memory.cpcall",status);lua_settop(L,0);
  status=lua_load(L,memory_reader,NULL,"=memory-reader");
  memory_result(L,"memory.load",status);lua_settop(L,0);
  child=lua_newthread(L);lua_pushcfunction(child,memory_oom);
  status=lua_resume(child,0);number("memory.coroutine.resume",status);
  number("memory.coroutine.status",lua_status(child));string("memory.coroutine.value",lua_tostring(child,-1));
  number("memory.coroutine.again",lua_resume(child,0));
  number("memory.coroutine.after",lua_status(child));lua_settop(L,0);
  status=luaL_dostring(L,
    "local calls=0;local function h(e) calls=calls+1;return 'handled' end;"
    "local ok,e=xpcall(memory_oom,h);assert(not ok and e=='not enough memory' and calls==0);"
    "local status,msg;ok,e=xpcall(function()status,msg=memory_load()end,h);"
    "assert(ok and status==4 and msg=='not enough memory' and calls==0, tostring(ok)..'/'..tostring(status)..'/'..tostring(msg)..'/'..tostring(calls));"
    "local co=coroutine.create(function()memory_oom()end);ok,e=coroutine.resume(co);"
    "assert(not ok and e=='not enough memory' and coroutine.status(co)=='dead');"
    "co=coroutine.create(function()memory_waiting()end);"
    "ok,e=xpcall(function()coroutine.resume(co)end,h);"
    "assert(not ok and e=='not enough memory' and calls==0);"
    "local u=newproxy(true);getmetatable(u).__gc=memory_oom;u=nil;"
    "ok,e=pcall(collectgarbage,'collect');assert(not ok and e=='not enough memory');"
    "local saved;ok,e=pcall(function() local x={n=42};saved=function()return x.n end;memory_oom()end);"
    "assert(not ok and e=='not enough memory' and saved()==42);"
    "collectgarbage('collect');ok,e=pcall(memory_oom);assert(not ok and e=='not enough memory');"
    "return 42");
  if(status){fprintf(stderr,"MEMORY_SCRIPT %s\n",lua_tostring(L,-1));exit(87);}
  number("memory.lua.recovery",lua_tointeger(L,-1));lua_settop(L,0);
  lua_newuserdata(L,16);number("memory.recovery.bytes",lua_objlen(L,-1));
  lua_close(L);
}
static int memory_panic(lua_State *L) {
  number("memory.panic.status",lua_status(L));number("memory.panic.top",lua_gettop(L));
  string("memory.panic.value",lua_tostring(L,-1));return 0;
}
static int memory_panic_probe(int callback,int child) {
  lua_State *L=luaL_newstate();CHECK(L);
  if(callback)lua_atpanic(L,memory_panic);
  if(child)L=lua_newthread(L);
  memory_oom(L);return 99;
}
