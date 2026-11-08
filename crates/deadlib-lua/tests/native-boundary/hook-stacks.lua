local m=m or assert(package.loadlib(assert(hook_stack_library),'luaopen_native'))('hook stacks')
local function pack(...)return {n=select('#',...),...}end
local function scalar(v)
  if v==nil then return 'nil' end
  if type(v)=='number' or type(v)=='boolean' then return tostring(v) end
  if type(v)=='table' then return 'table:'..tostring(v.kept) end
  if type(v)=='string' then return 'string:'..v end
  return type(v)
end
for _,event in ipairs{1,2,4,8} do
  for mode=0,13 do
    collectgarbage('stop')
    local co=coroutine.create(function(x,y,z)return x,y,z end)
    m.hook_stack(co,event,mode)
    local r=pack(coroutine.resume(co,17,29,42))
    local row={event,mode,r.n}
    for i=1,r.n do row[#row+1]=scalar(r[i])end
    print(unpack(row))
  end
end
for _,kind in ipairs{'string','table'} do
  for _,event in ipairs{1,2,4,8} do
    for _,mode in ipairs{0,1,2,3,4,5,6,8,11,12,13} do
      collectgarbage('stop')
      local co=coroutine.create(function(x,y,z)return x,y,z end)
      m.hook_stack(co,event,mode)
      local a,b,c
      if kind=='table' then a,b,c={kept=17},{kept=29},{kept=42}
      else a,b,c='alpha','beta','gamma' end
      local r=pack(coroutine.resume(co,a,b,c))
      local row={kind,event,mode,r.n}
      for i=1,r.n do row[#row+1]=scalar(r[i])end
      print(unpack(row))
    end
  end
end
for _,event in ipairs{1,2} do
  for mode=0,13 do
    collectgarbage('stop')
    local co=coroutine.create(function()return m.hook_stack_echo(17,29,42)end)
    m.hook_stack(co,event,mode,m.hook_stack_echo)
    local r=pack(coroutine.resume(co))
    local row={'native',event,mode,r.n}
    for i=1,r.n do row[#row+1]=scalar(r[i])end
    print(unpack(row))
  end
end
print('hook-stacks-complete')
