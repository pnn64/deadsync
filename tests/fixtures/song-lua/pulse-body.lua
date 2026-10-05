return Def.ActorFrame {
    Name="Camera", OnCommand=function(self) self:fov(80):zoomx(854/640):zoomz(854/640) end,
    Def.Quad {
        Name="Leaf", OnCommand=function(self)
            self:setsize(64,64):xy(200,80):z(40):zoom(16/15):pulse():effectclock("bgm")
                :effectperiod(2):effectmagnitude(16/15,16/15*1.6,1)
        end,
    },
    Def.ActorFrame {
        Name="Parent", OnCommand=function(self)
            self:xy(350,200):z(20):zoom(1.2):pulse():effectclock("bgm"):effectperiod(2)
        end,
        Def.Quad {Name="Nested", OnCommand=function(self) self:setsize(64,64):xy(35,-10):z(50):zoom(.8) end},
        Def.Quad {Name="Double", OnCommand=function(self)
            self:setsize(64,64):xy(0,50):z(40):zoom(.7):pulse():effectclock("bgm"):effectperiod(2)
                :effectoffset(.4):effectmagnitude(.8,1.2,1)
                :effectcolor1(.8,1.4,1,1):effectcolor2({1.2,.6,1,1})
        end},
    },
    Def.Quad {
        Name="Wrapped", OnCommand=function(self)
            self:setsize(64,64):xy(600,160):z(60):zoom(.9)
            self:AddWrapperState():xy(-10,15):z(10):pulse():effectclock("bgm"):effectperiod(2)
        end,
    },
    Def.Quad {
        Name="Signed", OnCommand=function(self)
            self:setsize(64,64):xy(150,350):z(-80):zoom(.8):pulse():effectclock("bgm")
                :effecttiming(.2,.1,.4,.2,.1):effect_hold_at_full(.3)
                :effectmagnitude(-.5,-1,1)
        end,
    },
}
