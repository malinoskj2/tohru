-- Production fixed route with a native request boundary fixture; no role starts.
local handler,calls
text={}
on=function(name,fn)assert(name=='session_toggle');handler=fn end
sys={call=function(...)calls=table.pack(...)end}
assert(dofile('shell/session-route.luau'))
handler()
assert(calls[1]=='say' and calls[2]=='tohru-session' and calls[3]=='emit menu_toggle' and calls.n==3)
assert(text.routing_error=='')
sys.call=function()error("native scene 'tohru-session' is not running")end
handler();assert(text.routing_error:find('not running'))
sys.call=function(...)calls=table.pack(...)end
handler();assert(text.routing_error=='')
print('fixed session route acceptance/missing-instance/cleared error: PASS')
