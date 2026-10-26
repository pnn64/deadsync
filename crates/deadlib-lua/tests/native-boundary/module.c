/* Built unchanged against official Lua or the development bridge. */
#ifdef REFERENCE
#include "lua.h"
#include "lauxlib.h"
#else
#include "api.h"
#endif
#define EXPORT __declspec(dllexport)

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
