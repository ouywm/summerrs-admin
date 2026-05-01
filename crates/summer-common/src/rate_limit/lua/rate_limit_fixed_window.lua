-- 固定窗口计数器（Fixed Window Counter）
--
-- 修复了之前版本的两个 bug：
-- 1. 之前的脚本用单一 key + EXPIRE 让窗口过期，但 redis_key 不带 window_id，
--    导致 key 实际存活 2*window 而 INCR 只在第一次设 EXPIRE，"窗口起点"等于
--    "首个请求落地的时刻"而不是按自然时间对齐 → 跨窗口计数串号。
-- 2. memory 端是按 window_id 对齐的，redis 端不是 → 行为不一致。
--
-- 本版本：脚本接收 now_ms + window_ms，自己计算 window_id 并拼到 key 末尾，
-- 让窗口按自然时间边界滚动；EXPIRE 设为 2*window，到期自动回收旧窗口。
--
-- KEYS[1]:  base key（不含 window_id）
-- ARGV[1]:  now_ms
-- ARGV[2]:  window_ms
-- ARGV[3]:  limit
--
-- 返回: { allowed, value_ms, remaining }
--   allowed=1: value_ms=0
--   allowed=0: value_ms=retry_after_ms（到下一窗口起点的距离）

local now_ms = tonumber(ARGV[1])
local window_ms = tonumber(ARGV[2])
local limit = tonumber(ARGV[3])

local window_id = math.floor(now_ms / window_ms)
local key = KEYS[1] .. ":" .. window_id

local current = redis.call("INCR", key)
if current == 1 then
    -- 仅在首次创建时设 TTL；INCR 不会刷新 TTL，让 key 在窗口结束 +1 个周期内自动回收。
    redis.call("PEXPIRE", key, window_ms * 2)
end

local window_end_ms = (window_id + 1) * window_ms
local reset_after_ms = window_end_ms - now_ms

if current > limit then
    return { 0, reset_after_ms, 0 }
end

return { 1, 0, limit - current }
