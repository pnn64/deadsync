local function model(name, piece, x)
    return Def.Model {
        Name=name, Meshes=piece, Materials=piece, Bones=piece,
        InitCommand=function(self)
            self:xy(x,200):diffuse(0.5,0.75,1,0.6):glow(1,0,0,0.25)
        end,
    }
end
return Def.ActorFrame {
    Name="Root", FOV=0,
    model("Single", "single.txt", 150),
    model("Repeated", "repeated.txt", 350),
}
