# Provider API Contract: Librarian

**Date**: 2026-04-17
**Scope**: LM Studio and OpenAI provider interfaces

## Provider Trait

All AI providers implement a common trait:

```text
trait Provider:
    fn name() -> &str
    async fn validate() -> Result<ModelInfo>
    async fn chat(messages, temperature, max_tokens) -> Result<ChatResponse>
    async fn chat_stream(messages, temperature, max_tokens) -> Result<Stream<ChatChunk>>
    async fn embed(texts: Vec<String>) -> Result<Vec<Vec<f32>>>
```

## LM Studio Endpoints

**Base URL**: Configurable, default `http://localhost:1234/v1`

### Validation
```text
GET /v1/models
Response 200: { "data": [{ "id": "model-name", ... }] }
```

### Chat Completion
```text
POST /v1/chat/completions
Content-Type: application/json

{
  "model": "<configured-model>",
  "messages": [{ "role": "system"|"user"|"assistant", "content": "..." }],
  "temperature": 0.3,
  "max_tokens": 1024,
  "stream": true|false
}

Response 200 (non-streaming):
{ "choices": [{ "message": { "content": "..." } }] }

Response 200 (streaming):
data: {"choices":[{"delta":{"content":"..."}}]}
data: [DONE]
```

### Embeddings
```text
POST /v1/embeddings
Content-Type: application/json

{
  "model": "<configured-embed-model>",
  "input": ["text1", "text2", ...]
}

Response 200:
{ "data": [{ "embedding": [0.1, 0.2, ...], "index": 0 }] }
```

## OpenAI Endpoints

**Base URL**: `https://api.openai.com/v1`

### Authentication
```text
Authorization: Bearer <OPENAI_API_KEY>
```

### Chat Completion (Responses API)
```text
POST /v1/chat/completions
Content-Type: application/json

{
  "model": "<configured-model>",
  "messages": [...],
  "temperature": 0.3,
  "max_tokens": 1024,
  "stream": true|false
}
```

Same response format as LM Studio (OpenAI-compatible).

### Embeddings
```text
POST /v1/embeddings
Content-Type: application/json

{
  "model": "<configured-embed-model>",
  "input": ["text1", "text2", ...]
}
```

Same response format as LM Studio.

### Rate Limiting

Token bucket: default 20 requests per minute. Implemented client-side. On HTTP 429, wait for `Retry-After` header value, then retry once. On second 429, fail the request.

## Error Handling

| Scenario | Behaviour |
|----------|-----------|
| Provider unreachable | Fail with descriptive error, exit code 2 |
| HTTP 401 (OpenAI) | "Invalid API key" error |
| HTTP 429 (OpenAI) | Wait Retry-After, retry once |
| HTTP 500 | Retry once after 2s, then fail |
| Timeout (30s default) | Fail with timeout error |
| Invalid JSON response | Log raw response, fail with parse error |
| Empty embedding response | Fail with descriptive error |

## SSE Stream Protocol

Line-based parsing:
1. Read lines until `\n\n` (event boundary)
2. Lines starting with `data: ` contain JSON payloads
3. `data: [DONE]` signals end of stream
4. Lines starting with `:` are comments (ignore)
5. Accumulate `delta.content` fields for full response
