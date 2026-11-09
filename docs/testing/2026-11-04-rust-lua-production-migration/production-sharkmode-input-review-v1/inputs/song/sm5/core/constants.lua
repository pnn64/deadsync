-- screen constants
scx = _G.SCREEN_CENTER_X
scy = _G.SCREEN_CENTER_Y
sw = _G.SCREEN_WIDTH
sh = _G.SCREEN_HEIGHT

-- dw = _G.DISPLAY:GetDisplayWidth()
-- dh = _G.DISPLAY:GetDisplayHeight()
-- this should probably pull from _G.THEME:GetMetric but we're already forcing things for consistency
NOTEFIELD_RENDER_BEGIN = -114
NOTEFIELD_RENDER_END = 410
nfrb = NOTEFIELD_RENDER_BEGIN
nfre = NOTEFIELD_RENDER_END

RECEPTOR_Y_STANDARD = _G.THEME:GetMetric('Player', 'ReceptorArrowsYStandard')
RECEPTOR_Y_REVERSE = _G.THEME:GetMetric('Player', 'ReceptorArrowsYReverse')
NOTEFIELD_Y = (RECEPTOR_Y_STANDARD + RECEPTOR_Y_REVERSE) / 2
ny = NOTEFIELD_Y

ARROW_SIZE = 64
as = ARROW_SIZE

-- scaling constants, because multiple aspect ratio compat is nice, yeah?
scalew = sw / 640
scaleh = sh / 480
scalez = (480 * (sw / sh)) / 640
scalefov = function(fov) return 360 / pi * atan(tan(pi * (1 / (360 / fov))) * sw / sh * 0.75) end

-- the max amount of players to set up this file for, max in nITG is 8
maximum_player_amount = 2

PLAYERS_PER_SCREEN_WIDTH = ceil((sw / scaleh) / as / 4) + 1
ppsw = PLAYERS_PER_SCREEN_WIDTH