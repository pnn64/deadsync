/* Public C checks for hash-capacity overflow, without huge live allocations. */
#include <limits.h>
static int table_limit(lua_State *L) {
  int records=(int)lua_tointeger(L,1);lua_createtable(L,0,records);return 1;
}
static const char *table_limit_reader(lua_State *L,void *data,size_t *length) {
  (void)data;(void)length;lua_createtable(L,0,INT_MAX);return NULL;
}
static void table_limit_checks(void) {
  const int sizes[]={ (1<<26)+1,1<<27,INT_MAX };
  const char *names[]={"table.limit.first","table.limit.power","table.limit.intmax"};
  lua_State *L=luaL_newstate(),*child;int i,status;CHECK(L);
  for(i=0;i<3;++i) {
    lua_pushcfunction(L,table_limit);lua_pushinteger(L,sizes[i]);
    status=lua_pcall(L,1,0,0);memory_result(L,names[i],status);lua_settop(L,0);
  }
  memory_handler_calls=0;
  lua_pushcfunction(L,memory_handler);lua_pushcfunction(L,table_limit);lua_pushinteger(L,INT_MAX);
  status=lua_pcall(L,1,0,1);memory_result(L,"table.limit.handler",status);
  number("table.limit.handler.calls",memory_handler_calls);lua_settop(L,0);
  status=lua_load(L,table_limit_reader,NULL,"=table-limit-reader");
  memory_result(L,"table.limit.reader",status);lua_settop(L,0);
  child=lua_newthread(L);lua_pushcfunction(child,table_limit);lua_pushinteger(child,INT_MAX);
  number("table.limit.coroutine.resume",lua_resume(child,1));
  number("table.limit.coroutine.status",lua_status(child));
  string("table.limit.coroutine.value",lua_tostring(child,-1));lua_settop(L,0);
  lua_createtable(L,8,8);lua_pushinteger(L,42);lua_rawseti(L,1,3);lua_rawgeti(L,1,3);
  number("table.limit.recovery",lua_tointeger(L,-1));lua_pop(L,1);
  number("table.limit.recovery.top",lua_gettop(L));lua_close(L);
}
static int table_limit_panic(lua_State *L) {
  number("table.limit.panic.status",lua_status(L));number("table.limit.panic.top",lua_gettop(L));
  string("table.limit.panic.value",lua_tostring(L,-1));return 0;
}
static int table_limit_child_panic(void) {
  lua_State *L=luaL_newstate();CHECK(L);lua_atpanic(L,table_limit_panic);
  L=lua_newthread(L);lua_createtable(L,0,INT_MAX);return 99;
}
