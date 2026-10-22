local button = Var "Button"
local t = Def.ActorFrame {
	Def.Sprite {
		Texture=NOTESKIN:GetPath( '_down', 'tap mine' );
		Frame0000=0;
		Delay0000=1;
	};
	Def.Sprite {
		Name="Spark_Left";
		Texture="_Mine Spark";
		Frames = Sprite.LinearFrames( 16, 1 );
		InitCommand=function(self)
			self:zoom(1.2):effectclock("timer"):SetAllStateDelays(0.05)
			if     button == "Left" then
				self:setstate(0):rotationz(90)
			elseif button == "1" then
				self:setstate(3):rotationz(45)
			elseif button == "Down" then
				self:setstate(5):rotationz(0)
			elseif button == "Up" then
				self:setstate(8):rotationz(180)
			elseif button == "2" then
				self:setstate(11):rotationz(135)
			elseif button == "Right" then
				self:setstate(13):rotationz(90)
			else
				Warn("Down Tap Mine.lua - Unsupported Button "..button)
			end
		end;
	};
};
return t;
