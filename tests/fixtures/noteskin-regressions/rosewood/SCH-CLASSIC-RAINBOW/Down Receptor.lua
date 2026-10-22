--If a Command has "NOTESKIN:GetMetricA" in it, that means it gets the command from the metrics.ini, else use cmd(); to define command.
--If you dont know how "NOTESKIN:GetMetricA" works here is an explanation.
--NOTESKIN:GetMetricA("The [Group] in the metrics.ini", "The actual Command to fallback on in the metrics.ini");



--press receptor animation
local function pressreceptor(player)
	return function(self) self:finishtweening()
			self:zoom(0.9):sleep(1/60)
			:linear(4/60):zoom(1.0)
	end
end

local player = Var "Player";

local function GetPlayerSongBeat(player)
	local steps = GAMESTATE:GetCurrentSteps(player);
	local timing = steps:GetTimingData();
	return timing:GetBeatFromElapsedTime(GAMESTATE:GetSongPosition():GetMusicSeconds());
end;

local t = Def.ActorFrame {
	LoadActor(NOTESKIN:GetPath( '_down', 'Go Receptor' ))..{
		Name="Receptor";
		InitCommand=cmd(effectclock,"beat");
        NoneCommand=NOTESKIN:GetMetricA("ReceptorArrow", "NoneCommand");
		PressCommand=pressreceptor(player);
		OnCommand=function(s) s:animate(false):setstate(2) end,
	};
};

local function update(self)
	local song = GAMESTATE:GetCurrentSong();
	local start;
	
	if song then
		start = song:GetFirstBeat()-8
	end
	
	local receptor = self:GetChild("Receptor");
	local beat = GetPlayerSongBeat(player);
	local range = (beat*10)%10;
	
	if beat >= start then
		if range >= 1 and range < 9 then
			receptor:setstate(1);
		else
			receptor:setstate(0);
		end;
	else
		receptor:setstate(2);
	end;
		
end;

	t.InitCommand=cmd(SetUpdateFunction,update;);

return t;