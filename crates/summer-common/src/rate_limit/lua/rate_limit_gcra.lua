-- GCRA (Generic Cell Rate Algorithm) — 顶级限流核心实现
--
-- 业界标准（Cloudflare / Stripe / Envoy / governor 内核）。
-- 单变量状态：只存 TAT（Theoretical Arrival Time），原生支持 burst，纯整数算术，无浮点漂移。
--
-- 取代浮点 token bucket。同时承担三种语义：
--   - GCRA 标准:    burst >= 1
--   - TokenBucket:  burst = 配置 burst（默认 = rate）
--   - LeakyBucket:  burst = 1（无突发容量）
--
-- 同时支持 **Token Cost-Based 限流**：通过 ARGV[5] 传入本次请求消耗的 cost 单位（默认 1），
-- 实现"按消耗计费"的限流（典型场景：LLM TPM, 文件上传按字节, daily cost 按金额）。
--
-- KEYS[1]:  state key
-- ARGV[1]:  now_ms (i64)
-- ARGV[2]:  emission_interval_ms (i64, > 0) 每单位理论间隔 = ceil(window_ms / capacity)
-- ARGV[3]:  burst (i64, >= 1) 桶容量（单位数）
-- ARGV[4]:  ttl_seconds (i64, > 0)
-- ARGV[5]:  cost (i64, >= 1, 默认 1) 本次请求消耗的单位数
--
-- 返回: { allowed, value_ms, remaining }
--   allowed=1: value_ms=0
--   allowed=0: value_ms=retry_after_ms
--   remaining: 桶剩余可用单位数

local now_ms = tonumber(ARGV[1])
local emission = tonumber(ARGV[2])
local burst = tonumber(ARGV[3])
local ttl_seconds = tonumber(ARGV[4])
local cost = tonumber(ARGV[5]) or 1
if cost < 1 then cost = 1 end

local capacity = burst * emission
local cost_emission = cost * emission

local tat = tonumber(redis.call("GET", KEYS[1]))
if not tat then
    tat = now_ms
end

local arrival = tat
if arrival < now_ms then
    arrival = now_ms
end

local diff = arrival - now_ms

-- 通用 GCRA 公式（cost 个单位的请求 = 推进 TAT 由 cost*emission）：
-- 拒绝条件：推进后桶超容（diff + cost_emission > capacity）
if diff + cost_emission > capacity then
    local retry_after_ms = diff + cost_emission - capacity
    if retry_after_ms < 0 then retry_after_ms = 0 end
    return { 0, retry_after_ms, 0 }
end

-- 通过：推进 TAT
local new_tat = arrival + cost_emission
redis.call("SET", KEYS[1], new_tat, "EX", ttl_seconds)

-- 剩余 = (capacity - new_diff) / emission, 其中 new_diff = diff + cost_emission
local remaining = math.floor((capacity - diff - cost_emission) / emission)
if remaining < 0 then
    remaining = 0
end

return { 1, 0, remaining }
