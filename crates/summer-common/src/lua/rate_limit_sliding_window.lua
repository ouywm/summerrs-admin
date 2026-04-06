local now_ms = tonumber(ARGV[1])
local window_ms = tonumber(ARGV[2])
local limit = tonumber(ARGV[3])
local member = ARGV[4]
local expire_seconds = tonumber(ARGV[5])

redis.call("ZREMRANGEBYSCORE", KEYS[1], "-inf", now_ms - window_ms)

local current = redis.call("ZCARD", KEYS[1])
if current >= limit then
    redis.call("EXPIRE", KEYS[1], expire_seconds)
    return 0
end

redis.call("ZADD", KEYS[1], now_ms, member)
redis.call("EXPIRE", KEYS[1], expire_seconds)
return 1
