assert(ease == nil, 'engine must not preinstall chart easing functions')
if not ease then
    ease = {}
    ease.t_linear = function(t,b,c,d) return c*t/d+b end
    ease.outQuad = function(t,b,c,d) t=t/d; return -c*t*(t-2)+b end
end
assert(ease.t_linear(0.25,10,20,1) == 15)
assert(ease.outQuad(0.25,10,20,1) == 18.75)
assert(ease.unknown == nil)
return Def.ActorFrame{}
