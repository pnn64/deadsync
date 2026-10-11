local children = {Name="TimingDataControl"}
local song = GAMESTATE:GetCurrentSong()
local selected = GAMESTATE:GetCurrentSteps(0)
local charts = song:GetAllSteps()
local sources = {{"Song",song}, {"Selected",selected}}
for i,steps in ipairs(charts) do sources[#sources+1] = {"Chart"..i,steps} end
for _,source in ipairs(sources) do
    local name,owner = source[1],source[2]
    children[#children+1] = Def.Actor{Name=name,InitCommand=function(self)
        local timing = owner:GetTimingData()
        local bpms = timing:GetBPMsAndTimes(true)
        self:x(timing:GetElapsedTimeFromBeat(0)):y(timing:GetElapsedTimeFromBeat(6))
            :z(timing:GetBeatFromElapsedTime(3)):aux(timing:GetBPMAtBeat(6))
            :zoomy(#bpms):zoomx(bpms[1][2])
            :zoomz(type(timing:GetBPMsAndTimes()[1]) == "string" and 1 or 0)
            :rotationx(timing:GetBPMAtBeat(3.995))
    end}
end
return Def.ActorFrame(children)
