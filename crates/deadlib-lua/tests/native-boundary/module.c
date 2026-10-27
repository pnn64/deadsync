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
