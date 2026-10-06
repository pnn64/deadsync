local names = {
    "PlayerP1", "PlayerP2", "Overlay", "Underlay", "SongBackground",
    "SongForeground", "LifeP1", "LifeP2", "ScoreP1", "ScoreP2",
    "StepsDisplayP1", "StepsDisplayP2", "", "Line", "Line",
    "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789",
    "長い名前の子役ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789", "final-child",
}
local function keys(items)
    local names = {}
    for name in pairs(items) do names[#names + 1] = name end
    return table.concat(names, ",")
end
local root = Def.ActorFrame {
    OnCommand=function(self)
        assert(self:GetParent():GetChildren()[""] == self)
        local children = self:GetChildren()
        assert(keys(children) == ",SongForeground,ScoreP1,final-child,Line,LifeP2,ScoreP2,長い名前の子役ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789,PlayerP1,LifeP1,Underlay,StepsDisplayP1,0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789,Overlay,PlayerP2,SongBackground,StepsDisplayP2",
            "native Lua 5.1 child traversal changed")
        local lines = self:GetChild("Line")
        assert(#lines == 2)
        assert(lines:x(17) == lines[2], "duplicate groups forward to the last child")
        assert(lines[1]:GetX() == 0 and lines[2]:GetX() == 17)
        local getx = lines.GetX
        lines[3] = lines[1]
        assert(getx(lines) == 0, "forwarding uses the current last group member")
        assert(lines["3"] == nil, "numeric strings do not resolve as methods")
        assert(#self:GetChild("Line") == 2, "GetChild returns a group snapshot")
        assert(#self:GetChildren().Line == 2, "GetChildren returns a group snapshot")
        MESSAGEMAN:Broadcast("ChildTableChecked")
    end,
}
for _, name in ipairs(names) do root[#root + 1] = Def.Quad {Name=name} end
return root
