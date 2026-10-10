local piece = "../model-material/triangle.txt"
local function model(name, x, y, z, rotation)
    return Def.Model {
        Name=name, Meshes=piece, Materials=piece, Bones=piece,
        InitCommand=function(self)
            self:xy(x,y):z(z):rotationz(rotation or 0)
                :diffuse(0.5,0.75,1,0.6):glow(1,0,0,0.25)
        end,
    }
end
return Def.ActorFrame {
    Name="Root", FOV=40,
    InitCommand=function(self) self:vanishpoint(311,175) end,
    model("Perspective",207,160,30),
    Def.ActorFrame {
        Name="Inherit",
        model("Inherited",531,240,-12),
    },
    Def.ActorFrame {
        Name="Reset", FOV=0,
        model("Orthographic",310,350,70),
    },
    Def.ActorFrame {
        Name="Squash",
        InitCommand=function(self) self:zoomy(0.9) end,
        model("Rotated",470,280,50,90),
    },
}
