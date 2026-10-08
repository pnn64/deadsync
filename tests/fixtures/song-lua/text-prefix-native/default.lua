local o = GAMESTATE:GetPlayerState(PLAYER_1):GetPlayerOptions('ModsLevel_Song')
o:Tipsy(7)
o:FromString('0.8 0% Tipsy')
assert(o:Tipsy() == 0)
o:FromString('*2 10% *5 25% Drunk')
assert(o:Drunk() == 0.25 and select(2, o:Drunk()) == 5)
o:FromString('50% no Flip')
assert(o:Flip() == 0)
o:FromString('no 75% Flip')
assert(o:Flip() == 0.75)
o:FromString('*inf 25% Mini')
assert(o:Mini() == 0.25 and select(2, o:Mini()) == 1)
o:FromString('*-2 25% Mini')
assert(select(2, o:Mini()) == -2)
o:FromString('100ms Passmark')
assert(math.abs(o:Passmark() - 0.1) < 1e-6)
o:FromString('25* Drunk')
assert(o:Drunk() == 0.25)
o:FromString('.5 Drunk')
assert(o:Drunk() == 1)
local ticks = 0
local queued = false
return Def.ActorFrame {
    OnCommand=function(self)
        self:SetUpdateFunction(function(self)
            ticks = ticks + 1
            self:GetChild('Count'):settext(queued and ('queued:' .. ticks) or ticks)
        end)
    end,
    LoadFont('Common Normal') .. {
        Name='Count', Text='initial',
        OnCommand=function(self) self:settext('start'):sleep(0.025):queuecommand('Queued') end,
        QueuedCommand=function(self) queued = true; self:settext('queued') end,
    },
}
