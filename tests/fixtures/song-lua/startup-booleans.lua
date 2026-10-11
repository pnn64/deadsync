local function each_option(callback)
    for _, player in ipairs(GAMESTATE:GetEnabledPlayers()) do
        callback(GAMESTATE:GetPlayerState(player):GetPlayerOptions('ModsLevel_Song'))
    end
end
return Def.Actor{
    InitCommand=function()
        each_option(function(options)
            assert(options:Mirror(true) == false)
            assert(options:NoHolds(false) == false)
        end)
    end,
    OnCommand=function(self)
        each_option(function(options)
            assert(options:Mirror(nil, false) == options)
            assert(options:Mirror(false) == true)
        end)
        self:sleep(0.5):queuecommand('Advance')
    end,
    AdvanceCommand=function()
        each_option(function(options)
            assert(options:Mirror(true) == false)
        end)
    end,
}
