return Def.ActorFrame{
    Def.Quad{
        Name="SetterAdd", OnCommand=cmd(zoomto,64,32;queuecommand,"Update"),
        UpdateCommand=function(self)
            self:xy(100,100):z(5):addx(5):addy(60):addz(-2)
            self:rotationx(10):addrotationx(2):rotationy(20):addrotationy(-3):rotationz(30):addrotationz(4)
            self:sleep(0.02):queuecommand("Update")
        end,
    },
}
