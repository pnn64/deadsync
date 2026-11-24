-- EASING --
local path = GAMESTATE:GetCurrentSong():GetSongDir()..'lua/'
local images = FILEMAN:GetDirListing(GAMESTATE:GetCurrentSong():GetSongDir()..'lua/car/')
loadfile(path..'easing.lua')()

LIST_ACTORS = false

checked = false

local linear, inQuad, outQuad, inOutQuad, outInQuad, inCubic, outCubic, inOutCubic, outInCubic, inQuart,outQuart, inOutQuart, outInQuart, inQuint, outQuint, inOutQuint, outInQuint, inSine, outSine, inOutSine,outInSine, inExpo, outExpo, inOutExpo, outInExpo, inCirc, outCirc, inOutCirc, outInCirc, inElastic,outElastic, inOutElastic, outInElastic, inBack, outBack, inOutBack, outInBack, inBounce, outBounce,inOutBounce, outInBounce = ease.linear, ease.inQuad, ease.outQuad, ease.inOutQuad, ease.outInQuad,ease.inCubic, ease.outCubic, ease.inOutCubic, ease.outInCubic, ease.inQuart, ease.outQuart,ease.inOutQuart, ease.outInQuart, ease.inQuint, ease.outQuint, ease.inOutQuint, ease.outInQuint,ease.inSine, ease.outSine, ease.inOutSine, ease.outInSine, ease.inExpo, ease.outExpo, ease.inOutExpo,ease.outInExpo, ease.inCirc, ease.outCirc, ease.inOutCirc, ease.outInCirc, ease.inElastic,ease.outElastic, ease.inOutElastic, ease.outInElastic, ease.inBack, ease.outBack, ease.inOutBack,ease.outInBack, ease.inBounce, ease.outBounce, ease.inOutBounce, ease.outInBounce

	sw = SCREEN_WIDTH
	sh = SCREEN_HEIGHT --shortcuts
	
	scx = sw/2
	scy = sh/2
	
local po = {GAMESTATE:GetPlayerState(0):GetPlayerOptions'ModsLevel_Song', GAMESTATE:GetPlayerState(1):GetPlayerOptions'ModsLevel_Song'}
function mod_do(str,pn)
	--if pn then if GAMESTATE:IsPlayerEnabled(pn-1) then handle_mods(str,pn) end
	--else for pn=1,2 do if GAMESTATE:IsPlayerEnabled(pn-1) then handle_mods(str,pn) end end end
	
    if pn then
        po[pn]:FromString(str)
    else
        po[1]:FromString(str)
        po[2]:FromString(str)
    end
end

_numplayers = 0
local _diff = {0,0}
if GAMESTATE:IsHumanPlayer(PLAYER_1) then
	local stepsr = (GAMESTATE:GetCurrentSteps(PLAYER_1):GetDifficulty())
	if stepsr == "Difficulty_Beginner" then _diff[1] = 0 end
	if stepsr == "Difficulty_Easy" then _diff[1] = 1 end
	if stepsr == "Difficulty_Medium" then _diff[1] = 2 end
	if stepsr == "Difficulty_Hard" then _diff[1] = 3 end
	if stepsr == "Difficulty_Challenge" then _diff[1] = 4 end
	_numplayers = _numplayers+1
end
if GAMESTATE:IsHumanPlayer(PLAYER_2) then
	local stepsr = (GAMESTATE:GetCurrentSteps(PLAYER_2):GetDifficulty())
	if stepsr == "Difficulty_Beginner" then _diff[2] = 0 end
	if stepsr == "Difficulty_Easy" then _diff[2] = 1 end
	if stepsr == "Difficulty_Medium" then _diff[2] = 2 end
	if stepsr == "Difficulty_Hard" then _diff[2] = 3 end
	if stepsr == "Difficulty_Challenge" then _diff[2] = 4 end
	_numplayers = _numplayers+1
end


--print("tech num players: ".._numplayers)

-- thank u ky dashiel
local poptions = {GAMESTATE:GetPlayerState(0):GetPlayerOptions('ModsLevel_Song'), GAMESTATE:GetPlayerState(1):GetPlayerOptions('ModsLevel_Song')}
_isusingreverse = {false, false}
_isusinghallway = {false, false}
_isusingdistant = {false, false}
_miniscale = {1,1}
_coverusage = {1,1}
for pn = 1, 2 do
	if poptions[pn] then
		_isusingreverse[pn] = poptions[pn]:Reverse() ~= 0
		_isusinghallway[pn] = poptions[pn]:Tilt() == -1
		_isusingdistant[pn] = poptions[pn]:Tilt() == 1
		_miniscale[pn] = (200 - poptions[pn]:Mini()) * 0.005
		_coverusage[pn] = poptions[pn]:Cover()
	end
end

			sw,sh = SCREEN_WIDTH,SCREEN_HEIGHT
			
			curmod = 1
			m_current = {{},{}}
			m_lastframe = {{},{}}
			m_startv = {{},{}}
			m_startt = {{},{}}
			m_endv = {{},{}}
			m_len = {{},{}}
			m_ease = {{},{}}
			m_opt1 = {{},{}}
			m_opt2 = {{},{}}
			m_tweening = {{},{}}
			modnames = {}
			m_isreal = {} --REAL mods that are part of the engine
			
			function definemod(name,initv,real)
				if not initv then initv = 0 end
				if real == nil then real = true end
				table.insert(modnames,name)
				m_isreal[name] = real
				for pn=1,2 do
					m_tweening[pn][name] = false
					m_current[pn][name] = initv
					m_lastframe[pn][name] = -1
					m_startv[pn][name] = initv
					m_endv[pn][name] = initv
					m_startt[pn][name] = 0
					m_len[pn][name] = 0
					m_opt1[pn][name] = 0
					m_opt2[pn][name] = 0
					m_ease[pn][name] = linear
				end
			end
			
			function modulo(a, b)
				return a - math.floor(a/b)*b;
			end
			
			function randomXD(t)
				if t == 0 then return 0.5 else
				return modulo(math.sin(t * 3229.3) * 43758.5453, 1) end
			end
			
			--regular ass mods
			definemod('drunk')
			definemod('tipsy')
			definemod('stealth')
			definemod('sudden')
			definemod('suddenoffset')
			definemod('brake')
			definemod('beat')
			definemod('tiny')
			definemod('dark')
			definemod('flip')
			definemod('invert')
			definemod('centered')
			definemod('split')
			definemod('cross')
			definemod('alternate')
			definemod('reverse')
			definemod('hallway')
			definemod('distant')
			definemod('confusionoffset')
			definemod('cover')
			for i=1,8 do
				definemod('movex'..i)
				definemod('movey'..i)
				definemod('dark'..i)
				definemod('tiny'..i)
				definemod('confusionoffset'..i)
			end
			
			--tweens as mods lol
			definemod('x',0,false)
			definemod('y',0,false)
			definemod('z',0,false)
			definemod('rotationx',0,false)
			definemod('rotationy',0,false)
			definemod('rotationz',0,false)
			definemod('skewx',0,false)
			definemod('zoomx',1,false)
			definemod('zoomy',1,false)
			definemod('zoomz',1,false)
			definemod('zoom',1,false)

			definemod('thewagger',0,false)
			definemod('thewagger2',0,false)
			definemod('pixelamount',0,false)
			definemod('screenvibby',0,false)
			
			mods, ma, ca, mfsb = {}, {}, 1, GAMESTATE:GetSongBeat();
			local me, m2 = function (t)
				local beat,len,ease,str1,mod = unpack(t)
				for pn=1,2 do
					--print(_diff[pn],t.min)
					if not t.min and not t.only or t.only and _diff[pn] == t.only or t.min and _diff[pn] >= t.min then
						if not t.plr or pn == t.plr then
							table.insert(mods,{beat,len,ease,str1,mod,startVal=t.startVal,opt1=t.opt1,opt2=t.opt2,plr=t.plr})
						end
					end
				end
			end, function (beat,fun,per) table.insert(ma,{beat,fun,per}) end
			function ome(t)
				me{t[1],t[2],t[3],startVal=t[4],t[5],t[6],min=t.min,only=t.only,opt1=t.opt1,opt2=t.opt2,plr=t.plr}
			end
			
			capturedOrig = false
			origx = {SCREEN_WIDTH*0.25,SCREEN_WIDTH*0.75}
			origy = {SCREEN_HEIGHT*0.5,SCREEN_HEIGHT*0.5}
			origz = {0,0}
			
			function sm2(t)
				local b,l,e,v2,v1,mo = unpack(t)
				me{b-.05,.1,linear,v2,mo,min=t.min,plr=t.plr,only=t.only,opt1=t.opt1,opt2=t.opt2}
				me{b+.05,l-.05,e,v1,mo,min=t.min,plr=t.plr,only=t.only,opt1=t.opt1,opt2=t.opt2}
			end
			
			function set(t)
				local b,v1,mo = unpack(t)
				me{b,0,linear,v1,mo,min=t.min,plr=t.plr,only=t.only}
				return set
			end
			
			_anyplayers = false
			_firstplayer = -1
			
			m2(0,function()
				for pn=1,2 do
					local a = SCREENMAN:GetTopScreen():GetChild('PlayerP'..pn)
					
					local darkSize = math.min((SCREEN_WIDTH*.5)/320,SCREEN_HEIGHT/480)
					
					if a then
						origx[pn] = a:GetX()
						origy[pn] = a:GetY()
						origz[pn] = a:GetZ()
						
						_anyplayers = true
						if _firstplayer == -1 then
							_firstplayer = pn
						end
						
					end
					
				end
				if _firstplayer == -1 then
					_firstplayer = 1
				end
				capturedOrig = true
			end,true)

			m2(0,function()
				if GAMESTATE:IsHumanPlayer(PLAYER_1) and GAMESTATE:IsHumanPlayer(PLAYER_2) then
					if _diff[1] <= 1 then
						MESSAGEMAN:Broadcast('P1InvalidDiff')
						--SCREENMAN:SystemMessage('P1 screen effects removed')
					end
					if _diff[2] <= 1 then
						MESSAGEMAN:Broadcast('P2InvalidDiff')
						--SCREENMAN:SystemMessage('P2 screen effects removed')
					end
				else
					if GAMESTATE:IsHumanPlayer(PLAYER_1) then
						if _diff[1] <= 1 then
							MESSAGEMAN:Broadcast('NoValidDiff')
							--SCREENMAN:SystemMessage('Screen effects removed globally')
						end
					end
					if GAMESTATE:IsHumanPlayer(PLAYER_2) then
						if _diff[2] <= 1 then
							MESSAGEMAN:Broadcast('NoValidDiff')
							--SCREENMAN:SystemMessage('Screen effects removed globally')
						end
					end
				end
			end,true)
			
			
			--thanks mr thatkid!!!
			local log0_5 = 1/math.log(0.5)
			--Save this so we do one less math.log & division every call, maybe.

			local skawx = function(x)
				for i = 1, 2 do
					if mod_plr[i] then
						mod_plr[i]:skewx(x/100)
					end
				end
			end
			
			function SM5Tiny(amt)
				--takes a nitg tiny% and returns a sm5 tiny%
				--return amt --in nitg, just return base val
				return (math.log(1-0.5*(amt*0.01))*log0_5)*100
			end
			
			-- template resets important mods for player comfort, put them back if picked up
			for pn = 1, 2 do
				if _isusingreverse[pn] then
					me{0,.01,linear,100,'reverse',plr=pn}
				end
				me{0,.01,linear,_coverusage[pn],'cover',plr=pn}
				if _isusinghallway[pn] then
					me{0,.01,linear,100,"hallway",plr=pn}
				elseif _isusingdistant[pn] then
					me{0,.01,linear,100,"distant",plr=pn}
				end
			end
			
			-- YOUR MODS HERE
			
			-- "me" = mod ease
			-- example usage | me{beat, length, ease type, value, 'mod name'}
			for i = 1, 4 do
				me{-8, 1, linear, 800, 'tiny'..i}
			end
			for i = 1, 2 do
				me{5, 1.5, inOutBack, 0, 'tiny'..i}
			end
			for i = 3, 4 do
				me{5.5, 1.5, inOutBack, 0, 'tiny'..i}
			end

			me{24, 0.05, outExpo, 50, 'stealth'}
			me{24.5, 0.05, outExpo, 0, 'stealth'}

			me{62, 0.05, outExpo, 100, 'dark'}

			for i = 62, 62.5, 0.5 do
				me{i, 0.05, linear, 0, 'stealth'}
				me{i+.25, 0.05, linear, 100, 'stealth'}
			end
			me{64, 1, outCirc, 0, 'dark'}
			me{64, 2, outCirc, 0, 'stealth'}

			ome{100, 8, outCirc, 100, 0, 'brake'}
			for i = 116, 116.5, 0.5 do
				me{i, 0.25, outSine, 1.03, 'zoomy'}
				me{i+.25, 0.25, outSine, 0.97, 'zoomy'}
			end
			me{117, 0.5, outSine, 1.03, 'zoomy'}
			me{117.5, 0.5, outSine, 1, 'zoomy'}

			me{119, 0.5, outSine, 5, 'rotationz'}
			me{119.5, 0.5, outSine, 0, 'rotationz'}
			for i = 125.5, 126, 0.5 do
				me{i, 0.05, linear, 25, 'stealth'}
				me{i+.25, 0.05, linear, 0, 'stealth'}
			end
			me{127.5, 0.5, outSine, 0.9, 'zoomx'}
			me{128, 0.5, outSine, 1.1, 'zoomx'}
			me{128.5, 0.5, outSine, 1, 'zoomx'}
			me{132, 2, linear, 15, 'drunk'}
			me{134, 2, linear, 0, 'drunk'}

			me{154, 1, inSine, 25, 'thewagger'}
			me{155, 1, outSine, 0, 'thewagger'}

			me{157, 0.5, outSine, -5, 'rotationz'}
			me{157.5, 0.5, outSine, 0, 'rotationz'}

			ome{172, 1, outCirc, -35, 0, 'tiny'}
			ome{179.5, 1, outCirc, -35, 0, 'tiny'}
			ome{180, 1, outCirc, -35, 0, 'tiny'}
			ome{204, 1, outBack, -50, 0, 'tiny'}
			ome{211.5, 1, outBack, -50, 0, 'tiny'}
			ome{212, 1, outBack, -50, 0, 'tiny'}

			for i = 220, 228, 2 do
				ome{i, 1, outBack, -50, 0, 'tiny'}
			end

			ome{236, 1, outCirc, 15, 0, 'screenvibby'}
			ome{237.5, 1, outCirc, 15, 0, 'screenvibby'}

			me{239, 0.1, outCirc, 30, 'stealth'}
			me{239.5, 0.1, outCirc, 15, 'stealth'}
			me{240, 0.1, outCirc, 0, 'stealth'}

			ome{307, 2, outBack, -10, 0, 'thewagger2'}

			ome{404, 1, outBack, -50, 0, 'tiny'}
			ome{411.5, 1, outBack, -50, 0, 'tiny'}
			ome{412, 1, outBack, -50, 0, 'tiny'}

			for i = 420, 428, 2 do
				ome{i, 1, outBack, -50, 0, 'tiny'}
			end

			me{434.5, 1.5, linear, 3, 'pixelamount'}
			me{442.5, 1.5, linear, 0, 'pixelamount'}
			me{481.75, 0.25, outExpo, 3, 'pixelamount'}
			me{484, 1, linear, 0, 'pixelamount'}
			me{499, 1, inExpo, 3, 'pixelamount'}
			me{505.75, 0.75, inExpo, 0, 'pixelamount'}

			ome{539, 2, outBack, 10, 0, 'thewagger2'}
			me{548, 16, outCirc, 100, 'stealth'}

			-- the rest
			
			function cmp(a,b)
				return a[1] < b[1]
			end
			if table.getn(ma) > 1 then
				table.sort(ma, cmp)
			end
			if table.getn(mods) > 1 then
				table.sort(mods, cmp)
			end

local ac = Def.ActorFrame{
	
	Def.Actor{
		OnCommand=function(s)
			s:queuecommand('Loop')
		end,
		LoopCommand=function(self)
			local beat = GAMESTATE:GetSongBeat()
			if beat > mfsb+0.1 then
				
				while curmod <= #mods and beat > mods[curmod][1] do
					local v = mods[curmod]
					
					local mn = v[5]
					local dur = v[2]
					if v.timing and v.timing == 'end' then
						dur = v[2]-v[1]
					end
					
					print('launch attack '..v[4]..' '..mn..' at beat '..v[1])
					
					if v.plr and not v.pn then v.pn = v.plr end
					
					for pn=1,2 do
						if not v.pn or pn == v.pn then
							m_startt[pn][mn] = v[1]
							m_len[pn][mn] = dur
							m_ease[pn][mn] = v[3]
							if v.startVal then
								m_startv[pn][mn] = v.startVal
							else
								m_startv[pn][mn] = m_current[pn][mn]
							end
							if v.add then
								m_endv[pn][mn] = m_current[pn][mn]+v[4]
							else
								m_endv[pn][mn] = v[4]
							end
							m_opt1[pn][mn] = v.opt1
							m_opt2[pn][mn] = v.opt2
							m_tweening[pn][mn] = true
						end
					end
					
					curmod = curmod+1
				end
				
				for pn=1,2 do
					for _,v in pairs(modnames) do
						if m_tweening[pn][v] then
							local curtime = beat - m_startt[pn][v]
							local duration = m_len[pn][v]
							local startstrength = m_startv[pn][v]
							local diff = m_endv[pn][v] - startstrength
							local curve = m_ease[pn][v]
							local strength = curve(curtime, startstrength, diff, duration, m_opt1[pn][v], m_opt2[pn][v])
							m_current[pn][v] = strength
							if beat > m_startt[pn][v]+duration then
								m_tweening[pn][v] = false
							end
						else
							m_current[pn][v] = m_endv[pn][v]
						end
					end
				end
				
				--ONLY apply mods if there were changes THIS frame
				for pn=1,2 do
					local mstr = ''
					for _,v in pairs(modnames) do
						if m_isreal[v] and m_lastframe[pn][v] ~= m_current[pn][v] then
							mstr = mstr..'*10000 '..m_current[pn][v]..' '..v..','
							m_lastframe[pn][v] = m_current[pn][v]
						end
					end
					if mstr ~= '' then
						mod_do(mstr,pn)
					end
				end
				
				if capturedOrig then
					SCREENMAN:GetTopScreen():vibrate()
					for pn=1,2 do
						local a = SCREENMAN:GetTopScreen():GetChild('PlayerP'..pn)
						if a then
							a:x(origx[pn]+m_current[pn].x*SCREEN_HEIGHT/480)
							if _isusingreverse[pn] then
								a:y(origy[pn]-m_current[pn].y*SCREEN_HEIGHT/480)
							else
								a:y(origy[pn]+m_current[pn].y*SCREEN_HEIGHT/480)
							end
							a:z(origz[pn]+m_current[pn].z)
							if _isusingreverse[pn] then
								a:rotationx(-m_current[pn].rotationx)
							else
								a:rotationx(m_current[pn].rotationx)
							end
							a:rotationy(m_current[pn].rotationy)
							a:rotationz(m_current[pn].rotationz)
							a:skewx(m_current[pn].skewx)
							a:basezoomx(m_current[pn].zoomx*m_current[pn].zoom)
							a:basezoomy(m_current[pn].zoomy*m_current[pn].zoom)
							a:zoomz(m_current[pn].zoomz*m_current[pn].zoom)
							
							--custom mods below
							a:wag()
							a:effectmagnitude(0,m_current[pn].thewagger,m_current[pn].thewagger2)
							a:effectperiod(1)
							a:effectclock('bgm')
							SCREENMAN:GetTopScreen():effectmagnitude(m_current[pn].screenvibby,0,0)
							
							screen_pixel1:zoom(1/m_current[pn].pixelamount)
							screen_pixel2:zoom(m_current[pn].pixelamount)
							
							if m_current[pn].pixelamount == 0 then
								screen_pixel2:visible(false)
							else
								screen_pixel2:visible(true)
							end
						end
					end
					
				end
				
				while ca<=table.getn(ma) and GAMESTATE:GetSongBeat()>=ma[ca][1] do
					if ma[ca][3] or GAMESTATE:GetSongBeat() < ma[ca][1]+2 then
						ma[ca][2]()
					end
					ca = ca+1
				end
			end
			self:sleep(1/60) self:queuecommand('Loop')
		end
	},
	
	--pixel starts here
	
	Def.ActorFrameTexture{
	Name = "ScreenTex";
	InitCommand=function(self)
		self:SetTextureName( "ScreenTex" )
		self:SetWidth(sw);
		self:SetHeight(sh);
		self:EnableAlphaBuffer(true);
		self:Create();
	end,
	PixelMessageCommand=function(self)
		self:visible(true);
		self:Draw();
	end,
		Def.Quad{ OnCommand=cmd(x,SCREEN_CENTER_X;y,SCREEN_CENTER_Y;zoomto,SCREEN_WIDTH,SCREEN_HEIGHT;diffuse,0,0,0,1), },
		--LoadActor("../bg.png")..{
		--	OnCommand=cmd(x,SCREEN_CENTER_X;y,SCREEN_CENTER_Y;zoomto,SCREEN_WIDTH,SCREEN_HEIGHT;diffuse,1,1,1,PREFSMAN:GetPreference('BGBrightness')),
		--},
		Def.ActorProxy{ Name = "ProxyUL"; OnCommand=function(self) self:x(0); self:y(0) self:SetTarget(SCREENMAN:GetTopScreen():GetChild('Underlay')); end },
		Def.ActorProxy{ Name = "ProxyP1"; OnCommand=function(self) self:x(0); self:y(0)
			
			if not string.find(SCREENMAN:GetTopScreen():GetName(),"^ScreenEdit") then
				if SCREENMAN:GetTopScreen():GetChild('PlayerP1') then
					self:SetTarget(SCREENMAN:GetTopScreen():GetChild('PlayerP1'));
				end
			else
				for _, actor in ipairs(SCREENMAN:GetTopScreen():GetChild('')) do
					if tostring(actor):find("Player") then self:SetTarget(actor); end
				end
			end
			
		end },
		Def.ActorProxy{ Name = "ProxyP2"; OnCommand=function(self) self:x(0); self:y(0)
			if SCREENMAN:GetTopScreen():GetChild('PlayerP2') then
				self:SetTarget(SCREENMAN:GetTopScreen():GetChild('PlayerP2'));
			end
		end },
		Def.ActorProxy{ Name = "ProxyOL"; OnCommand=function(self) self:x(0); self:y(0) self:SetTarget(SCREENMAN:GetTopScreen():GetChild('Overlay')); end },
	
	},
	
	Def.ActorFrameTexture{
	Name = "ScreenPixel";
	InitCommand=function(self)
		self:SetTextureName( "ScreenPixel" )
		self:SetWidth(sw);
		self:SetHeight(sh);
		self:EnableAlphaBuffer(true);
		self:Create();
	end,
	PixelMessageCommand=function(self)
		self:visible(true);
		self:Draw();
	end,
		Def.Sprite{
			Texture = "ScreenTex";
			OnCommand = function(self)
				screen_pixel1 = self
				self:y(scy)
				self:x(scx)
				self:zoom(1)
				self:visible(true);
			end,
		},
	},

	Def.Sprite{
		Texture = "ScreenPixel";
		OnCommand = function(self)
			screen_pixel2 = self
			self:y(scy)
			self:x(scx)
			self:zoom(1)
			self:SetTextureFiltering(false)
			self:visible(false);
		end,
		P1InvalidDiffMessageCommand=function(self)
			self:cropleft(0.5);
		end,
		P2InvalidDiffMessageCommand=function(self)
			self:cropright(0.5);
		end,
		NoValidDiffMessageCommand=function(self)
			self:croptop(1);
			self:diffusealpha(0);
		end,
	},

	Def.Quad{ InitCommand=cmd(visible,false), OnCommand=cmd(sleep,9E9) },
	
	Name= "xtl_actor_d",
	
}
return ac