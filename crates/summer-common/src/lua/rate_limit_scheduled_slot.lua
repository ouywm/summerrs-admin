local now_ms = tonumber(ARGV[1])
local interval_ms = tonumber(ARGV[2])
local max_wait_ms = tonumber(ARGV[3])
local expire_seconds = tonumber(ARGV[4])

local next_available_ms = tonumber(redis.call("GET", KEYS[1]))
if not next_available_ms then
    next_available_ms = now_ms
end

local scheduled_at_ms = math.max(now_ms, next_available_ms)
local delay_ms = scheduled_at_ms - now_ms

if delay_ms > max_wait_ms then
    return -1
end

local new_next_available_ms = scheduled_at_ms + interval_ms
redis.call("SET", KEYS[1], new_next_available_ms, "EX", expire_seconds)

return delay_ms
