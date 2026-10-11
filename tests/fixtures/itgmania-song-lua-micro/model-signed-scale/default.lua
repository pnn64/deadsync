local piece = "../model-material/triangle.txt"
local function model(name, x, y, scale)
    return Def.Model {
        Name=name, Meshes=piece, Materials=piece, Bones=piece,
        InitCommand=function(self)
            self:xy(x,y):z(35):zoomx(scale[1]):zoomy(scale[2]):zoomz(scale[3])
                :rotationx(13):rotationy(-23):rotationz(37)
                :diffuse(0.5,0.75,1,0.6):glow(1,0,0,0.25)
        end,
    }
end
return Def.ActorFrame {
    Name="Root", FOV=0,
    model("IndependentZ",150,120,{1.25,0.65,2.25}),
    model("MirrorX",300,120,{-1.25,0.65,2.25}),
    model("MirrorY",450,120,{1.25,-0.65,2.25}),
    model("MirrorZ",600,120,{1.25,0.65,-2.25}),
    Def.ActorFrame {
        Name="ScaledParent",
        InitCommand=function(self) self:zoomx(1.4):zoomy(-0.8):zoomz(-1.7) end,
        model("Inherited",220,320,{0.75,1.3,-0.6}),
    },
}
