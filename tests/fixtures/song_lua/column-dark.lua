-- Exercise both PlayerOptions methods and percentage-form modifier strings.
local p1 = GAMESTATE:GetPlayerState(0):GetPlayerOptions('ModsLevel_Song')
local p2 = GAMESTATE:GetPlayerState(1):GetPlayerOptions('ModsLevel_Song')
return Def.ActorFrame{
    OnCommand = function(self)
        self:SetUpdateFunction(function()
            local beat = GAMESTATE:GetSongBeat()
            p1:Dark(0.25, 10000)
            p2:Dark1(0.75, 10000)
            if beat >= 2 and beat < 4 then
                p1:FromString('*10000 0 dark1,*10000 50 dark2,*10000 -25 dark3,*10000 150 dark4')
            else
                p1:Dark1(1, 10000)
                p1:Dark2(-0.5, 10000)
                p1:Dark3(0.25, 10000)
                p1:Dark4(0, 10000)
            end
        end)
    end,
}
