-- Named native children are separate from the authored array. Preserve aliases,
-- duplicate groups and their creation order when exporting a lookup snapshot.
local type, next, rawget, rawset = type, next, rawget, rawset
local getmetatable, setmetatable, error = getmetatable, setmetatable, error
return function(group, getname)
    local function istable(t)
        if type(t)~='table' then error('child table expected') end
        return t
    end
    local function isgroup(t)
        local mt=getmetatable(t)
        return mt and mt.__songlua_child_group
    end
    local function registry(actor)
        local children=actor.__songlua_children
        if children~=nil then return istable(children) end
        local order={}
        children=setmetatable({}, {__songlua_child_names=order,
            __newindex=function(t,k,v)
                if v~=nil then order[#order+1]=k end
                rawset(t,k,v)
            end})
        actor.__songlua_children=children
        return children
    end
    local function push_once(t,value)
        for i=1,#t do if rawget(t,i)==value then return end end
        rawset(t,#t+1,value)
    end
    local function named(actor)
        local result={}
        local source
        if not rawget(actor,'__songlua_owned_child_lookup') then source=registry(actor) end
        local function copy(key,value)
            if value==nil or rawget(result,key)~=nil then return end
            if type(value)=='table' and isgroup(value) then
                local values={}
                local i=1
                while rawget(value,i)~=nil do
                    push_once(values,istable(rawget(value,i)));i=i+1
                end
                value=#values==1 and values[1] or group(values)
            end
            rawset(result,key,value)
        end
        local mt=source and getmetatable(source)
        local order=mt and mt.__songlua_child_names
        if order~=nil then
            istable(order)
            local i=1
            while rawget(order,i)~=nil do
                local key=rawget(order,i);copy(key,rawget(source,key));i=i+1
            end
        end
        if source then for key,value in next,source do copy(key,value) end end
        local i=1
        while rawget(actor,i)~=nil do
            local child=rawget(actor,i);i=i+1
            if type(child)=='table' then
                local name=getname(child)
                local old=rawget(result,name)
                if type(old)=='table' and isgroup(old) then push_once(old,child)
                elseif type(old)=='table' then
                    if old~=child then rawset(result,name,group({old,child})) end
                else rawset(result,name,child) end
            end
        end
        return result
    end
    local function direct(actor)
        local result={}
        local i=1
        while rawget(actor,i)~=nil do
            local child=rawget(actor,i);i=i+1
            if type(child)=='table' then push_once(result,child) end
        end
        for _,child in next,registry(actor) do
            if type(child)=='table' then
                if isgroup(child) then
                    local i=1
                    while rawget(child,i)~=nil do
                        local value=rawget(child,i);i=i+1
                        if type(value)=='table' then push_once(result,value) end
                    end
                else push_once(result,child) end
            end
        end
        return result
    end
    local function remove(actor, name, all)
        if all then
            for i=1,#actor do rawset(actor,i,nil) end
        end
        local children=registry(actor)
        local mt=getmetatable(children)
        local order=mt and mt.__songlua_child_names
        if all then
            for key in next,children do rawset(children,key,nil) end
            if order~=nil then
                istable(order)
                for i=1,#order do rawset(order,i,nil) end
            end
            return
        end
        children[name]=nil
        if order~=nil then
            istable(order)
            local write=1
            local count=#order
            for read=1,count do
                local key=rawget(order,read)
                if key~=name then rawset(order,write,key);write=write+1 end
            end
            for i=write,count do rawset(order,i,nil) end
        end
        local write=1
        local count=#actor
        for read=1,count do
            local value=rawget(actor,read)
            if not(type(value)=='table' and getname(value)==name) then
                if write~=read then rawset(actor,write,value) end
                write=write+1
            end
        end
        for i=write,count do rawset(actor,i,nil) end
    end
    return registry, named, direct, isgroup, remove
end
