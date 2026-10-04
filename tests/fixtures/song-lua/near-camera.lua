return Def.ActorFrame {
    Name = "Camera",
    OnCommand = function(self) self:zoomx(854/640):zoomz(854/640):fov(150) end,
    Def.Quad {
        Name = "Tween",
        OnCommand = function(self)
            self:zoomto(64,64):xy(516,31):z(-700):zoom(16/15):linear(3):z(700)
        end,
    },
}
