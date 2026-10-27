/* Lua execution stays in Rust. Only this C boundary owns setjmp/longjmp. */
#define BOUNDARY_BUILD
#include "api.h"
#include <setjmp.h>
#include <stdlib.h>
#include <string.h>

typedef struct BoundaryFrame {
  void *context;
  BoundaryDispatch dispatch;
  jmp_buf failure;
  struct BoundaryFrame *previous;
  lua_State *previous_active;
} BoundaryFrame;
struct lua_State {
  BoundaryFrame *frame;
};
static __declspec(thread) lua_State *active_state;

static void operation(lua_State *L, int op, int a, int b, BoundaryValue *value) {
  /* dispatch has returned, releasing every Rust borrow/local, before jumping. */
  BoundaryFrame *frame = L->frame;
  if (!frame || active_state != L) abort(); /* Other/inactive-thread API access remains work. */
  if (frame->dispatch(frame->context, op, a, b, value)) longjmp(frame->failure, 1);
}

API double lua_tonumber(lua_State *L, int index) {
  BoundaryValue value = {0}; operation(L, 35, index, 0, &value); return value.number;
}
API int lua_isnumber(lua_State *L, int index) {
  BoundaryValue value = {0}; operation(L, 35, index, 0, &value); return value.integer;
}
API lua_Integer lua_tointeger(lua_State *L, int index) {
  BoundaryValue value = {0}; operation(L, 36, index, 0, &value); return value.wide_integer;
}
API int lua_toboolean(lua_State *L, int index) {
  BoundaryValue value = {0}; operation(L, 37, index, 0, &value); return value.integer;
}
API void lua_pushinteger(lua_State *L, lua_Integer integer) {
  BoundaryValue value = {0}; value.wide_integer = integer; operation(L, 38, 0, 0, &value);
}
API int lua_isstring(lua_State *L, int index) {
  int type = lua_type(L, index); return type == 3 || type == 4;
}
API int lua_isuserdata(lua_State *L, int index) {
  int type = lua_type(L, index); return type == 2 || type == 7;
}
API int lua_iscfunction(lua_State *L, int index) {
  BoundaryValue value = {0}; operation(L, 41, index, 0, &value); return value.integer;
}
API void lua_pushstring(lua_State *L, const char *string) {
  if (string) lua_pushlstring(L, string, strlen(string)); else lua_pushnil(L);
}
API const char *lua_typename(lua_State *L, int type) {
  static const char *const names[] = {"no value", "nil", "boolean", "userdata", "number", "string", "table", "function", "userdata", "thread"};
  (void)L; return type >= -1 && type <= 8 ? names[type+1] : NULL;
}
API int lua_equal(lua_State *L, int left, int right) {
  BoundaryValue value = {0}; operation(L, 39, left, right, &value); return value.integer;
}
API int lua_lessthan(lua_State *L, int left, int right) {
  BoundaryValue value = {0}; value.integer = 1; operation(L, 39, left, right, &value); return value.integer;
}
API void lua_concat(lua_State *L, int count) {
  BoundaryValue value = {0}; operation(L, 40, count, 0, &value);
}

API void *lua_newuserdata(lua_State *L, size_t size) {
  BoundaryValue value = {0}; value.length = size;
  operation(L, 29, 0, 0, &value);
  return lua_touserdata(L, -1);
}
API size_t lua_objlen(lua_State *L, int index) {
  BoundaryValue value = {0}; operation(L, 30, index, 0, &value);
  return value.length;
}
API const char *lua_tolstring(lua_State *L, int index, size_t *length) {
  BoundaryValue value = {0}; operation(L, 31, index, 0, &value);
  if (length) *length = value.length;
  return value.bytes;
}
API void lua_getfenv(lua_State *L, int index) {
  BoundaryValue value = {0}; operation(L, 32, index, 0, &value);
}
API int lua_setfenv(lua_State *L, int index) {
  BoundaryValue value = {0}; operation(L, 33, index, 0, &value);
  return value.integer;
}
API int lua_pcall(lua_State *L, int arguments, int results, int handler) {
  BoundaryValue value = {0}; value.integer = handler;
  operation(L, 34, arguments, results, &value);
  return value.integer;
}
API int lua_cpcall(lua_State *L, lua_CFunction function, void *data) {
  lua_pushcclosure(L, function, 0);
  lua_pushlightuserdata(L, data);
  return lua_pcall(L, 1, 0, 0);
}

API lua_State *boundary_state_new(void) {
  return (lua_State *)calloc(1, sizeof(lua_State));
}
API size_t boundary_state_size(void) { return sizeof(lua_State); }
API int boundary_state_free(lua_State *state) {
  if (!state || state->frame) return 0;
  free(state); return 1;
}
API int boundary_invoke(lua_State *state, void *context, BoundaryDispatch dispatch, lua_CFunction f) {
  BoundaryFrame frame;
  int count;
  frame.context = context;
  frame.dispatch = dispatch;
  frame.previous = state->frame;
  frame.previous_active = active_state;
  state->frame = &frame;
  active_state = state;
  if (setjmp(frame.failure)) {
    state->frame = frame.previous;
    active_state = frame.previous_active;
    return -1;
  }
  count = f(state);
  state->frame = frame.previous;
  active_state = frame.previous_active;
  return count;
}

API int lua_gettop(lua_State *L) {
  BoundaryValue value = {0};
  operation(L, 0, 0, 0, &value);
  return value.integer;
}
API void lua_pushnumber(lua_State *L, double number) {
  BoundaryValue value = {0}; value.number = number;
  operation(L, 1, 0, 0, &value);
}
API void lua_pushboolean(lua_State *L, int boolean) {
  BoundaryValue value = {0}; value.integer = boolean;
  operation(L, 2, 0, 0, &value);
}
API void lua_pushnil(lua_State *L) {
  BoundaryValue value = {0}; operation(L, 3, 0, 0, &value);
}
API void lua_pushlstring(lua_State *L, const char *bytes, size_t length) {
  BoundaryValue value = {0}; value.bytes = bytes; value.length = length;
  operation(L, 4, 0, 0, &value);
}
API void lua_pushvalue(lua_State *L, int index) {
  BoundaryValue value = {0}; operation(L, 5, index, 0, &value);
}
API double luaL_checknumber(lua_State *L, int index) {
  BoundaryValue value = {0}; operation(L, 6, index, 0, &value);
  return value.number;
}
API int lua_error(lua_State *L) {
  BoundaryValue value = {0}; operation(L, 7, 0, 0, &value);
  return 0;
}
API void lua_call(lua_State *L, int arguments, int results) {
  BoundaryValue value = {0}; operation(L, 8, arguments, results, &value);
}
API int lua_checkstack(lua_State *L, int extra) {
  BoundaryValue value = {0}; operation(L, 9, extra, 0, &value);
  return value.integer;
}
API void lua_settop(lua_State *L, int top) {
  BoundaryValue value = {0}; operation(L, 10, top, 0, &value);
}
API void lua_replace(lua_State *L, int index) {
  BoundaryValue value = {0}; operation(L, 11, index, 0, &value);
}
API void lua_pushlightuserdata(lua_State *L, void *pointer) {
  BoundaryValue value = {0}; value.bytes = (const char *)pointer;
  operation(L, 12, 0, 0, &value);
}
API void *lua_touserdata(lua_State *L, int index) {
  BoundaryValue value = {0}; operation(L, 13, index, 0, &value);
  return (void *)value.bytes;
}
API void lua_pushcclosure(lua_State *L, lua_CFunction function, int count) {
  /* The fixture targets the pinned MSVC ABI, where these pointer sizes match. */
  BoundaryValue value = {0}; value.bytes = (const char *)function;
  operation(L, 14, count, 0, &value);
}
API void lua_createtable(lua_State *L, int array, int records) {
  BoundaryValue value = {0}; operation(L, 15, array, records, &value);
}
API void lua_gettable(lua_State *L, int index) {
  BoundaryValue value = {0}; operation(L, 16, index, 0, &value);
}
API void lua_rawget(lua_State *L, int index) {
  BoundaryValue value = {0}; operation(L, 16, index, 1, &value);
}
API void lua_settable(lua_State *L, int index) {
  BoundaryValue value = {0}; operation(L, 17, index, 0, &value);
}
API void lua_rawset(lua_State *L, int index) {
  BoundaryValue value = {0}; operation(L, 17, index, 1, &value);
}
API void lua_getfield(lua_State *L, int index, const char *name) {
  BoundaryValue value = {0}; value.bytes = name; value.length = strlen(name);
  operation(L, 18, index, 0, &value);
}
API void lua_setfield(lua_State *L, int index, const char *name) {
  BoundaryValue value = {0}; value.bytes = name; value.length = strlen(name);
  operation(L, 19, index, 0, &value);
}
API void lua_rawgeti(lua_State *L, int index, int key) {
  BoundaryValue value = {0}; operation(L, 20, index, key, &value);
}
API void lua_rawseti(lua_State *L, int index, int key) {
  BoundaryValue value = {0}; operation(L, 21, index, key, &value);
}
API int lua_getmetatable(lua_State *L, int index) {
  BoundaryValue value = {0}; operation(L, 22, index, 0, &value); return value.integer;
}
API int lua_setmetatable(lua_State *L, int index) {
  BoundaryValue value = {0}; operation(L, 23, index, 0, &value); return 1;
}
API int lua_next(lua_State *L, int index) {
  BoundaryValue value = {0}; operation(L, 24, index, 0, &value); return value.integer;
}
API int lua_type(lua_State *L, int index) {
  BoundaryValue value = {0}; operation(L, 25, index, 0, &value); return value.integer;
}
API int lua_rawequal(lua_State *L, int first, int second) {
  BoundaryValue value = {0}; operation(L, 26, first, second, &value); return value.integer;
}
API void lua_remove(lua_State *L, int index) {
  BoundaryValue value = {0}; operation(L, 27, index, 0, &value);
}
API void lua_insert(lua_State *L, int index) {
  BoundaryValue value = {0}; operation(L, 28, index, 0, &value);
}
