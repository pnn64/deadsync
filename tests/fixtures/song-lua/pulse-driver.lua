local started = false
return Def.ActorFrame {
    Name="Camera", OnCommand=function(self) self:fov(80):zoomx(854/640):zoomz(854/640) end,
    Def.ActorFrame {
        Name="Parent", OnCommand=function(self)
            self:xy(323,358):pulse():effectclock("bgm"):effectperiod(1)
                :effectoffset(.05):effectmagnitude(1,1,1)
            self:SetUpdateFunction(function(actor)
                if not started and GAMESTATE:GetSongBeat() >= 1 then
                    started = true
                    pulse_driver:accelerate(1):x(.05)
                end
                local x = pulse_driver:GetX()
                actor:effectcolor1(1+x,1-x,1,1):effectcolor2(1-x,1+x,1,1)
            end)
        end,
        Def.Quad {Name="Body", OnCommand=function(self) self:zoomto(480,480):y(-204) end},
    },
    Def.Actor {
        InitCommand=function(self) pulse_driver=self end,
    },
}
