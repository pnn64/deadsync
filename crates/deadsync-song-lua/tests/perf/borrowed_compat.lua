local name = ""
local version
for i = 1, 2048 do
    name = cleanGSub(Var("folder/song.lua"), "/", "-")
    version = GetVersionParts("5.1.0-alpha")
end
local command = Var("GameCommand")
return Def.ActorFrame {
    Name = name,
    Def.Quad {
        Name = command:GetName(),
        InitCommand = function(self)
            self:xy(version[1] * 10, version[2] * 10):zoomto(64, 32)
        end,
    },
}
