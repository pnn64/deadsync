local pool, game = {}, 0
return Def.ActorFrame{
    Def.Quad{
        OnCommand=function(self) self:sleep(1):queuecommand('Start') end,
        StartCommand=function()
            MESSAGEMAN:Broadcast('ShowPool')
            game = 2
        end,
    },
    Def.ActorFrame{
        ShowPoolMessageCommand=function(self) self:queuecommand('Start') end,
        Def.Quad{
            StartCommand=function(self)
                self:sleep(0.02)
                if game == 2 then self:queuecommand('Update') end
            end,
            UpdateCommand=function()
                assert(#pool == 2)
                for _, item in ipairs(pool) do
                    item.actor:visible(true):diffusealpha(0):linear(0.2):diffusealpha(1)
                        :linear(0.2):diffusealpha(0):queuecommand('Hide')
                end
            end,
        },
        Def.Quad{
            Name='First',
            OnCommand=function(self) self:visible(false):sleep(0.02):queuecommand('SetMe') end,
            SetMeCommand=function(self) table.insert(pool, {actor=self}); self:aux(#pool) end,
            HideCommand=function(self) self:visible(false); pool[self:getaux()].done = true end,
        },
        Def.Quad{
            Name='Second',
            OnCommand=function(self) self:visible(false):sleep(0.02):queuecommand('SetMe') end,
            SetMeCommand=function(self) table.insert(pool, {actor=self}); self:aux(#pool) end,
            HideCommand=function(self) self:visible(false); pool[self:getaux()].done = true end,
        },
    },
}
