-- GCRA Refund — 配额预扣的退还实现
--
-- 用于 Reservation::commit(actual) 和 Reservation::release()，
-- 把 TAT 往回拨 cost*emission 毫秒，相当于"归还"已经扣除的 token。
--
-- 注意：refund 后 TAT 不会低于 now_ms（防止"未来桶"的诡异状态）。
--
-- **要求 Redis ≥ 6.0**（依赖 SET ... KEEPTTL 选项）。
-- 在更老的 Redis 上脚本会报 syntax error，调用方走 backend_failures 计数 +
-- failure_policy 兜底。生产部署前请确认 Redis 版本（`INFO server`）。
--
-- KEYS[1]:  state key
-- ARGV[1]:  now_ms
-- ARGV[2]:  emission_interval_ms
-- ARGV[3]:  cost (退还单位数)
--
-- 返回: 退还后的新 TAT (单值, i64)

local now_ms = tonumber(ARGV[1])
local emission = tonumber(ARGV[2])
local cost = tonumber(ARGV[3])

local tat = tonumber(redis.call("GET", KEYS[1]))
if not tat then
    -- 没有状态可退；幂等返回 now_ms
    return now_ms
end

local new_tat = tat - cost * emission
if new_tat < now_ms then
    new_tat = now_ms
end

redis.call("SET", KEYS[1], new_tat, "KEEPTTL")
return new_tat
