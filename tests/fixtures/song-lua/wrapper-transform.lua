return Def.ActorFrame {
    Name = "Root",
    FOV = 60,
    Def.Sprite {
        Name = "Mixed", Texture = "fit-rect.png",
        OnCommand = function(self)
            self:setsize(64,32)
            self:AddWrapperState():xy(66,220):zoomx(.5):zoomy(.5):zoomz(1.334375)
            self:skewx(-.4):skewy(-.03):rotationy(-40):rotationx(-10):rotationz(-30)
        end,
    },
    Def.Sprite {
        Name = "Layered", Texture = "fit-rect.png",
        OnCommand = function(self)
            self:setsize(64,32):xy(10,-8):rotationz(40):skewy(.07)
            self:AddWrapperState():xy(32,-10):zoomx(.8):zoomy(.7):rotationz(20)
                :linear(1):rotationz(-35)
            self:AddWrapperState():xy(300,230):zoomx(-.6):zoomy(1.1):rotationz(-15):skewx(.15):diffusealpha(.6)
            assert(self:GetNumWrapperStates() == 2)
            assert(self:GetWrapperState(1):GetZoomX() > .79)
            assert(self:GetWrapperState(2):GetZoomX() < -.59)
        end,
    },
    Def.Sprite {
        Name = "Callback", Texture = "fit-rect.png",
        OnCommand = function(self)
            self:setsize(64,32):rotationz(-20):skewx(.12)
            local wrapper = self:AddWrapperState():xy(620,240):zoom(.5):rotationz(30)
            wrapper:SetUpdateFunction(function(state)
                local beat = GAMESTATE:GetSongBeat()
                state:x(620+beat*10):rotationz(30+beat*10)
            end)
        end,
    },
}
