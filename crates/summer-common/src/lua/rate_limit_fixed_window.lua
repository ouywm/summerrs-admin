local expire_seconds = tonumber(ARGV[1])
local limit = tonumber(ARGV[2])

local current = redis.call("INCR", KEYS[1])
if current == 1 then
    redis.call("EXPIRE", KEYS[1], expire_seconds)
end

if current > limit then
    return 0
end

return 1
