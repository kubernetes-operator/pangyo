// === 계정 API 클라이언트 (같은 출처의 /api/v1, 계약: packages/protocol/openapi.json) ===
// 웹은 HttpOnly 세션 쿠키를 쓰므로 토큰을 JS에서 다루지 않습니다.
// 상태를 바꾸는 요청에는 CSRF 방지 헤더를 붙입니다 (서버 crates/server/src/security.rs CSRF_HEADER).

export const API_BASE = '/api/v1'; // 같은 출처 (note.studiobasa.com / 개발 시 Vite 프록시)
const CSRF_HEADER = { 'X-MemoNest-Request': '1' } as const;
const REQUEST_TIMEOUT_MS = 10_000; // 서버가 응답하지 않으면 '서버 없음'으로 처리

export interface Account {
  id: string;
  email: string | null;
  displayName: string;
  authProvider: 'local' | 'google';
  tier: string;
  quotaBytes: number;
  usedBytes: number;
  createdAt: string;
}
export interface Providers {
  local: boolean;
  google: boolean;
  registration: 'open' | 'closed';
}
export interface PairingStart {
  code: string;
  expiresAt: string;
}
export interface Device {
  id: string;
  name: string;
  platform: string;
  createdAt: string;
  lastSeenAt: string | null;
  revokedAt: string | null;
}

/** 서버가 보낸 오류 (error 코드 + 사용자에게 보여줄 메시지) */
export class ApiError extends Error {
  constructor(
    readonly status: number,
    readonly code: string,
    message: string,
  ) {
    super(message);
  }
}

/** 서버에 연결할 수 없음 (오프라인, 서버 없는 로컬 실행 등) */
export class ServerUnavailable extends Error {}

async function request<T>(method: string, path: string, body?: unknown): Promise<T> {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), REQUEST_TIMEOUT_MS);
  let response: Response;
  try {
    response = await fetch(API_BASE + path, {
      method,
      credentials: 'same-origin',
      headers: {
        Accept: 'application/json',
        ...(body === undefined ? {} : { 'Content-Type': 'application/json' }),
        ...(method === 'GET' ? {} : CSRF_HEADER),
      },
      body: body === undefined ? undefined : JSON.stringify(body),
      signal: controller.signal,
    });
  } catch {
    throw new ServerUnavailable('server unreachable');
  } finally {
    clearTimeout(timer);
  }
  if (response.status === 204) return undefined as T;
  const isJson = response.headers.get('content-type')?.includes('application/json');
  const data: unknown = isJson ? await response.json() : undefined;
  if (!isJson) throw new ServerUnavailable(`unexpected response ${response.status}`); // 정적 서버 등
  if (!response.ok) {
    const err = data as { error?: string; message?: string };
    if (['database_not_configured', 'database_connecting', 'not_implemented'].includes(err.error ?? '')) {
      throw new ServerUnavailable(err.error);
    }
    throw new ApiError(response.status, err.error ?? 'unknown', err.message ?? `HTTP ${response.status}`);
  }
  return data as T;
}

export const accountApi = {
  providers: () => request<Providers>('GET', '/auth/providers'),
  register: (email: string, password: string, displayName: string) =>
    request<Account>('POST', '/auth/register', { email, password, displayName }),
  login: (email: string, password: string) => request<Account>('POST', '/auth/login', { email, password }),
  logout: () => request<void>('POST', '/auth/logout'),
  /** 로그인하지 않았으면 null */
  async me(): Promise<Account | null> {
    try {
      return await request<Account>('GET', '/account');
    } catch (e) {
      if (e instanceof ApiError && e.status === 401) return null;
      throw e;
    }
  },
  pairingStart: () => request<PairingStart>('POST', '/pairing/start'),
  devices: () => request<Device[]>('GET', '/devices'),
  revokeDevice: (id: string) => request<void>('DELETE', `/devices/${encodeURIComponent(id)}`),
};

/** 104857600 → '100 MB' */
export function formatBytes(bytes: number): string {
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit++;
  }
  return `${Number.isInteger(value) ? value : value.toFixed(1)} ${units[unit]}`;
}
