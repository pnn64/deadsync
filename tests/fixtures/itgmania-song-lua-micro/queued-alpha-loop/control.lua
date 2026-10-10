return Def.ActorFrame{
    FOV=0,
    Def.Quad{
        Name='PlainFade',
        InitCommand=function(self) self:xy(427,240):setsize(32,32):diffusealpha(1) end,
        OnCommand=function(self) self:sleep(2):linear(1):diffusealpha(0) end
    },
    Def.Quad{
        Name='LoopFade',
        InitCommand=function(self) self:x(884):y(240):setsize(32,32):diffusealpha(0) end,
        OnCommand=function(self) self:playcommand('Move') end,
        MoveCommand=function(self)
            if self:GetX() <= -30 then self:x(884) else self:x(self:GetX()-4) end
            if GAMESTATE:GetSongBeat()>1 and self:GetX()>854 then self:diffusealpha(1) end
            if GAMESTATE:GetSongBeat()>2 then self:linear(1):diffusealpha(0) end
            self:sleep(.007):queuecommand('Move')
        end
    },
    Def.Model{
        Name='NativeFade', Meshes='model.txt', Materials='model.txt', Bones='model.txt',
        InitCommand=function(self) self:xy(427,240):diffusealpha(1) end,
        OnCommand=function(self) self:sleep(2):linear(1):diffusealpha(0) end
    }
}
