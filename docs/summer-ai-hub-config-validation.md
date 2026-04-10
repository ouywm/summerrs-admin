# summer-ai-hub Config Validation

更新时间：2026-03-25

本文档记录 `Step 4` 阶段用 MCP 对当前 AI 配置面的实际检查结果。

> 2026-03-26 追加说明：代码侧已补 `channel.endpoint_scopes`、
> `model_config.supported_endpoints`、`token.endpoint_scopes` 的白名单校验与规范化；
> Anthropic / Gemini 这类 chat-only provider 也会在渠道保存时直接拒绝不支持的 scope，
> 不再静默过滤。

## 1. 检查范围

本次重点校验：

1. `ai.ability` 是否覆盖新增 `endpoint_scope`
2. `ai.channel.endpoint_scopes` 是否覆盖新增 HTTP 接口面
3. `ai.model_config` 是否存在用于测试的新模型

## 2. MCP 查询结果

### ability 总量

- `ai.ability` 总记录数：`8`
- `endpoint_scope` 种类数：`2`

### 当前 ability scope

| endpoint_scope | count |
|---|---:|
| `chat` | 4 |
| `responses` | 4 |

### 当前 ability 明细

当前只看到一个分组：

- `channel_group`: `default`

当前只覆盖 4 个模型别名，且每个模型仅覆盖：

1. `chat`
2. `responses`

对应关系如下：

| channel_group | model | endpoint_scope | channel_id |
|---|---|---|---:|
| `default` | `gpt-5.4` | `chat` | `910001` |
| `default` | `gpt-5.4` | `responses` | `910001` |
| `default` | `gpt5.4` | `chat` | `910001` |
| `default` | `gpt5.4` | `responses` | `910001` |
| `default` | `gpt-5.4 xhigh` | `chat` | `910001` |
| `default` | `gpt-5.4 xhigh` | `responses` | `910001` |
| `default` | `gpt5.4 xhigh` | `chat` | `910001` |
| `default` | `gpt5.4 xhigh` | `responses` | `910001` |

### 当前 model_config

| model_name | enabled |
|---|---|
| `gpt-5.4` | `true` |
| `gpt5.4` | `true` |
| `gpt-5.4 xhigh` | `true` |
| `gpt5.4 xhigh` | `true` |

### 当前 channel scope

仅看到一个演示渠道：

| id | name | channel_type | status | endpoint_scopes |
|---|---|---:|---:|---|
| `910001` | `codex-demo-channel` | `1` | `1` | `["chat","responses"]` |

### 当前 channel account

仅看到一个可调度账号：

| id | channel_id | name | status | schedulable |
|---|---:|---|---:|---|
| `910001` | `910001` | `codex-demo-account` | `1` | `true` |

## 3. 结论

当前数据库配置仍然停留在“最早主链路”阶段。

这意味着虽然代码里已经补齐了以下接口：

- `completions`
- `images/*`
- `audio/*`
- `moderations`
- `rerank`
- `files*`
- `assistants*`
- `threads*`
- `vector_stores*`
- `fine_tuning/jobs*`
- `uploads*`

但在现有配置下，这些接口大概率会因为以下原因失败：

1. `ability.endpoint_scope` 没有对应 scope，导致选路失败
2. `channel.endpoint_scopes` 没有放开对应 scope，导致渠道层面不可调度
3. `model_config` 没有对应模型，导致计费配置缺失
4. 上游实际未必支持这些 OpenAI 接口

## 4. 当前阶段判断

因此，`Step 4` 的重点不是“继续加接口”，而是：

1. 按准备测试的上游能力补齐 scope 配置
2. 为真实要测的模型补齐 `model_config`
3. 再按 [summer-ai-hub-curl-regression.md](summer-ai-hub-curl-regression.md) 做端到端验证
4. 让管理侧配置和 `/v1/models` 的可见面保持一致，避免只反映 `chat` 子集

## 5. 建议的最小配置补齐顺序

如果你下一步要做真实打通，建议按这个顺序补配置：

1. `completions`
2. `files`
3. `assistants`
4. `threads`
5. `uploads`

原因：

- 这些接口最容易组成一条“资源创建 -> 资源读取 -> 资源继续使用”的完整回归链
- 也最能验证我们这次新增的 `ResourceAffinityService`

## 6. MCP 查询 SQL

```sql
select count(*) as ability_count, count(distinct endpoint_scope) as endpoint_scope_count
from ai.ability;

select endpoint_scope, count(*) as cnt
from ai.ability
group by endpoint_scope
order by endpoint_scope;

select model_name, enabled
from ai.model_config
order by model_name
limit 50;

select id, name, channel_type, status, endpoint_scopes
from ai.channel
order by id
limit 50;

select channel_group, model, endpoint_scope, channel_id
from ai.ability
order by channel_group, model, endpoint_scope, channel_id
limit 50;

select id, channel_id, name, status, schedulable
from ai.channel_account
order by id
limit 50;
```
