/* Built unchanged against official Lua or the development bridge. */
#ifdef REFERENCE
#include "lua.h"
#include "lauxlib.h"
#else
#include "api.h"
#endif
#define EXPORT __declspec(dllexport)
#include <stdint.h>
#include <string.h>
#include <stdio.h>

static int scalar_index(lua_State *L, int index) {
  double number = lua_tonumber(L, index);
  lua_Integer integer = lua_tointeger(L, index);
  int before = lua_type(L, index);
  int isnumber = lua_isnumber(L, index), isstring = lua_isstring(L, index);
  int isuserdata = lua_isuserdata(L, index), iscfunction = lua_iscfunction(L, index);
  int boolean = lua_toboolean(L, index), after = lua_type(L, index);
  const char *name = lua_typename(L, before);
  lua_createtable(L, 0, 10);
  lua_pushnumber(L, before); lua_setfield(L, -2, "type");
  lua_pushnumber(L, after); lua_setfield(L, -2, "after");
  lua_pushstring(L, name); lua_setfield(L, -2, "name");
  if (number != number) lua_pushstring(L, "nan");
  else lua_pushlstring(L, (const char *)&number, sizeof(number));
  lua_setfield(L, -2, "number");
  lua_pushlstring(L, (const char *)&integer, sizeof(integer)); lua_setfield(L, -2, "integer");
  lua_pushboolean(L, isnumber); lua_setfield(L, -2, "isnumber");
  lua_pushboolean(L, isstring); lua_setfield(L, -2, "isstring");
  lua_pushboolean(L, isuserdata); lua_setfield(L, -2, "isuserdata");
  lua_pushboolean(L, iscfunction); lua_setfield(L, -2, "iscfunction");
  lua_pushboolean(L, boolean); lua_setfield(L, -2, "boolean");
  return 1;
}
EXPORT int boundary_scalar(lua_State *L) { return scalar_index(L, 1); }
EXPORT int boundary_scalar_null(lua_State *L) {
  lua_pushlightuserdata(L, NULL); return scalar_index(L, lua_gettop(L));
}
static int scalar_capture(lua_State *L) { return scalar_index(L, lua_upvalueindex(1)); }
EXPORT int boundary_scalar_capture(lua_State *L) {
  lua_pushvalue(L, 1); lua_pushcclosure(L, scalar_capture, 1); return 1;
}
EXPORT int boundary_integers(lua_State *L) {
  static const lua_Integer values[] = {0, 1, -1, 2147483647, (lua_Integer)2147483648LL,
    ((lua_Integer)1<<53)-1, ((lua_Integer)1<<53)+1, -(((lua_Integer)1<<53)+1), PTRDIFF_MIN, PTRDIFF_MAX};
  size_t i;
  lua_createtable(L, (int)(sizeof(values)/sizeof(values[0])), 0);
  for (i=0; i<sizeof(values)/sizeof(values[0]); i++) {
    double number; lua_Integer integer;
    lua_pushinteger(L, values[i]);
    number = lua_tonumber(L, -1); integer = lua_tointeger(L, -1);
    lua_settop(L, -2);
    lua_createtable(L, 0, 3);
    lua_pushlstring(L, (const char *)&values[i], sizeof(lua_Integer)); lua_setfield(L, -2, "source");
    lua_pushlstring(L, (const char *)&number, sizeof(number)); lua_setfield(L, -2, "number");
    lua_pushlstring(L, (const char *)&integer, sizeof(integer)); lua_setfield(L, -2, "integer");
    lua_rawseti(L, -2, (int)i+1);
  }
  lua_pushstring(L, NULL); lua_pushstring(L, "a\0ignored");
  return 3;
}
EXPORT int boundary_compare(lua_State *L) {
  int top = lua_gettop(L), mode = (int)lua_tointeger(L, 3);
  int left = top >= 4 ? (int)lua_tointeger(L, 4) : 1;
  int right = top >= 5 ? (int)lua_tointeger(L, 5) : 2;
  int result = mode ? lua_lessthan(L, left, right) : lua_equal(L, left, right);
  lua_pushboolean(L, result); lua_pushboolean(L, lua_gettop(L) == top + 1);
  return 2;
}
EXPORT int boundary_concat(lua_State *L) {
  lua_concat(L, lua_gettop(L)); return lua_gettop(L);
}
EXPORT int boundary_concat_prefix(lua_State *L) {
  lua_concat(L, lua_gettop(L) - 1); return lua_gettop(L);
}

typedef struct Payload {
  double number;
  size_t visits;
  char binary[6];
} Payload;

EXPORT int boundary_userdata(lua_State *L) {
  double number = luaL_checknumber(L, 1);
  Payload *payload = (Payload *)lua_newuserdata(L, sizeof(Payload));
  memset(payload, 0, sizeof(*payload));
  payload->number = number;
  memcpy(payload->binary, "a\0b\r\n\xff", 6);
  if (lua_type(L, 2) == 5) {
    lua_pushvalue(L, 2); lua_setmetatable(L, -2);
  }
  lua_pushlightuserdata(L, payload);
  return 2;
}
EXPORT int boundary_userdata_read(lua_State *L) {
  Payload *payload = (Payload *)lua_touserdata(L, 1);
  if (lua_type(L, 1) != 7 || lua_objlen(L, 1) != sizeof(Payload)) return 0;
  if (!payload || (uintptr_t)payload % __alignof(Payload)) return 0;
  payload->number += luaL_checknumber(L, 2);
  payload->visits++;
  lua_pushnumber(L, payload->number);
  lua_pushnumber(L, (double)payload->visits);
  lua_pushlstring(L, payload->binary, sizeof(payload->binary));
  lua_pushboolean(L, payload == lua_touserdata(L, 3));
  return 4;
}
EXPORT int boundary_userdata_finalize(lua_State *L) {
  Payload *payload = (Payload *)lua_touserdata(L, 1);
  double count;
  lua_getfenv(L, 1);
  lua_getfield(L, -1, "finalized");
  count = luaL_checknumber(L, -1); lua_settop(L, -2);
  lua_pushnumber(L, count + 1); lua_setfield(L, -2, "finalized");
  lua_pushnumber(L, payload->number); lua_setfield(L, -2, "last_value");
  return 0;
}
EXPORT int boundary_bytes(lua_State *L) {
  size_t length = 999, again = 999;
  const char *bytes = lua_tolstring(L, 1, &length);
  const char *after;
  int kind = lua_type(L, 1);
  if (lua_type(L, 2) == 6) {
    lua_pushvalue(L, 2); lua_call(L, 0, 0);
  }
  after = lua_tolstring(L, 1, &again);
  if (bytes) lua_pushlstring(L, bytes, length); else lua_pushnil(L);
  lua_pushnumber(L, (double)length);
  lua_pushnumber(L, kind);
  lua_pushboolean(L, bytes == after && length == again && (!bytes || bytes[length] == 0));
  return 4;
}
EXPORT int boundary_environment(lua_State *L) {
  int supported = -1;
  if (lua_type(L, 2) == 5) {
    lua_pushvalue(L, 2); supported = lua_setfenv(L, 1);
  }
  lua_getfenv(L, 1);
  if (supported < 0) lua_pushnil(L); else lua_pushboolean(L, supported);
  return 2;
}
static int capture_bytes(lua_State *L) {
  size_t length;
  int before;
  const char *bytes;
  if (lua_gettop(L)) {
    lua_pushvalue(L, 1); lua_replace(L, lua_upvalueindex(1));
  }
  before = lua_type(L, lua_upvalueindex(1));
  bytes = lua_tolstring(L, lua_upvalueindex(1), &length);
  if (bytes) lua_pushlstring(L, bytes, length); else lua_pushnil(L);
  lua_pushnumber(L, before);
  lua_pushnumber(L, lua_type(L, lua_upvalueindex(1)));
  return 3;
}
EXPORT int boundary_capture_bytes(lua_State *L) {
  lua_pushvalue(L, 1); lua_pushcclosure(L, capture_bytes, 1); return 1;
}
EXPORT int boundary_length(lua_State *L) {
  size_t length = lua_objlen(L, 1);
  lua_pushnumber(L, (double)length);
  lua_pushnumber(L, lua_type(L, 1));
  return 2;
}
EXPORT int boundary_userdata_empty(lua_State *L) {
  void *first = lua_newuserdata(L, 0);
  void *second = lua_newuserdata(L, 0);
  lua_pushboolean(L, first && second && first != second);
  return 3;
}

static int protected_call(lua_State *L, int negative) {
  int results = (int)luaL_checknumber(L, 3);
  int arguments = lua_gettop(L) - 3;
  int handler = lua_type(L, 1) == 0 ? 0 : (negative ? -lua_gettop(L) + 1 : 1);
  int status;
  lua_remove(L, 3);
  status = lua_pcall(L, arguments, results, handler);
  lua_pushnumber(L, status); lua_insert(L, 2);
  return lua_gettop(L) - 1;
}
EXPORT int boundary_protected(lua_State *L) { return protected_call(L, 0); }
EXPORT int boundary_protected_negative(lua_State *L) { return protected_call(L, 1); }
EXPORT int boundary_protected_argument(lua_State *L) {
  int status = lua_pcall(L, 1, -1, 2);
  lua_pushnumber(L, status); lua_insert(L, 1); return lua_gettop(L);
}
static int replacement_handler(lua_State *L) {
  lua_pushlstring(L, "native-replacement", 18); return 1;
}
EXPORT int boundary_replace_handler(lua_State *L) {
  lua_pushcfunction(L, replacement_handler); lua_replace(L, 1);
  lua_pushnil(L); return lua_error(L);
}
typedef struct ProtectedData { int fail; } ProtectedData;
static int protected_cfunction(lua_State *L) {
  ProtectedData *data = (ProtectedData *)lua_touserdata(L, 1);
  if (lua_gettop(L) != 1 || lua_type(L, lua_upvalueindex(1)) != -1 || !data) {
    lua_pushlstring(L, "invalid cpcall layout", 21); return lua_error(L);
  }
  if (data->fail) {
    lua_pushlstring(L, "cpcall\0\xff", 8); return lua_error(L);
  }
  lua_pushnumber(L, 42); lua_pushnil(L); return 2;
}
EXPORT int boundary_cpcall(lua_State *L) {
  ProtectedData data;
  int before = lua_gettop(L), status, balanced;
  data.fail = luaL_checknumber(L, 1) != 0;
  status = lua_cpcall(L, protected_cfunction, &data);
  balanced = lua_gettop(L) == before + (status != 0);
  lua_pushnumber(L, status); lua_insert(L, before + 1);
  lua_pushboolean(L, balanced);
  return lua_gettop(L) - before;
}

EXPORT int boundary_sum(lua_State *L) {
  double a = luaL_checknumber(L, 1);
  double b = luaL_checknumber(L, 2);
  lua_pushnumber(L, a + b);
  lua_pushnumber(L, lua_gettop(L));
  return 2;
}
EXPORT int boundary_identity(lua_State *L) {
  lua_pushvalue(L, 1);
  lua_pushnil(L);
  lua_pushboolean(L, 0);
  return 3;
}
EXPORT int boundary_raise(lua_State *L) {
  lua_pushvalue(L, 1);
  return lua_error(L);
}
EXPORT int boundary_binary(lua_State *L) {
  static const char bytes[] = {'a', 0, 'b', '\r', '\n', (char)255};
  int arguments = lua_gettop(L);
  lua_pushlstring(L, bytes, sizeof(bytes));
  if (arguments) {
    lua_pushvalue(L, 1);
    lua_call(L, 0, 0);
  }
  return 1;
}
EXPORT int boundary_call(lua_State *L) {
  int arguments = lua_gettop(L);
  lua_pushvalue(L, 1);
  lua_pushvalue(L, 2);
  lua_call(L, 1, -1);
  if (arguments > 2) {
    lua_pushvalue(L, 3);
    lua_call(L, 0, 0);
  }
  return lua_gettop(L) - arguments;
}
EXPORT int boundary_stack(lua_State *L) {
  int i;
  if (!lua_checkstack(L, 128)) return 0;
  for (i = 0; i < 96; i++) {
    char bytes[3]; bytes[0] = (char)i; bytes[1] = 0; bytes[2] = (char)(255-i);
    lua_pushlstring(L, bytes, sizeof(bytes));
  }
  lua_pushvalue(L, 1);
  lua_call(L, 0, 0);
  return 96;
}
EXPORT int boundary_mutate(lua_State *L) {
  lua_settop(L, 3);
  lua_pushnumber(L, 99);
  lua_replace(L, 1);
  lua_pushvalue(L, -2);
  lua_settop(L, -2);
  lua_pushlstring(L, "7.25", 4);
  lua_pushnumber(L, luaL_checknumber(L, -1) * 2);
  return 5;
}
EXPORT int boundary_state(lua_State *L) {
  lua_pushlightuserdata(L, L); return 1;
}
EXPORT int boundary_state_reenter(lua_State *L) {
  lua_State *saved = L;
  lua_pushvalue(L, 1); lua_call(L, 0, 1);
  lua_pushboolean(L, lua_touserdata(L, -1) == saved);
  return 1;
}
EXPORT int boundary_cached_top(lua_State *L) {
  static lua_State *cached;
  if (!cached) cached = L;
  lua_pushnumber(L, lua_gettop(cached));
  return 1;
}
static int counter(lua_State *L) {
  double value = luaL_checknumber(L, lua_upvalueindex(1));
  value += lua_gettop(L) ? luaL_checknumber(L, -1) : 1;
  lua_pushnumber(L, value); lua_replace(L, lua_upvalueindex(1));
  lua_pushvalue(L, lua_upvalueindex(1));
  lua_pushvalue(L, lua_upvalueindex(2));
  return 2;
}
EXPORT int boundary_make_counter(lua_State *L) {
  lua_pushvalue(L, 1); lua_pushvalue(L, 2);
  lua_pushcclosure(L, counter, 2); return 1;
}
EXPORT int boundary_snapshot(lua_State *L) {
  lua_pushvalue(L, LUA_REGISTRYINDEX);
  lua_pushvalue(L, LUA_ENVIRONINDEX);
  lua_pushvalue(L, LUA_GLOBALSINDEX);
  return 3;
}
EXPORT int boundary_set_environment(lua_State *L) {
  lua_pushvalue(L, 1); lua_replace(L, LUA_ENVIRONINDEX);
  lua_pushcfunction(L, boundary_snapshot); return 1;
}
EXPORT int boundary_set_globals(lua_State *L) {
  lua_pushvalue(L, LUA_GLOBALSINDEX);
  lua_pushvalue(L, 1); lua_replace(L, LUA_GLOBALSINDEX); return 1;
}
EXPORT int boundary_registry_roundtrip(lua_State *L) {
  int original = lua_gettop(L) + 1;
  lua_pushvalue(L, LUA_REGISTRYINDEX);
  lua_pushvalue(L, 1); lua_replace(L, LUA_REGISTRYINDEX);
  lua_pushvalue(L, 2); lua_call(L, 0, 0);
  lua_pushvalue(L, LUA_REGISTRYINDEX);
  lua_pushvalue(L, original); lua_replace(L, LUA_REGISTRYINDEX);
  return 1;
}
static int many(lua_State *L) {
  double first = luaL_checknumber(L, lua_upvalueindex(1));
  double last = luaL_checknumber(L, lua_upvalueindex(255));
  lua_pushnumber(L, first); lua_pushnumber(L, last);
  lua_pushvalue(L, lua_upvalueindex(256)); return 3;
}
EXPORT int boundary_make_many(lua_State *L) {
  int i;
  if (!lua_checkstack(L, 255)) return 0;
  for (i = 1; i <= 255; i++) lua_pushnumber(L, i);
  lua_pushcclosure(L, many, 255); return 1;
}
EXPORT int boundary_table(lua_State *L) {
  lua_createtable(L, 4, 2);
  lua_pushvalue(L, 1); lua_setfield(L, -2, "payload");
  lua_pushnumber(L, 17); lua_rawseti(L, -2, 1);
  lua_pushcfunction(L, boundary_snapshot); lua_setfield(L, -2, "snapshot");
  lua_getfield(L, -1, "snapshot"); lua_call(L, 0, 3);
  return 4;
}
EXPORT int boundary_get(lua_State *L) {
  lua_pushvalue(L, 2); lua_gettable(L, 1); return 1;
}
EXPORT int boundary_set(lua_State *L) {
  lua_pushvalue(L, 2); lua_pushvalue(L, 3); lua_settable(L, 1); return 0;
}
EXPORT int boundary_field(lua_State *L) {
  if (lua_gettop(L) >= 2) {
    lua_pushvalue(L, 2); lua_setfield(L, 1, "field");
  }
  lua_getfield(L, 1, "field"); return 1;
}
EXPORT int boundary_raw(lua_State *L) {
  if (lua_gettop(L) >= 3) {
    lua_pushvalue(L, 2); lua_pushvalue(L, 3); lua_rawset(L, 1);
  }
  lua_pushvalue(L, 2); lua_rawget(L, 1); return 1;
}
EXPORT int boundary_metatable(lua_State *L) {
  if (lua_gettop(L) >= 2) {
    lua_pushvalue(L, 2); lua_setmetatable(L, 1);
  }
  if (!lua_getmetatable(L, 1)) lua_pushnil(L);
  return 1;
}
EXPORT int boundary_count(lua_State *L) {
  int count = 0;
  if (lua_gettop(L) >= 2) {
    lua_pushvalue(L, 2);
    if (lua_next(L, 1)) return 2;
    lua_pushnil(L); return 1;
  }
  lua_pushnil(L);
  while (lua_next(L, 1)) {
    count++; lua_settop(L, -2);
  }
  lua_pushnumber(L, count); return 1;
}
EXPORT int boundary_shuffle(lua_State *L) {
  lua_pushvalue(L, 1); lua_insert(L, 2); lua_remove(L, 1);
  lua_pushnumber(L, lua_type(L, -1));
  lua_pushboolean(L, lua_rawequal(L, 1, 2));
  lua_pushboolean(L, lua_rawequal(L, 1, 99));
  lua_pushnumber(L, lua_type(L, 99));
  return lua_gettop(L);
}


static const char *aux_vformat(lua_State *L, const char *format, ...) {
  const char *result; va_list arguments;
  va_start(arguments,format); result=lua_pushvfstring(L,format,arguments); va_end(arguments);
  return result;
}
EXPORT int boundary_format(lua_State *L) {
  const char *text=lua_type(L,1)<=0 ? NULL : luaL_checklstring(L,1,NULL);
  const char *cached; int record, token=17; char expected[64];
  lua_createtable(L,0,6); record=lua_gettop(L);
  cached=lua_pushfstring(L,"s=%s d=%d f=%f c=%c z=%c %% %z",text,-17,1.25,255,0);
  lua_setfield(L,record,"main");
  aux_vformat(L,"%d:%f:%s",2147483647,-0.0,NULL); lua_setfield(L,record,"va");
  lua_pushfstring(L,"literal %% / %q %02d",123); lua_setfield(L,record,"unknown");
  sprintf(expected,"%p",(void *)&token);
  lua_pushboolean(L,!strcmp(lua_pushfstring(L,"%p",(void *)&token),expected));
  lua_setfield(L,record,"pointer"); lua_settop(L,record);
  if (lua_type(L,3)==6) { lua_pushvalue(L,3);lua_call(L,0,0); }
  lua_getfield(L,record,"main");
  lua_pushboolean(L,cached==lua_tolstring(L,-1,NULL)); lua_setfield(L,record,"stable");
  lua_settop(L,record);return 1;
}
static int format_finalizers;
static int format_gc(lua_State *L) { (void)L;format_finalizers++;return 0; }
EXPORT int boundary_format_gc(lua_State *L) {
  int i, before;
  lua_newtable(L);lua_pushcfunction(L,format_gc);lua_setfield(L,-2,"__gc");
  for (i=0;i<64;i++) {
    lua_newuserdata(L,0);lua_pushvalue(L,-2);lua_setmetatable(L,-2);lua_settop(L,-2);
  }
  before=format_finalizers;
  lua_pushfstring(L,"parts:%s:%d:%f", "message",17,2.5);
  lua_pushboolean(L,format_finalizers==before);
  return 2;
}
EXPORT int boundary_auxwhere(lua_State *L) {
  luaL_where(L,(int)luaL_checkinteger(L,1));return 1;
}
EXPORT int boundary_auxerror(lua_State *L) {
  const char *text=luaL_optlstring(L,1,"fallback",NULL);
  return luaL_error(L,"aux %s %d",text,17);
}
EXPORT int boundary_auxcheck(lua_State *L) {
  int mode=(int)luaL_optinteger(L,2,0);
  size_t length;const char *text;lua_Integer integer;
  switch (mode) {
    case 0: case 1: case 2:
      text=mode==0 ? luaL_checklstring(L,1,&length) : luaL_optlstring(L,1,mode==1 ? "fallback\0ignored" : NULL,&length);
      if (text)lua_pushlstring(L,text,length);else lua_pushnil(L);
      lua_pushinteger(L,(lua_Integer)length);lua_pushnumber(L,lua_type(L,1));return 3;
    case 3: case 4:
      integer=mode==3 ? luaL_checkinteger(L,1) : luaL_optinteger(L,1,77);
      lua_pushlstring(L,(const char *)&integer,sizeof(integer));lua_pushinteger(L,integer);
      lua_pushnumber(L,lua_type(L,1));return 3;
    case 5: lua_pushnumber(L,luaL_optnumber(L,1,2.5));return 1;
    case 6: luaL_checktype(L,1,5);lua_pushvalue(L,1);return 1;
    case 7: luaL_checkany(L,1);lua_pushvalue(L,1);return 1;
    case 8: {
      static const char *const options[]={"first","second","1",NULL};
      int option=luaL_checkoption(L,1,"second",options);
      lua_pushinteger(L,option);lua_pushnumber(L,lua_type(L,1));return 2;
    }
    case 9: luaL_checkstack(L,(int)luaL_checkinteger(L,1),"aux room");lua_pushboolean(L,1);return 1;
    case 10: return luaL_typerror(L,1,"special");
    case 11: return luaL_argerror(L,1,lua_type(L,3)<=0 ? NULL : luaL_checklstring(L,3,NULL));
    default: lua_pushnumber(L,luaL_checknumber(L,1));return 1;
  }
}
static int aux_value(lua_State *L) {
  double *number=(double *)luaL_checkudata(L,1,"deadlua.aux");lua_pushnumber(L,*number);return 1;
}
static int aux_set(lua_State *L) {
  double *number=(double *)luaL_checkudata(L,1,"deadlua.aux");
  *number=luaL_checknumber(L,2);lua_pushvalue(L,1);return 1;
}
static const luaL_Reg auxiliary_methods[]={{"get",aux_value},{"set",aux_set},{NULL,NULL}};
EXPORT int boundary_auxuserdata(lua_State *L) {
  double number=luaL_optnumber(L,1,17);
  double *payload=(double *)lua_newuserdata(L,sizeof(double));*payload=number;
  luaL_newmetatable(L,"deadlua.aux");luaL_register(L,NULL,auxiliary_methods);
  lua_pushvalue(L,-1);lua_setfield(L,-2,"__index");lua_setmetatable(L,-2);
  lua_pushlightuserdata(L,payload);return 2;
}
EXPORT int boundary_auxget(lua_State *L) {
  int index=lua_type(L,2)<=0 ? 1 : (int)luaL_checkinteger(L,2);
  double *number=(double *)luaL_checkudata(L,index,"deadlua.aux");lua_pushnumber(L,*number);return 1;
}
EXPORT int boundary_auxnewmetatable(lua_State *L) {
  int created=luaL_newmetatable(L,luaL_checklstring(L,1,NULL));
  lua_pushboolean(L,created);lua_insert(L,-2);return 2;
}
EXPORT int boundary_auxmetafield(lua_State *L) {
  int before=lua_gettop(L),present;
  const char *event=luaL_checklstring(L,2,NULL);
  present=lua_toboolean(L,3) ? luaL_callmeta(L,1,event) : luaL_getmetafield(L,1,event);
  lua_pushboolean(L,present);lua_insert(L,before+1);return lua_gettop(L)-before;
}
static int aux_echo(lua_State *L) {
  lua_pushvalue(L,lua_upvalueindex(1));lua_pushvalue(L,lua_upvalueindex(2));
  lua_getfield(L,LUA_ENVIRONINDEX,"_private_tag");return 3;
}
static const luaL_Reg auxiliary_library[]={{"echo",aux_echo},{"checked",aux_value},{"format",boundary_format},{"error",boundary_auxerror},{"where",boundary_auxwhere},{NULL,NULL}};
EXPORT int boundary_auxopen(lua_State *L) {
  const char *name=lua_type(L,1)<=0 ? NULL : luaL_checklstring(L,1,NULL);
  int captures=lua_gettop(L)-(name ? 1 : 2);
  if (!name)luaL_checktype(L,2,5);
  if (captures)luaI_openlib(L,name,auxiliary_library,captures);
  else luaL_register(L,name,auxiliary_library);
  return 1;
}
EXPORT int boundary_auxfind(lua_State *L) {
  const char *name=luaL_checklstring(L,2,NULL);
  const char *conflict=luaL_findtable(L,1,name,3);
  if (conflict)lua_pushstring(L,conflict);else lua_pushnil(L);
  return conflict ? 1 : 2;
}
EXPORT int boundary_auxrefs(lua_State *L) {
  int mode=(int)luaL_optinteger(L,3,0),reference;
  if (mode) { luaL_unref(L,1,(int)luaL_checkinteger(L,2));lua_pushboolean(L,1);return 1; }
  lua_pushvalue(L,2);reference=luaL_ref(L,1);lua_pushinteger(L,reference);return 1;
}

EXPORT int boundary_auxbuffer_info(lua_State *L) {
  lua_pushinteger(L, LUAL_BUFFERSIZE);
  lua_pushinteger(L, sizeof(luaL_Buffer));
  lua_pushinteger(L, offsetof(luaL_Buffer, buffer));
  return 3;
}
/* Each operation leaves only the buffer's chunk suffix above the trace. */
EXPORT int boundary_auxbuffer(lua_State *L) {
  luaL_Buffer buffer;
  int i, count, trace, prefix;
  luaL_checktype(L, 1, 5);
  count = (int)lua_objlen(L, 1);
  lua_createtable(L, count, 0);
  trace = lua_gettop(L);
  prefix = trace;
  luaL_buffinit(L, &buffer);
  for (i=1; i<=count; i++) {
    int mode, top, used, level;
    size_t length = 0;
    const char *bytes = NULL;
    lua_rawgeti(L, 1, i);
    lua_rawgeti(L, -1, 1);
    mode = (int)luaL_checkinteger(L, -1);
    lua_settop(L, -2);
    lua_rawgeti(L, -1, 2);
    if (mode == 2) {
      lua_remove(L, -2);
      luaL_addvalue(&buffer);
    } else {
      if (mode != 5 && mode != 6) {
        bytes = luaL_checklstring(L, -1, &length);
        /* Root a coerced number's string in the original step across GC. */
        lua_pushvalue(L, -1); lua_rawseti(L, -3, 2);
      }
      lua_settop(L, -3);
      switch (mode) {
        case 0: luaL_addlstring(&buffer, bytes, length); break;
        case 1: luaL_addstring(&buffer, bytes); break;
        case 3: {
          size_t j; for (j=0; j<length; j++) luaL_addchar(&buffer, bytes[j]);
          break;
        }
        case 4: {
          size_t position = 0;
          do {
            char *destination = luaL_prepbuffer(&buffer);
            size_t part = length-position;
            if (part > LUAL_BUFFERSIZE) part = LUAL_BUFFERSIZE;
            if (part) memcpy(destination, bytes+position, part);
            luaL_addsize(&buffer, part);
            position += part;
          } while (position < length);
          break;
        }
        case 5: luaL_prepbuffer(&buffer); break;
        case 6: luaL_pushresult(&buffer); break;
        default: return luaL_error(L, "invalid buffer fixture mode");
      }
    }
    if (lua_type(L, 2) == 6) {
      lua_pushvalue(L, 2); lua_call(L, 0, 0);
    }
    top = lua_gettop(L)-prefix;
    used = (int)(buffer.p-buffer.buffer);
    level = buffer.lvl;
    lua_createtable(L, 3, 0);
    lua_pushinteger(L, top); lua_rawseti(L, -2, 1);
    lua_pushinteger(L, used); lua_rawseti(L, -2, 2);
    lua_pushinteger(L, level); lua_rawseti(L, -2, 3);
    lua_rawseti(L, trace, i);
  }
  luaL_pushresult(&buffer);
  lua_pushvalue(L, trace);
  return 2;
}
EXPORT int boundary_auxgsub(lua_State *L) {
  const char *string = luaL_checklstring(L, 1, NULL);
  const char *pattern = luaL_checklstring(L, 2, NULL);
  const char *replacement = luaL_checklstring(L, 3, NULL);
  const char *result = luaL_gsub(L, string, pattern, replacement);
  lua_pushboolean(L, result == lua_tolstring(L, -1, NULL));
  return 2;
}
EXPORT int boundary_upget(lua_State *L) {
  int number=(int)luaL_checkinteger(L,2);
  int index=(int)luaL_optinteger(L,4,1);
  int before=lua_gettop(L),delta;
  const char *name=lua_getupvalue(L,index,number);
  delta=lua_gettop(L)-before;
  if (lua_type(L,3)==6) { lua_pushvalue(L,3);lua_call(L,0,0); }
  /* The borrowed name must survive collection and other debug API calls. */
  lua_pushstring(L,name);
  if (name) lua_insert(L,-2); else lua_pushnil(L);
  lua_pushinteger(L,delta);
  return 3;
}
EXPORT int boundary_upset(lua_State *L) {
  int number=(int)luaL_checkinteger(L,2);
  int index=(int)luaL_optinteger(L,4,1);
  int before,delta;
  const char *name;
  lua_pushvalue(L,3);
  before=lua_gettop(L);
  /* A negative index refers to the stack including this value. */
  name=lua_setupvalue(L,index,number);
  delta=lua_gettop(L)-before;
  lua_pushstring(L,name);lua_pushinteger(L,delta);
  return 2;
}
EXPORT int boundary_upbind(lua_State *L) {
  lua_pushvalue(L,1);lua_pushcclosure(L,boundary_upget,1);
  lua_pushvalue(L,1);lua_pushcclosure(L,boundary_upset,1);
  return 2;
}
EXPORT int boundary_gc(lua_State *L) {
  int what=(int)luaL_checkinteger(L,1),data=(int)luaL_optinteger(L,2,0);
  int before=lua_gettop(L),result=lua_gc(L,what,data),after=lua_gettop(L);
  lua_pushinteger(L,result);lua_pushinteger(L,after-before);lua_pushvalue(L,1);
  return 3;
}
EXPORT int boundary_gc_counts(lua_State *L) {
  int whole=lua_gc(L,LUA_GCCOUNT,0),part=lua_gc(L,LUA_GCCOUNTB,0);
  int repeated=lua_gc(L,LUA_GCCOUNT,0),remainder=lua_gc(L,LUA_GCCOUNTB,0);
  lua_pushboolean(L,whole>=0 && part>=0 && part<1024);
  lua_pushboolean(L,whole==repeated && part==remainder);
  return 2;
}

/* Reserve slot four before lua_load: parser-owned roots must remain untouched. */
typedef struct ReaderData { int calls; } ReaderData;
static const char *reader_lua(lua_State *L, void *data, size_t *size) {
  ReaderData *reader=(ReaderData *)data;
  ++reader->calls;
  lua_pushvalue(L,1);lua_pushinteger(L,reader->calls);lua_call(L,1,1);
  lua_replace(L,4);
  return lua_tolstring(L,4,size);
}
EXPORT int boundary_load(lua_State *L) {
  ReaderData reader={0};
  const char *name=lua_tolstring(L,2,NULL);
  int status,before;
  lua_settop(L,4);before=lua_gettop(L);
  status=lua_load(L,reader_lua,&reader,name);
  lua_pushinteger(L,status);lua_insert(L,-2);
  lua_pushinteger(L,lua_gettop(L)-before-1);
  lua_pushinteger(L,reader.calls);lua_pushvalue(L,4);
  return 5;
}
EXPORT int boundary_loadbuffer(lua_State *L) {
  size_t length;
  int mode=(int)luaL_checkinteger(L,1),before=lua_gettop(L),status;
  const char *source=luaL_checklstring(L,2,&length);
  const char *name=lua_tolstring(L,3,NULL);
  status=mode ? luaL_loadstring(L,source) : luaL_loadbuffer(L,source,length,name);
  lua_pushinteger(L,status);lua_insert(L,-2);
  lua_pushinteger(L,lua_gettop(L)-before-1);
  return 3;
}
/* Separate observation of implementation scratch slots, not a parity assertion. */
static const char *reader_top(lua_State *L, void *data, size_t *size) {
  int *calls=(int *)data;
  lua_pushvalue(L,1);lua_pushinteger(L,++*calls);lua_pushinteger(L,lua_gettop(L)-2);
  lua_call(L,2,0);
  *size=*calls==1?7:0;
  return "return ";
}
EXPORT int boundary_loadtop(lua_State *L) {
  int calls=0;
  lua_load(L,reader_top,&calls,"=scratch");
  return 1;
}
EXPORT int boundary_loadfile(lua_State *L) {
  const char *path=lua_tolstring(L,1,NULL);
  int before=lua_gettop(L),status=luaL_loadfile(L,path);
  lua_pushinteger(L,status);lua_insert(L,-2);
  lua_pushinteger(L,lua_gettop(L)-before-1);
  return 3;
}
typedef struct WriterData { int calls,trace,pieces,stop,status; } WriterData;
static int writer_lua(lua_State *L, const void *bytes, size_t length, void *data) {
  WriterData *writer=(WriterData *)data;
  int status=0;
  ++writer->calls;
  lua_pushinteger(L,length);lua_rawseti(L,writer->trace,writer->calls);
  lua_pushlstring(L,(const char *)bytes,length);lua_rawseti(L,writer->pieces,writer->calls);
  if (lua_type(L,2)==6) {
    lua_pushvalue(L,2);lua_pushinteger(L,writer->calls);
    lua_pushlstring(L,(const char *)bytes,length);lua_pushboolean(L,bytes==NULL);
    lua_call(L,3,1);status=(int)lua_tointeger(L,-1);lua_settop(L,-2);
  }
  return status ? status : writer->calls==writer->stop ? writer->status : 0;
}
EXPORT int boundary_dump(lua_State *L) {
  WriterData writer;
  int before,status,delta;
  writer.calls=0;writer.stop=(int)luaL_optinteger(L,3,0);writer.status=(int)luaL_optinteger(L,4,17);
  lua_settop(L,4);
  lua_newtable(L);writer.trace=lua_gettop(L);
  lua_newtable(L);writer.pieces=lua_gettop(L);
  lua_pushvalue(L,1);before=lua_gettop(L);
  status=lua_dump(L,writer_lua,&writer);delta=lua_gettop(L)-before;
  lua_pushinteger(L,status);lua_pushvalue(L,writer.pieces);lua_pushvalue(L,writer.trace);
  lua_pushinteger(L,delta);lua_pushvalue(L,1);lua_pushinteger(L,writer.calls);
  return 6;
}
EXPORT int boundary_dump_null(lua_State *L) {
  lua_pushvalue(L,1);lua_pushinteger(L,lua_dump(L,NULL,NULL));return 1;
}
EXPORT int boundary_thread(lua_State *L) {
  int before=lua_gettop(L),main=lua_pushthread(L),current;
  current=lua_tothread(L,-1)==L;
  lua_pushboolean(L,main);lua_pushboolean(L,current);
  lua_pushinteger(L,lua_gettop(L)-before-2);lua_pushinteger(L,lua_status(L));return 5;
}
EXPORT int boundary_thread_status(lua_State *L) {
  lua_State *thread=lua_tothread(L,1);
  if(thread)lua_pushinteger(L,lua_status(thread));else lua_pushnil(L);return 1;
}
EXPORT int boundary_thread_pointer(lua_State *L) {
  int index=(int)luaL_optinteger(L,2,1);
  lua_State *thread=lua_tothread(L,index);
  lua_pushlightuserdata(L,thread);lua_pushboolean(L,thread==L);return 2;
}
static int thread_capture(lua_State *L) {
  lua_State *thread=lua_tothread(L,lua_upvalueindex(1));
  lua_pushlightuserdata(L,thread);lua_pushinteger(L,thread?lua_status(thread):-1);return 2;
}
EXPORT int boundary_thread_capture(lua_State *L) {
  lua_pushvalue(L,1);lua_pushcclosure(L,thread_capture,1);return 1;
}
