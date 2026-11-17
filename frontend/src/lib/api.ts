export class ApiError extends Error {
  constructor(
    public status: number,
    public code: string,
    message: string,
    public correlationId?: string,
  ) {
    super(message)
  }
}

/** Dépôt d'un fichier brut (corps = octets du fichier, métadonnées en paramètres d'URL). */
export async function uploadFile<T>(path: string, file: File): Promise<T> {
  const res = await fetch(`/api/v1${path}`, {
    method: 'POST',
    credentials: 'same-origin',
    headers: { 'X-Requested-With': 'mysuite', 'Content-Type': file.type || 'application/octet-stream' },
    body: file,
  })
  const text = await res.text()
  const data: unknown = text ? JSON.parse(text) : undefined
  if (!res.ok) {
    const e = (data ?? {}) as { code?: string; message?: string; correlation_id?: string }
    throw new ApiError(res.status, e.code ?? 'error', e.message ?? `Erreur ${res.status}`, e.correlation_id)
  }
  return data as T
}

async function request<T>(method: string, path: string, body?: unknown): Promise<T> {
  const res = await fetch(`/api/v1${path}`, {
    method,
    credentials: 'same-origin',
    headers: {
      // En-tête exigé par le serveur sur toute requête modifiante (protection CSRF).
      'X-Requested-With': 'mysuite',
      ...(body !== undefined ? { 'Content-Type': 'application/json' } : {}),
    },
    body: body !== undefined ? JSON.stringify(body) : undefined,
  })
  if (res.status === 204) return undefined as T
  const text = await res.text()
  const data: unknown = text ? JSON.parse(text) : undefined
  if (!res.ok) {
    const e = (data ?? {}) as { code?: string; message?: string; correlation_id?: string }
    throw new ApiError(res.status, e.code ?? 'error', e.message ?? `Erreur ${res.status}`, e.correlation_id)
  }
  return data as T
}

export const api = {
  get: <T>(path: string) => request<T>('GET', path),
  post: <T>(path: string, body?: unknown) => request<T>('POST', path, body ?? {}),
  put: <T>(path: string, body?: unknown) => request<T>('PUT', path, body ?? {}),
  del: <T>(path: string) => request<T>('DELETE', path),
}

export function qs(params: Record<string, string | number | undefined | null>): string {
  const p = new URLSearchParams()
  for (const [k, v] of Object.entries(params))
    if (v !== undefined && v !== null && v !== '') p.set(k, String(v))
  const s = p.toString()
  return s ? `?${s}` : ''
}
