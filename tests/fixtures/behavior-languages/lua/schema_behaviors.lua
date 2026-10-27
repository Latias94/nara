local function number(input, field)
    local value = input.fields[field]
    if type(value) ~= "table" or value.type ~= "f64" or type(value.value) ~= "number" then
        error("expected f64 field '" .. field .. "'")
    end
    return value.value
end

local function text(input, field)
    local value = input.fields[field]
    if type(value) ~= "table" or value.type ~= "string" or type(value.value) ~= "string" then
        error("expected string field '" .. field .. "'")
    end
    return value.value
end

local callbacks = {
    ["trial.behavior.Mover"] = function(input)
        local amount = number(input, "amount")
        local gain = 1
        if input.fields.gain ~= nil then
            gain = number(input, "gain")
        end
        return {
            {
                target = input.actor,
                value = "Moved " .. string.format("%.17g", amount * gain),
            },
        }
    end,
    ["trial.behavior.Sensor"] = function(input)
        local commands = {}
        for _, target in ipairs(input.targets) do
            commands[#commands + 1] = {
                target = target.id,
                value = target.name .. " signalled",
            }
        end
        return commands
    end,
    ["trial.behavior.Label"] = function(input)
        return { { target = input.actor, value = text(input, "text") } }
    end,
}

local bindings = {}
for schema in pairs(callbacks) do
    bindings[#bindings + 1] = { schema = schema, version = 1 }
end
table.sort(bindings, function(left, right)
    return left.schema < right.schema
end)

return {
    bindings = bindings,
    invoke = function(input)
        if input.fault == "throw" then
            error("trial callback failure")
        end
        local callback = callbacks[input.schema]
        if callback == nil then
            error("unknown schema '" .. input.schema .. "'")
        end
        local commands = callback(input)
        if input.fault == "bad_target" then
            commands[#commands + 1] = { target = "unavailable", value = "bad" }
        end
        return commands
    end,
}
