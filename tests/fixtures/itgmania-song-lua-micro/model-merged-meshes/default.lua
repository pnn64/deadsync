local piece = "model-merged-meshes.txt"
return Def.ActorFrame {
    Name="Root",
    Def.Model {
        Name="Merged", Meshes=piece, Materials=piece, Bones=piece,
        InitCommand=function(self) self:xy(100,200):z(30):glow(1,0,0,0.25) end,
    },
}
