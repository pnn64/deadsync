function inquad(t) return t * t end	
function outquad(t) return -t * (t - 2) end
function incubic(t) return t * t * t end
function outcubic(t) return 1 - (1 - t) ^ 3 end
function inquart(t) return t * t * t * t end
function outquart(t) return 1 - (1 - t) ^ 4 end
function inquint(t) return t ^ 5 end
function outquint(t) return 1 - (1 - t) ^ 5 end
function insine(t) return 1 - cos(t * (pi * 0.5)) end
function outsine(t) return sin(t * (pi * 0.5)) end
function inexpo(t) return 1000 ^ (t - 1) end
function outexpo(t) return t == 1 and 1 or 1.001 - 1000 ^ -t end
function incirc(t) return 1 - sqrt(1 - t * t) end
function outcirc(t) return sqrt(-t * t + 2 * t) end
function inelastic(t, a, p) p, a, t = p or 0.3, a or 1, t - 1 return -(a * pow(2, 10 * t) * sin((t - (p / (2 * pi) * asin(1 / a))) * (2 * pi) / p)) end -- amplitude, period
function outelastic(t, a, p) p, a = p or 0.3, a or 1 return a * pow(2, -10 * t) * sin((t - (p / (2 * pi) * asin(1 / a))) * (2 * pi) / p) + 1 end
function outback(t, s) s = s or 1.70158 return ((t - 1) * (t - 1) * ((s + 1) * (t - 1) + s) + 1) end -- sping
function inback(t, s) s = s or 1.70158 return t * t * ((s + 1) * t - s) end
function outbounce(t) if t < 1 / 2.75 then return 7.5625 * t * t elseif t < 2 / 2.75 then t = t - 1.5 / 2.75 return 7.5625 * t * t + 0.75 elseif t < 2.5 / 2.75 then t = t - 2.25 / 2.75 return 7.5625 * t * t + 0.9375 else t = t - 2.625 / 2.75 return 7.5625 * t * t + 0.984375 end end
function inbounce(t) return 1 - outbounce(1 - t) end
function bounce(t) return 4 * t * (1 - t) end
function tri(t) return 1 - abs(2 * t - 1) end
function bell(t) return inoutquint(tri(t)) end
function pop(t) return 3.5 * (1 - t) * (1 - t) * sqrt(t) end
function tap(t) return 3.5 * t * t * sqrt(1 - t) end
function pulse(t) return t < 0.5 and tap(t * 2) or -pop(t * 2 - 1) end
function spike(t) return exp(-10 * abs(2 * t - 1)) end
function inverse(t) return t * t * (1 - t) * (1 - t) / (0.5 - t) end
function square(t) return t == 1 and 0 or 1 end
function sawtooth(t) return t * square(t) end
function smoothstep(t) return 3 * t ^ 2 - 2 * t ^ 3 end
function smootherstep(t) return t ^ 5 * (5 * t * (t * (7 * t * (2 * t - 9) + 108) - 84) + 126) end
function smootheststep(t) return t ^ 7 * 1716 + 7 * t ^ 8 * (2 * t * (3 * t * (t * (11 * t * (2 * t - 13) + 390) - 572) + 1430) - 1287) end
function flicker(t) if t == 1 then return 0 end return frame % 2 end
function tricos(t) return 1 - (1.3 / pi * asin(cos(4 * pi / 2 * t) / 1.0695) + 0.5) end

-- generate inout/outin eases
for _, ease in ipairs{'quad', 'cubic', 'quart', 'quint', 'sine', 'expo', 'circ', 'elastic', 'back', 'bounce'} do
	local ease = ease
	_ENV['inout' .. ease] = function(t, a, p) return t < 0.5 and (_ENV['in' .. ease](t * 2, a, p) * 0.5) or (_ENV['out' .. ease](t * 2 - 1, a, p) * 0.5 + 0.5) end
	_ENV['outin' .. ease] = function(t, a, p) return t < 0.5 and (_ENV['out' .. ease](t * 2, a, p) * 0.5) or (_ENV['in' .. ease](t * 2 - 1, a, p) * 0.5 + 0.5) end
end

-- collates the start and end of two eases, yoinked from oatmealine
function mixease(e1, e2, point)
	point = point or 0.5
	return function(t, a, p) if t < point then return e1(t / point, a, p) * point else return e2((t - point) / (1 - point), a, p) * (1 - point) + point end end
end