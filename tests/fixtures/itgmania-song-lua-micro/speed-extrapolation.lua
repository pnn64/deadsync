return Def.ActorFrame { InitCommand=function()
    local origin = GAMESTATE:GetCurrentSteps(0):GetTimingData():GetElapsedTimeFromBeat(0)
    MESSAGEMAN:Broadcast('NativeSpeedPositive', {
        value=_ITG_TIMING_Y_OFFSET(5,4,-10-origin)/64,
        origin=origin,
    })
end }
