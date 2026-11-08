do local co=coroutine.create(function()boundary_state()end);assert(coroutine.resume(co))end;collectgarbage()
local sources={
 '', 'return ', 'return 42', 'local a=17;return a+25',
 'return function(a,...)local b="hello";return function(c)return a,b,c,... end end',
 'local function f(a)return function(b)return a+b end end;return f',
 'return {x="hi",[17]=false,true,nil}',
 '-- comment\nreturn [=[text]=]', 'return "a\\000b"', 'local =',
 'return function(x)return x end, function(y)return y end',
 'return function(self)return self.x end',
 'for i=1,2 do local x=i end;return true',
 'return 1+2, -0, false==nil, true and "x"',
 'local a; return a or true', 'function t:f(x)return self,x end',
 'return function(a,...)local b="hello";return function(c,...)return a,b,c,... end end',
 'for k,v in pairs({1}) do local y=k end;return 42',
 'local f=function()return 1 end;local g=function()return f end;return g',
}
for _,path in ipairs({'reader-roots-nested.luac','reader-roots-stripped.luac'}) do
 local file=assert(io.open(path,'rb'));sources[#sources+1]=file:read('*a');file:close()
end
local function describe(value,owner)
 if value==owner then return 'self' end
 local t=type(value)
 if t=='string' then return 's'..string.format('%q',value) end
 if t=='number' then return 'n'..string.format('%.17g',value) end
 return t..':'..tostring(value)
end
local completed,observations=0,0
for case,source in ipairs(sources) do
 for _,mode in ipairs({0,1,2}) do
  for width=1,math.max(#source,1) do
   if mode==1 or width==1 or width==2 or width==7 or width==#source then
    local rows={}
    local status,value,calls=boundary_loadroots(source,width,function(n,top,roots)
     local row={tostring(n),tostring(top)}
     for _,root in ipairs(roots)do
      local s=tostring(root[1])
      if root[1]==5 then
       local fields={}
       for k,v in pairs(root[2])do fields[#fields+1]=describe(k,root[2])..'='..describe(v,root[2])end
       table.sort(fields);s=s..'{'..table.concat(fields,',')..'}'
      else
       assert(root[1]==9 and root[2] and type(root[4])=='proto')
      end
      assert(root[3]);row[#row+1]=s
     end
     rows[#rows+1]=table.concat(row,'|')
     observations=observations+1
     collectgarbage()
    end,mode)
    print('roots-case',case,width,mode,status,calls,table.concat(rows,';'))
    completed=completed+1
   end
  end
 end
end
-- A native-reader prototype is the object later owned by its compiled closure.
-- A surviving child closure retains its own descendants, without its parent or
-- sibling. Constants tables have no owner after parsing finishes.
local prototypes,tables={},setmetatable({},{__mode='k'})
local weak=setmetatable({},{__mode='v'})
local status,entry=boundary_loadroots('return function()return 17 end,function()return 42 end',1,function(_,_,roots)
 for _,root in ipairs(roots)do
  if root[1]==5 then tables[root[4]]=true
  elseif not prototypes[root[4]]then
   local id=#weak+1;prototypes[root[4]]=id;weak[id]=root[4]
  end
 end
 collectgarbage()
end)
assert(status==0 and #weak==3)
prototypes=nil;collectgarbage();assert(next(tables)==nil and weak[1] and weak[2] and weak[3])
local first,second=entry();assert(first()==17 and second()==42)
entry=nil;second=nil;collectgarbage();assert(weak[1]==nil and weak[2] and weak[3]==nil)
first=nil;collectgarbage();assert(next(weak)==nil)
print('native-reader-roots-complete',completed,observations)
