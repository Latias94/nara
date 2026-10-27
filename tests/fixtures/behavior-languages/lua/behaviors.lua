return function(input)
    if input.fault == "throw" then
        error("trial callback failure")
    end
    local commands = {}
    if input.event == "tick" then
        commands[1] = {
            target = input.actor,
            value = "Moved " .. string.format("%.17g", input.amount),
        }
    elseif input.event == "signal" then
        for _, target in ipairs(input.targets) do
            commands[#commands + 1] = {
                target = target.id,
                value = target.name .. " signalled",
            }
        end
    else
        error("unknown event")
    end
    if input.fault == "bad_target" then
        commands[#commands + 1] = { target = "unavailable", value = "bad" }
    end
    return commands
end
