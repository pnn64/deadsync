local m=m or assert(package.loadlib(assert(hook_stack_library),'luaopen_native'))('hook stacks')
local function pack(...)return {n=select('#',...),...}end
local function scalar(v)
  if v==nil then return 'nil' end
  if type(v)=='number' or type(v)=='boolean' then return tostring(v) end
  if type(v)=='table' then return 'table:'..tostring(v.kept) end
  if type(v)=='string' then return 'string:'..v end
  return type(v)
end
for _,event in ipairs{4,8} do
  for mode=14,15 do
    for _,extra in ipairs{false,true} do
      collectgarbage('stop')
      local co=coroutine.create(function(x,y,z)return x,y,z end)
      m.hook_stack(co,event,mode)
      local first=pack(coroutine.resume(co,17,29,42))
      local second
      if extra then second=pack(coroutine.resume(co,{kept=99}))
      else second=pack(coroutine.resume(co)) end
      local row={'yield',event,mode,extra,first.n,scalar(first[1]),second.n}
      for i=1,second.n do row[#row+1]=scalar(second[i])end
      row[#row+1]=coroutine.status(co)
      print(unpack(row))
    end
  end
end
print('hook-stacks-yield-complete')
