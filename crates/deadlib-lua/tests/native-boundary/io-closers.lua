local function pack(...)return {n=select('#',...),...}end
local args={17,nil,false,'tail'}
local path=os.tmpname();local empty=assert(io.open(path,'w'));assert(empty:close())
local rows=0
for _,scope in ipairs{'close','method','gc','iterator','default'}do
  for _,lookup in ipairs{'direct','index'}do
    for _,mode in ipairs{'observe','results','replace','forward','error'}do
      for count=0,(scope=='default'and 0 or 4)do
        local f,fn,expected
        if scope=='iterator'then
          fn=io.lines(path);f=boundary_file_capture(fn,1);expected=fn
        else
          f=assert(io.tmpfile())
          expected=scope=='gc'and getmetatable(f).__gc or scope=='method'and f.close or io.close
          fn=expected
        end
        local original=debug.getfenv(f);local marker={}
        local closer=boundary_file_closer(mode,{unread='target upvalue'})
        debug.setfenv(closer,{unread='target environment'})
        local env={original=original.__close,marker=marker};local lookups=0
        if lookup=='direct'then env.__close=closer
        else setmetatable(env,{__index=function(t,key)
          assert(key=='__close'and debug.getinfo(2,'f').func==expected)
          lookups=lookups+1;return closer
        end})end
        debug.setfenv(f,env);closer_observation=nil
        local output=scope=='default'and io.output()
        if scope=='default'then io.output(f)end
        local result
        if scope=='iterator'then result=pack(pcall(fn,unpack(args,1,count)))
        elseif scope=='default'then result=pack(pcall(io.close))
        else result=pack(pcall(fn,f,unpack(args,1,count)))end
        if output then io.output(output)end
        local observed=closer_observation
        local status=mode=='error'and not result[1]and result[2]==marker
          or mode~='error'and result[1]
        local stack=observed and observed.n==(scope=='iterator'and 3 or scope=='default'and 3 or 3+count)
        local context=observed and observed['function']==expected and observed.environment==debug.getfenv(expected)
        local captures=observed and (scope=='iterator'and observed.first==f and observed.second==true
          or scope~='iterator'and observed.first==nil and observed.second==nil)
        local suffix=observed and observed[observed.n-1]==env and observed[observed.n]==closer
        local returned=true
        if mode~='error'then
          if scope=='iterator'or scope=='gc'then returned=result.n==1
          elseif mode=='observe'then returned=result.n==2 and result[2]==observed
          elseif mode=='results'then returned=result.n==4 and result[2]==17 and result[3]==nil and result[4]==false
          elseif mode=='replace'then returned=result.n==2 and result[2]==29
          else returned=result.n==2 and result[2]==true end
        end
        print('closer',scope,lookup,mode,count,not not status,not not stack,not not context,not not captures,not not suffix,returned,lookups)
        assert(status and stack and context and captures and suffix and returned)
        assert(lookups==(lookup=='index'and 1 or 0))
        for i=1,(scope=='iterator'or scope=='default')and 0 or count do assert(observed[i+1]==args[i])end
        assert(observed[1]==f)
        debug.setfenv(f,original)
        if io.type(f)=='file'then assert(f:close())end
        rows=rows+1
      end
    end
  end
end

-- A callback's changed C window is visible to its caller's return hook.
for _,scope in ipairs{'close','gc','iterator'}do
  for _,mode in ipairs{'results','replace'}do
    local f,fn
    if scope=='iterator'then fn=io.lines(path);f=boundary_file_capture(fn,1)
    else f=assert(io.tmpfile());fn=scope=='gc'and getmetatable(f).__gc or io.close end
    local original=debug.getfenv(f)
    local closer=boundary_file_closer(mode,false)
    debug.setfenv(f,{__close=closer,original=original.__close})
    local calls,returns,values=0,0
    debug.sethook(function(event)
      if debug.getinfo(2,'f').func~=fn then return end
      if event=='call'then calls=calls+1
      elseif event=='return'then
        returns=returns+1;values={n=0}
        for i=1,20 do local name,value=debug.getlocal(2,i);if not name then break end;values.n=i;values[i]=value end
      end
    end,'cr')
    if scope=='iterator'then fn()else fn(f)end
    debug.sethook()
    assert(calls==1 and returns==1)
    if mode=='replace'then assert(values.n==2 and values[1]==29 and type(values[2])=='table')
    else assert(values.n==7 and values[1]==f and values[3]==closer and values[4]==17 and values[5]==nil and values[6]==false and type(values[7])=='table')end
    print('closer-return',scope,mode,calls,returns,values.n)
    debug.setfenv(f,original);if io.type(f)=='file'then assert(f:close())end;rows=rows+1
  end
end

-- Calling a body does not retain the selected closure's Lua captures or env.
closer_observation=nil
for _,capture in ipairs{false,true}do
  closer_erased=false
  local proxy=newproxy(true);getmetatable(proxy).__gc=function()closer_erased=true end
  local closer=boundary_file_closer('erase',capture and proxy or false)
  debug.setfenv(closer,capture and {}or {proxy=proxy})
  local file=assert(io.tmpfile());local original=debug.getfenv(file)
  debug.setfenv(file,{__close=closer});proxy=nil;closer=nil
  assert(io.close(file)==true and closer_erased)
  debug.setfenv(file,original);assert(file:close())
  print('closer-erased',capture,closer_erased);rows=rows+1
end

-- A delegated body uses the calling closure's environment and captures.
for _,scope in ipairs{'close','gc','iterator'}do
  local f,target
  if scope=='iterator'then target=io.lines(path);f=boundary_file_capture(target,1)
  else f=assert(io.tmpfile());target=scope=='gc'and getmetatable(f).__gc or io.close end
  local original=debug.getfenv(f);local marker={}
  local first,second=scope=='iterator'and f or marker,scope=='iterator'
  local call=boundary_file_body(first,second);local env={marker='calling closure'}
  debug.setfenv(call,env)
  local closer=boundary_file_closer('observe',{unread='target capture'})
  debug.setfenv(closer,{unread='target environment'})
  debug.setfenv(f,{__close=closer});closer_observation=nil
  local result=scope=='iterator'and pack(call(target))or pack(call(target,f,17,nil))
  local observation=closer_observation
  assert(observation['function']==call and observation.environment==env)
  assert(observation.first==first and observation.second==second)
  assert(result.n==(scope=='close'and 1 or 0))
  print('closer-body',scope,observation.n,observation['function']==call,observation.environment==env,observation.first==first,observation.second==second)
  debug.setfenv(f,original);assert(f:close());rows=rows+1
end

-- Direct callbacks and delegated library bodies preserve a yielded C prefix.
for _,delegate in ipairs{false,true}do
  for _,mode in ipairs{'yield-tail','yield-zero','builtin'}do
    local f=assert(io.tmpfile());local original=debug.getfenv(f)
    local closer=mode=='builtin'and coroutine.yield or boundary_file_closer(mode,false)
    debug.setfenv(f,{__close=closer})
    local call=delegate and boundary_file_body(false,false)
    local co=coroutine.create(function()
      if call then return call(io.close,f)else return io.close(f)end
    end)
    local returned
    debug.sethook(co,function(event)
      if event~='return'or debug.getinfo(2,'f').func~=(call or io.close)then return end
      returned={n=0}
      for i=1,20 do local name,value=debug.getlocal(2,i);if not name then break end;returned.n=i;returned[i]=value end
    end,'r')
    local first=pack(coroutine.resume(co))
    assert(first[1]and coroutine.status(co)=='suspended')
    if mode=='yield-tail'then assert(first.n==2 and first[2]==41)
    elseif mode=='yield-zero'then assert(first.n==1)
    else assert(first.n==4 and first[2]==f and first[4]==closer)end
    local result=pack(coroutine.resume(co,11,nil,false))
    assert(result.n==4 and result[1]and result[2]==11 and result[3]==nil and result[4]==false)
    assert(coroutine.status(co)=='dead')
    local prefix=mode=='builtin'and 0 or 3
    print('closer-yield',delegate,mode,first.n,result.n,coroutine.status(co),returned and returned.n)
    assert(returned and returned.n==prefix+4 and returned[prefix+1]==11 and returned[prefix+2]==nil and returned[prefix+3]==false)
    if prefix~=0 then assert(returned[1]==f and returned[3]==closer)end
    debug.setfenv(f,original);assert(f:close());rows=rows+1
  end
end

-- A hook's raw C body retains the interrupted Lua activation.
for _,mode in ipairs{'observe','results','replace','forward'}do
  local file=assert(io.tmpfile());local original=debug.getfenv(file)
  local closer=boundary_file_closer(mode,false)
  debug.setfenv(file,{__close=closer,original=original.__close})
  local fn=function(f)return 17 end;local env={marker='Lua activation'}
  debug.setfenv(fn,env);closer_observation=nil
  local co=coroutine.create(fn)
  boundary_file_hook(io.close,fn,co)
  local result=pack(coroutine.resume(co,file))
  assert(result.n==2 and result[1]and result[2]==17 and coroutine.status(co)=='dead')
  local observed=closer_observation
  assert(observed['function']==fn and observed.environment==env and observed.first==nil and observed.second==nil)
  print('closer-hook',mode,result.n,observed and observed.n or 0,coroutine.status(co))
  debug.setfenv(file,original);if io.type(file)=='file'then assert(file:close())end;rows=rows+1
end

assert(os.remove(path))
print('io-closers-complete',rows)
