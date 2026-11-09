-- Draft for the existing live observer port; not installed in production yet.
return function(remember, hook)
local gs, sm, stats = GAMESTATE, SCREENMAN, STATSMAN
local song = gs:GetCurrentSong()
local songdir = song:GetSongDir()
local stage = stats:GetCurStageStats()
local style = gs:GetCurrentStyle()
remember(song, 'Song', songdir)
remember(stage, 'StageStats', 'stage:current')
remember(style, 'Style', style:GetName())
hook(style, 'Style', 'GetName')
for player=0,1 do
    local state=gs:GetPlayerState(player)
    local path='player-state:PlayerNumber_P'..(player+1)
    remember(state, 'PlayerState', path)
    for _,level in ipairs{'ModsLevel_Preferred','ModsLevel_Stage','ModsLevel_Song','ModsLevel_Current'} do
        local options=state:GetPlayerOptions(level)
        remember(options, 'PlayerOptions', path..'/options:'..level)
        for _,name in ipairs{'Reverse','Boost','ConfusionOffset','Tiny1','Tiny2','Tiny3','Tiny4'} do
            hook(options, 'PlayerOptions', name)
        end
    end
    for _,name in ipairs{'GetPlayerOptions','GetPlayerOptionsString','SetPlayerOptions'} do
        hook(state, 'PlayerState', name)
    end
end
for _,level in ipairs{'ModsLevel_Preferred','ModsLevel_Stage','ModsLevel_Song','ModsLevel_Current'} do
    local options=gs:GetSongOptionsObject(level)
    remember(options, 'SongOptions')
    hook(options, 'SongOptions', 'MusicRate')
end
for index,step in ipairs(song.__songlua_steps) do
    remember(step, 'Steps', songdir, index, step:GetStepsType(),
        step:GetDifficulty(), step:GetDescription(), step:GetMeter())
    hook(step, 'Steps', 'GetDifficulty')
    hook(step, 'Steps', 'GetStepsType')
end
for index,player in ipairs(stage.__songlua_player_stats) do
    remember(player, 'PlayerStageStats', 'stage:current/player:P'..index)
    -- These two native bindings emit their own selected query observations.
    hook(player, 'PlayerStageStats', 'GetPossibleDancePoints')
    hook(player, 'PlayerStageStats', 'GetCurrentPossibleDancePoints')
end
for _,name in ipairs{'GetCurrentSong','GetCurrentSteps','GetCurrentStyle','GetPlayerState',
    'GetSongBeat','IsPlayerEnabled','SetCurrentSteps','GetSongOptionsObject'} do
    hook(gs, 'GameState', name)
end
hook(song, 'Song', 'GetStepsByStepsType')
hook(song, 'Song', 'GetSongDir')
hook(DISPLAY, 'RageDisplay', 'SupportsRenderToTexture')
hook(PREFSMAN, 'PrefsManager', 'GetPreference')
hook(stats, 'StatsManager', 'GetCurStageStats')
hook(stage, 'StageStats', 'GetPlayerStageStats')
hook(sm, 'ScreenManager', 'GetTopScreen')

-- The three authenticated scalar forwards retain their original closures;
-- their observations are emitted from scalar_methods::forward instead.
for _,name in ipairs{'GetName','GetParent','name','aux','getaux','zoomx','vibrate',
    'effectmagnitude','Center','FullScreen','animate','decelerate','diffuse',
    'diffusealpha','effectperiod','linear','queuemessage','rainbow','rotationz',
    'spin','zoom','zoomto','sleep','queuecommand'} do
    hook(Actor, 'Actor', name)
end
for _,name in ipairs{'GetChild','GetNumChildren','SetUpdateFunction'} do
    hook(ActorFrame, 'ActorFrame', name)
end
hook(ActorProxy, 'ActorProxy', 'SetTarget')
hook(Sprite, 'Sprite', 'GetTexture')
hook(ActorFrameTexture, 'ActorFrameTexture', 'GetTexture')
hook(Actor, 'Sprite', 'loop')
hook(Actor, 'Sprite', 'rate')
end
