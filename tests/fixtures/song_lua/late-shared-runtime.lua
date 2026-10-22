-- Mawaru7 starts its foreground after beat zero and uses ordinary Player
-- setters to leave and return to the notefield between minigames.
local phase = 0
return Def.ActorFrame{
    OnCommand=function(self) self:ztest(1) end,
    InitCommand=function(self)
        self:SetUpdateFunction(function()
            local beat = GAMESTATE:GetSongBeat()
            local player = SCREENMAN:GetTopScreen():GetChild('PlayerP1')
            local options = GAMESTATE:GetPlayerState(PLAYER_1):GetPlayerOptions('ModsLevel_Song')
            if beat > 4 and phase == 0 then
                player:x(-9999)
                SCREENMAN:GetTopScreen():y(12)
                options:Stealth(1, 10000)
                phase = 1
            elseif beat > 6 and phase == 1 then
                player:x(SCREEN_WIDTH * .25)
                SCREENMAN:GetTopScreen():y(0)
                options:Stealth(0, 10000)
                phase = 2
            end
        end)
    end,
    Def.Quad{
        Name='LateDepthDisabled',
        OnCommand=function(self) self:ztest(0):blend("BlendMode_NoEffect") end,
        InitCommand=function(self) self:xy(120, 80):zoomto(8, 8) end,
    },
    Def.Quad{
        Name='LateForegroundMarker',
        InitCommand=function(self) self:xy(80, 80):zoomto(16, 16) end,
    },
}
