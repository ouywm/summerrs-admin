-- 滑动窗口日志（Sliding Window Log）
--
-- ZSET 存请求时间戳，每次请求先清理过期、再判额度。
-- 比 Fixed Window 更平滑，比 GCRA 占用更多内存（O(rate) 元素），用于精确控制。
--
-- KEYS[1]:  zset key
-- ARGV[1]:  now_ms
-- ARGV[2]:  window_ms
-- ARGV[3]:  limit
-- ARGV[4]:  member（唯一标识：now_ms:uuid，避免 ZADD 去重）
-- ARGV[5]:  ttl_seconds
--
-- 返回: { allowed, value_ms, remaining }
--   allowed=0 时 value_ms=retry_after_ms（最老一条 + window - now，即等到最老一条出窗）

local now_ms = tonumber(ARGV[1])
local window_ms = tonumber(ARGV[2])
local limit = tonumber(ARGV[3])
local member = ARGV[4]
local ttl_seconds = tonumber(ARGV[5])

local cutoff = now_ms - window_ms
redis.call("ZREMRANGEBYSCORE", KEYS[1], "-inf", cutoff)

local current = redis.call("ZCARD", KEYS[1])

if current >= limit then
    -- 找最老一条计算 retry_after：oldest.score + window - now
    local oldest = redis.call("ZRANGE", KEYS[1], 0, 0, "WITHSCORES")
    local retry_after_ms = window_ms
    if #oldest >= 2 then
        retry_after_ms = tonumber(oldest[2]) + window_ms - now_ms
        if retry_after_ms < 0 then
            retry_after_ms = 0
        end
    end
    redis.call("PEXPIRE", KEYS[1], ttl_seconds * 1000)
    return { 0, retry_after_ms, 0 }
end

redis.call("ZADD", KEYS[1], now_ms, member)
redis.call("PEXPIRE", KEYS[1], ttl_seconds * 1000)

return { 1, 0, limit - current - 1 }
