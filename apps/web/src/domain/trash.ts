// === 휴지통 (2026-11-13 사용 의견: 지우면 1주일 보관) — 이 기기 쪽 규칙 ===
// 서버에 올라간 메모·프로젝트는 서버 휴지통(GET /api/v1/trash, 7일)에 보관되고,
// 서버에 없는(로그인 전·오프라인에서 만든) 메모·프로젝트는 이 기기의 meta 'localTrash'에 7일 보관합니다.
import type { LocalProject } from './projects';
import type { Note } from './types';

export const LOCAL_TRASH_META_KEY = 'localTrash';
export const TRASH_DAYS = 7;
const DAY_MS = 24 * 60 * 60 * 1000;

export type LocalTrashItem =
  | { kind: 'note'; note: Note; deletedAt: string }
  | { kind: 'project'; project: LocalProject; deletedAt: string };

/** 보관 기간이 지난 항목을 뺌 */
export function pruneTrash(items: LocalTrashItem[], now = Date.now(), days = TRASH_DAYS): LocalTrashItem[] {
  return items.filter((i) => now - Date.parse(i.deletedAt) < days * DAY_MS);
}

/** 남은 날 (올림, 최소 0) */
export function daysLeft(expiresAt: string, now = Date.now()): number {
  return Math.max(0, Math.ceil((Date.parse(expiresAt) - now) / DAY_MS));
}

export const expiresAtOf = (deletedAt: string, days = TRASH_DAYS) =>
  new Date(Date.parse(deletedAt) + days * DAY_MS).toISOString();

/** 같은 이름이 있으면 " (복구)", " (복구 2)" … 를 붙인 이름 */
export function restoredName(name: string, taken: (candidate: string) => boolean, suffix = '복구'): string {
  if (!taken(name)) return name;
  for (let i = 1; ; i++) {
    const candidate = `${name} (${suffix}${i > 1 ? ' ' + i : ''})`;
    if (!taken(candidate)) return candidate;
  }
}
