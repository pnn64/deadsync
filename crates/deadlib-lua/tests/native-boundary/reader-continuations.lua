-- Golden stack/status rows from the pinned standalone Lua 5.1.5 oracle.
-- A binary reader count of -1 uses the ABI-specific wire length.
local expected={
  {2,1,2,0,0,1,"string","suspended"},
  {2,1,2,0,0,2,"string","suspended"},
  {2,1,2,0,0,1,"string","suspended"},
  {2,1,2,0,0,2,"string","suspended"},
  {2,1,2,0,0,1,"string","suspended"},
  {2,1,2,0,0,2,"string","suspended"},
  {2,1,2,0,0,1,"string","suspended"},
  {2,1,2,0,0,2,"string","suspended"},
  {2,1,2,0,0,1,"string","suspended"},
  {2,1,2,0,0,2,"string","suspended"},
  {2,1,2,0,0,1,"string","suspended"},
  {2,1,2,0,0,2,"string","suspended"},
  {2,1,2,0,0,1,"string","suspended"},
  {2,1,2,0,0,2,"string","suspended"},
  {2,1,2,0,0,1,"string","suspended"},
  {2,1,2,0,0,2,"string","suspended"},
  {2,1,2,0,0,1,"string","suspended"},
  {2,1,2,0,0,2,"string","suspended"},
  {2,1,2,0,0,1,"string","suspended"},
  {2,1,2,0,0,2,"string","suspended"},
  {2,1,2,0,0,1,"string","suspended"},
  {2,1,2,0,0,2,"string","suspended"},
  {2,1,2,0,0,1,"string","suspended"},
  {0,1,2,0,0,-1,"function","suspended"},
  {0,0,2,0,1,2,"function","suspended"},
  {0,0,2,0,1,2,"function","suspended"},
  {0,0,2,0,1,2,"function","suspended"},
  {0,0,2,0,1,2,"function","suspended"},
  {0,0,2,0,1,2,"function","suspended"},
  {0,0,2,0,1,2,"function","suspended"},
  {0,0,2,0,1,10,"function","suspended"},
  {0,0,0,0,1,10,"nil","suspended"},
  {0,0,2,0,1,4,"function","suspended"},
  {0,0,0,0,1,4,"nil","suspended"},
  {0,0,2,0,1,2,"function","suspended"},
  {2,0,1,0,0,2,"string","dead"},
  {3,0,1,0,1,17,"string","suspended"},
  {3,0,1,0,1,17,"string","suspended"},
  {3,0,1,0,1,7,"string","suspended"},
  {3,0,1,0,1,7,"string","suspended"},
  {3,0,1,0,1,2,"string","suspended"},
  {2,0,1,0,0,2,"string","dead"},
  {0,0,2,0,1,-1,"function","suspended"},
  {0,0,2,0,1,-1,"function","suspended"},
  {0,0,2,0,1,-1,"function","suspended"},
  {0,0,2,0,1,-1,"function","suspended"},
  {0,0,2,0,1,-1,"function","suspended"},
  {0,0,1,0,0,-1,"function","dead"},
  {0,0,2,1,1,2,"function","suspended"},
  {0,0,2,1,1,2,"function","suspended"},
  {0,0,2,1,1,2,"function","suspended"},
  {0,0,2,1,1,2,"function","suspended"},
  {0,0,2,1,1,2,"function","suspended"},
  {0,0,2,1,1,2,"function","suspended"},
  {0,0,2,1,1,10,"function","suspended"},
  {0,0,0,1,1,10,"nil","suspended"},
  {0,0,2,1,1,4,"function","suspended"},
  {0,0,0,1,1,4,"nil","suspended"},
  {0,0,2,1,1,2,"function","suspended"},
  {2,0,1,1,1,2,"string","suspended"},
  {3,0,1,1,1,17,"string","suspended"},
  {3,0,1,1,1,17,"string","suspended"},
  {3,0,1,1,1,7,"string","suspended"},
  {3,0,1,1,1,7,"string","suspended"},
  {3,0,1,1,1,2,"string","suspended"},
  {2,0,1,1,1,2,"string","suspended"},
  {0,0,2,1,1,-1,"function","suspended"},
  {0,0,2,1,1,-1,"function","suspended"},
  {0,0,2,1,1,-1,"function","suspended"},
  {0,0,2,1,1,-1,"function","suspended"},
  {0,0,2,1,1,-1,"function","suspended"},
  {0,0,1,1,1,-1,"function","suspended"},
  {0,2,2,2,1,2,"function","suspended"},
  {0,2,2,2,1,2,"function","suspended"},
  {0,2,2,2,1,2,"function","suspended"},
  {0,2,2,2,1,2,"function","suspended"},
  {0,2,2,2,1,2,"function","suspended"},
  {0,2,2,2,1,2,"function","suspended"},
  {0,2,2,2,1,10,"function","suspended"},
  {0,2,0,2,1,10,"nil","suspended"},
  {0,2,2,2,1,4,"function","suspended"},
  {0,2,0,2,1,4,"nil","suspended"},
  {0,2,2,2,1,2,"function","suspended"},
  {2,2,3,2,2,2,"string","dead"},
  {3,2,3,2,1,17,"string","suspended"},
  {3,2,3,2,1,17,"string","suspended"},
  {3,2,3,2,1,7,"string","suspended"},
  {3,2,3,2,1,7,"string","suspended"},
  {3,2,3,2,1,2,"string","suspended"},
  {2,2,3,2,2,2,"string","dead"},
  {0,2,2,2,1,-1,"function","suspended"},
  {0,2,2,2,1,-1,"function","suspended"},
  {0,2,2,2,1,-1,"function","suspended"},
  {0,2,2,2,1,-1,"function","suspended"},
  {0,2,2,2,1,-1,"function","suspended"},
  {0,2,3,2,2,-1,"function","dead"},
}
local diagnostics={
  "attempt to yield across metamethod/C-call boundary",
  "attempt to yield across metamethod/C-call boundary",
  "attempt to yield across metamethod/C-call boundary",
  "attempt to yield across metamethod/C-call boundary",
  "attempt to yield across metamethod/C-call boundary",
  "attempt to yield across metamethod/C-call boundary",
  "attempt to yield across metamethod/C-call boundary",
  "attempt to yield across metamethod/C-call boundary",
  "attempt to yield across metamethod/C-call boundary",
  "attempt to yield across metamethod/C-call boundary",
  "attempt to yield across metamethod/C-call boundary",
  "attempt to yield across metamethod/C-call boundary",
  "attempt to yield across metamethod/C-call boundary",
  "attempt to yield across metamethod/C-call boundary",
  "attempt to yield across metamethod/C-call boundary",
  "attempt to yield across metamethod/C-call boundary",
  "attempt to yield across metamethod/C-call boundary",
  "attempt to yield across metamethod/C-call boundary",
  "attempt to yield across metamethod/C-call boundary",
  "attempt to yield across metamethod/C-call boundary",
  "attempt to yield across metamethod/C-call boundary",
  "attempt to yield across metamethod/C-call boundary",
  "attempt to yield across metamethod/C-call boundary",
  "",
  "",
  "",
  "",
  "",
  "",
  "",
  "",
  "",
  "",
  "",
  "",
  "attempt to yield across metamethod/C-call boundary",
  "reader-once:1: <name> or '...' expected near '<eof>'",
  "reader-once:1: <name> or '...' expected near '<eof>'",
  "reader-once:1: <name> or '...' expected near '<eof>'",
  "reader-once:1: <name> or '...' expected near '<eof>'",
  "reader-once:1: <name> or '...' expected near '<eof>'",
  "attempt to yield across metamethod/C-call boundary",
  "",
  "",
  "",
  "",
  "",
  "",
  "",
  "",
  "",
  "",
  "",
  "",
  "",
  "",
  "",
  "",
  "",
  "attempt to yield across metamethod/C-call boundary",
  "reader-once:1: <name> or '...' expected near '<eof>'",
  "reader-once:1: <name> or '...' expected near '<eof>'",
  "reader-once:1: <name> or '...' expected near '<eof>'",
  "reader-once:1: <name> or '...' expected near '<eof>'",
  "reader-once:1: <name> or '...' expected near '<eof>'",
  "attempt to yield across metamethod/C-call boundary",
  "",
  "",
  "",
  "",
  "",
  "",
  "",
  "",
  "",
  "",
  "",
  "",
  "",
  "",
  "",
  "",
  "",
  "attempt to yield across metamethod/C-call boundary",
  "reader-once:1: <name> or '...' expected near '<eof>'",
  "reader-once:1: <name> or '...' expected near '<eof>'",
  "reader-once:1: <name> or '...' expected near '<eof>'",
  "reader-once:1: <name> or '...' expected near '<eof>'",
  "reader-once:1: <name> or '...' expected near '<eof>'",
  "attempt to yield across metamethod/C-call boundary",
  "",
  "",
  "",
  "",
  "",
  "",
}

local count=0
local labels={'empty','text','syntax','binary'}
local forms={'','return 17','return function(',string.dump(assert(loadstring('return 17','=reader-wire')))}
local function make(kind)
  local co=coroutine.create(function()
    if kind=='failed'then error('old')end
    if kind=='yielded'then coroutine.yield()end
  end)
  if kind~='fresh'then coroutine.resume(co)end
  return co
end
local function load(kind,source,block,visit)
  local co=make(kind)
  return co,boundary_thread_load(co,source,function(i)
    if i==visit then coroutine.yield(29)end
  end,'=reader-once',block)
end
for _,kind in ipairs{'fresh','completed','yielded','failed'}do
  for index,source in ipairs(forms)do
    for _,block in ipairs{1,3,1000}do
      for _,visit in ipairs{1,2}do
        local co,s,b,a,old,new,n,result=load(kind,source,block,visit)
        local actual={s,b,a,old,new,n,type(result),coroutine.status(co)}
        local row=expected[count+1]
        if row[6]==-1 then row[6]=math.ceil(#source/block)end
        for i=1,8 do assert(actual[i]==row[i],kind..'-'..labels[index]..'-'..block..'-'..visit..' field '..i..': '..tostring(actual[i])..' ~= '..tostring(row[i]))end
        if type(result)=='string'then assert(result==diagnostics[count+1])end
        if type(result)=='function'then
          if index==1 then assert(select('#',result())==0)else assert(result()==17)end
        end
        print(kind..'-'..labels[index],block,visit,unpack(actual));count=count+1
        collectgarbage();collectgarbage()
      end
    end
  end
end
local function pack(...)return select('#',...),{...}end
for _,index in ipairs{1,2,4}do
  for _,block in ipairs{1,3,1000}do
    for _,visit in ipairs{1,2}do
      local co,s,b,a,old,new,n,result=load('completed',forms[index],block,visit)
      collectgarbage();collectgarbage()
      local nr,values=pack(coroutine.resume(co,41,43))
      local shape='empty'
      if index==2 and block<1000 and visit==2 then
        -- With a captured reader condition the reference returns the parser's
        -- two anchors after this yield. Validate them without printing addresses.
        assert(nr==3 and values[1]and type(values[2])=='table'and type(values[3])=='proto')
        assert(getmetatable(values[2])==nil and values[2]['return']==true and values[2][17]==0)
        local entries=0;for k,v in pairs(values[2])do entries=entries+1 end;assert(entries==2)
        collectgarbage();assert(values[2]['return']==true and type(values[3])=='proto')
        shape='constants+proto'
      elseif new==1 then assert(nr==1 and values[1])
      else assert(nr==2 and not values[1]and values[2]=='cannot resume dead coroutine');shape=values[2]end
      assert(coroutine.status(co)=='dead')
      print('reader-resume',labels[index],block,visit,nr,values[1],shape,coroutine.status(co));count=count+1
      collectgarbage();collectgarbage()
    end
  end
end
print('reader-continuations-complete',count)
