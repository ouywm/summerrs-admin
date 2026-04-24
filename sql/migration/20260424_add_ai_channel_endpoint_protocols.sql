ALTER TABLE ai.channel
    ADD COLUMN IF NOT EXISTS endpoint_protocols JSONB NOT NULL DEFAULT '{}'::jsonb;

COMMENT ON COLUMN ai.channel.endpoint_protocols IS
    'endpoint 到协议/风味的显式映射（JSON，如 {"chat":{"protocol":"openai","flavor":"native"}}）';
