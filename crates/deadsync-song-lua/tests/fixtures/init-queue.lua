local phase = "loading"
return Def.ActorFrame {
    OnCommand = function() phase = "parent-on" end,
    Def.Actor {
        InitCommand = function(self) self:playcommand("Prepare") end,
        PrepareCommand = function(self) self:queuecommand("Later") end,
        OnCommand = function() phase = "child-on" end,
        LaterCommand = function(self)
            assert(phase == "child-on", "Init queue ran before On commands")
            self:x(23)
            MESSAGEMAN:Broadcast("InitQueueLate")
        end,
    },
}
