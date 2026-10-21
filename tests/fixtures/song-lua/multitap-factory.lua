-------------------------------------------------------------------------------
--
--		Multitap Factory & Assistance
--
--		Author: 	Telperion
--		Date: 		2023-10-18
--		Version:	1.2
--		Target:		SM5.0.12+ (primarily), OutFox alpha 4.12+ (where possible)
--
-------------------------------------------------------------------------------
--
--		So, like...it's been three years since UKSRT8 and TaroNuke's
--		"Hardware Bullshit Tournament", the event where a prototype of a dance
--		pad with fine-grained pressure response got an exhibition with a whole
--		set of files featuring new (to 4-panel) chart mechanics. It really was
--		a blast! and I've wished for a while now that there would eventually be
--		some way to play the whole thing at home.
--
--		During a post-UKSRTX hangout, Taro lugged the platform out so newcomers
--		who hadn't been around for UKSRT8 could give the HBT files a shot.
--		While watching, something clicked in my brain: the pieces of a few
--		half-finished SM5 mods files I had lying around could be assembled,
--		with a little extra work, into the HBT multitap note type.
--		[https://www.youtube.com/watch?v=OQiZJ38fDJM&t=1m04s]
--
--		I got to sweep out some very dark corners of StepMania 5 with this one:
--		*	You can just *make* fakes and explosions, if you hook them up right
--		*	Really glad non-beat-subtracting offset zoom splines work the way
--			I expected, because I couldn't think of any sensible other options
--		*	Parameter ordering in ArrowEffects:Get<>() calls is spicy
--				(which will eventually require an update to this code, because
--				I opened my big mouth :P)
--		*	Sad about lack of access to the FOV and vanishing point of an actor
--				(straight up translated portions of the C++ source for that)
--		*	Mysterious version-dependent radian poltergeist?? hello??????
--		*	I didn't actually know propagatecommand existed until I wrote a
--			(poorly-covering) function to do the same thing with reflection
--		*	Kinda wish there was a generic way to retrieve the color scheme of
--			a rhythm noteskin (e.g., solo vs. note)
--		*	Playfields aren't vertically centered in the screen and this is
--			*theme-dependent* and although I understand why that might be
--			useful I sure as hell am allowed to complain about it
--
--		But the upshot is:
--		*	You can write your own multitap files!
--			1.	Copy this FG animation (multitap\*) into your song directory
--			2.	Copy the #FGCHANGES: line into your .ssc file
--			3.	Replace multitap_data.lua to suit your chart
--				(eventually I will also provide autogeneration code for this
--				based on interpreting the corresponding double slot)
--			4.	Increase brain wrinkliness
--			5a.	Have a tappy slappy time
--			5b.	Recoil in horror from what you have brought into existence
--		*	Multitaps should be compatible with most common SM5 noteskins
--		*	Multitaps will act like regular taps under all* mods
--		*	Multitap-enabled files will work on most cabs running
--			SM5.0.12+ and Simply Love
--
--		TODO:
--		*	Soften the hardcoding of 4-panel mode
--		*	Soften use of 45-degree FOV for spoofing perspective mods (just in
--			case other FG changes want to mess with that)
--		*	Haven't figured out how to implement arrow glow yet (for use
--			during stealth/hidden/sudden sections)
--		*	Track down the mysterious version-dependent radian poltergeist
--		*	Multitaps under Cmod don't move smoothly. For now I'm pretending
--			this is a feature :)
--		*	nITG compatibility...
--
--		TRICKY:
--		*	This implementation of multitaps locks down zoom splines - no
--			additional FG animations should attempt to use them without
--			accounting for the multitap regions
--		*	Anything in the multitap regions will be hidden, but only taps get
--			re-presented to the player. Lifts, mines, and fakes will be
--			invisible (but still hittable...)
--
--
--		Changelog:
--		v1.2 (Who The Hell Is Edgar?, 2023-10-18)
--		-	Special behavior for Poe taps, gated by song title
--		-	Revise fall-through heuristic for coloring by noteskin
--		-	Update coverage for Mysterious Version-Dependent Radian Poltergeist
--		-	Propagate the custom-rolled mini and tilt mods that OutFox themes
--			soundwaves and superuser may apply
--		-	GenerateMultitaps.py now included with this simfile!
--
-------------------------------------------------------------------------------

local whoTheHellIsEdgar = (GAMESTATE:GetCurrentSong():GetMainTitle() == "Who the Hell Is Edgar?")
if whoTheHellIsEdgar then
	-- SCREENMAN:SystemMessage("Oh, mio padre...there's a ghost in my body!")
end

--[[##]]--[[##]]--[[##]]--[[##]]--[[##]]--[[##]]--[[##]]--[[##]]--[[##]]--
--
-- Multitap Description Table
--
-- Generate this file by using the accompanying Python script.
--      python3 GenerateMultitaps.py "C:\path\to\simfile.sm"
--      GenerateMultitaps(r"C:\path\to\simfile.sm")
--
-- The script uses the 2P side of the double chart in the matching slot
-- to group tap notes on the 1P side into multitap regions.
-- By copying the 4-panel chart to the corresponding 8-panel slot and
-- inserting holds in the corresponding lane on the other side, you can
-- tell the script which swaths of notes should become multitaps.
--
--[[##]]--[[##]]--[[##]]--[[##]]--[[##]]--[[##]]--[[##]]--[[##]]--[[##]]--

-- Version matching performed here.
multitaps = {}
multitap_version = {1, 2}

-- Load multitap data into workspace
local whereTheFlipAmI = GAMESTATE:GetCurrentSong():GetSongDir()
dofile(whereTheFlipAmI .. "multitap/multitap_data.lua")

-- Compare version of multitap data and multitap parser.
local version_mismatch = function(mv_data, mv_parser)
	SCREENMAN:SystemMessage("### Multitap version mismatch: data @ "..mv_data[1].."."..mv_data[2]..", parser @ "..mv_parser[1].."."..mv_parser[2])
end
local version_record = function(mv_data, mv_parser)
	Trace("### Multitap versions: data @ "..mv_data[1].."."..mv_data[2]..", parser @ "..mv_parser[1].."."..mv_parser[2])
end

if multitaps["_version"] then
	version_record(multitaps["_version"], multitap_version)
	-- Data version major can't be greater than parser version major
	if multitaps["_version"][1] > multitap_version[1] then
		version_mismatch(multitaps["_version"], multitap_version)
		return Def.ActorFrame{}
	end
	-- Data version minor can't be greater than parser version minor
	if (multitaps["_version"][1] == multitap_version[1]) and
		(multitaps["_version"][2] > multitap_version[2]) then
		version_mismatch(multitaps["_version"], multitap_version)
		return Def.ActorFrame{}
	end
else
	SCREENMAN:SystemMessage("### Found unversioned multitap data")
end

--[[##]]--[[##]]--[[##]]--[[##]]--[[##]]--[[##]]--[[##]]--[[##]]--[[##]]--
--
-- Some helper functions I haven't isolated yet
--
--[[##]]--[[##]]--[[##]]--[[##]]--[[##]]--[[##]]--[[##]]--[[##]]--[[##]]--

function TryCommandOnLeaves(act, command_name, command_params, verbose, d, fake_name)
	-- I ended up not needing this but I'm gonna leave it here anyway
	-- this was An Adventure

	if not act then
		print("::: ??? null ???")												-- Am I Pregant?
		return																	-- Am I pragnent?
	end

	d = d and d or 0															-- Am I pargant?
	fake_name = fake_name and fake_name or "<N/A>"								-- Am i gregnant?

	local indent = string.rep("\t", d)											-- Am i pegnate?? Help!?

	local n_format = (act["GetName"] and act:GetName() or "")					-- Is there a possibly that i'm pegrent?
		  n_format = (n_format ~= "") and n_format or fake_name					-- Am I pregegnant or am I okay?

	local n_kids = 0															-- Could I be pregonate?
	local s, r = pcall(function() return act:GetNumChildren() end);				-- How do I know if I'M prengan?
	if s then n_kids = r end													-- Can i be prregnant????

	local n_items = 0															-- Can u get pregante...?
	for k,m in pairs(act) do
		n_items = n_items + 1													-- Can u blink while u are pergert?
	end

	if verbose then
		Trace(	":#: "..indent..n_format..": "..								-- Can u down a 20 beat waterspline pegnat?
				n_kids.." children, "..											-- How can i get my notefield pragnet?
				n_items.." standard members"									-- What happen when get pergenat?
		)
	end

	-- Actual kids, with actual actor properties.
	if n_kids > 0 then
		local kids = act:GetChildren()											-- How can a ninefoot chart get prangnet?
		local i = 0																-- Will my get pragnan?
		for k,m in pairs(kids) do
			i = i + 1															-- What is the best time to step to be come pregnart
			TryCommandOnLeaves(m, command_name, command_params,					-- Does any one know how many tweens get bregant a year????
				verbose, d+1, n_format.."->"..i)								-- Are these systoms of being pregarnt?
		end
	end

	-- Imagine giving birth to a bunch of tables?? that's lua I guess
	for k,m in pairs(act) do
		if type(m) == "table" then
			local n_m = 0; for _ in pairs(m) do n_m = n_m + 1 end				-- Notefield aint had effectperiod since she got pregat?
			if verbose then
				Trace(	"::: "..indent.."\t"..n_format.."."..k..				-- Is it possible having step to a 8 miss fregnant?
						": table with "..n_m.." elements"						-- If an actors has starch masks on her texture does that mean she has been pargnet before.?
				)
			end
			TryCommandOnLeaves(m, command_name, command_params,					-- My circle is nomal,but yet i still dont get peegnant.wat can i use.?
				verbose, d+1, n_format.."."..k)									-- Has anybody got pergnut by just preset while using BothAtOnce control?
		else
			if verbose then
				Trace(	"::: "..indent.."\t"..n_format.."."..k..				-- Did most you actors FEEL pgrenant before find out?
						": "..m..", of type "..type(m))							-- I am pragananat last 5 wayoff so can i start step?
			end
		end
	end

	-- Dare ka ga fucking hear me dekimasu ka?
	if act[command_name.."Command"] then
		--if verbose then
			Trace("#:# Attempting "..command_name.."Command on "..n_format)		-- Dangerops prangent step? will it hurt arrow top of his head?
		--end
		if command_params then
			pcall(function()													-- Me and my notefield are tying to get prefnat and j havent took my BothAtOnce control in 12 plays?
				act:playcommand(command_name, unpack(command_params))			-- 38 + 2 wayoffs pregananant?
			end)																-- I think my down is pregernet???
		else
			pcall(function()													-- How long can u go being prognant to get an FailImmediately?
				act:playcommand(command_name)									-- Can i get prengt if he had a condor on?
			end)																-- I think I'm pretnet with my 14th table?
		end
	end
end

--[[##]]--[[##]]--[[##]]--[[##]]--[[##]]--[[##]]--[[##]]--[[##]]--[[##]]--
--
-- 		holh fucf?
--
--
-- 													HOLKY FUCY???
--
--[[##]]--[[##]]--[[##]]--[[##]]--[[##]]--[[##]]--[[##]]--[[##]]--[[##]]--

local sm_version = ProductVersion()
local sm_product = ProductID()
local is_outfox = (string.sub(sm_product, 1, 6) == "OutFox")
MYSTERIOUS_VERSION_DEPENDENT_RADIAN_POLTERGEIST = (
	string.sub(sm_version, 1, 3) == "5.1" or
	sm_product == "ITGmania" or is_outfox
) and (-180 / math.pi) or 0
-- SCREENMAN:SystemMessage(SCREEN_WIDTH)

--[[##]]--[[##]]--[[##]]--[[##]]--[[##]]--[[##]]--[[##]]--[[##]]--[[##]]--
--
-- Multitap generation code begins here
--
--[[##]]--[[##]]--[[##]]--[[##]]--[[##]]--[[##]]--[[##]]--[[##]]--[[##]]--

local multitap_parent = Def.ActorFrame {
	OnCommand = function(self)
		self:sleep(1000);
	end
};

-- Controls for tweaking visual behavior of multitaps
local multitap_error = false 					-- How is multitap parsing going?
local multitap_previsible = 8					-- Make the multitaps visible this many beats in advance of the first hit
local multitap_basebounce = 1.5					-- Multiplier for initial bounce velocity (1x matches inbound speed)
local multitap_elasticity = 1.05				-- Subsequent bounces get their rebound speed multiplied by this
local multitap_squishy = 0.2					-- Cartoonishly squish the arrow when traveling slowly and expand when fast.
local multitap_splines_calc = {false, false}	-- Spline Times EX should have stayed in pop'n 8. and that's the tea

if whoTheHellIsEdgar then
	multitap_basebounce = 3
	multitap_previsible = 16
	multitap_squishy = 0.15
end

-- Gotta know how many of these to make.
local multitap_max = 0
for _,mt_list in pairs(multitaps) do
	if multitap_max < #mt_list then
		multitap_max = #mt_list
	end
end

-- Initialize the multitap actor list.
--
-- 	Player number (1 or 2)
--		Multitap index
--			{frame, arrow, count}
local multitap_actors = {
	{},
	{}
}
for pn = 1,2 do
	for i = 1,multitap_max do
		multitap_actors[pn][i] = {}
	end
end
local multitap_explosions = {
	{},
	{}
}
local multitap_fields = {}

-- Slot and noteskin selection for each player.
local multitap_chart_sel = {
	"Hard",
	"Hard"
}
local noteskin_names = {
	"shadow",
	"shadow"
}

-- Precalculate a table of quantization colors by beat fraction.
-- Used to select a texture offset in the noteskin asset for the tap arrow.
local qtzn_lookup = {}
for _,qtzn in ipairs({48, 24, 16, 12, 8, 6, 4, 3, 2, 1}) do
	for i = 0,48,(48/qtzn) do
		qtzn_lookup[i] = qtzn
	end
end

local qtzn_tex = {}
qtzn_tex[ 0] = 0
qtzn_tex[ 1] = 0
qtzn_tex[ 2] = 1
qtzn_tex[ 3] = 2
qtzn_tex[ 4] = 3
qtzn_tex[ 6] = 4
qtzn_tex[ 8] = 5
qtzn_tex[12] = 6
qtzn_tex[16] = 7
qtzn_tex[24] = 7
qtzn_tex[48] = 7


-- I don't think it's reasonable within the UPS5 submission timeframe to
-- dynamically pull/calculate the actual *color* of each quantization for
-- an arbitrary noteskin, so I'm precalculating some options based on the
-- Cabby noteskin pack.
local qtzn_color_tables = {
	vivid = {					-- whole texture I'm fucking busy only get few color
		{"ffffff", "cccccc"},	-- 4ths
		{"ffffff", "cccccc"},	-- 8ths
		{"ffffff", "cccccc"},	-- 12ths
		{"ffffff", "cccccc"},	-- 16ths
		{"ffffff", "cccccc"},	-- 24ths
		{"ffffff", "cccccc"},	-- 32nds
		{"ffffff", "cccccc"},	-- 64ths
		{"ffffff", "cccccc"},	-- 192nds
	},
	shadow = {					-- best colorblindness acuity in the noteskins sharing the ITG palette
		{"ff6100", "ff0000"},	-- 4ths
		{"00a2ff", "00f0ff"},	-- 8ths
		{"fa81d1", "7a15fe"},	-- 12ths
		{"e2f90f", "09a357"},	-- 16ths
		{"fa81d1", "7a15fe"},	-- 24ths
		{"f1db03", "e67b02"},	-- 32nds
		{"33fc7b", "04b8b6"},	-- 64ths
		{"33fc7b", "04b8b6"},	-- 192nds
	},
	note = {					-- this is what the DDR Note noteskin should have been
		{"ff7c7c", "ff2121"},	-- 4ths
		{"7e86f4", "2432ec"},	-- 8ths
		{"be77fb", "9018f8"},	-- 12ths
		{"faff73", "f7ff11"},	-- 16ths
		{"f383bf", "eb2c93"},	-- 24ths
		{"ff966d", "ff4d06"},	-- 32nds
		{"90e3ff", "43d0ff"},	-- 64ths
		{"85ff7c", "30ff20"},	-- 192nds
	},
	color = {					-- the most like unto the true DDR Note noteskin
		{"ffc5c5", "ff0000"},	-- 4ths
		{"0000ff", "c5c5ff"},	-- 8ths
		{"00ff00", "c5ffc5"},	-- 12ths
		{"fff617", "646001"},	-- 16ths
		{"00ff00", "c5ffc5"},	-- 24ths
		{"00ff00", "c5ffc5"},	-- 32nds
		{"00ff00", "c5ffc5"},	-- 64ths
		{"00ff00", "c5ffc5"},	-- 192nds
	},
	rainbow = {					-- some time...you just haven`t care
		{"ff6100", "ff0000"},	-- 4ths
		{"00a2ff", "00f0ff"},	-- 8ths
		{"fa81d1", "7a15fe"},	-- 12ths
		{"fa81d1", "7a15fe"},	-- 16ths
		{"fa81d1", "7a15fe"},	-- 24ths
		{"fa81d1", "7a15fe"},	-- 32nds
		{"fa81d1", "7a15fe"},	-- 64ths
		{"fa81d1", "7a15fe"},	-- 192nds
	},
	horse = {				    -- I think it's very unprofessional of the official Trot 100 News account to try and cancel an artist.
		{"dfa9db", "a96fba"},	-- 4ths
		{"faba61", "d49234"},	-- 8ths
		{"98d3f1", "2c78b6"},	-- 12ths
		{"fe96b9", "b7366e"},	-- 16ths
		{"b6b3d5", "6947bf"},	-- 24ths
		{"f0e56e", "eae6bf"},	-- 32nds
		{"8b7bff", "503497"},	-- 64ths
		{"ebe6ad", "edb032"},	-- 192nds
	},
}

-- Anything not explicitly assigned here will pick up the "vivid" behavior (no color distinction).

qtzn_color_tables["ascii"]							= qtzn_color_tables["note"]
qtzn_color_tables["cel"]							= qtzn_color_tables["shadow"]
--qtzn_color_tables["color"]
qtzn_color_tables["cyber"]							= qtzn_color_tables["shadow"]
qtzn_color_tables["default"]						= qtzn_color_tables["note"]
qtzn_color_tables["delta"]							= qtzn_color_tables["shadow"]
qtzn_color_tables["ddrlike"]					    = qtzn_color_tables["shadow"]
qtzn_color_tables["enchantment"]					= qtzn_color_tables["shadow"]
qtzn_color_tables["easy"]							= qtzn_color_tables["note"]
qtzn_color_tables["exact"]						    = qtzn_color_tables["note"]
qtzn_color_tables["excel"]							= qtzn_color_tables["shadow"]
--qtzn_color_tables["horse"]
qtzn_color_tables["lambda"]							= qtzn_color_tables["note"]
qtzn_color_tables["metal"]							= qtzn_color_tables["shadow"]
--qtzn_color_tables["note"]
qtzn_color_tables["onlyonecouples"]					= qtzn_color_tables["shadow"]
--qtzn_color_tables["rainbow"]
qtzn_color_tables["retro"]							= qtzn_color_tables["note"]
--qtzn_color_tables["shadow"]
qtzn_color_tables["scalable"]						= qtzn_color_tables["shadow"]
qtzn_color_tables["solo"]							= qtzn_color_tables["rainbow"]
qtzn_color_tables["spotlight"]						= qtzn_color_tables["shadow"]
qtzn_color_tables["toonprints"]						= qtzn_color_tables["horse"]
qtzn_color_tables["trax"]							= qtzn_color_tables["note"]
qtzn_color_tables["vel"]							= qtzn_color_tables["shadow"]
qtzn_color_tables["vintage"]						= qtzn_color_tables["shadow"]
--qtzn_color_tables["vivid"]

local qtzn_color_lookup = function (noteskin_name)
	for color_paradigm, color_table in pairs(qtzn_color_tables) do
		if string.find(noteskin_name, color_paradigm) then
			return color_table
		end
	end
	return qtzn_color_tables["vivid"]
end

local _BB = function(b)
	-- Measure beats in increments of 192nds (= 1/48 of quarter notes).
	return math.floor(b*48 + 0.5)
end

local calc_qtzn = function(b)
	-- What quantization is this beat number?
	-- e.g., quarter = 1, 16th = 4, 24th = 6, etc.

	-- Graceful fallback for no-tap case.
	if not b then
		return 0
	end

	-- Each quarter can be divided into 48 steps.
	-- Get the nearest proper step.
	local d48 = math.floor(b*48 + 0.5) - math.floor(b)*48

	-- Decide where it falls.
	return qtzn_lookup[d48]
end

local parabolator = function(b, t, elastic)
	-- b = beat length of this multitap iteration
	-- t = time in beats since the start of this multitap iteration
	-- elastic = scaling of arrowpath position (1 = perfect bounce from approach speed, 0 = dead stop)
	-- returns distance back up the arrow path to travel
	--
	-- f(t) = v/b * t * (b - t), where v = approach speed
	-- we can get around needing to know the pixelar speed of the arrow by calculating distance
	-- in terms of beats traveled @ whatever the reading speed is.
	-- therefore the approach is always 1 beat/beat >:)
	if not elastic then
		elastic = 1
	end
	return elastic * t * (b-t) / b
end

local parabolator_dt = function(b, t, elastic)
	-- b = beat length of this multitap iteration
	-- t = time in beats since the start of this multitap iteration
	-- elastic = scaling of arrowpath position (1 = perfect bounce from approach speed, 0 = dead stop)
	-- returns distance back up the arrow path to travel
	--
	-- f(t) = v/b * t * (b - t), where v = approach speed
	-- we can get around needing to know the pixelar speed of the arrow by calculating distance
	-- in terms of beats traveled @ whatever the reading speed is.
	-- therefore the approach is always 1 beat/beat >:)
	if not elastic then
		elastic = 1
	end
	return elastic * (b - 2*t) / b
end

local calc_multitap_phase = function(mt_desc, b)
	-- mt_desc = multitap descriptor with the following elements:
	--		lane: which lane the tap is in (unimportant here)
	--		taps: tap beat times included in this multitap
	-- b = current beat
	-- returns a table with the following elements:
	--		rem: # of hits left (the multitap should show a number if it's more than 1)
	--		pos: position in beats before receptors
	--		qtc: quantization of currently approaching note
	--		qtn: quantization of next note in the multitap (used to color the number)
	--		dif: diffuse arrow from 0 (dark) to 1 (full brightness)
	--		vis: currently visible (true/false)
	local ret = {
		rem = 0,
		pos = 0,
		sqh = 0,
		qtc = 0,
		qtn = 0,
		dif = 0,
		vis = false
	}

	if not mt_desc then
		Trace("No multitap descriptor??")
		multitap_error = true
		return ret
	end
	local mt_taps = mt_desc["taps"]
	if not mt_taps then
		Trace("No tap description in multitap??")
		multitap_error = true
		return ret
	end

	if #mt_taps == 0 then
		Trace("An empty multitap is fine I guess")
		return ret
	end

	if b > mt_taps[#mt_taps] then
		-- Already past the last tap! But don't yell about it. Loudmouth
		return ret
	end

	-- Basic case for when we're earlier than the first tap.
	ret.rem = #mt_taps
	ret.pos = mt_taps[1] - b
	ret.sqh = 0
	ret.qtc = calc_qtzn(mt_taps[1])
	ret.qtn = calc_qtzn(mt_taps[2])
	ret.dif = 0
	ret.vis = (ret.pos < multitap_previsible)

	-- Start with the elasticity at the baseline,
	-- or if the "peak" parameter is set for this multitap, substitute it directly.
	local el = (mt_desc["peak"] and mt_taps[2]) and (mt_desc["peak"] / (mt_taps[2] - mt_taps[1])) or multitap_basebounce


	for i = 1,#mt_taps do
		-- Any bounce cases happen here.
		if b <= mt_taps[i] then
			break
		end

		-- Compound the elasticity, or continue to hold the peak constant.
		el = mt_desc["peak"] and (mt_desc["peak"] / (mt_taps[i+1] - mt_taps[i])) or (el * multitap_elasticity)

		-- We're assured to have an i+1 element here because
		-- we've already jumped out when b > mt_taps[#mt_taps].
		ret.rem = #mt_taps - i
		ret.pos = parabolator(mt_taps[i+1] - mt_taps[i], b - mt_taps[i], el)
		ret.sqh = multitap_squishy*(math.abs(parabolator_dt(mt_taps[i+1] - mt_taps[i], b - mt_taps[i], el)) - 0.5)
		ret.qtc = calc_qtzn(mt_taps[i+1])
		ret.qtn = calc_qtzn(mt_taps[i+2])
		ret.dif = i / (#mt_taps-1)
		ret.vis = true
	end

	return ret
end

-- Provide a custom explosion callback message for each player.
-- The false explosions give better visual reinforcement for multitap hits.
for i=1,2 do
	local pn = i
	_G["multitap_note_callback_P"..i] = function(lane, tns, is_bright)
		if multitap_explosions[pn][lane] then
			--Trace("??? do explosion pls")
			multitap_explosions[pn][lane]:propagatecommand("Judgment")
			multitap_explosions[pn][lane]:propagatecommand("Dim")
			multitap_explosions[pn][lane]:propagatecommand(string.sub(tns, 14))
			--TryCommandOnLeaves(multitap_explosions[pn][lane], string.sub(tns, 14), nil, true)
		end
	end
end

local calc_zoom_splines = function(mt_table, pn)
	-- Use non-beat-subtracting offset zoom splines to hide real taps.
	--
	-- Wait, what?
	--
	-- Non-beat-subtracting
	--		Think of the spline as traveling along with the arrows, rather than
	--		staying fixed to the player's viewable section of the chart.
	-- Offset
	--		Instead of overwriting the original arrow path, the spline is
	--		applied as a change to that path. Here we use zooms of 0 or -1,
	--		representing (1 + 0)x = visible or (1 + -1)x = invisible.
	-- Zoom spline
	--		Set a mathematical function that describes the scaling of an arrow
	--		landing on any given beat.


	-- Calculate length of spline needed.
	local splSize = {}
	for mti,mt_desc in ipairs(mt_table) do
		if not splSize[mt_desc.lane] then
			splSize[mt_desc.lane] = 0
		end

		if #mt_desc.taps > 0 then
			if mt_desc.taps[#mt_desc.taps] > splSize[mt_desc.lane] then
				splSize[mt_desc.lane] = mt_desc.taps[#mt_desc.taps]
			end
		end
	end

	-- Convert to 192nds count.
	for i,v in ipairs(splSize) do
		splSize[i] = _BB(v) + 2
	end

	-- Apply the spline points.
	local pp = SCREENMAN:GetTopScreen():GetChild('PlayerP'..pn)
	local nf = pp:GetChild('NoteField')
	local ncr_table = nf:GetColumnActors()

	-- Apply the false explosion callback.
	nf:SetDidTapNoteCallback(_G["multitap_note_callback_P"..pn])

	for lane,ncr in ipairs(ncr_table) do
		-- Allocate spline space and set up the interpretation
		-- (1 point per 192nd note starting at 0, non-beat-subtracting offset)
		splHandle = ncr:GetZoomHandler()
		splHandle:SetSplineMode('NoteColumnSplineMode_Offset')
				 :SetSubtractSongBeat(false)
				 :SetReceptorT(0.0)
				 :SetBeatsPerT(1/48)
		local splObject = splHandle:GetSpline()
		splObject:SetSize(splSize[lane])
		for spli = 1,splSize[lane] do
			splObject:SetPoint(spli, {0, 0, 0})
		end

		-- Set every 192nd note within the multitap region to offset zoom by -1
		-- (i.e., hide the note by zooming it away)
		for mti,mt_desc in ipairs(mt_table) do
			if (mt_desc.lane == lane) and (#mt_desc.taps > 0) then
				for spli=_BB(mt_desc.taps[1]),_BB(mt_desc.taps[#mt_desc.taps]) do
					splObject:SetPoint(spli+1, {-1, -1, -1})
					--Trace("::: "..lane..".("..spli.." of "..splSize[lane]..") or ("..(spli/48)..")")
				end
			end
		end

		-- Calculate and apply spline
		splObject:Solve()
	end
end

local lane_permute = function(pops, l)
	-- Which lane is the desired arrow in?
	-- Account for Left, Right, and Mirror.
	-- Otherwise I honestly don't give heck. You are on your lonesome binch. Tohoku Evolved up in this jawn
	local lanes = {1, 2, 3, 4}

	if pops:Mirror() 	then lanes = {lanes[4], lanes[3], lanes[2], lanes[1]} end
	if pops:Left()		then lanes = {lanes[2], lanes[4], lanes[1], lanes[3]} end
	if pops:Right() 	then lanes = {lanes[3], lanes[1], lanes[4], lanes[2]} end

	return lanes[l]
end
local lane_rotation = {90, 0, 180, 270}					-- Give a tap note actor directions. It lost its GPS and has no concept of "land marks"

local copy_transforms = function(dst, src)
	-- All the
	-- Small things
	-- dst gets
	-- What src brings
	-- Tap, fake, or lift
	-- Transform your shit
	dst:x(src:GetX())
	   :y(src:GetY())
	   :z(src:GetZ())
	   :rotationx(src:GetRotationX())
	   :rotationy(src:GetRotationY())
	   :rotationz(src:GetRotationZ())
--	   :zoom(src:GetZoom())
	   :zoomx(src:GetZoomX())
	   :zoomy(src:GetZoomY())
	   :zoomz(src:GetZoomZ())
end

-- Why is this so inaccessibly hard!!
local GRAY_ARROWS_Y_STANDARD 		= THEME:GetMetric("Player", "ReceptorArrowsYStandard")
local GRAY_ARROWS_Y_REVERSE  		= THEME:GetMetric("Player", "ReceptorArrowsYReverse")
local CENTER_Y_FOR_DILLWEEDS_ONLY	= (GRAY_ARROWS_Y_STANDARD + GRAY_ARROWS_Y_REVERSE) / 2
Trace("### "..CENTER_Y_FOR_DILLWEEDS_ONLY.."x engineers can "..
	  "convert 'thought' into 'piss' in their balls, and issue it in an iterative fashion.")

local __SCALE = function(x, l1, h1, l2, h2)
	return (h2 - l2) * (x - l1) / (h1 - l1) + l2
end

--[[
	See also (in the stepmania source code):

	PlayerNoteFieldPositioner()
	PushPlayerMatrix()
	LoadMenuPerspective(
		fovDegrees=45,
		fWidth=SCREEN_WIDTH,
		fHeight=SCREEN_HEIGHT,
		fVanishPointX=SCALE(skew, 0.1f, 1.0f, x, SCREEN_CENTER_X),
		fVanishPointY=center_y
	)
]]--

local copy_transforms_player = function(dst, pp, skew, tilt, reverse)
	-- I probably ought to refactor this code a bit to reduce the parameter bus
	-- but we all goin to school today!! get your notebooks and noteskins

	skew = skew or 0.0
	tilt = tilt or 0.0
	reverse = reverse or false

	-- you know we could just have a god damn API call for this
	-- "I don't see a use case" yeah because your FOV is only 45 degrees!! owned
	local fov_in = 45
	local vpx_in = __SCALE(skew, 0.1, 1.0, pp:GetX(), SCREEN_CENTER_X)
	local vpy_in = pp:GetY() + CENTER_Y_FOR_DILLWEEDS_ONLY
	--Trace("### ... "..vpx_in..", "..vpy_in)

	local reverse_mult = (reverse and -1 or 1)
	local tilt_degrees = __SCALE(tilt, -1, 1, 30, -30) * reverse_mult
	local zoom_for_dipsticks_only = 0
	local yoff_for_dumbdumbs_only = 0
	if (tilt > 0) then
		zoom_for_dipsticks_only = __SCALE(tilt, 0, 1, 1, 0.9)
		yoff_for_dumbdumbs_only = __SCALE(tilt, 0, 1, 0, -45) * reverse_mult
	else
		zoom_for_dipsticks_only = __SCALE(tilt, 0, -1, 1, 0.9)
		yoff_for_dumbdumbs_only = __SCALE(tilt, 0, -1, 0, -20) * reverse_mult
	end

	-- "The iniquity of the parents on the children, and the children's
	-- children, to the third and the fourth generation." -- Exodus 34:7
	dst:GetParent():x(pp:GetX())
				   :y(pp:GetY())
				   :z(pp:GetZ())
	-- FOV here stands for "fuck off, venerated_stepmania_developers"
	   			   :fov(fov_in)
	   			   :vanishpoint(vpx_in, vpy_in)

	nf = pp:GetChild("NoteField")
	dst:x(nf:GetX())
	   :y(nf:GetY() + yoff_for_dumbdumbs_only)
	   :z(nf:GetZ())
	   :rotationx(nf:GetRotationX())
	   :rotationy(nf:GetRotationY())
	   :rotationz(nf:GetRotationZ())
	   :zoomx(nf:GetZoomX() * zoom_for_dipsticks_only)
	   :zoomy(nf:GetZoomY() * zoom_for_dipsticks_only)
	   :zoomz(nf:GetZoomZ() * zoom_for_dipsticks_only)

	if is_outfox then
		dst:GetParent():zoom(pp:GetZoom())
	end
end

local apply_rotations = function(p, r)
	px = p["x"] and p["x"] or 0
	py = p["y"] and p["y"] or 0
	pz = p["z"] and p["z"] or 0
	rx = r["x"] and r["x"] or 0
	ry = r["y"] and r["y"] or 0
	rz = r["z"] and r["z"] or 0
	sx = math.sin(-rx)
	sy = math.sin(-ry)
	sz = math.sin(-rz)
	cx = math.cos(-rx)
	cy = math.cos(-ry)
	cz = math.cos(-rz)

	-- X Y Z
	if rx ~= 0 then
		ty = py*cx - pz*sx
		tz = py*sx + pz*cx
		py = ty
		pz = tz
	end
	if ry ~= 0 then
		tz = pz*cy - px*sy
		tx = pz*sy + px*cy
		pz = tz
		px = tx
	end
	if rz ~= 0 then
		tx = px*cz - py*sz
		ty = px*sz + py*cz
		px = tx
		py = ty
	end

	return {x = px, y = py, z = pz}
end

local copy_transforms_arrow = function(dst, arrow_only, ps, lane, beat, pos, apply_extra)
	-- Additional translation and rotation are added.
	-- Additional zoom is multiplied.
	-- It's Only Natural

	-- For when the recipe calls for "one clove of garlic" and
	-- you know that isn't right
	apply_extra = apply_extra and apply_extra or {}

	-- Each arrow in its travels acquires a mystical quantity "YOffset",
	-- which dictates its location along the arrow path and when various
	-- arrow effects are applied.
	local y_off = ArrowEffects.GetYOffset(ps, lane, beat + pos) - ArrowEffects.GetYOffset(ps, lane, beat)

	if arrow_only then
		-- Don't rotate the multitap countdown.
		dst:rotationx(ArrowEffects.GetRotationX(ps, y_off, 0, lane) 		+ (apply_extra["rotationx"] and apply_extra["rotationx"] or 0) + MYSTERIOUS_VERSION_DEPENDENT_RADIAN_POLTERGEIST)	-- ??????
		   :rotationy(ArrowEffects.GetRotationY(ps, y_off, lane) 			+ (apply_extra["rotationy"] and apply_extra["rotationy"] or 0))
		   :rotationz(ArrowEffects.GetRotationZ(ps, beat+pos, false, lane) 	+ (apply_extra["rotationz"] and apply_extra["rotationz"] or 0))
--		   :glow(color(1, 1, 1, ArrowEffects.GetGlow(ps, lane, y_off)))		-- TODO: Seems to be always 1? that's weird. I'll fix this when it's actually important
	else
		pos = {
			x = ArrowEffects.GetXPos(ps, lane, y_off),
			y = ArrowEffects.GetYPos(ps, lane, y_off),
			z = ArrowEffects.GetZPos(ps, lane, y_off)
		}
		rot = {
			x = math.rad(apply_extra["rotationx"] and apply_extra["rotationx"] or 0),
			y = math.rad(apply_extra["rotationy"] and apply_extra["rotationy"] or 0),
			z = math.rad(apply_extra["rotationz"] and apply_extra["rotationz"] or 0)
		}
		pos_rotated = apply_rotations(pos, rot)
		dst:x(pos_rotated["x"]		+ (apply_extra["x"] and apply_extra["x"] or 0))
		   :y(pos_rotated["y"] 		+ (apply_extra["y"] and apply_extra["y"] or 0))
		   :z(pos_rotated["z"]		+ (apply_extra["z"] and apply_extra["z"] or 0))
		   :zoomx(ArrowEffects.GetZoom(ps, y_off, lane) 	* (apply_extra["zoomx"] and apply_extra["zoomx"] or 1))
		   :zoomy(ArrowEffects.GetZoom(ps, y_off, lane) 	* (apply_extra["zoomy"] and apply_extra["zoomy"] or 1))
		   :zoomz(ArrowEffects.GetZoom(ps, y_off, lane)		* (apply_extra["zoomz"] and apply_extra["zoomz"] or 1))
		   :diffusealpha(ArrowEffects.GetAlpha(ps, lane, y_off))

	end
end


local multitap_update_function = function()
	local status = 1
--	local status, errmsg = pcall( function() -- begin pcall()
		local beat = GAMESTATE:GetSongBeat()

		for _,pe in pairs(GAMESTATE:GetEnabledPlayers()) do
			local pn = tonumber(string.match(pe, "[0-9]+"))

			local ps 	= GAMESTATE:GetPlayerState('PlayerNumber_P'..pn)
			local pp 	= SCREENMAN:GetTopScreen():GetChild('PlayerP'..pn)
			local pops 	= ps:GetPlayerOptions("ModsLevel_Song")

			-- This is a convenient enough spot to do just-in-time one-time initialization lol
			if not multitap_splines_calc[pn] then
				-- Select the multitap list that matches the current chart slot.
				full_chart_name = GAMESTATE:GetCurrentSteps(pn-1):GetDifficulty()
				multitap_chart_sel[pn] = string.sub(full_chart_name, 12)

				-- Calculate vanishing splines for real taps in multitap regions.
				calc_zoom_splines(multitaps[multitap_chart_sel[pn]], pn)
				multitap_splines_calc[pn] = true
			end

			-- Adjust the multitap fields with the same transforms the players themselves get.
			-- See Player::PlayerNoteFieldPositioner::PlayerNoteFieldPositioner().
			copy_transforms_player(
				multitap_fields[pn],
				pp,
				pops:Skew(),
				pops:Tilt(),
				pops:GetReversePercentForColumn(0) > 0.5		-- ...sure, whatecer
			)

			local extra_tfms_frame = {}
			local extra_tfms_arrow = {}
			if is_outfox then
				-- OutFox honey...what is this
				local nf = pp:GetChild('NoteField')
				local ncr_table = nf:GetColumnActors()
				extra_tfms_frame["y"] = ncr_table[1]:GetY()
				extra_tfms_frame["rotationx"] = ncr_table[1]:GetRotationX()
				extra_tfms_arrow["rotationx"] = ncr_table[1]:GetRotationX()
			end

			-- Read the texture coordinate shift that changes quantization in the noteskin.
			-- Some noteskins are implemented as vertical shifts and some horizontal.
			local tex_color_interval = {
				x = NOTESKIN:GetMetricFForNoteSkin("", "TapNoteNoteColorTextureCoordSpacingX", noteskin_names[pn]),
				y = NOTESKIN:GetMetricFForNoteSkin("", "TapNoteNoteColorTextureCoordSpacingY", noteskin_names[pn]),
			}
			local tex_color_is_rhythm = NOTESKIN:GetMetricBForNoteSkin("", "TapNoteAnimationIsVivid", noteskin_names[pn])
			if multitap_chart_sel[pn] then
				local show_false_explosion = {false, false, false, false}

				for mti,mt_desc in ipairs(multitaps[multitap_chart_sel[pn]]) do
					-- I just wanna know how to present a multitap at this time.
					-- Let someone else do the calculations
					mt_stats = calc_multitap_phase(mt_desc, beat)

					local lperm = lane_permute(pops, mt_desc.lane)		-- Where does this arrow actually land?

					if mt_stats.vis then
						--Trace("??? "..pp:GetChild("NoteField"):GetY())
						--Trace("!!! reproach "..pn..", "..mti.." @ "..beat.." + "..mt_stats.pos.." -> "..y_off.." ("..pos_x..", "..pos_y..", "..pos_z..")")

						-- Show the multitap.
						--		Turn the arrow actor to the right lane direction
						--		Dim the arrow to make the countdown stand out initially
						--		Set the arrow color to the right quantization
						multitap_actors[pn][mti]["frame"]:visible(true)
						multitap_actors[pn][mti]["arrow"]:baserotationz(lane_rotation[lperm])
														 :diffuse(lerp_color(mt_stats.dif, color("#666666"), color("#ffffff")))
														 :texturetranslate(
							tex_color_interval["x"] * qtzn_tex[mt_stats.qtc],
							tex_color_interval["y"] * qtzn_tex[mt_stats.qtc]
							)
						extra_tfms_frame["zoomy"] = 1 + mt_stats.sqh

						-- To make the multitap convincing, spoof the transformation matrices
						-- and color modifiers from the same calculations as a real tap note.
						copy_transforms_arrow(
							multitap_actors[pn][mti]["frame"], false,
							ps,
							lperm,
							beat,
							mt_stats.pos,
							extra_tfms_frame
						)
						copy_transforms_arrow(
							multitap_actors[pn][mti]["arrow"], true,
							ps,
							lperm,
							beat,
							mt_stats.pos,
							extra_tfms_arrow
						)

						-- Be prepared to fire a fake explosion if any multitap in this lane is active.
						show_false_explosion[lperm] = true

						if mt_stats.rem > (not whoTheHellIsEdgar and 1 or 0) then
							-- Show the countdown until this multitap degrades into a regular tap
							-- (i.e., 1 hit left)

							-- Use color tables to coordinate with the noteskin and quantization.
							local noteskin_name = noteskin_names[pn]
							local color_pair = qtzn_color_tables["vivid"][1]
							if whoTheHellIsEdgar then
								noteskin_name = "vivid"		-- Learn the rhythm :hehe:
							end
							color_table = qtzn_color_lookup(noteskin_name)
							if color_table and not tex_color_is_rhythm then
								color_pair = color_table[qtzn_tex[mt_stats.qtn]+1]
							end

							-- Set the text actor up with the right number, and a pulsating
							-- color effect similar to what you'd see on most tap notes.
							multitap_actors[pn][mti]["count"]:visible(true)
															 :zoom(1)
															 :diffuseramp()
															 :effectclock("beat")
															 :effectcolor1(color("#"..color_pair[1]))
															 :effectcolor2(color("#"..color_pair[2]))
							if not whoTheHellIsEdgar then
								multitap_actors[pn][mti]["count"]:settext(mt_stats.rem)
							end
						else
							multitap_actors[pn][mti]["count"]:visible(false)
						end
					else
						multitap_actors[pn][mti]["frame"]:visible(false)
					end
				end

				for lane=1,4 do
					local lperm = lane_permute(pops, lane)		-- Where does this arrow actually land?

					-- Show the spoofed explosion if we need it, and make sure it's
					-- in the right spot.
					local ex_pos_x = ArrowEffects.GetXPos(ps, lperm, 0)
					local ex_pos_y = ArrowEffects.GetYPos(ps, lperm, 0)
					local ex_pos_z = ArrowEffects.GetZPos(ps, lperm, 0)

					-- TODO: I guess I could incorporate individual column zoom
					-- into this too, but no default mods affect that.
					multitap_explosions[pn][lperm]:xy(ex_pos_x, ex_pos_y)
												  :z(ex_pos_z)
												  :baserotationz(lane_rotation[lperm])
												  :visible(show_false_explosion[lperm])
				end
			end
		end

		TEST_last_beat = beat
--	end -- end pcall()
--	)
	if status then
		--Trace('### YAY TELP DID NOT MAKE A FUCKY WUCKY')
	else
		if not multitap_error then
			Trace('### OOPS TELP HAS MADE A FUCKO BOINGO (in update function)')
			Trace('### '..errmsg)
			Trace('### '..debug.traceback())
		end
	end
end


direction_names = {"Left", "Down", "Up", "Right"}
for _,pe in pairs(GAMESTATE:GetEnabledPlayers()) do
	local pn = tonumber(string.match(pe, "[0-9]+"))

	local pops = GAMESTATE:GetPlayerState(pe):GetPlayerOptions("ModsLevel_Song")
	local noteskin_name = string.lower(pops:NoteSkin())
	noteskin_names[pn] = noteskin_name

	-- Build out the bags of holding for all the multitap-related actors
	-- (explosions per lane, frames that hold arrows and countdowns)
	local multitap_prep = Def.ActorFrame {
		Name="MultitapFrameP"..pn,
		InitCommand = function(self)
		end,
		OnCommand = function(self)
			multitap_fields[pn] = self
		end,
	}

	for lane=1,4 do
		-- All noteskins should have a suitable actor that accommodates
		-- explosions of all grades.
		-- If this assumption fails, I wanna know about it :P
		multitap_prep[#multitap_prep+1] = NOTESKIN:LoadActorForNoteSkin("Down", "Explosion", noteskin_name)..{
			Name="MultitapExplosionP"..pn.."_"..lane,
			InitCommand=function(self)
			end,
			OnCommand=function(self)
				multitap_explosions[pn][lane] = self
				self:visible(true)
				--Trace("=== Added multitap actor explosion for P"..pn..", lane "..lane)
			end,
		}
	end

	for mti = 1,multitap_max do
		-- Each multitap is an ActorFrame with two elements:
		-- 		A tap note loaded from the active noteskin
		--		A text actor to show the remaining tap count
		multitap_prep[#multitap_prep+1] = Def.ActorFrame {
			Name="MultitapP"..pn.."_"..mti,
			InitCommand=function(self)
			end,
			OnCommand=function(self)
				local i = mti

				multitap_actors[pn][i]["frame"] = self
				self:visible(false)

				--Trace("=== Added multitap actor frame for P"..pn..", index "..i)
			end,
			NOTESKIN:LoadActorForNoteSkin("Down", "Tap Note", noteskin_name)..{
				Name="MultitapArrowP"..pn.."_"..mti,
				InitCommand=function(self)
				end,
				OnCommand=function(self)
					local i = mti

					multitap_actors[pn][i]["arrow"] = self
					self:visible(true)
					--Trace("=== Added multitap actor arrow for P"..pn..", index "..i)
				end,
			},
		}
		mt_prep = multitap_prep[#multitap_prep]
		if whoTheHellIsEdgar then
			multitap_prep[#multitap_prep][#mt_prep+1] = Def.Sprite {
				-- Poe, Poe,
				-- Poe, Poe, Poe,
				-- Poe, Poe,
				-- Poe, Poe, Poe,
				-- Poe, Poe,
				-- Poe, Poe, Poe,
				-- Edgar Allan,
				-- Edgar Allan,
				Name="MultitapTextP"..pn.."_"..mti,
				Texture="Poe.png",
				InitCommand=function(self)
				end,
				OnCommand=function(self)
					local i = mti

					multitap_actors[pn][i]["count"] = self
					self:visible(false)
						:z(is_outfox and 1.0 or 10.0)	-- Ensure depth z-testing
														-- (even though SM5 defaults to init order because That`s Great!)
					--Trace("=== Added multitap actor count for P"..pn..", index "..i)
				end,
			}
		else
			multitap_prep[#multitap_prep][#mt_prep+1] = Def.BitmapText {
				-- You can switch the font out, but I recommend:
				-- *	36-48px (42px is ideal; keep in mind arrows are 64px)
				-- *	Generate it with 10px+ padding and add a nice bold
				--		border in post, at least 4px, to distinguish it well
				--		from the underlying arrow
				Name="MultitapTextP"..pn.."_"..mti,
				Font="_komika axis 42px.ini",
				Text="",
				InitCommand=function(self)
				end,
				OnCommand=function(self)
					local i = mti

					multitap_actors[pn][i]["count"] = self
					self:visible(false)
						:z(is_outfox and 1.0 or 10.0)	-- Ensure depth z-testing
														-- (even though SM5 defaults to init order because That`s Great!)
						:strokecolor(color("#000000"))
					--Trace("=== Added multitap actor count for P"..pn..", index "..i)
				end,
			}
		end
	end

	-- The multitap bags of holding need one level of parent to handle
	-- FOV and positioning in the most accurate way.
	-- I probably could have used a wrapper state here...
	multitap_parent[#multitap_parent+1] = Def.ActorFrame{multitap_prep}
end

multitap_parent[#multitap_parent+1] = Def.ActorFrame {
	Name="Update",
	InitCommand=function(self)
		Trace("### im alive")

		-- Do all of the it
		self:SetUpdateFunction(multitap_update_function)
	end,

	Def.ActorFrame {
		InitCommand = function(self)
			self:sleep(69420)
		end
	}
}

-- Done!
return multitap_parent
