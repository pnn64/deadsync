return Def.ActorFrame {
    Name = 'DrawClock',
    OnCommand = function(self)
        self:SetDrawFunction(function()
            local pn = GAMESTATE:GetSongPosition():GetMusicSeconds() < 0.5 and 1 or 2
            local player = SCREENMAN:GetTopScreen():GetChild('PlayerP' .. pn)
            player:visible(true):Draw():visible(false)
        end)
    end,
}
