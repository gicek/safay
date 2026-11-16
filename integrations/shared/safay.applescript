-- Safay's launcher bridge, shared by the Alfred workflow and the Raycast / Tinycast extension.
-- Arguments: "find" <query>, or "copy" <account id>. Run with `osascript`, the arguments as argv: never put into the
-- script's text.
--
-- The commands go by their raw codes (Safay.sdef: SaFyfacc, SaFyccid), which need no terminology: a Mac without Safay
-- never fails to compile this. `running` never launches Safay, so a search does not start it on every keystroke.
-- The release build goes first, then the development one.
on run argv
	set command to item 1 of argv
	set argument to item 2 of argv
	repeat with appId in {"com.gicek.safay", "com.gicek.safay.debug"}
		set appId to contents of appId
		set isRunning to false
		try
			set isRunning to (application id appId is running)
		end try
		if isRunning then
			-- Copying waits for Touch ID or the master password.
			with timeout of 300 seconds
				tell application id appId
					if command is "find" then
						return «event SaFyfacc» argument
					else
						return «event SaFyccid» argument
					end if
				end tell
			end timeout
		end if
	end repeat
	return "not_running"
end run
