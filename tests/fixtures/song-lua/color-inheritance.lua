return Def.ActorFrame{
    Name='Root', InitCommand=cmd(diffuse,.5,.75,.4,.6;glow,.1,.2,.3,.2),
    Def.ActorFrame{
        Name='rainbow', InitCommand=function(self) self:xy(60,240):diffuse(.2,.4,.6,.8):glow(.3,.1,.2,.25):rainbow():effectclock("beat"):effectperiod(2):effectcolor1(0.9,0.2,0.6,0.35):effectcolor2(0.1,0.8,0.3,0.9) end,
        Def.ActorFrame{
            Name='rainbowTint', InitCommand=cmd(diffuse,.7,.9,.6,.5;glow,.05,.4,.1,.15),
            Def.Quad{Name='rainbow_plain', InitCommand=function(self) self:y(-90):setsize(48,40):diffuse(.8,.5,.3,.75):glow(.2,.1,.5,.3) end},
            Def.Quad{Name='rainbow_rainbow', InitCommand=function(self) self:y(-30):setsize(48,40):diffuse(.8,.5,.3,.75):glow(.2,.1,.5,.3):rainbow():effectclock("beat"):effectperiod(0.7):effectcolor1(0.9,0.2,0.6,0.35):effectcolor2(0.1,0.8,0.3,0.9) end},
            Def.Quad{Name='rainbow_diffuseramp', InitCommand=function(self) self:y(30):setsize(48,40):diffuse(.8,.5,.3,.75):glow(.2,.1,.5,.3):diffusetopedge(.4,.3,.2,.45):diffusebottomedge(.1,.2,.7,.5):diffuseramp():effectclock("beat"):effectperiod(0.7):effectcolor1(0.9,0.2,0.6,0.35):effectcolor2(0.1,0.8,0.3,0.9) end},
            Def.Quad{Name='rainbow_glowshift', InitCommand=function(self) self:y(90):setsize(48,40):diffuse(.8,.5,.3,.75):glow(.2,.1,.5,.3):glowshift():effectclock("beat"):effectperiod(0.7):effectcolor1(0.9,0.2,0.6,0.35):effectcolor2(0.1,0.8,0.3,0.9) end},
        },
    },
    Def.ActorFrame{
        Name='diffuseblink', InitCommand=function(self) self:xy(170,240):diffuse(.2,.4,.6,.8):glow(.3,.1,.2,.25):diffuseblink():effectclock("beat"):effectperiod(2):effectcolor1(0.9,0.2,0.6,0.35):effectcolor2(0.1,0.8,0.3,0.9) end,
        Def.ActorFrame{
            Name='diffuseblinkTint', InitCommand=cmd(diffuse,.7,.9,.6,.5;glow,.05,.4,.1,.15),
            Def.Quad{Name='diffuseblink_plain', InitCommand=function(self) self:y(-90):setsize(48,40):diffuse(.8,.5,.3,.75):glow(.2,.1,.5,.3) end},
            Def.Quad{Name='diffuseblink_rainbow', InitCommand=function(self) self:y(-30):setsize(48,40):diffuse(.8,.5,.3,.75):glow(.2,.1,.5,.3):rainbow():effectclock("beat"):effectperiod(0.7):effectcolor1(0.9,0.2,0.6,0.35):effectcolor2(0.1,0.8,0.3,0.9) end},
            Def.Quad{Name='diffuseblink_diffuseramp', InitCommand=function(self) self:y(30):setsize(48,40):diffuse(.8,.5,.3,.75):glow(.2,.1,.5,.3):diffusetopedge(.4,.3,.2,.45):diffusebottomedge(.1,.2,.7,.5):diffuseramp():effectclock("beat"):effectperiod(0.7):effectcolor1(0.9,0.2,0.6,0.35):effectcolor2(0.1,0.8,0.3,0.9) end},
            Def.Quad{Name='diffuseblink_glowshift', InitCommand=function(self) self:y(90):setsize(48,40):diffuse(.8,.5,.3,.75):glow(.2,.1,.5,.3):glowshift():effectclock("beat"):effectperiod(0.7):effectcolor1(0.9,0.2,0.6,0.35):effectcolor2(0.1,0.8,0.3,0.9) end},
        },
    },
    Def.ActorFrame{
        Name='diffuseshift', InitCommand=function(self) self:xy(280,240):diffuse(.2,.4,.6,.8):glow(.3,.1,.2,.25):diffuseshift():effectclock("beat"):effectperiod(2):effectcolor1(0.9,0.2,0.6,0.35):effectcolor2(0.1,0.8,0.3,0.9) end,
        Def.ActorFrame{
            Name='diffuseshiftTint', InitCommand=cmd(diffuse,.7,.9,.6,.5;glow,.05,.4,.1,.15),
            Def.Quad{Name='diffuseshift_plain', InitCommand=function(self) self:y(-90):setsize(48,40):diffuse(.8,.5,.3,.75):glow(.2,.1,.5,.3) end},
            Def.Quad{Name='diffuseshift_rainbow', InitCommand=function(self) self:y(-30):setsize(48,40):diffuse(.8,.5,.3,.75):glow(.2,.1,.5,.3):rainbow():effectclock("beat"):effectperiod(0.7):effectcolor1(0.9,0.2,0.6,0.35):effectcolor2(0.1,0.8,0.3,0.9) end},
            Def.Quad{Name='diffuseshift_diffuseramp', InitCommand=function(self) self:y(30):setsize(48,40):diffuse(.8,.5,.3,.75):glow(.2,.1,.5,.3):diffusetopedge(.4,.3,.2,.45):diffusebottomedge(.1,.2,.7,.5):diffuseramp():effectclock("beat"):effectperiod(0.7):effectcolor1(0.9,0.2,0.6,0.35):effectcolor2(0.1,0.8,0.3,0.9) end},
            Def.Quad{Name='diffuseshift_glowshift', InitCommand=function(self) self:y(90):setsize(48,40):diffuse(.8,.5,.3,.75):glow(.2,.1,.5,.3):glowshift():effectclock("beat"):effectperiod(0.7):effectcolor1(0.9,0.2,0.6,0.35):effectcolor2(0.1,0.8,0.3,0.9) end},
        },
    },
    Def.ActorFrame{
        Name='diffuseramp', InitCommand=function(self) self:xy(390,240):diffuse(.2,.4,.6,.8):glow(.3,.1,.2,.25):diffuseramp():effectclock("beat"):effectperiod(2):effectcolor1(0.9,0.2,0.6,0.35):effectcolor2(0.1,0.8,0.3,0.9) end,
        Def.ActorFrame{
            Name='diffuserampTint', InitCommand=cmd(diffuse,.7,.9,.6,.5;glow,.05,.4,.1,.15),
            Def.Quad{Name='diffuseramp_plain', InitCommand=function(self) self:y(-90):setsize(48,40):diffuse(.8,.5,.3,.75):glow(.2,.1,.5,.3) end},
            Def.Quad{Name='diffuseramp_rainbow', InitCommand=function(self) self:y(-30):setsize(48,40):diffuse(.8,.5,.3,.75):glow(.2,.1,.5,.3):rainbow():effectclock("beat"):effectperiod(0.7):effectcolor1(0.9,0.2,0.6,0.35):effectcolor2(0.1,0.8,0.3,0.9) end},
            Def.Quad{Name='diffuseramp_diffuseramp', InitCommand=function(self) self:y(30):setsize(48,40):diffuse(.8,.5,.3,.75):glow(.2,.1,.5,.3):diffusetopedge(.4,.3,.2,.45):diffusebottomedge(.1,.2,.7,.5):diffuseramp():effectclock("beat"):effectperiod(0.7):effectcolor1(0.9,0.2,0.6,0.35):effectcolor2(0.1,0.8,0.3,0.9) end},
            Def.Quad{Name='diffuseramp_glowshift', InitCommand=function(self) self:y(90):setsize(48,40):diffuse(.8,.5,.3,.75):glow(.2,.1,.5,.3):glowshift():effectclock("beat"):effectperiod(0.7):effectcolor1(0.9,0.2,0.6,0.35):effectcolor2(0.1,0.8,0.3,0.9) end},
        },
    },
    Def.ActorFrame{
        Name='glowblink', InitCommand=function(self) self:xy(500,240):diffuse(.2,.4,.6,.8):glow(.3,.1,.2,.25):glowblink():effectclock("beat"):effectperiod(2):effectcolor1(0.9,0.2,0.6,0.35):effectcolor2(0.1,0.8,0.3,0.9) end,
        Def.ActorFrame{
            Name='glowblinkTint', InitCommand=cmd(diffuse,.7,.9,.6,.5;glow,.05,.4,.1,.15),
            Def.Quad{Name='glowblink_plain', InitCommand=function(self) self:y(-90):setsize(48,40):diffuse(.8,.5,.3,.75):glow(.2,.1,.5,.3) end},
            Def.Quad{Name='glowblink_rainbow', InitCommand=function(self) self:y(-30):setsize(48,40):diffuse(.8,.5,.3,.75):glow(.2,.1,.5,.3):rainbow():effectclock("beat"):effectperiod(0.7):effectcolor1(0.9,0.2,0.6,0.35):effectcolor2(0.1,0.8,0.3,0.9) end},
            Def.Quad{Name='glowblink_diffuseramp', InitCommand=function(self) self:y(30):setsize(48,40):diffuse(.8,.5,.3,.75):glow(.2,.1,.5,.3):diffusetopedge(.4,.3,.2,.45):diffusebottomedge(.1,.2,.7,.5):diffuseramp():effectclock("beat"):effectperiod(0.7):effectcolor1(0.9,0.2,0.6,0.35):effectcolor2(0.1,0.8,0.3,0.9) end},
            Def.Quad{Name='glowblink_glowshift', InitCommand=function(self) self:y(90):setsize(48,40):diffuse(.8,.5,.3,.75):glow(.2,.1,.5,.3):glowshift():effectclock("beat"):effectperiod(0.7):effectcolor1(0.9,0.2,0.6,0.35):effectcolor2(0.1,0.8,0.3,0.9) end},
        },
    },
    Def.ActorFrame{
        Name='glowshift', InitCommand=function(self) self:xy(610,240):diffuse(.2,.4,.6,.8):glow(.3,.1,.2,.25):glowshift():effectclock("beat"):effectperiod(2):effectcolor1(0.9,0.2,0.6,0.35):effectcolor2(0.1,0.8,0.3,0.9) end,
        Def.ActorFrame{
            Name='glowshiftTint', InitCommand=cmd(diffuse,.7,.9,.6,.5;glow,.05,.4,.1,.15),
            Def.Quad{Name='glowshift_plain', InitCommand=function(self) self:y(-90):setsize(48,40):diffuse(.8,.5,.3,.75):glow(.2,.1,.5,.3) end},
            Def.Quad{Name='glowshift_rainbow', InitCommand=function(self) self:y(-30):setsize(48,40):diffuse(.8,.5,.3,.75):glow(.2,.1,.5,.3):rainbow():effectclock("beat"):effectperiod(0.7):effectcolor1(0.9,0.2,0.6,0.35):effectcolor2(0.1,0.8,0.3,0.9) end},
            Def.Quad{Name='glowshift_diffuseramp', InitCommand=function(self) self:y(30):setsize(48,40):diffuse(.8,.5,.3,.75):glow(.2,.1,.5,.3):diffusetopedge(.4,.3,.2,.45):diffusebottomedge(.1,.2,.7,.5):diffuseramp():effectclock("beat"):effectperiod(0.7):effectcolor1(0.9,0.2,0.6,0.35):effectcolor2(0.1,0.8,0.3,0.9) end},
            Def.Quad{Name='glowshift_glowshift', InitCommand=function(self) self:y(90):setsize(48,40):diffuse(.8,.5,.3,.75):glow(.2,.1,.5,.3):glowshift():effectclock("beat"):effectperiod(0.7):effectcolor1(0.9,0.2,0.6,0.35):effectcolor2(0.1,0.8,0.3,0.9) end},
        },
    },
    Def.ActorFrame{
        Name='glowramp', InitCommand=function(self) self:xy(720,240):diffuse(.2,.4,.6,.8):glow(.3,.1,.2,.25):glowramp():effectclock("beat"):effectperiod(2):effectcolor1(0.9,0.2,0.6,0.35):effectcolor2(0.1,0.8,0.3,0.9) end,
        Def.ActorFrame{
            Name='glowrampTint', InitCommand=cmd(diffuse,.7,.9,.6,.5;glow,.05,.4,.1,.15),
            Def.Quad{Name='glowramp_plain', InitCommand=function(self) self:y(-90):setsize(48,40):diffuse(.8,.5,.3,.75):glow(.2,.1,.5,.3) end},
            Def.Quad{Name='glowramp_rainbow', InitCommand=function(self) self:y(-30):setsize(48,40):diffuse(.8,.5,.3,.75):glow(.2,.1,.5,.3):rainbow():effectclock("beat"):effectperiod(0.7):effectcolor1(0.9,0.2,0.6,0.35):effectcolor2(0.1,0.8,0.3,0.9) end},
            Def.Quad{Name='glowramp_diffuseramp', InitCommand=function(self) self:y(30):setsize(48,40):diffuse(.8,.5,.3,.75):glow(.2,.1,.5,.3):diffusetopedge(.4,.3,.2,.45):diffusebottomedge(.1,.2,.7,.5):diffuseramp():effectclock("beat"):effectperiod(0.7):effectcolor1(0.9,0.2,0.6,0.35):effectcolor2(0.1,0.8,0.3,0.9) end},
            Def.Quad{Name='glowramp_glowshift', InitCommand=function(self) self:y(90):setsize(48,40):diffuse(.8,.5,.3,.75):glow(.2,.1,.5,.3):glowshift():effectclock("beat"):effectperiod(0.7):effectcolor1(0.9,0.2,0.6,0.35):effectcolor2(0.1,0.8,0.3,0.9) end},
        },
    },
}
