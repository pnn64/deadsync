local p1 = GAMESTATE:GetPlayerState(PLAYER_1)
local p2 = GAMESTATE:GetPlayerState(PLAYER_2)
local phase = 0
return Def.ActorFrame {
    OnCommand = function(self)
        local options = p1:GetPlayerOptions("ModsLevel_Song")
        assert(options:FailSetting("FailType_Off") == "FailType_Immediate")
        assert(options:FailSetting() == "FailType_Off")
        assert(p1:GetPlayerOptionsString("ModsLevel_Song"):find("FailOff"))
        assert(not pcall(function() options:FailSetting("invalid") end))
        assert(options:FailSetting() == "FailType_Off")
        local other = p2:GetPlayerOptions("ModsLevel_Song")
        assert(other:FailSetting("FailType_ImmediateContinue", true) == other)
        self:SetUpdateFunction(function()
            local beat = GAMESTATE:GetSongBeat()
            if beat >= 2 and phase < 2 then
                p1:SetPlayerOptions("ModsLevel_Song", "")
                assert(options:FailSetting() == "FailType_Immediate")
                phase = 2
            elseif beat >= 1 and phase < 1 then
                options:FromString("failatend")
                assert(options:FailSetting() == "FailType_EndOfSong")
                assert(p1:GetPlayerOptionsString("ModsLevel_Song"):find("FailAtEnd"))
                phase = 1
            end
        end)
    end
}
