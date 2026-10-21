-- Reduced regression for a parent exit tween followed by an unrelated phase.
-- The sprite keeps its local pose and stays visible; culling must use its
-- completed parent transform and one cell of the 4x3 sprite sheet.
return Def.ActorFrame {
    OnCommand=function(self) self:sleep(0.5):queuecommand("Leave") end,
    LeaveCommand=function(self) MESSAGEMAN:Broadcast("ParentExit") end,
    Def.ActorFrame {
        Name="ExitingParent",
        ParentExitMessageCommand=function(self) self:linear(0.5):x(427) end,
        Def.Sprite {
            Texture=GAMESTATE:GetCurrentSong():GetSongDir() .. "cagney/idle 4x3.png",
            InitCommand=function(self) self:xy(587, 240):animate(false):setstate(0) end,
        },
    },
}
