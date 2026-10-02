local actors = {}
return Def.ActorFrame{
    InitCommand = function(self)
        local fired = {}
        self:SetUpdateFunction(function()
            local beat = GAMESTATE:GetSongBeat()
            local position = GAMESTATE:GetSongPosition():GetSongBeat()
            for index, ready in ipairs({beat > 62, beat >= 62, position > 62}) do
                if ready and not fired[index] then
                    actors[index]:x(1)
                    fired[index] = true
                end
            end
        end)
    end,
    Def.Quad{Name = "Strict", InitCommand = function(self) actors[1] = self end},
    Def.Quad{Name = "Inclusive", InitCommand = function(self) actors[2] = self end},
    Def.Quad{Name = "Position", InitCommand = function(self) actors[3] = self end},
}
