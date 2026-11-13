// === 계정 API 클라이언트 (같은 출처의 /api/v1, 계약: packages/protocol/openapi.json) ===
// 웹은 HttpOnly 세션 쿠키를 쓰므로 토큰을 JS에서 다루지 않습니다.
// 상태를 바꾸는 요청에는 CSRF 방지 헤더를 붙입니다 (서버 crates/server/src/security.rs CSRF_HEADER).

export const API_BASE = '/api/v1'; // 같은 출처 (note.studiobasa.com / 개발 시 Vite 프록시)
const CSRF_HEADER = { 'X-MemoNest-Request': '1' } as const;
// 서버가 응답하지 않으면 '서버 없음'으로 처리. 국내→Cloudflare 미국 엣지 경유로 응답이 최대 9초까지 걸려(2026-11-12 측정) 30초.
// 편집 임대(90초, 1/3마다 갱신)보다 짧아야 함. 앱(src-tauri remote.rs REQUEST_TIMEOUT)과 같은 값
const REQUEST_TIMEOUT_MS = 30_000;

export interface Account {
  id: string;
  email: string | null;
  displayName: string;
  authProvider: 'local' | 'google';
  tier: string;
  quotaBytes: number;
  usedBytes: number;
  createdAt: string;
  /** 관리자 화면(시스템) 표시 여부 */
  isAdmin?: boolean;
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
    readonly details: Record<string, unknown> = {},
  ) {
    super(message);
  }
}

/** 서버에 연결할 수 없음 (오프라인, 서버 없는 로컬 실행 등) */
export class ServerUnavailable extends Error {}

/** 서버 응답 원본 (전송 방식과 무관하게 같은 모양) */
export interface RawResponse {
  status: number;
  /** JSON 응답이면 true (아니면 정적 서버 등 → '서버 없음') */
  json: boolean;
  data: unknown;
}

/**
 * 요청 전송 방식. 웹은 같은 출처 fetch(세션 쿠키), 데스크톱·모바일 앱은 Tauri 명령(장비 토큰, storage/tauri.ts).
 * 연결할 수 없으면 ServerUnavailable을 던집니다.
 */
export type Transport = (
  method: string,
  path: string,
  body: unknown,
  options: { keepalive?: boolean },
) => Promise<RawResponse>;

const fetchTransport: Transport = async (method, path, body, options) => {
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
      keepalive: options.keepalive, // 페이지를 닫을 때 임대 반납 요청이 끝까지 가도록
    });
  } catch {
    throw new ServerUnavailable('server unreachable');
  } finally {
    clearTimeout(timer);
  }
  if (response.status === 204) return { status: 204, json: true, data: undefined };
  const json = !!response.headers.get('content-type')?.includes('application/json');
  return { status: response.status, json, data: json ? await response.json() : undefined };
};

let transport: Transport = fetchTransport;

/** 데스크톱·모바일 앱에서 장비 토큰 전송으로 바꿀 때 (storage/tauri.ts) */
export function setTransport(next: Transport): void {
  transport = next;
}

export async function request<T>(
  method: string,
  path: string,
  body?: unknown,
  options: { keepalive?: boolean } = {},
): Promise<T> {
  const response = await transport(method, path, body, options);
  if (response.status === 204) return undefined as T;
  if (!response.json) throw new ServerUnavailable(`unexpected response ${response.status}`); // 정적 서버 등
  const data = response.data;
  if (response.status < 200 || response.status >= 300) {
    const err = (data ?? {}) as { error?: string; message?: string; details?: Record<string, unknown> };
    if (['database_not_configured', 'database_connecting', 'not_implemented'].includes(err.error ?? '')) {
      throw new ServerUnavailable(err.error);
    }
    throw new ApiError(response.status, err.error ?? 'unknown', err.message ?? `HTTP ${response.status}`, err.details);
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

export interface IntegrationToken {
  id: string;
  name: string;
  scopes: string[];
  createdAt: string;
  expiresAt: string;
  lastUsedAt: string | null;
  revokedAt: string | null;
}
export interface SearchHit {
  id: string;
  title: string;
  folder: string;
  format: 'md' | 'txt';
  snippet: string;
  score: number;
}

/** 자동화 토큰(n8n) 관리 — 웹 세션 전용 */
export const tokenApi = {
  list: () => request<IntegrationToken[]>('GET', '/integrations/tokens'),
  create: (name: string, scopes: string[], ttlDays: number) =>
    request<IntegrationToken & { token: string }>('POST', '/integrations/tokens', { name, scopes, ttlDays }),
  revoke: (id: string) => request<void>('DELETE', `/integrations/tokens/${encodeURIComponent(id)}`),
};

/** 서버 검색·연관 메모 */
export const searchApi = {
  search: (q: string, limit = 20) =>
    request<SearchHit[]>('GET', `/search?q=${encodeURIComponent(q)}&limit=${limit}`),
  similar: (id: string, limit = 5) =>
    request<SearchHit[]>('GET', `/notes/${encodeURIComponent(id)}/similar?limit=${limit}`),
};

export interface DownloadFile {
  id: string;
  platform: string;
  label: string;
  fileName: string;
  size: number;
  sha256: string;
}

/** 설치 파일: 목록 + 누를 때마다 짧게 유효한 일회성 주소 발급 (실제 저장 위치는 드러나지 않음) */
export const downloadApi = {
  list: () => request<{ version: string; files: DownloadFile[] }>('GET', '/downloads'),
  link: (id: string) =>
    request<{ url: string; expiresAt: string }>('POST', `/downloads/${encodeURIComponent(id)}/link`),
};

export interface AdminSettings {
  defaultQuotaMb: number;
  envDefaultQuotaMb: number;
  updatedAccounts: number;
}
export interface AdminAccount {
  id: string;
  email: string | null;
  displayName: string;
  usedBytes: number;
  quotaBytes: number;
  quotaCustom: boolean;
  createdAt: string;
}

/** 관리자: 기본 저장공간·계정별 저장공간 (2026-11-13 사용 의견 6번) */
export const adminApi = {
  settings: () => request<AdminSettings>('GET', '/admin/settings'),
  setDefaultQuota: (defaultQuotaMb: number) => request<AdminSettings>('PUT', '/admin/settings', { defaultQuotaMb }),
  accounts: () => request<{ accounts: AdminAccount[] }>('GET', '/admin/accounts'),
  /** null이면 기본값으로 되돌림 */
  setQuota: (id: string, quotaMb: number | null) =>
    request<AdminAccount>('PUT', `/admin/accounts/${encodeURIComponent(id)}/quota`, { quotaMb }),
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
