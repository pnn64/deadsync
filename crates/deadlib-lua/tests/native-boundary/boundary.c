/* Lua execution stays in Rust. Only this C boundary owns setjmp/longjmp. */
#define BOUNDARY_BUILD
#include "api.h"
#include <setjmp.h>
#include <stdlib.h>

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
