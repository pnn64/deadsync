local globalValues = { amount = 0, calls = 0 }
mods_ease = {
    {1, 2, 4, 12, function(value)
        globalValues.amount = value
        globalValues.calls = globalValues.calls + 1
    end},
}

local function updateEntries(entries, values, actor)
    local beat = GAMESTATE:GetSongBeat()
    for _, entry in ipairs(entries) do
        if beat >= entry[1] and beat <= entry[1] + entry[2] then
            local t = (beat - entry[1]) / entry[2]
            entry[5](entry[3] + (entry[4] - entry[3]) * t * t)
        end
    end
    actor:x(values.amount):y(values.calls)
end

local function localReader()
    local values = { amount = 0, calls = 0 }
    local mods_ease = {
        {1, 2, 20, 40, function(value)
            values.amount = value
            values.calls = values.calls + 1
        end},
    }
    return Def.Quad{
        Name = "LocalEaseTable",
        InitCommand = function(self)
            self:zoomto(10, 10)
            self:SetUpdateFunction(function()
                updateEntries(mods_ease, values, self)
            end)
        end,
    }
end

return Def.ActorFrame{
    Def.Quad{
        Name = "GlobalEaseTable",
        InitCommand = function(self)
            self:zoomto(10, 10)
            self:SetUpdateFunction(function()
                updateEntries(mods_ease, globalValues, self)
            end)
        end,
    },
    localReader(),
}
