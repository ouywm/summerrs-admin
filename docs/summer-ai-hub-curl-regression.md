# summer-ai-hub Curl Regression

更新时间：2026-03-25

本文档用于 `Step 4` 回归验证，默认网关地址为本地开发环境。

## 0. 真实上游联调备注

更新时间：2026-03-26

- 某些 `new-api` 风格的 Claude 上游会同时暴露 OpenAI 风格 `/v1/models` 与 Anthropic 风格 `/v1/messages`，但不代表 `messages` 真的可用。
- 本轮联调中，目标上游对 `GET /v1/models` 返回 `200`，但对标准 Anthropic `POST /v1/messages` 在流式和非流式下都返回：
  - `500`
  - `error.type = "new_api_error"`
  - `error.message = "invalid claude code request (...request id...)"`
- 同一上游对 `POST /v1/chat/completions` + Claude 模型则返回 `404`。
- 因此当前对 Anthropic 实盘联调的结论是：
  - 先把这类错误形状纳入 provider/hub 回归，确保错误归一化稳定
  - 不把该上游当作“标准 Anthropic Messages 可用”的正向样板

## 1. 环境变量

```bash
export HUB_BASE_URL="http://127.0.0.1:8080/api"
export HUB_API_KEY="sk-your-token"
export CHAT_MODEL="gpt-5.4"
export IMAGE_MODEL="gpt-image-1"
export MODERATION_MODEL="omni-moderation-latest"
export REQUEST_ID="step4-$(date +%s)"
```

## 1.1 后台渠道测速

支持按 probeable scope 显式测速：

```bash
curl -i -X POST "$HUB_BASE_URL/ai/channel/<channel_id>/test?endpointScope=chat"
curl -i -X POST "$HUB_BASE_URL/ai/channel/<channel_id>/test?endpointScope=responses"
curl -i -X POST "$HUB_BASE_URL/ai/channel/<channel_id>/test?endpointScope=embeddings"
```

## 2. 已稳定主链路

### models

```bash
curl -i "$HUB_BASE_URL/v1/models" \
  -H "Authorization: Bearer $HUB_API_KEY" \
  -H "x-request-id: $REQUEST_ID-models"
```

### model detail

```bash
curl -i "$HUB_BASE_URL/v1/models/$CHAT_MODEL" \
  -H "Authorization: Bearer $HUB_API_KEY" \
  -H "x-request-id: $REQUEST_ID-model-detail"
```

### chat completions

```bash
curl -i "$HUB_BASE_URL/v1/chat/completions" \
  -H "Authorization: Bearer $HUB_API_KEY" \
  -H "Content-Type: application/json" \
  -H "x-request-id: $REQUEST_ID-chat" \
  -d '{
    "model": "'"$CHAT_MODEL"'",
    "messages": [{"role":"user","content":"say hello in one sentence"}],
    "stream": false
  }'
```

### responses

```bash
curl -i "$HUB_BASE_URL/v1/responses" \
  -H "Authorization: Bearer $HUB_API_KEY" \
  -H "Content-Type: application/json" \
  -H "x-request-id: $REQUEST_ID-responses" \
  -d '{
    "model": "'"$CHAT_MODEL"'",
    "input": "say hello in one sentence",
    "stream": false
  }'
```

### embeddings

```bash
curl -i "$HUB_BASE_URL/v1/embeddings" \
  -H "Authorization: Bearer $HUB_API_KEY" \
  -H "Content-Type: application/json" \
  -H "x-request-id: $REQUEST_ID-embeddings" \
  -d '{
    "model": "'"$CHAT_MODEL"'",
    "input": "hello embeddings"
  }'
```

## 3. 新增模型型接口

### completions

```bash
curl -i "$HUB_BASE_URL/v1/completions" \
  -H "Authorization: Bearer $HUB_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "'"$CHAT_MODEL"'",
    "prompt": "Write one short greeting.",
    "max_tokens": 32
  }'
```

### image generations

```bash
curl -i "$HUB_BASE_URL/v1/images/generations" \
  -H "Authorization: Bearer $HUB_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "'"$IMAGE_MODEL"'",
    "prompt": "A minimalist ink painting of bamboo."
  }'
```

### moderations

```bash
curl -i "$HUB_BASE_URL/v1/moderations" \
  -H "Authorization: Bearer $HUB_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "'"$MODERATION_MODEL"'",
    "input": "hello"
  }'
```

### rerank

```bash
curl -i "$HUB_BASE_URL/v1/rerank" \
  -H "Authorization: Bearer $HUB_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "'"$CHAT_MODEL"'",
    "query": "rust web framework",
    "documents": ["axum", "actix-web", "SeaORM"]
  }'
```

### audio speech

```bash
curl -i "$HUB_BASE_URL/v1/audio/speech" \
  -H "Authorization: Bearer $HUB_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "'"$CHAT_MODEL"'",
    "input": "hello from summer ai hub",
    "voice": "alloy"
  }'
```

## 4. Multipart 接口

```bash
printf 'hello from summer ai hub\n' > /tmp/summer-ai-hub-sample.txt
```

### files create

```bash
curl -i "$HUB_BASE_URL/v1/files" \
  -H "Authorization: Bearer $HUB_API_KEY" \
  -F "purpose=assistants" \
  -F "file=@/tmp/summer-ai-hub-sample.txt;type=text/plain"
```

### image edits

```bash
curl -i "$HUB_BASE_URL/v1/images/edits" \
  -H "Authorization: Bearer $HUB_API_KEY" \
  -F "model=$IMAGE_MODEL" \
  -F "prompt=Add a red sun to the corner." \
  -F "image=@/tmp/summer-ai-hub-sample.txt;type=text/plain"
```

### audio transcriptions

```bash
curl -i "$HUB_BASE_URL/v1/audio/transcriptions" \
  -H "Authorization: Bearer $HUB_API_KEY" \
  -F "model=$CHAT_MODEL" \
  -F "file=@/tmp/summer-ai-hub-sample.txt;type=text/plain"
```

## 5. 资源型接口链路样例

推荐优先跑下面两条最短链：

### 最短链 A：responses 资源回归

1. `POST /v1/responses`
2. 从响应体取出 `response_id = body.id`
3. `GET /v1/responses/{response_id}`
4. 可选：`GET /v1/responses/{response_id}/input_items`
5. 可选：`POST /v1/responses/{response_id}/cancel`

### 最短链 B：assistant/thread/run 资源回归

1. `POST /v1/assistants` -> `assistant_id = body.id`
2. `POST /v1/threads` -> `thread_id = body.id`
3. `POST /v1/threads/{thread_id}/runs` -> `run_id = body.id`
4. `GET /v1/threads/{thread_id}/runs/{run_id}`

### responses create

```bash
curl -i "$HUB_BASE_URL/v1/responses" \
  -H "Authorization: Bearer $HUB_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "'"$CHAT_MODEL"'",
    "input": "hello",
    "stream": false
  }'
```

### responses get

```bash
curl -i "$HUB_BASE_URL/v1/responses/<response_id>" \
  -H "Authorization: Bearer $HUB_API_KEY"
```

### responses input items

```bash
curl -i "$HUB_BASE_URL/v1/responses/<response_id>/input_items" \
  -H "Authorization: Bearer $HUB_API_KEY"
```

### assistants create

```bash
curl -i "$HUB_BASE_URL/v1/assistants" \
  -H "Authorization: Bearer $HUB_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "'"$CHAT_MODEL"'",
    "name": "step4-demo-assistant",
    "instructions": "Be concise."
  }'
```

### threads create

```bash
curl -i "$HUB_BASE_URL/v1/threads" \
  -H "Authorization: Bearer $HUB_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{}'
```

### thread message create

```bash
curl -i "$HUB_BASE_URL/v1/threads/<thread_id>/messages" \
  -H "Authorization: Bearer $HUB_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "role": "user",
    "content": "hello"
  }'
```

### thread run create

```bash
curl -i "$HUB_BASE_URL/v1/threads/<thread_id>/runs" \
  -H "Authorization: Bearer $HUB_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "assistant_id": "<assistant_id>"
  }'
```

### thread run create stream

```bash
curl -i -N "$HUB_BASE_URL/v1/threads/<thread_id>/runs" \
  -H "Authorization: Bearer $HUB_API_KEY" \
  -H "Content-Type: application/json" \
  -H "x-request-id: $REQUEST_ID-thread-run-stream" \
  -d '{
    "model": "'"$CHAT_MODEL"'",
    "assistant_id": "<assistant_id>",
    "stream": true
  }'
```

### thread run get

```bash
curl -i "$HUB_BASE_URL/v1/threads/<thread_id>/runs/<run_id>" \
  -H "Authorization: Bearer $HUB_API_KEY"
```

### submit tool outputs

```bash
curl -i "$HUB_BASE_URL/v1/threads/<thread_id>/runs/<run_id>/submit_tool_outputs" \
  -H "Authorization: Bearer $HUB_API_KEY" \
  -H "Content-Type: application/json" \
  -H "x-request-id: $REQUEST_ID-submit-tool-outputs" \
  -d '{
    "model": "'"$CHAT_MODEL"'",
    "tool_outputs": [{
      "tool_call_id": "call_123",
      "output": "done"
    }],
    "stream": false
  }'
```

### submit tool outputs stream

```bash
curl -i -N "$HUB_BASE_URL/v1/threads/<thread_id>/runs/<run_id>/submit_tool_outputs" \
  -H "Authorization: Bearer $HUB_API_KEY" \
  -H "Content-Type: application/json" \
  -H "x-request-id: $REQUEST_ID-submit-tool-outputs-stream" \
  -d '{
    "model": "'"$CHAT_MODEL"'",
    "tool_outputs": [{
      "tool_call_id": "call_123",
      "output": "done"
    }],
    "stream": true
  }'
```

### 需要单独覆盖时再加的资源链

1. `POST /v1/files` -> `file_id = body.id`，再测 `GET /v1/files/{file_id}`
2. `POST /v1/threads/{thread_id}/messages` -> `message_id = body.id`，再测 `GET /v1/threads/{thread_id}/messages/{message_id}`
3. `POST /v1/vector_stores` -> `vector_store_id = body.id`，再串 `POST /v1/vector_stores/{vector_store_id}/files`

## 6. 验证要点

每条回归至少检查：

1. 状态码不是 `401`、`429`、`502`、`503`
2. 响应头里存在 `x-request-id`
3. 失败时返回的是 OpenAI 风格 `{"error": {...}}`
4. 成功后 `ai.log` 有记录
5. 资源型创建接口成功后，后续读取命中同一路由资源

## 7. 当前注意事项

截至 2026-03-25，是否能真正跑通仍取决于：

1. `ai.channel.endpoint_scopes` 是否包含目标 scope
2. `ai.ability.endpoint_scope` 是否包含目标 scope
3. `ai.model_config` 是否存在目标模型
4. 上游实际是否支持该接口
