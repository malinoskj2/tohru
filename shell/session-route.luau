-- The session menu has a separate lifetime; this root never starts that role.
on("session_toggle", function()
    local ok, failure=pcall(sys.call,"say","tohru-session","emit menu_toggle")
    text.routing_error=ok and "" or tostring(failure)
end)
return true
