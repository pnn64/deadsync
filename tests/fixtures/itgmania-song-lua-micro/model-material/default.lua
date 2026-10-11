local cyber = "../../../../assets/noteskins/dance/cyber/_down tap note model.txt"
local function model(name, piece, tint)
    return Def.Model {
        Name=name, Meshes=piece, Materials=piece, Bones=piece,
        InitCommand=function(self)
            self:diffuse(unpack(tint)):glow(1,0,0,0.25)
        end,
    }
end
return Def.ActorFrame {
    Name="Root", FOV=0,
    model("Triangle", "triangle.txt", {0.5,0.75,1,0.6}),
    model("Unassigned", "unassigned.txt", {0.5,0.75,1,0.6}),
    model("Cyber", cyber, {0.5,0.75,1,0.6}),
    model("DarkCyber", cyber, {0.25,0.25,0.25,0.25}),
}
