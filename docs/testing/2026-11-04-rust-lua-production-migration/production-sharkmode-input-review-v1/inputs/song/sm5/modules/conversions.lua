local screen = SCREENMAN:GetTopScreen()
local we_are_in_a_stupid_editor = not PlayerOptions.DisableMines and screen and find(screen:GetName(),"^ScreenEdit")
-- workaround for certain default aspects of notitg, and make things a little more consistent, most (all) taken from the outfox counterparts 
-- assumes values are passed in as percentages (0 to 100)
local conversions = {}
-- one arrow size (the width, and heigh of an arrow are ussumed to be square and usually 64 x 64 pixels in size)
local ARROW_SIZE = 64
-- some conversions require theme metrics to be read in order to make consistent
local THEME = _G.THEME
-- in order to follow the behavior of stealth, dark only has visual effects in the range of 50 to 100 %
function conversions.dark(p) return p end

-- https://gist.github.com/MrThatKid/2a8525a1abe1cb95073ef4eb64590b00
-- square, digital, tandigital
-- squarez, digitalz, tandigitalz
function conversions.period(p) return p end
function conversions.zperiod(p) return p end

-- grain is a multiplier, instead of additive, wherin 0% -> 400% and 100 -> 800%
function conversions.grain(p) return p end

-- convert pixel values to a drawsize percentage
local renderdistancefront = THEME:GetMetric('Player', 'DrawDistanceBeforeTargetsPixels')
if we_are_in_a_stupid_editor then renderdistancefront = 410 end
function conversions.drawsize(px) return (px / renderdistancefront - 1) * 100 end
local renderdistanceback = THEME:GetMetric('Player', 'DrawDistanceAfterTargetsPixels')
if we_are_in_a_stupid_editor then renderdistanceback = -114 end
function conversions.drawsizeback(px) return (px / renderdistanceback - 1) * 100 end

-- scale the grain step to a precise pixel value
function conversions.grainpixel(px) return px * (ARROW_SIZE * 0.75) end

function conversions.croppixel(px) return px / sw end

-- function conversions.tiny(p) return (2 - 2 * 0.5^(p / 100)) * 100 end

-- close enough
function conversions.tiny(p) return (-1.4427 * log(1 - 0.5 * min(p / 100, 1.99))) * 100 end

return conversions