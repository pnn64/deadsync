local child
return Def.ActorFrame{
    InitCommand=function(self)
        self:SetUpdateFunction(function()
            local value=GAMESTATE:GetSongBeat()%1
            child:sleep(0):diffuse({value,value,value,value}):glow({value,value,value,value})
        end)
    end,
    Def.Quad{
        Name="Color",
        InitCommand=function(self) child=self; self:zoomto(64,32):xy(100,100) end,
    },
}
