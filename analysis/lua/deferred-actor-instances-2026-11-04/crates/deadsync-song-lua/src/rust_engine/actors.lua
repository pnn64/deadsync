-- Private structural helpers for the Rust actor host. Instance state and command
-- execution live in actors.rs. Capture primitives before authored scripts run.
local pairs, next, rawget = pairs, next, rawget
local tonumber, tostring, type = tonumber, tostring, type
local setmetatable, ipairs, error, sort = setmetatable, ipairs, error, table.sort

return function(methods)
    Actor = methods.Actor
    ActorFrame = setmetatable(methods.ActorFrame, {__index=Actor, __metatable='(hidden)'})
    Sprite = setmetatable({}, {__index=Actor, __metatable='(hidden)'})
    setmetatable(Actor, {__metatable='(hidden)'})
    local classes = {Actor=Actor, ActorFrame=ActorFrame, Quad=Sprite}
    local metas = {}
    for name, class in pairs(classes) do
        metas[name] = {__index=class, __metatable='(hidden)'}
    end
    local function create(class, parent)
        local ctx = {}
        setmetatable(ctx, ctx)
        if parent then ctx.__index = parent.ctx end
        return setmetatable({ctx=ctx}, metas[class])
    end
    -- Snapshot the complete XNode-shaped input before running any Init callback.
    -- Array entries precede next traversal; the native duplicate range counts
    -- table entries, not array slots. Ancestors are skipped, shared siblings copied.
    local function snapshot(source, active)
        local attrs, entries, nodes = {}, {}, {}
        local i = 1
        while rawget(source, i) ~= nil do
            local value = rawget(source, i)
            if type(value) == 'table' then entries[#entries+1] = {tostring(i), value} end
            i = i + 1
        end
        local prefix = #entries
        for key, value in next, source do
            if type(key) ~= 'string' and type(key) ~= 'number' then
                error('actor declaration keys must be strings or numbers')
            end
            if type(value) == 'table' then
                local index = tonumber(key)
                if not (index and index >= 1 and index <= prefix and index % 1 == 0) then
                    entries[#entries+1] = {tostring(key), value}
                end
            else
                attrs[tostring(key)] = value
            end
        end
        for _, entry in ipairs(entries) do
            local key, value = entry[1], entry[2]
            if not active[value] then
                active[value] = true
                nodes[#nodes+1] = {key, snapshot(value, active)}
                active[value] = nil
            end
        end
        return {attrs, nodes}
    end
    local function group(actors, class)
        return setmetatable(actors, {__index=function(t, key)
            if tonumber(key) then return rawget(t, key) end
            -- Native duplicate-name groups use the last child's class method,
            -- ignoring an instance override, and forward every result.
            local method = class[key]
            return function(self, ...) return method(self[#self], ...) end
        end})
    end
    return create, function(source) return snapshot(source, {}) end, group, classes,
        function(t) local keys={} for k in next,t do keys[#keys+1]=k end sort(keys) return keys end
end
