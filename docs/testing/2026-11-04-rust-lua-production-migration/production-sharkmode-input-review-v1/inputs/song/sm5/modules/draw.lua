function x(x) target:x(x) end
function y(y) target:y(y) end
function z(z) target:z(z) end
function xy(x, y) target:xy(x, y) end
function xyz(x, y, z) target:xyz(x, y, z) end
function w(w) target:zoomtowidth(w) end
function h(h) target:zoomtoheight(h) end
function wh(w, h) target:zoomto(w, h or w) end
function setw(w) target:SetWidth(w) end
function seth(h) target:SetHeight(h) end
function setwh(w, h) target:SetWidth(w) target:SetHeight(w or h) end
function xywh(x, y, w, h) xy(x, y) wh(w, h) end
function rotx(rx) target:rotationx(rx) end
function roty(ry) target:rotationy(ry) end
function rotz(rz) target:rotationz(rz) end
function rotxyz(rx, ry, rz) rotx(rx) roty(ry) rotz(rz) end
function skewx(sx) target:skewx(sx) end
function skewy(st) target:skewy(st) end
function skewxy(sx, sy) skewx(sx) skewy(sy) end
function hidden(n) target:hidden(n) end
function draw() target:Draw() end
function zoom(z) target:zoom(z) target:zoomz(z) end
function zoomx(x) target:zoomx(x) end
function zoomy(y) target:zoomy(y) end
function zoomz(z) target:zoomz(z) end
function basezoomx(x) target:basezoomx(x) end
function basezoomy(x) target:basezoomy(x) end
function basezoom(x, y) basezoomx(x) basezoomy(y or x) end
function zoomto(x, y) target:zoomto(x, y or x) end
function zoomxy(x, y) zoomx(x) zoomy(y) end
function rgb(r, g, b, a) target:diffuse(r / 255, g / 255, b / 255, a or 1) end
function alpha(a) target:diffusealpha(a) end
function rgbcache(cache) target:diffuse(cache[1], cache[2], cache[3], cache[4]) end
local hexcache = {}
function hex(c)
	if hexcache[c] then return rgbcache(hexcache[c]) end
	local colour = {}
	gsub(c, '%x%x', function(c)
		insert(colour, tonumber(c, 16))
		return c
	end)
	hexcache[c] = {colour[1] / 255, colour[2] / 255, colour[3] / 255, (colour[4] or 255) / 255}
	rgb(colour[1], colour[2], colour[3], (colour[4] or 255) / 255)
end