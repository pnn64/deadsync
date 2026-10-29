local m=m or assert(package.loadlib(assert(hook_stack_library),'luaopen_native'))('hook yields')
local function pack(...)return {n=select('#',...),...}end
local function scalar(v)
  if v==nil then return 'nil' end
  if type(v)=='number' or type(v)=='boolean' then return tostring(v) end
  if type(v)=='table' then return 'table:'..tostring(v.kept) end
  return type(v)
end
local function emit(label,event,mode,style,r)
  local row={label,event,mode,style,r.n}
  for i=1,r.n do row[#row+1]=scalar(r[i])end
  print(unpack(row))
end
for _,event in ipairs{4,8,12} do
  for mode=14,23 do
   for _,style in ipairs{'lua','kept','collected'} do
    collectgarbage('stop')
    local co=coroutine.create(function(x,y,z)return x,y,z end)
    local storage={};local sink
    if mode==23 then sink=newproxy(true);local mt=getmetatable(sink);mt.__index=storage;mt.__newindex=storage
    else sink=setmetatable({}, {__index=setmetatable({}, {__index=storage}), __newindex=setmetatable({}, {__newindex=storage})})end
    m.hook_stack(co,event,mode,nil,sink)
    local first
    if style=='lua' then first=pack(coroutine.resume(co,17,29,42))
    else local status,n,t=m.hook_stack_resume(co,17,29,42);assert(status==1);first={n=n+1,true,unpack(t,1,n)}end
    emit('first',event,mode,style,first)
    local saved=debug.getregistry()['native.hook.stack.saved_top']
    local after=debug.getregistry()['native.hook.stack.after_yield']
    local status=debug.getregistry()['native.hook.stack.yield_status']
    print('window',event,mode,style,saved,mode>=16 and after or '-',mode>=16 and status or '-')
    local chosen=mode==16 and 1 or mode==18 and saved or mode==19 and 1 or mode==21 and 2 or 0
    local args={};for i=1,saved-chosen do args[i]=100+i end
    local second
    if style=='lua' then second=pack(coroutine.resume(co,unpack(args)))
    else
      if style=='collected' then m.hook_stack_clear(co);collectgarbage('collect') else args={} end
      local code,n,t=m.hook_stack_resume(co,unpack(args));assert(code==0);second={n=n+1,true,unpack(t,1,n)}
    end
    emit('second',event,mode,style,second)
    print('state',event,mode,style,coroutine.status(co),storage.kept or '-')
    collectgarbage('collect')
   end
  end
end
print('hook-yields-complete')
