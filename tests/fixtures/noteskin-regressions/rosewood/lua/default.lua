-- Reduced intro regression using this host's song-local noteskin support.
local song = GAMESTATE:GetCurrentSong():GetSongDir()
prefix_globals = {actions={}}
for _, step in ipairs({
    {"SchIntroLine", -5.76}, {"SchIntroCard", -5.58},
    {"SchIntroReady", -1.50}, {"SchIntroGo", 0.0},
}) do
    table.insert(prefix_globals.actions, {(step[2]+0.001)*140/60, step[1], true})
end
local root = Def.ActorFrame{
    OnCommand=function(self)
        for pn=0,1 do
            local _, accepted = GAMESTATE:GetPlayerState(pn):GetPlayerOptions("ModsLevel_Song"):NoteSkin("SCH-CLASSIC-RAINBOW")
            assert(accepted)
        end
    end,
    InitCommand=function(self)
        local screen = SCREENMAN:GetTopScreen()
        screen:SetMinSecondsToMusic(6.01)
        screen:GetChild("In"):visible(false)
    end,
    Def.Quad{InitCommand=function(self) self:zoomto(32,16) end},
}
for _, name in ipairs({"ann-1-finalstage.ogg", "ann-2-areyouready.ogg", "ann-3-okletsstart.ogg",
    "ann-35s-showtime.ogg", "ann-end.ogg", "ann-end-crowd.ogg"}) do
    root[#root+1] = Def.Sound{File=song..name}
end
return root
