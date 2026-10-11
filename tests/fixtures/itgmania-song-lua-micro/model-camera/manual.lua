local piece = "../model-material/triangle.txt"
local function model(name, x, y, z)
    return Def.Model {
        Name=name, Meshes=piece, Materials=piece, Bones=piece,
        InitCommand=function(self)
            self:xy(x,y):z(z):diffuse(0.5,0.75,1,0.6):glow(1,0,0,0.25)
        end,
    }
end
return Def.ActorFrame {
    Name="Root", FOV=40,
    InitCommand=function(self)
        self:vanishpoint(311,175)
        self:SetDrawFunction(function()
            local stored = self:GetChild("Storage"):GetChild("Perspective")
            stored:Draw()
            self:GetChild("Reset"):Draw()
            stored:Draw()
        end)
    end,
    Def.ActorFrame {
        Name="Storage", FOV=0,
        model("Perspective",207,160,30),
    },
    Def.ActorFrame {
        Name="Reset", FOV=0,
        model("Orthographic",310,350,70),
    },
}
