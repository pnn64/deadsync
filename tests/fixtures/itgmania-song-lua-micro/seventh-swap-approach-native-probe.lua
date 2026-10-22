-- Diagnostic observer only. Runs in an isolated native ITGmania install.
local frames, strips, pending, done = {}, {}, false, false
local function noop() end
local function in_window(beat)
  return (beat >= 129.5 and beat <= 131.1)
end
local function draw(actor, primitive, vertices, state)
  if primitive ~= 'SymmetricQuadStrip' then return end
  local beat = GAMESTATE:GetSongPosition():GetSongBeatVisible()
  if not in_window(beat) then return end
  strips[#strips+1] = {beat=beat, actor=tostring(actor), vertices=vertices, state=state}
end
ITGMANIA_HARNESS_BEFORE_SCREEN_UPDATE = function()
  ITGMANIA_HARNESS_PACE_CAPTURE(1/120)
  local beat = GAMESTATE:GetSongPosition():GetSongBeatVisible()
  ITGMANIA_HARNESS_DRAW_VERTICES = in_window(beat) and draw or nil
end
ITGMANIA_HARNESS_AFTER_SCREEN_DRAW = function()
  if done then return end
  local beat = GAMESTATE:GetSongPosition():GetSongBeatVisible()
  local screen = SCREENMAN:GetTopScreen()
  local player = screen and screen:GetChild('PlayerP1')
  if player and in_window(beat) then
    local columns = player:GetChild('NoteField'):GetColumnActors()
    local col = 4
    local note_beat = 130.75
    local ca = columns[col]
    local ps = ca:GetPosHandler():GetSpline()
    local zs = ca:GetZoomHandler():GetSpline()
    local t = (note_beat-beat)/.125
    local pp,zp = {},{}
    for p=1,ps:GetSize() do pp[p]=ps:Evaluate(p-1); zp[p]=zs:Evaluate(p-1) end
    frames[#frames+1] = {beat=beat,seconds=GAMESTATE:GetSongPosition():GetMusicSeconds(),
      column=col,note_beat=note_beat,position=ps:Evaluate(t),zoom=zs:Evaluate(t),
      position_points=pp,zoom_points=zp,neighbor_position=columns[3]:GetPosHandler():GetSpline():Evaluate((131-beat)/.125),neighbor_zoom=columns[3]:GetZoomHandler():GetSpline():Evaluate((131-beat)/.125)}
  end
  if pending then
    done=true
    local file=RageFileUtil:CreateRageFile()
    assert(file:Open('/Save/itgmania-harness/native-swap-130-direct.json',2))
    file:Write(JsonEncode({frames=frames,strips=strips,version=ProductVersion(),
      native_source=ITGMANIA_HARNESS_NATIVE_SOURCE,
      input={headless_graphics=ITGMANIA_HARNESS_HEADLESS_GRAPHICS==true,
        window_visible=ITGMANIA_HARNESS_WINDOW_VISIBLE(),
        constant_update_delta_seconds=PREFSMAN:GetPreference('ConstantUpdateDeltaSeconds'),
        global_offset_seconds=PREFSMAN:GetPreference('GlobalOffsetSeconds'),
        catalog_inputs=ITGMANIA_HARNESS_CATALOG_INPUTS(),
        player_options=GAMESTATE:GetPlayerState(PLAYER_1):GetPlayerOptionsString('ModsLevel_Current')},
      observer_error=ITGMANIA_HARNESS_INTERACTIVE_ERROR}))
    file:Close();file:destroy()
  end
end
ITGMANIA_HARNESS_TRACE = {
  capture_catalog_inputs=noop,capture_inputs=noop,scene_init=noop,
  sample_update=noop,sample_stage=noop,screen_changed=noop,
  seed_random=function() MersenneTwister.Seed(1) end,
  finish=function() pending=true end,
}
