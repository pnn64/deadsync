
function HSVToColor(hsv)
    local s, v = hsv.Sat, hsv.Value
    local a = hsv.Alpha or 0
    if s == 0 then
        return {v, v, v, a}
    end
    local h = hsv.Hue / 60
    local i = math.floor(h)
    local f = h - i
    local p = v * (1 - s)
    local q = v * (1 - s * f)
    local t = v * (1 - s * (1 - f))
    if i == 0 then return {v, t, p, a}
    elseif i == 1 then return {q, v, p, a}
    elseif i == 2 then return {p, v, t, a}
    elseif i == 3 then return {p, q, v, a}
    elseif i == 4 then return {t, p, v, a}
    else return {v, p, q, a}
    end
end
function HSVA(h, s, v, a)
    return HSVToColor{Hue=h, Sat=s, Value=v, Alpha=a}
end
