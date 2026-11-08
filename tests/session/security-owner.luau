local ok, err = pcall(function() return sys.ask("auth.unlock", "not-a-real-secret") end)
owner_error = ok and "unexpected success" or tostring(err)
