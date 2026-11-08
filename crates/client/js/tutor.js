// Optional text-only transport. Credentials never enter a preview or durable storage.
const jobs = new Map();
const encoder = new TextEncoder();
const failure = (kind, message) => ({ ok: false, error: { kind, message } });
function endpoint(raw) {
  const url = new URL(raw);
  if (url.username || url.password || url.search || url.hash) throw new Error('Use an endpoint without URL credentials, query parameters or fragments.');
  const loopback = ['localhost', '127.0.0.1', '[::1]'].includes(url.hostname);
  if (url.protocol !== 'https:' && !(url.protocol === 'http:' && loopback)) throw new Error('Use HTTPS, or HTTP on your own loopback model service.');
  return url.href;
}
function validatePreview(preview) {
  if (!['ollama', 'openai_chat'].includes(preview.protocol)) throw new Error('Unsupported text protocol.');
  endpoint(preview.endpoint);
  if (!Number.isInteger(preview.timeout_ms) || preview.timeout_ms < 100 || preview.timeout_ms > 120000) throw new Error('Request timeout must be 100–120000 ms.');
  if (!Number.isInteger(preview.max_response_bytes) || preview.max_response_bytes < 1024 || preview.max_response_bytes > 262144) throw new Error('Response limit must be 1024–262144 bytes.');
  if (typeof preview.body !== 'string' || encoder.encode(preview.body).length > 65536) throw new Error('Selected context exceeds 64 KiB. Select less context.');
  const body = JSON.parse(preview.body);
  if (typeof body.model !== 'string' || !body.model.trim() || body.model.length > 256 || body.stream !== false) throw new Error('Choose a model and a nonstreaming request.');
  if (!Array.isArray(body.messages) || !body.messages.length || body.messages.length > 32 || body.messages.some(m => !['system', 'user', 'assistant'].includes(m.role) || typeof m.content !== 'string')) throw new Error('Invalid text messages.');
  if (body.tools || body.tool_choice || body.functions) throw new Error('Model tool execution is not supported.');
}
export function prepareModel(configJson, messagesJson) {
  try {
    const config = JSON.parse(configJson), messages = JSON.parse(messagesJson);
    const body = { model: config.model, messages, stream: false };
    if (config.protocol === 'ollama') body.options = { num_predict: 512 };
    else body.store = false;
    const preview = { protocol: config.protocol, endpoint: endpoint(config.endpoint), body: JSON.stringify(body), timeout_ms: config.timeout_ms, max_response_bytes: config.max_response_bytes };
    validatePreview(preview);
    return JSON.stringify({ ok: true, preview });
  } catch (error) { return JSON.stringify(failure('configuration', error.message)); }
}
export function cancelModel(id) {
  const job = jobs.get(id);
  if (!job) return false;
  job.reason = 'cancelled'; job.controller.abort(); return true;
}
export async function sendModel(previewJson, secret, id) {
  let preview;
  try { preview = JSON.parse(previewJson); validatePreview(preview); }
  catch (_) { return JSON.stringify(failure('configuration', 'Prepare a valid context preview before sending.')); }
  if (typeof id !== 'string' || !id || id.length > 128 || jobs.has(id) || jobs.size >= 8 || typeof secret !== 'string' || secret.length > 8192 || /[\r\n]/.test(secret)) return JSON.stringify(failure('configuration', 'Invalid or already active request.'));
  const job = { controller: new AbortController(), reason: null };
  jobs.set(id, job);
  const timer = setTimeout(() => { job.reason = 'timeout'; job.controller.abort(); }, preview.timeout_ms);
  try {
    const headers = { 'Content-Type': 'application/json', 'Accept': 'application/json' };
    if (secret) headers.Authorization = `Bearer ${secret}`;
    const response = await fetch(preview.endpoint, { method: 'POST', headers, body: preview.body,
      signal: job.controller.signal, credentials: 'omit', redirect: 'error', cache: 'no-store', referrerPolicy: 'no-referrer' });
    if (!response.ok) {
      await response.body?.cancel();
      const kind = response.status === 429 ? 'usage_limit' : [401, 403].includes(response.status) ? 'authentication' : response.status === 404 ? 'unavailable' : 'provider_error';
      return JSON.stringify(failure(kind, `The endpoint returned HTTP ${response.status}. Review the endpoint, model and account settings; authored help is still available.`));
    }
    if (!response.body) return JSON.stringify(failure('malformed', 'The endpoint returned no response body.'));
    const reader = response.body.getReader(), chunks = []; let bytes = 0;
    for (;;) {
      const { value, done } = await reader.read(); if (done) break;
      bytes += value.byteLength;
      if (bytes > preview.max_response_bytes) { await reader.cancel(); return JSON.stringify(failure('response_limit', 'The model response exceeded the selected byte limit. No partial reply was retained.')); }
      chunks.push(value);
    }
    if (job.reason) return JSON.stringify(failure(job.reason, job.reason === 'timeout' ? 'The request exceeded its time limit. No partial reply was retained.' : 'Request cancelled. No partial reply was retained.'));
    const joined = new Uint8Array(bytes); let offset = 0;
    for (const chunk of chunks) { joined.set(chunk, offset); offset += chunk.byteLength; }
    let data;
    try { data = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(joined)); }
    catch (_) { return JSON.stringify(failure('malformed', 'The endpoint did not return valid UTF-8 JSON.')); }
    const message = preview.protocol === 'ollama' ? data.message : data.choices?.[0]?.message;
    if (message?.refusal || message?.tool_calls?.length || message?.function_call) return JSON.stringify(failure('rejected', 'The endpoint returned a refusal or a tool request. Only text suggestions are supported.'));
    if (typeof message?.content !== 'string' || !message.content.trim() || (preview.protocol === 'ollama' && data.done !== true)) return JSON.stringify(failure('malformed', 'The endpoint did not return a complete text message.'));
    // A hostile endpoint can reflect a header. Keep that secret out of displayed text.
    const text = secret ? message.content.split(secret).join('[credential redacted]') : message.content;
    const reason = preview.protocol === 'ollama' ? data.done_reason : data.choices?.[0]?.finish_reason;
    return JSON.stringify({ ok: true, reply: { text, truncated: reason === 'length', bytes, model: JSON.parse(preview.body).model } });
  } catch (_) {
    return JSON.stringify(failure(job.reason || 'network', job.reason === 'timeout' ? 'The request exceeded its time limit. No partial reply was retained.' : job.reason === 'cancelled' ? 'Request cancelled. No partial reply was retained.' : 'Network or browser policy failure. Check endpoint availability and allowed origins. No hidden proxy or retry was used.'));
  } finally { clearTimeout(timer); jobs.delete(id); }
}
