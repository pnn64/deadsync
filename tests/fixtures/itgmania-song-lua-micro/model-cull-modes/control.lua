local children={Name="CullModes",FOV=0}
for index,mode in ipairs{"default","CullMode_None","CullMode_Back","CullMode_Front"} do
    children[#children+1]=Def.Model{
        Name=mode,Meshes="model.txt",Materials="model.txt",Bones="model.txt",
        InitCommand=function(self)
            self:xy(100+index*100,200):zoom(2)
            if mode~="default" then self:cullmode(mode) end
        end,
    }
end
for index,kind in ipairs{"BoolTrue","BoolFalse","Update","Tween","Numeric","Legacy","BoolZero","BoolHalf"} do
    children[#children+1]=Def.Model{
        Name=kind,Meshes="model.txt",Materials="model.txt",Bones="model.txt",
        InitCommand=function(self)
            self:xy(100+index*40,250):zoom(2)
            if kind=="BoolTrue" then self:backfacecull(true); assert(not pcall(function() self:backfacecull() end))
            elseif kind=="BoolFalse" then self:backfacecull(false)
            elseif kind=="BoolZero" then self:backfacecull(0)
            elseif kind=="BoolHalf" then self:backfacecull(0.5)
            elseif kind=="Numeric" then self:cullmode(1)
            elseif kind=="Legacy" then self:cullmode("FRONT") end
        end,
        OnCommand=function(self)
            if kind=="Tween" then
                self:linear(2):x(500):cullmode(CullMode[2])
                self:queuecommand("Swap")

            end
        end,
        SwapCommand=function(self) self:backfacecull(false) end,
    }
end
children.OnCommand=function(self)
    local model=self:GetChild("Update")
    self:SetUpdateFunction(function(frame)
        local second=GAMESTATE:GetSongPosition():GetMusicSeconds()
        if second<1 then model:cullmode("CullMode_None")
        elseif second<2 then model:cullmode("CullMode_Front")
        else model:backfacecull(true) end
    end)
end
return Def.ActorFrame(children)
