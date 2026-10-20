-- Authored table writes can override compiler ownership markers.
return Def.Quad {
    OnCommand = function(self)
        local previous = getmetatable(self) or {}
        local meta = {}
        for key, value in pairs(previous) do meta[key] = value end
        meta.__newindex = function(actor, key, value)
            if key == '__songlua_compile_layer' then
                rawset(actor, key, 1)
            elseif type(previous.__newindex) == 'function' then
                previous.__newindex(actor, key, value)
            else
                rawset(actor, key, value)
            end
        end
        setmetatable(self, meta)
    end,
}
