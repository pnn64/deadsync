local child
return Def.ActorFrame{
    InitCommand=function(self)
        self:SetUpdateFunction(function()
            local value=GAMESTATE:GetSongBeat()%1
            child:sleep(0):diffuse({value,value,value,value}):glow({0,0,0,0})
            child:shadowlength(value*10):shadowlengthy(value*20):shadowcolor({0.2,0.3,0.4,0.5})
        end)
    end,
    Def.Quad{
        Name="Shadow",
        InitCommand=function(self) child=self; self:zoomto(64,32):xy(100,100) end,
    },
}
