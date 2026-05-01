-- 调度时槽（Scheduled Slot）
--
-- 单变量 state：next_available_ms。每次请求把"下一个可用时刻"往前推 interval_ms。
-- 同时承担两种语义：
--   - LeakyBucket:    max_wait_ms = 0（任何延迟都拒绝）
--   - ThrottleQueue:  max_wait_ms > 0（在 max_wait 内排队等待，超出则拒绝）
--
-- KEYS[1]:  state key
-- ARGV[1]:  now_ms
-- ARGV[2]:  interval_ms（每请求间隔 = ceil(window_ms / rate)）
-- ARGV[3]:  max_wait_ms（0 = 立即模式 / leaky bucket）
-- ARGV[4]:  ttl_seconds
--
-- 返回: { allowed, value_ms, remaining }
--   allowed=1: value_ms = delay_ms（>=0），调用方需 sleep 后再放行
--   allowed=0: value_ms = retry_after_ms

local now_ms = tonumber(ARGV[1])
local interval_ms = tonumber(ARGV[2])
local max_wait_ms = tonumber(ARGV[3])
local ttl_seconds = tonumber(ARGV[4])

local next_available = tonumber(redis.call("GET", KEYS[1]))
if not next_available then
    next_available = now_ms
end

local scheduled = next_available
if scheduled < now_ms then
    scheduled = now_ms
end

local delay_ms = scheduled - now_ms

if delay_ms > max_wait_ms then
    return { 0, delay_ms, 0 }
end

redis.call("SET", KEYS[1], scheduled + interval_ms, "EX", ttl_seconds)
return { 1, delay_ms, 0 }
