// === 동기화 API 연결 (계약: packages/protocol/openapi.json — /api/v1/notes*) ===
import { ApiError, request, ServerUnavailable } from '../account/api';
import type { LeaseGrant, ServerNote, ServerProject, SyncApi, SyncError } from './engine';

/** 계정 API 오류를 엔진이 이해하는 모양으로 */
function toSyncError(e: unknown): SyncError {
  if (e instanceof ServerUnavailable) return { offline: true, message: e.message };
  if (e instanceof ApiError) return { status: e.status, code: e.code, message: e.message, details: e.details };
  return { offline: true, message: String(e) }; // 네트워크 계층 오류는 오프라인으로 취급
}

async function call<T>(fn: () => Promise<T>): Promise<T> {
  try {
    return await fn();
  } catch (e) {
    throw toSyncError(e);
  }
}

const notePath = (id: string) => `/notes/${encodeURIComponent(id)}`;

export const syncApi: SyncApi = {
  changes: (since, projects) =>
    call(() =>
      request<{
        notes: ServerNote[];
        projects?: ServerProject[];
        defaultProjectName?: string;
        cursor: number;
        hasMore: boolean;
      }>(
        'GET',
        `/notes?since=${since}${projects ? '&projects=1' : ''}`,
      ),
    ),
  putProject: (id, name) => call(() => request<ServerProject>('PUT', `/projects/${encodeURIComponent(id)}`, { name })),
  deleteProject: (id) => call(() => request<ServerProject>('DELETE', `/projects/${encodeURIComponent(id)}`)),
  get: (id) => call(() => request<ServerNote>('GET', notePath(id))),
  put: (id, body) => call(() => request<ServerNote>('PUT', notePath(id), body)),
  acquireLease: (id, clientId, label) =>
    call(() => request<LeaseGrant>('POST', `${notePath(id)}/lease`, { clientId, label })),
  releaseLease: (id, clientId, keepalive) =>
    call(() =>
      request<void>('DELETE', `${notePath(id)}/lease?clientId=${encodeURIComponent(clientId)}`, undefined, {
        keepalive,
      }),
    ),
  remove: (id, baseVersion, clientId, fencingToken) =>
    call(() =>
      request<ServerNote>(
        'DELETE',
        `${notePath(id)}?baseVersion=${baseVersion}&clientId=${encodeURIComponent(clientId)}` +
          (fencingToken === null ? '' : `&fencingToken=${fencingToken}`),
      ),
    ),
};

/** 이 브라우저 탭을 다른 탭/기기와 구별하는 ID (탭마다 다름 → 같은 브라우저의 두 탭도 서로 막음) */
export function newClientId(): string {
  return crypto.randomUUID().replaceAll('-', '');
}

/** 다른 기기에 보일 이름. 예: "Chrome · Linux" */
export function deviceLabel(): string {
  const ua = navigator.userAgent;
  const browser = /Edg\//.test(ua) ? 'Edge' : /Firefox\//.test(ua) ? 'Firefox' : /Chrome\//.test(ua) ? 'Chrome'
    : /Safari\//.test(ua) ? 'Safari' : 'Browser';
  const os = /Android/.test(ua) ? 'Android' : /iPhone|iPad/.test(ua) ? 'iOS' : /Windows/.test(ua) ? 'Windows'
    : /Mac OS/.test(ua) ? 'macOS' : /Linux/.test(ua) ? 'Linux' : '';
  return os ? `${browser} · ${os}` : browser;
}
