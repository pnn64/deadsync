local fired = false
return Def.Quad{
    Name="Result",
    OnCommand=function(self)
        self:SetUpdateFunction(function(self)
            local player = SCREENMAN:GetTopScreen():GetChild("PlayerP1")
            if not fired and GAMESTATE:GetSongBeat() >= 1 then
                player:rotationz(5):zoomy(1.18):rotationx(-30):skewx(.1)
                    :decelerate(.5):rotationz(0):zoomy(1):rotationx(0):skewx(0)
                    :sleep(.25):skewx(-.1):sleep(.25):skewx(0)
                SCREENMAN:GetTopScreen():GetChild("PlayerP2"):zoom(.7):linear(.5):zoom(1)
                fired = true
            end
            self:diffusealpha((player:GetRotationZ() + 5) / 10)
                :x(player:GetZoomY()):y(player:GetRotationX())
        end)
    end,
}
