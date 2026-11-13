// === 도메인 타입: IndexedDB v1(memonest_local_v1)에 저장되는 레코드 형식과 동일 ===
// 이 형식을 바꾸면 storage의 DB_VERSION을 올리고 migration + 호환 테스트를 추가해야 합니다.

export type NoteFormat = 'md' | 'txt';

/** Undo/Redo 한 칸에 저장되는 전체 스냅샷 (프로토타입 방식; 정식판은 델타) */
export interface Snapshot {
  title: string;
  content: string;
  format: NoteFormat;
}

/**
 * 편집 세션 (요구 23, 24): 메모를 연 시점부터 다른 메모로 전환·창 숨김·종료까지.
 * 다시 열면 새 세션(재개)으로 기록합니다.
 */
export interface EditSession {
  id: string;
  startedAt: string; // ISO 8601
  endedAt: string | null;
  activeMs: number; // 작성 시간: 키 입력 간격(무입력 한도 이하)의 합
  openMs: number; // 열어둔 시간: 메모가 열려 있고 창이 화면에 보이는 시간 (v0.1 레코드에는 없음 → 0)
}

/** 서버에서 받은 메모 (충돌 검토용 사본에도 사용) */
export interface ServerCopy extends Snapshot {
  version: number;
  folder: string;
  modifiedAt: string;
  deleted: boolean;
}

/**
 * 동기화 상태 (로드맵 06, ADR-0003). 로그인해서 동기화한 적이 없으면 없음.
 * 편집 세션·Undo 기록은 기기별 정보라 동기화하지 않습니다.
 */
export interface NoteSync {
  version: number; // 이 사본이 기반으로 한 서버 버전 (0 = 아직 올리지 않음)
  dirty: boolean; // 올리지 않은 로컬 변경 (오프라인 초안 포함)
  conflict?: ServerCopy | null; // 로컬 변경 중 서버에서도 바뀐 경우: 사용자가 고를 때까지 보관
  error?: string | null; // 서버가 거부한 이유 (경로 중복, 용량 등). 다시 편집하면 지움
}

export interface Note extends Snapshot {
  id: string; // UUID, 경로가 바뀌어도 불변
  folder: string; // '' = 루트, 최대 2단계 'A/B'
  createdAt: string;
  modifiedAt: string;
  sessions: EditSession[];
  undo: Snapshot[];
  redo: Snapshot[];
  sync?: NoteSync;
  /** 프로젝트 ID (ADR-0005). 없거나 null = 기본 프로젝트. v0.1 레코드에는 없음 */
  projectId?: string | null;
}

export interface FolderRecord {
  path: string;
}

/** v0.1 세션에는 openMs가 없으므로 0으로 채웁니다. */
export function normalizeSession(raw: Partial<EditSession> & { id: string }): EditSession {
  return {
    id: raw.id,
    startedAt: raw.startedAt ?? new Date(0).toISOString(),
    endedAt: raw.endedAt ?? null,
    activeMs: raw.activeMs ?? 0,
    openMs: raw.openMs ?? 0,
  };
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
    sessions: Array.isArray(raw.sessions) ? raw.sessions.map(normalizeSession) : [],
    undo: Array.isArray(raw.undo) ? raw.undo : [],
    redo: Array.isArray(raw.redo) ? raw.redo : [],
    ...(raw.sync && typeof raw.sync.version === 'number' ? { sync: { ...raw.sync, dirty: !!raw.sync.dirty } } : {}),
    // 프로젝트 (ADR-0005): 문자열만 유지, 그 밖(없음·null·잘못된 값)은 기본 프로젝트
    ...(typeof raw.projectId === 'string' && raw.projectId ? { projectId: raw.projectId } : {}),
  };
}
