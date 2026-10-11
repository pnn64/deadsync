local root = Def.ActorFrame{ FOV = 60 }
for _, value in ipairs({
    {"Zero", 0},
    {"Inherited", -1},
    {"Negative", -2},
    {"Small", 0.01},
}) do
    root[#root + 1] = Def.ActorFrame{
        FOV = value[2],
        Def.Quad{
            Name = value[1],
            InitCommand = function(self)
                self:xy(320, 180):z(125):zoomto(40, 20)
            end,
        },
    }
end
root[#root + 1] = Def.ActorFrame{
    Fov = 0,
    Def.Quad{
        Name = "WrongCase",
        InitCommand = function(self) self:xy(320, 180):z(125):zoomto(40, 20) end,
    },
}
return root
