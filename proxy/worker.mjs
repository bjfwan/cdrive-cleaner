// CSD AI proxy worker
//
// Sits between the CSD desktop app and an upstream OpenAI-compatible AI
// service. Two responsibilities:
//
//   1. Hide the project-paid API key (lives in Worker env, never in the
//      CSD binary). Frontend sets X-Csd-Builtin: 1 to opt into this mode.
//   2. Apply identity headers required by some upstreams (configured via
//      CSD_BUILTIN_IDENTITY env var, e.g. "kilocode").
//
// Required env vars when builtin mode is used:
//   CSD_BUILTIN_BASE_URL   e.g. https://agentrouter.org/v1
//   CSD_BUILTIN_API_KEY    e.g. sk-...
//   CSD_BUILTIN_IDENTITY   optional, e.g. "kilocode"
//
// Optional env vars:
//   CSD_ALLOWED_MODELS     comma-separated allowlist, e.g. "glm-4.5,deepseek-chat"
//                          if unset, all models accepted
//   CSD_DAILY_LIMIT        per-IP daily quota for builtin calls (default 50)
//
// Optional bindings:
//   CSD_KV                 a KV namespace; if bound it's used for the daily
//                          quota counter (survives Worker restarts). Without
//                          KV we fall back to an in-memory Map.

const VERSION = '0.1.1'
const MAX_REQUEST_BYTES = 32 * 1024 // 32 KB

const HOP_BY_HOP = new Set([
  'host',
  'connection',
  'keep-alive',
  'transfer-encoding',
  'upgrade',
  'proxy-authenticate',
  'proxy-authorization',
  'te',
  'trailer',
])

const FORBIDDEN_FORWARD = new Set([
  ...HOP_BY_HOP,
  'x-csd-builtin',
  'origin',
  'referer',
  'cookie',
  'user-agent',
  'authorization',
])

const IDENTITY_PROFILES = {
  kilocode: {
    'user-agent': 'Kilo-Code/4.0.0',
    'http-referer': 'https://kilocode.ai',
    'x-title': 'Kilo Code',
  },
}

const ALLOWED_PATHS = new Set([
  '/v1/chat/completions',
  '/chat/completions',
])

// In-memory quota fallback for environments without KV.
const MEMORY_QUOTA = new Map() // key -> { day: 'YYYY-MM-DD', count: n }

function jsonResponse(payload, status = 200) {
  return new Response(JSON.stringify(payload), {
    status,
    headers: { 'content-type': 'application/json; charset=utf-8' },
  })
}

function jsonError(status, code, message) {
  return jsonResponse({ error: { code, message } }, status)
}

function clientIp(request) {
  return (
    request.headers.get('cf-connecting-ip')
    || request.headers.get('x-real-ip')
    || request.headers.get('x-forwarded-for')?.split(',')[0]?.trim()
    || 'unknown'
  )
}

function todayUtc() {
  return new Date().toISOString().slice(0, 10)
}

async function bumpQuota(env, ip, limit) {
  const day = todayUtc()
  const key = `csd:quota:${day}:${ip}`

  if (env?.CSD_KV) {
    const raw = await env.CSD_KV.get(key)
    const current = raw ? Number(raw) || 0 : 0
    if (current >= limit) {
      return { ok: false, count: current }
    }
    const next = current + 1
    // 36h ttl is plenty — entry naturally rolls at midnight UTC.
    await env.CSD_KV.put(key, String(next), { expirationTtl: 60 * 60 * 36 })
    return { ok: true, count: next }
  }

  const slot = MEMORY_QUOTA.get(key)
  const current = slot && slot.day === day ? slot.count : 0
  if (current >= limit) {
    return { ok: false, count: current }
  }
  const next = current + 1
  MEMORY_QUOTA.set(key, { day, count: next })
  // Best-effort cleanup: drop entries from previous days.
  if (MEMORY_QUOTA.size > 4096) {
    for (const [k, v] of MEMORY_QUOTA) {
      if (v.day !== day) MEMORY_QUOTA.delete(k)
    }
  }
  return { ok: true, count: next }
}

export default {
  async fetch(request, env) {
    const url = new URL(request.url)

    if (
      request.method === 'GET'
      && (url.pathname === '/' || url.pathname === '/health' || url.pathname === '/healthz')
    ) {
      return jsonResponse({
        ok: true,
        name: 'csd-proxy',
        runtime: 'cloudflare-workers',
        version: VERSION,
        mode: 'builtin',
        builtinReady: Boolean(env?.CSD_BUILTIN_BASE_URL && env?.CSD_BUILTIN_API_KEY),
        identity: env?.CSD_BUILTIN_IDENTITY || null,
        quotaBackend: env?.CSD_KV ? 'kv' : 'memory',
        timestamp: new Date().toISOString(),
      })
    }

    // Only POST /v1/chat/completions (or /chat/completions) is allowed.
    if (request.method !== 'POST') {
      return jsonError(405, 'METHOD_NOT_ALLOWED', 'only POST is supported')
    }
    if (!ALLOWED_PATHS.has(url.pathname)) {
      return jsonError(404, 'PATH_NOT_ALLOWED', `path ${url.pathname} is not exposed by csd-proxy`)
    }

    const isBuiltin = (request.headers.get('x-csd-builtin') || '').trim() === '1'
    if (!isBuiltin) {
      return jsonError(403, 'BUILTIN_REQUIRED', 'csd-proxy only serves builtin mode (X-Csd-Builtin: 1)')
    }

    const base = String(env?.CSD_BUILTIN_BASE_URL || '').trim()
    const key = String(env?.CSD_BUILTIN_API_KEY || '').trim()
    if (!base || !key) {
      return jsonError(503, 'BUILTIN_NOT_CONFIGURED', 'CSD_BUILTIN_BASE_URL / CSD_BUILTIN_API_KEY not set')
    }

    // Reject oversized bodies early via Content-Length when available.
    const declaredLength = Number(request.headers.get('content-length') || 0)
    if (declaredLength > MAX_REQUEST_BYTES) {
      return jsonError(
        413,
        'BODY_TOO_LARGE',
        `request body exceeds ${MAX_REQUEST_BYTES} bytes (got ${declaredLength})`,
      )
    }

    // Read request body once (we need to inspect model field for allowlist).
    let bodyBytes
    try {
      bodyBytes = await request.arrayBuffer()
    } catch (error) {
      return jsonError(400, 'BODY_READ_FAILED', String(error?.message || error))
    }

    if (bodyBytes.byteLength > MAX_REQUEST_BYTES) {
      return jsonError(
        413,
        'BODY_TOO_LARGE',
        `request body exceeds ${MAX_REQUEST_BYTES} bytes (got ${bodyBytes.byteLength})`,
      )
    }

    // Parse once for model allowlist enforcement (also validates it is JSON).
    let parsed
    try {
      parsed = JSON.parse(new TextDecoder().decode(bodyBytes))
    } catch {
      return jsonError(400, 'INVALID_BODY', 'request body must be JSON with a "model" field')
    }

    // Enforce model allowlist if configured (always enforced when set).
    const allowedModels = String(env?.CSD_ALLOWED_MODELS || '')
      .split(',')
      .map((s) => s.trim())
      .filter(Boolean)
    if (allowedModels.length > 0) {
      const model = String(parsed?.model || '').trim()
      if (!model || !allowedModels.includes(model)) {
        return jsonError(
          400,
          'MODEL_NOT_ALLOWED',
          `model "${model}" is not in CSD_ALLOWED_MODELS allowlist`,
        )
      }
    }

    // Per-IP daily quota for builtin calls.
    const dailyLimit = Number(env?.CSD_DAILY_LIMIT || 50)
    if (dailyLimit > 0) {
      const ip = clientIp(request)
      const quota = await bumpQuota(env, ip, dailyLimit)
      if (!quota.ok) {
        return jsonError(
          429,
          'QUOTA_EXCEEDED',
          `daily builtin quota exceeded (limit ${dailyLimit})`,
        )
      }
    }

    const cleanBase = base.replace(/\/+$/, '')
    const upstreamPath = url.pathname.startsWith('/v1/') ? url.pathname.slice(3) : url.pathname
    let upstreamUrl
    try {
      upstreamUrl = new URL(cleanBase + upstreamPath + url.search)
      if (upstreamUrl.protocol !== 'https:' && upstreamUrl.protocol !== 'http:') {
        throw new Error('upstream must be http(s)')
      }
    } catch (error) {
      return jsonError(500, 'INVALID_UPSTREAM', String(error?.message || error))
    }

    const forwardHeaders = new Headers()
    for (const [name, value] of request.headers.entries()) {
      if (FORBIDDEN_FORWARD.has(name.toLowerCase())) continue
      forwardHeaders.set(name, value)
    }
    forwardHeaders.set('authorization', `Bearer ${key}`)
    forwardHeaders.set('content-type', 'application/json')

    const identity = String(env?.CSD_BUILTIN_IDENTITY || '').trim().toLowerCase()
    const profile = identity && IDENTITY_PROFILES[identity]
    if (profile) {
      for (const [headerName, headerValue] of Object.entries(profile)) {
        forwardHeaders.set(headerName, headerValue)
      }
    }

    let upstreamResp
    try {
      upstreamResp = await fetch(upstreamUrl.toString(), {
        method: 'POST',
        headers: forwardHeaders,
        body: bodyBytes,
      })
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error)
      console.error(`upstream fetch failed: ${upstreamUrl.host} → ${message}`)
      return jsonError(502, 'PROXY_FETCH_FAILED', `upstream fetch failed: ${message}`)
    }

    const responseHeaders = new Headers()
    for (const [name, value] of upstreamResp.headers.entries()) {
      if (HOP_BY_HOP.has(name.toLowerCase())) continue
      responseHeaders.set(name, value)
    }

    const responseBody = await upstreamResp.arrayBuffer()
    return new Response(responseBody, {
      status: upstreamResp.status,
      statusText: upstreamResp.statusText,
      headers: responseHeaders,
    })
  },
}
