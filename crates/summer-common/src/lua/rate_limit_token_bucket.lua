local now_ms = tonumber(ARGV[1])
local refill_window_ms = tonumber(ARGV[2])
local refill_rate = tonumber(ARGV[3])
local capacity = tonumber(ARGV[4])
local expire_seconds = tonumber(ARGV[5])

local bucket = redis.call("HMGET", KEYS[1], "tokens", "last_refill_ms")
local tokens = tonumber(bucket[1])
local last_refill_ms = tonumber(bucket[2])

if not tokens or not last_refill_ms then
    tokens = capacity
    last_refill_ms = now_ms
else
    local elapsed_ms = math.max(0, now_ms - last_refill_ms)
    local refill = (elapsed_ms * refill_rate) / refill_window_ms
    tokens = math.min(capacity, tokens + refill)
    last_refill_ms = now_ms
end

local allowed = 0
if tokens >= 1 then
    tokens = tokens - 1
    allowed = 1
end

redis.call("HSET", KEYS[1], "tokens", tokens, "last_refill_ms", last_refill_ms)
redis.call("EXPIRE", KEYS[1], expire_seconds)

return allowed
