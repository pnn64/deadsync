return Def.ActorFrame{
    InitCommand=function(self)
        self:SetUpdateFunction(function(self)
            if GAMESTATE:GetSongBeat() > 0.19 then
                self:GetChild("RandomProbe"):x(math.random(100,200)):y(math.random())
            end
        end)
    end,
    Def.Quad{
        Name="RandomProbe", OnCommand=cmd(zoomto,16,16),
        UnusedMessageCommand=function(self) self:x(math.random(100,200)):y(math.random()) end,
    },
}
