-- Riddle's recurring Song-option writes: fast swaps, then a slower merge.
local options = GAMESTATE:GetPlayerState(PLAYER_1):GetPlayerOptions('ModsLevel_Song')
return Def.Actor {
    OnCommand=function(self) self:queuecommand('Update') end,
    UpdateCommand=function(self)
        local beat = GAMESTATE:GetSongBeat()
        if beat < 0.25 then
            options:Flip(0, 1000):Invert(0, 1000)
        elseif beat < 0.5 then
            options:Invert(1, 1000)
        elseif beat < 1 then
            options:Flip(1, 1000):Invert(0, 1000)
        elseif beat < 2 then
            options:Flip(0, 1000)
        else
            options:Flip(0.5, 10):Mini(-1, 10):Reverse(0.1, 10)
        end
        self:sleep(1/60):queuecommand('Update')
    end,
}
