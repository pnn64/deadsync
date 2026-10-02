local resized, beat
return Def.ActorFrame{
    Name = "Root",
    InitCommand = function(self)
        local done, started = false, false
        self:SetUpdateFunction(function()
            if not started and GAMESTATE:GetSongBeat() > 0 then
                beat:playcommand("Beat")
                started = true
            end
            if not done and GAMESTATE:GetSongBeat() > 1 then
                resized:playcommand("Resize")
                done = true
            end
        end)
    end,
    Def.Quad{
        Name = "Resized",
        InitCommand = function(self)
            resized = self
            self:setsize(1280, 720):xy(427, 240)
        end,
        ResizeCommand = function(self) self:setsize(854, 480) end,
    },
    Def.Sprite{
        Name = "Beat", Texture = "fit-rect.png",
        InitCommand = function(self)
            beat = self
            self:xy(427, 240):zoom(0)
        end,
        BeatCommand = function(self)
            self:decelerate(0.585):zoom(1.4):linear(0):zoom(0)
        end,
    },
}
