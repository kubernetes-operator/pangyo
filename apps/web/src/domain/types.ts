// === 도메인 타입: IndexedDB v1(memonest_local_v1)에 저장되는 레코드 형식과 동일 ===
// 이 형식을 바꾸면 storage의 DB_VERSION을 올리고 migration + 호환 테스트를 추가해야 합니다.

export type NoteFormat = 'md' | 'txt';

/** Undo/Redo 한 칸에 저장되는 전체 스냅샷 (프로토타입 방식; 정식판은 델타) */
export interface Snapshot {
  title: string;
  content: string;
  format: NoteFormat;
}

/** 키보드 활동 기준 편집 세션 (요구 23, 24) */
export interface EditSession {
  id: string;
  startedAt: string; // ISO 8601
  endedAt: string | null;
  activeMs: number;
}

export interface Note extends Snapshot {
  id: string; // UUID, 경로가 바뀌어도 불변
  folder: string; // '' = 루트, 최대 2단계 'A/B'
  createdAt: string;
  modifiedAt: string;
  sessions: EditSession[];
  undo: Snapshot[];
  redo: Snapshot[];
}

export interface FolderRecord {
  path: string;
}

/**
 * 저장소에서 읽은 레코드를 안전한 Note로 보정합니다.
 * v0.1 JS 프로토타입은 undo/redo/sessions가 없는 레코드를 남길 수 있었으므로 기본값을 채웁니다.
 */
export function normalizeNote(raw: Partial<Note> & { id: string }): Note {
  const format: NoteFormat = raw.format === 'txt' ? 'txt' : 'md';
  const now = new Date().toISOString();
  return {
    id: raw.id,
    title: raw.title ?? 'Untitled',
    content: raw.content ?? '',
    format,
    folder: raw.folder ?? '',
    createdAt: raw.createdAt ?? raw.modifiedAt ?? now,
    modifiedAt: raw.modifiedAt ?? raw.createdAt ?? now,
    sessions: Array.isArray(raw.sessions) ? raw.sessions : [],
    undo: Array.isArray(raw.undo) ? raw.undo : [],
    redo: Array.isArray(raw.redo) ? raw.redo : [],
  };
}
