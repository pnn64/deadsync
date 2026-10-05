return Def.ActorFrame {
    Name="Camera", OnCommand=function(self) self:fov(80):zoomx(854/640):zoomz(854/640) end,
    Def.Quad {Name="LeafBob", OnCommand=function(self)
        self:setsize(64,64):xy(180,80):z(40):zoom(1.2):bob():effectperiod(2):effectmagnitude(8,12,5)
    end},
    Def.Quad {Name="LeafBounce", OnCommand=function(self)
        self:setsize(64,64):xy(380,80):z(-40):bounce():effectclock("bgm"):effectperiod(1):effectmagnitude(-7,20,12):effectoffset(.05)
    end},
    Def.Quad {Name="LeafWag", OnCommand=function(self)
        self:setsize(64,64):xy(580,80):z(70):wag():effectclock("bgm"):effectperiod(1):effectmagnitude(13,23,37):effectoffset(.13)
    end},
    Def.ActorFrame {Name="BobParent", OnCommand=function(self)
        self:xy(180,240):z(-10):zoom(1.2):rotationz(12):bob():effectperiod(2):effectmagnitude(8,12,5)
    end,
        Def.Quad {Name="NestedBob", OnCommand=function(self)
            self:setsize(64,64):valign(0):xy(25,-15):z(30):zoom(.8):bob():effectclock("bgm"):effectperiod(2):effectmagnitude(11,-5,4)
        end},
    },
    Def.ActorFrame {Name="BounceParent", OnCommand=function(self)
        self:xy(400,240):z(20):zoom(.9):rotationz(-7):bounce():effectclock("bgm"):effectperiod(1):effectmagnitude(0,15,0)
    end,
        Def.Quad {Name="NestedBounce", OnCommand=function(self) self:setsize(64,64):xy(20,-10):z(40):zoom(.8) end},
    },
    Def.ActorFrame {Name="WagParent", OnCommand=function(self)
        self:xy(630,240):z(-20):zoom(.8):wag():effectclock("bgm"):effectperiod(1):effectmagnitude(12,20,30)
    end,
        Def.Quad {Name="NestedWag", OnCommand=function(self)
            self:setsize(64,64):xy(22,-15):z(30):zoom(.9):rotationy(30):bob():effectclock("bgm"):effectperiod(2):effectmagnitude(3,7,4)
        end},
    },
    Def.Quad {Name="Wrapped", OnCommand=function(self)
        self:setsize(64,64):xy(250,390):z(30):zoom(.9):bounce()
        self:AddWrapperState():xy(-15,10):z(12):wag():effectclock("bgm"):effectperiod(1):effectmagnitude(10,0,18)
    end},
    Def.ActorFrame {Name="TripleY", OnCommand=function(self)
        self:xy(480,380):z(-30):zoom(1.1):bob():effectmagnitude(0,20,0):effectperiod(7)
    end,
        Def.ActorFrame {Name="TripleX", OnCommand=function(self) self:bob():effectmagnitude(20,0,0):effectperiod(9) end,
            Def.ActorFrame {Name="TripleZ", OnCommand=function(self) self:bob():effectmagnitude(0,0,20):effectperiod(6) end,
                Def.Quad {Name="Triple", OnCommand=function(self) self:setsize(64,64):xy(15,30):z(20):rotationy(40) end},
            },
        },
    },
    Def.Quad {Name="TimedBob", OnCommand=function(self)
        self:setsize(64,64):xy(60,380):bob():effectclock("bgm"):effectmagnitude(-8,6,3)
            :effecttiming(.2,.1,.4,.2,.1):effect_hold_at_full(.3)
    end},
    Def.Quad {Name="DelayedBob", OnCommand=function(self)
        self:setsize(64,64):xy(60,160):sleep(.5):queuecommand("Enable"):sleep(.5):queuecommand("Enable")
    end, EnableCommand=function(self) self:bob():effectperiod(1.2):effectmagnitude(16,0,0) end},
    Def.Quad {Name="HeldBob", OnCommand=function(self)
        self:setsize(64,64):xy(60,240):bob():effectmagnitude(16,0,0):sleep(.5):queuecommand("Again")
    end, AgainCommand=function(self) self:bob():effectmagnitude(16,0,0) end},
    Def.Quad {Name="WagBounce", OnCommand=function(self)
        self:setsize(64,64):xy(60,320):wag():effectmagnitude(0,0,25):sleep(.5):queuecommand("Again")
            :sleep(.5):queuecommand("Bounce"):sleep(.5):queuecommand("Bounce")
    end, AgainCommand=function(self) self:wag():effectmagnitude(0,0,25) end,
    BounceCommand=function(self) self:bounce():effectmagnitude(16,0,0) end},
    Def.Quad {Name="ResetPulse", OnCommand=function(self)
        self:setsize(64,64):valign(1):xy(170,440):pulse():effectclock("bgm")
            :effectperiod(.6):effect_hold_at_full(.4):effectoffset(.3):effectmagnitude(1,1,1)
            :effectcolor1(.99,1.01,1,1):effectcolor2(1.01,.99,1,1)
            :sleep(.5):queuecommand("Reset")
    end, ResetCommand=function(self)
        self:pulse():effectperiod(.6):effect_hold_at_full(.4):effectmagnitude(1,1,1)
    end},
}
