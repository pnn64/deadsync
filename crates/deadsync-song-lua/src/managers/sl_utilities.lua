------------------------------------------------------------
-- 06 SL-Utilities.lua
-- Utility Functions for Development
--
-- The filename starts with "06" so that it loads before other SL scripts that rely on
-- global functions defined here.  For more information on this numbering system that
-- pretty much no one uses, see: ./Themes/_fallback/Scripts/hierarchy.txt

------------------------------------------------------------
-- define helper functions local to this file first
-- global utility functions (below) will depend on these
------------------------------------------------------------

-- TableToString() function via:
-- http://www.hpelbers.org/lua/print_r
-- Copyright 2009: hans@hpelbers.org
TableToString = function(t, name, indent)
	local tableList = {}
	local table_r

	table_r = function(t, name, indent, full)
		local id = not full and name or type(name)~="number" and tostring(name) or '['..name..']'
		local tag = indent .. id .. ' = '
		local out = {}	-- result

		if type(t) == "table" then
			if tableList[t] ~= nil then
				table.insert(out, tag .. '{} -- ' .. tableList[t] .. ' (self reference)')
			else
				tableList[t]= full and (full .. '.' .. id) or id
				if next(t) then -- Table not empty
					table.insert(out, tag .. '{')
					for key,value in pairs(t) do
						table.insert(out,table_r(value,key,indent .. '|    ',tableList[t]))
					end
					table.insert(out,indent .. '}')
				else
					table.insert(out,tag .. '{}')
				end
			end
		else
			local val = type(t)~="number" and type(t)~="boolean" and '"'..tostring(t)..'"' or tostring(t)
			table.insert(out, tag .. val)
		end

		return table.concat(out, '\n')
	end

	return table_r(t,name or 'Value',indent or '')
end


------------------------------------------------------------
-- GLOBAL UTILITY FUNCTIONS
-- use these to assist in theming/scripting efforts
------------------------------------------------------------

-- SM()
-- Shorthand for SCREENMAN:SystemMessage(), this is useful for rapid iterative
-- testing by allowing us to pretty-print tables and variables to the screen.
--
-- If the first argument is a table, SM() will use TableToString (from above)
-- to display children recursively.  Larger tables will spill offscreen, so
-- rec_print_table() from the _fallback theme is good to know about and use when
-- debugging.  That will recursively pretty-print table structures to ./Logs/Log.txt
--
-- The second arugment is optional and allows you to provide a specific duration,
-- in seconds, for how long you want the text to appear on screen.
-- in Simply Love, the default SystemMessage duration used in ./BGA/ScreenSystemLayer overlay.lua is 3

SM = function( arg, duration, stack )
	local msg

	-- if a table has been passed in, recursively stringify the table's keys and values
	if type( arg ) == "table" then
		msg = TableToString(arg)

	-- otherwise, Lua's standard tostring() should suffice
	else
		msg = tostring(arg)
	end

	-- SCREENMAN:SystemMessage() is effectively a convenience function for broadcasting
	-- "SystemMessage" with certain parameters.  see: ScreenManager.cpp
	-- let's broadcast directly using MESSAGEMAN so that we can also pack in a duration
	-- value (how long to display the SystemMessage for) if so desired
	MESSAGEMAN:Broadcast("SystemMessage", {Message=msg, Duration=duration, Stack=stack})
	Trace(msg)
end