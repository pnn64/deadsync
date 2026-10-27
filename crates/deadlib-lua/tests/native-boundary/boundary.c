/* Lua execution stays in Rust. Only this C boundary owns setjmp/longjmp. */
#define BOUNDARY_BUILD
#include "api.h"
#include <setjmp.h>
#include <stdlib.h>
#include <string.h>
#include <stdio.h>

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

API int boundary_read(lua_State *L, void *context, BoundaryDispatch dispatch, lua_Reader reader, void *data, BoundaryValue *result) {
  BoundaryFrame frame;
  frame.context=context;frame.dispatch=dispatch;frame.previous=L->frame;frame.previous_active=active_state;
  L->frame=&frame;active_state=L;
  if (setjmp(frame.failure)) {
    L->frame=frame.previous;active_state=frame.previous_active;return -1;
  }
  result->length=0;
  result->bytes=reader(L,data,&result->length);
  L->frame=frame.previous;active_state=frame.previous_active;
  return 0;
}
API int boundary_write(lua_State *L, void *context, BoundaryDispatch dispatch, lua_Writer writer, void *data, const void *bytes, size_t length, BoundaryValue *result) {
  BoundaryFrame frame;
  frame.context=context;frame.dispatch=dispatch;frame.previous=L->frame;frame.previous_active=active_state;
  L->frame=&frame;active_state=L;
  if (setjmp(frame.failure)) {
    L->frame=frame.previous;active_state=frame.previous_active;return -1;
  }
  result->integer=writer(L,bytes,length,data);
  L->frame=frame.previous;active_state=frame.previous_active;return 0;
}

static void operation(lua_State *L, int op, int a, int b, BoundaryValue *value) {
  /* dispatch has returned, releasing every Rust borrow/local, before jumping. */
  BoundaryFrame *frame = L->frame;
  if (!frame || active_state != L) abort(); /* Other/inactive-thread API access remains work. */
  if (frame->dispatch(frame->context, op, a, b, value)) longjmp(frame->failure, 1);
}
API int lua_load(lua_State *L, lua_Reader reader, void *data, const char *name) {
  BoundaryValue value={0};value.bytes=name;value.reader=reader;value.userdata=data;
  operation(L,48,0,0,&value);return value.integer;
}
API int lua_dump(lua_State *L, lua_Writer writer, void *data) {
  BoundaryValue value={0};value.writer=writer;value.userdata=data;
  operation(L,50,0,0,&value);return value.integer;
}
typedef struct BufferReader {const char *bytes;size_t size;} BufferReader;
static const char *read_buffer(lua_State *L, void *data, size_t *size) {
  BufferReader *input=(BufferReader *)data;
  const char *result=input->bytes;(void)L;
  *size=input->size;input->size=0;return result;
}
API int luaL_loadbuffer(lua_State *L, const char *bytes, size_t length, const char *name) {
  BufferReader input;input.bytes=bytes;input.size=length;
  return lua_load(L,read_buffer,&input,name);
}
API int luaL_loadstring(lua_State *L, const char *string) {
  return luaL_loadbuffer(L,string,strlen(string),string);
}
API int luaL_loadfile(lua_State *L, const char *name) {
  BoundaryValue value={0};value.bytes=name;operation(L,49,0,0,&value);return value.integer;
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
API const char *lua_getupvalue(lua_State *L, int index, int number) {
  BoundaryValue value = {0}; operation(L, 45, index, number, &value); return value.bytes;
}
API const char *lua_setupvalue(lua_State *L, int index, int number) {
  BoundaryValue value = {0}; operation(L, 46, index, number, &value); return value.bytes;
}
API int lua_gc(lua_State *L, int what, int data) {
  BoundaryValue value = {0}; operation(L, 47, what, data, &value); return value.integer;
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
  double number = lua_tonumber(L, index);
  if (number == 0 && !lua_isnumber(L, index)) luaL_typerror(L, index, "number");
  return number;
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


/* C varargs stay in the ABI shim. String/number/table execution uses the VM. */
API const char *lua_pushvfstring(lua_State *L, const char *format, va_list arguments) {
  const char *at = format, *percent;
  int parts = 1;
  lua_pushstring(L, "");
  while ((percent = strchr(at, '%')) != NULL) {
    char one[3] = {0};
    lua_pushlstring(L, at, (size_t)(percent-at));
    switch (percent[1]) {
      case 's': { const char *text = va_arg(arguments, const char *);
        lua_pushstring(L, text ? text : "(null)"); break; }
      case 'd': lua_pushnumber(L, va_arg(arguments, int)); break;
      case 'f': lua_pushnumber(L, va_arg(arguments, double)); break;
      case 'c': one[0] = (char)va_arg(arguments, int); lua_pushstring(L, one); break;
      case 'p': { char pointer[4*sizeof(void *)+8];
        sprintf(pointer, "%p", va_arg(arguments, void *)); lua_pushstring(L, pointer); break; }
      case '%': lua_pushstring(L, "%"); break;
      default: one[0] = '%'; one[1] = percent[1]; lua_pushstring(L, one); break;
    }
    parts += 2;
    if (!percent[1]) { at = percent+1; break; }
    at = percent+2;
  }
  lua_pushstring(L, at);
  { BoundaryValue value = {0}; operation(L, 44, parts+1, 0, &value); }
  return lua_tolstring(L, -1, NULL);
}
API const char *lua_pushfstring(lua_State *L, const char *format, ...) {
  const char *text; va_list arguments;
  va_start(arguments, format); text = lua_pushvfstring(L, format, arguments); va_end(arguments);
  return text;
}
API void luaL_where(lua_State *L, int level) {
  BoundaryValue value = {0}; operation(L, 43, level, 0, &value);
}
API int luaL_error(lua_State *L, const char *format, ...) {
  va_list arguments;
  luaL_where(L, 1); va_start(arguments, format);
  lua_pushvfstring(L, format, arguments); va_end(arguments);
  lua_concat(L, 2); return lua_error(L);
}
API int luaL_argerror(lua_State *L, int index, const char *detail) {
  BoundaryValue value = {0};
  if (!detail) detail = "(null)";
  value.bytes = detail; value.length = strlen(detail);
  operation(L, 42, index, 0, &value);
  luaL_where(L, 1); lua_insert(L, -2); lua_concat(L, 2);
  return lua_error(L);
}
API int luaL_typerror(lua_State *L, int index, const char *expected) {
  const char *detail = lua_pushfstring(L, "%s expected, got %s", expected, lua_typename(L, lua_type(L, index)));
  return luaL_argerror(L, index, detail);
}
API void luaL_checktype(lua_State *L, int index, int type) {
  if (lua_type(L, index) != type) luaL_typerror(L, index, lua_typename(L, type));
}
API void luaL_checkany(lua_State *L, int index) {
  if (lua_type(L, index) == -1) luaL_argerror(L, index, "value expected");
}
API const char *luaL_checklstring(lua_State *L, int index, size_t *length) {
  const char *text = lua_tolstring(L, index, length);
  if (!text) luaL_typerror(L, index, "string");
  return text;
}
API const char *luaL_optlstring(lua_State *L, int index, const char *fallback, size_t *length) {
  if (lua_type(L, index) <= 0) {
    if (length) *length = fallback ? strlen(fallback) : 0;
    return fallback;
  }
  return luaL_checklstring(L, index, length);
}
API double luaL_optnumber(lua_State *L, int index, double fallback) {
  return lua_type(L, index) <= 0 ? fallback : luaL_checknumber(L, index);
}
API lua_Integer luaL_checkinteger(lua_State *L, int index) {
  lua_Integer integer = lua_tointeger(L, index);
  if (!integer && !lua_isnumber(L, index)) luaL_typerror(L, index, "number");
  return integer;
}
API lua_Integer luaL_optinteger(lua_State *L, int index, lua_Integer fallback) {
  return lua_type(L, index) <= 0 ? fallback : luaL_checkinteger(L, index);
}
API void luaL_checkstack(lua_State *L, int extra, const char *detail) {
  if (!lua_checkstack(L, extra)) luaL_error(L, "stack overflow (%s)", detail);
}
API int luaL_checkoption(lua_State *L, int index, const char *fallback, const char *const options[]) {
  const char *text = fallback ? luaL_optlstring(L,index,fallback,NULL) : luaL_checklstring(L,index,NULL);
  int i;
  for (i=0; options[i]; i++) if (!strcmp(text, options[i])) return i;
  return luaL_argerror(L,index,lua_pushfstring(L,"invalid option '%s'",text));
}
API int luaL_newmetatable(lua_State *L, const char *name) {
  lua_getfield(L,LUA_REGISTRYINDEX,name);
  if (lua_type(L,-1) != 0) return 0;
  lua_settop(L,-2); lua_newtable(L); lua_pushvalue(L,-1);
  lua_setfield(L,LUA_REGISTRYINDEX,name); return 1;
}
API void *luaL_checkudata(lua_State *L, int index, const char *name) {
  void *pointer = lua_touserdata(L,index);
  if (pointer && lua_getmetatable(L,index)) {
    lua_getfield(L,LUA_REGISTRYINDEX,name);
    if (lua_rawequal(L,-1,-2)) { lua_settop(L,-3); return pointer; }
  }
  luaL_typerror(L,index,name); return NULL;
}
API int luaL_getmetafield(lua_State *L, int index, const char *event) {
  if (!lua_getmetatable(L,index)) return 0;
  lua_pushstring(L,event); lua_rawget(L,-2);
  if (lua_type(L,-1) == 0) { lua_settop(L,-3); return 0; }
  lua_remove(L,-2); return 1;
}
static int absolute_index(lua_State *L, int index) {
  return index > 0 || index <= LUA_REGISTRYINDEX ? index : lua_gettop(L)+index+1;
}
API int luaL_callmeta(lua_State *L, int index, const char *event) {
  index = absolute_index(L,index);
  if (!luaL_getmetafield(L,index,event)) return 0;
  lua_pushvalue(L,index); lua_call(L,1,1); return 1;
}
API const char *luaL_findtable(lua_State *L, int index, const char *path, int hint) {
  const char *part = path;
  lua_pushvalue(L,index);
  for (;;) {
    const char *end = strchr(part,'.');
    if (!end) end = part+strlen(part);
    lua_pushlstring(L,part,(size_t)(end-part)); lua_rawget(L,-2);
    if (lua_type(L,-1) == 0) {
      lua_settop(L,-2); lua_createtable(L,0,*end == '.' ? 1 : hint);
      lua_pushlstring(L,part,(size_t)(end-part)); lua_pushvalue(L,-2); lua_settable(L,-4);
    } else if (lua_type(L,-1) != 5) {
      lua_settop(L,-3); return part;
    }
    lua_remove(L,-2);
    if (!*end) return NULL;
    part = end+1;
  }
}
API void luaL_openlib(lua_State *L, const char *name, const luaL_Reg *functions, int captures) {
  const luaL_Reg *entry;
  if (name) {
    int fields = 0;
    for (entry=functions; entry->name; entry++) fields++;
    luaL_findtable(L,LUA_REGISTRYINDEX,"_LOADED",1);
    lua_getfield(L,-1,name);
    if (lua_type(L,-1) != 5) {
      lua_settop(L,-2);
      if (luaL_findtable(L,LUA_GLOBALSINDEX,name,fields)) luaL_error(L,"name conflict for module '%s'",name);
      lua_pushvalue(L,-1); lua_setfield(L,-3,name);
    }
    lua_remove(L,-2); lua_insert(L,-(captures+1));
  }
  for (entry=functions; entry->name; entry++) {
    int i;
    for (i=0; i<captures; i++) lua_pushvalue(L,-captures);
    lua_pushcclosure(L,entry->func,captures); lua_setfield(L,-(captures+2),entry->name);
  }
  lua_settop(L,-captures-1);
}
API void luaL_register(lua_State *L, const char *name, const luaL_Reg *functions) {
  luaL_openlib(L,name,functions,0);
}
API int luaL_ref(lua_State *L, int index) {
  int reference;
  index = absolute_index(L,index);
  if (lua_type(L,-1) == 0) { lua_settop(L,-2); return -1; }
  lua_rawgeti(L,index,0); reference = (int)lua_tointeger(L,-1); lua_settop(L,-2);
  if (reference) { lua_rawgeti(L,index,reference); lua_rawseti(L,index,0); }
  else reference = (int)lua_objlen(L,index)+1;
  lua_rawseti(L,index,reference); return reference;
}
API void luaL_unref(lua_State *L, int index, int reference) {
  if (reference < 0) return;
  index = absolute_index(L,index);
  lua_rawgeti(L,index,0); lua_rawseti(L,index,reference);
  lua_pushinteger(L,reference); lua_rawseti(L,index,0);
}

/* Only strings emitted by this buffer occupy its live stack suffix. */
static int buffer_emit(luaL_Buffer *buffer) {
  size_t used = (size_t)(buffer->p - buffer->buffer);
  if (!used) return 0;
  lua_pushlstring(buffer->L, buffer->buffer, used);
  buffer->p = buffer->buffer;
  buffer->lvl++;
  return 1;
}
static void buffer_merge(luaL_Buffer *buffer) {
  int count = 1;
  size_t total;
  if (buffer->lvl < 2) return;
  total = lua_objlen(buffer->L, -1);
  while (count < buffer->lvl) {
    size_t next = lua_objlen(buffer->L, -count-1);
    if (buffer->lvl-count+1 < 10 && total <= next) break;
    total += next;
    count++;
  }
  lua_concat(buffer->L, count);
  buffer->lvl -= count-1;
}
API void luaL_buffinit(lua_State *L, luaL_Buffer *buffer) {
  buffer->L = L;
  buffer->p = buffer->buffer;
  buffer->lvl = 0;
}
API char *luaL_prepbuffer(luaL_Buffer *buffer) {
  if (buffer_emit(buffer)) buffer_merge(buffer);
  return buffer->buffer;
}
API void luaL_addlstring(luaL_Buffer *buffer, const char *bytes, size_t length) {
  while (length) {
    size_t room = (size_t)(buffer->buffer+LUAL_BUFFERSIZE-buffer->p);
    size_t part;
    if (!room) { luaL_prepbuffer(buffer); room = LUAL_BUFFERSIZE; }
    part = length < room ? length : room;
    memcpy(buffer->p, bytes, part);
    buffer->p += part;
    bytes += part;
    length -= part;
  }
}
API void luaL_addstring(luaL_Buffer *buffer, const char *string) {
  luaL_addlstring(buffer, string, strlen(string));
}
API void luaL_addvalue(luaL_Buffer *buffer) {
  size_t length;
  const char *bytes = lua_tolstring(buffer->L, -1, &length);
  size_t room = (size_t)(buffer->buffer+LUAL_BUFFERSIZE-buffer->p);
  if (length <= room) {
    /* Nonstrings have a null pointer and zero length in Lua 5.1. */
    if (length) memcpy(buffer->p, bytes, length);
    buffer->p += length;
    lua_settop(buffer->L, -2);
  } else {
    if (buffer_emit(buffer)) lua_insert(buffer->L, -2);
    buffer->lvl++;
    buffer_merge(buffer);
  }
}
API void luaL_pushresult(luaL_Buffer *buffer) {
  buffer_emit(buffer);
  lua_concat(buffer->L, buffer->lvl);
  buffer->lvl = 1;
}
API const char *luaL_gsub(lua_State *L, const char *string, const char *pattern, const char *replacement) {
  luaL_Buffer buffer;
  const char *match;
  size_t width = strlen(pattern);
  luaL_buffinit(L, &buffer);
  while ((match = strstr(string, pattern)) != NULL) {
    luaL_addlstring(&buffer, string, (size_t)(match-string));
    luaL_addstring(&buffer, replacement);
    string = match+width;
  }
  luaL_addstring(&buffer, string);
  luaL_pushresult(&buffer);
  return lua_tolstring(L, -1, NULL);
}
