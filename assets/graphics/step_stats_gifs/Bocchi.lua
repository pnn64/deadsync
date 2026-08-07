t = Def.ActorFrame {}

t[#t+1] = Def.Sprite {
	
	Texture="Bocchi 6x3.png",
	Frame0000=0,	Delay0000=0.125,
	Frame0001=1,	Delay0001=0.125,
	Frame0002=2,	Delay0002=0.125,
	Frame0003=3,	Delay0003=0.125,
	Frame0004=4,	Delay0004=0.125,
	Frame0005=5,	Delay0005=0.125,
	Frame0006=6,	Delay0006=0.125,
	Frame0007=7,	Delay0007=0.125,
	Frame0008=8,	Delay0008=0.125,
	Frame0009=9,	Delay0009=0.125,
	Frame0010=10,	Delay0010=0.125,
	Frame0011=11,	Delay0011=0.125,
	Frame0012=12,	Delay0012=0.125,
	Frame0013=13,	Delay0013=0.125,
	Frame0014=14,	Delay0014=0.125,
	Frame0015=15,	Delay0015=0.125,
	Frame0016=16,	Delay0016=0.125,
	Frame0017=17,	Delay0017=0.125,
	
	OnCommand=function(self)
		self:effectclock("bgm")
		self:cropright(0.02)
		self:cropleft(0.02)
		self:croptop(0.02)
		self:cropbottom(0.02)
		self:zoom(2)
	end
	
}

return t