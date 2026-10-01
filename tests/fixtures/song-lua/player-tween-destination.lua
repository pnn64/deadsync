local fired = false
return Def.Quad{
    Name="Destination",
    OnCommand=function(self)
        self:SetUpdateFunction(function(self)
            local player = SCREENMAN:GetTopScreen():GetChild("PlayerP1")
            local beat = GAMESTATE:GetSongBeat()
            if not fired and beat >= 1 then
                player:zoomx(1.5):linear(2):zoomx(1)
                fired = true
            end
            if beat >= 2 then player:zoomx(.25):zoomy(.5) end
            self:x(player:GetZoomX())
        end)
    end,
}
