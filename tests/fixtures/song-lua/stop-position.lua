local target
local stopped = false
return Def.ActorFrame{
    Name="Root",
    Def.ActorFrame{
        Name="Driver",
        OnCommand=function(self)
            self:SetUpdateFunction(function()
                if not stopped and GAMESTATE:GetSongBeat() >= 1 then
                    stopped = true
                    target:stoptweening():linear(.25):addx(10):addy(20):addz(60)
                end
            end)
        end,
    },
    Def.Quad{
        Name="Target",
        InitCommand=function(self) target=self; self:setsize(20,20):x(100):y(200):z(-100) end,
        OnCommand=function(self)
            self:linear(.25):x(200):y(300):z(-75)
                :linear(.75):x(300):y(400):z(100)
                :linear(.5):x(400):y(500):z(200)
        end,
    },
    Def.Quad{
        Name="Witness",
        OnCommand=function(self)
            self:SetUpdateFunction(function(self)
                self:x(target:GetDestX()):y(target:GetDestY()):z(target:GetDestZ())
            end)
        end,
    },
}
