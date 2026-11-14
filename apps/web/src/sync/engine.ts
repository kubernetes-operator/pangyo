// === 동기화 엔진 (로드맵 06, ADR-0003 A안) ===
// - 가져오기(pull): 서버 변경 목록을 커서로 받아 로컬에 반영. 로컬에 올리지 않은 변경이 있으면 덮어쓰지 않고 '충돌'로 보관
// - 올리기(push): 바뀐 메모를 baseVersion과 함께 저장. 기존 메모는 편집 임대 + fencing token 필요
// - 편집 임대: 편집을 시작할 때 받고, 주기적으로 갱신, 끝나면 반납. 다른 기기가 갖고 있으면 읽기 전용
// - 오프라인: 임대 없이 로컬 '초안'(dirty)으로만 저장 → 다시 연결되면 올리기 시도
// 화면(DOM)과 저장소에 직접 의존하지 않아 단위 테스트로 검증합니다 (engine.test.ts).
import type { Note, NoteFormat, NoteSync, ServerCopy } from '../domain/types';
import type { LocalProject } from '../domain/projects';

// === 0. 동기화 정책 변수 ===
/** 삭제 대기 중 다른 기기에서 고쳐져 삭제를 취소했을 때 메모에 남기는 안내 */
export const DELETE_CANCELLED = '지우려던 사이 다른 기기에서 고쳐져 지우지 않았습니다.';

export const SYNC_SETTINGS = Object.freeze({
  pushDebounceMs: 1_500, // 입력이 멈춘 뒤 올리기까지 대기
  leaseRenewFraction: 1 / 3, // 임대 유지 시간의 1/3마다 갱신
  leaseSafetyMs: 5_000, // 만료 이만큼 전이면 저장 전에 미리 갱신
  cursorMetaKey: 'syncCursor', // 마지막으로 받은 변경 커서 (계정별 기기 저장)
  // 프로젝트를 알게 된 뒤 처음 한 번은 처음부터 다시 받음: 예전 버전일 때 건너뛴 프로젝트와
  // (예전 클라이언트에 묘비로 보내져 지워진) 다른 프로젝트 메모를 되찾기 위해 (ADR-0005, 2026-11-13 사용자 확인)
  projectsAwareMetaKey: 'syncProjectsAware',
});

export interface ServerNote {
  id: string;
  title: string;
  folder: string;
  format: NoteFormat;
  content: string;
  version: number;
  createdAt: string;
  modifiedAt: string;
  deleted: boolean;
  changeSeq: number;
  /** 프로젝트 (null = 기본, ADR-0005). 예전 서버에는 없음 */
  projectId?: string | null;
}

/** 서버의 프로젝트 (ADR-0005) */
export interface ServerProject {
  id: string;
  name: string;
  createdAt: string;
  modifiedAt: string;
  deleted: boolean;
  changeSeq: number;
}

export interface LeaseGrant {
  fencingToken: number;
  expiresAt: string;
  ttlSeconds: number;
}

/** 서버 API (sync/api.ts가 실제 구현, 테스트는 가짜 구현) */
export interface SyncApi {
  /** projects=true: 프로젝트를 아는 클라이언트 (프로젝트 변경과 다른 프로젝트 메모도 받음) */
  changes(
    since: number,
    projects?: boolean,
  ): Promise<{
    notes: ServerNote[];
    projects?: ServerProject[];
    /** 기본 프로젝트 표시 이름 (projects=1일 때, 비어 있으면 화면 기본값) */
    defaultProjectName?: string;
    cursor: number;
    hasMore: boolean;
  }>;
  putProject?(id: string, name: string): Promise<ServerProject>;
  deleteProject?(id: string): Promise<ServerProject>;
  get(id: string): Promise<ServerNote>;
  put(id: string, body: Record<string, unknown>): Promise<ServerNote>;
  /** takeover: 다른 기기가 편집 중이어도 편집 권한을 가져옴 (2026-11-14 사용자 승인) */
  acquireLease(id: string, clientId: string, label: string, takeover?: boolean): Promise<LeaseGrant>;
  releaseLease(id: string, clientId: string, keepalive?: boolean): Promise<void>;
  /** 서버에서 삭제 (묘비). 임대·fencing token 필요 */
  remove(id: string, baseVersion: number, clientId: string, fencingToken: number | null): Promise<ServerNote>;
}

/** 앱 밖에서 지운 메모를 서버에서도 지운 결과 */
export type RemoteDeleteResult =
  | 'deleted' // 서버에서도 삭제됨
  | 'gone' // 이미 서버에 없음
  | 'changed' // 지운 뒤 다른 기기에서 바뀜 → 삭제하지 않음 (다시 받기 권장)
  | 'busy'; // 다른 기기가 편집 중 → 나중에 다시

/** 화면 상태 + 저장소 연결 (app.ts가 구현) */
export interface SyncStore {
  list(): Note[];
  save(note: Note): Promise<void>;
  add(note: Note): Promise<void>;
  remove(id: string): Promise<void>;
  getMeta<T>(key: string): Promise<T | undefined>;
  setMeta(key: string, value: unknown): Promise<void>;
  /** 이 메모들이 바뀌었으니 다시 그리기 */
  changed(ids: string[]): void;
  /** 프로젝트를 쓰는 화면만 (웹). 없으면 예전 방식(기본 프로젝트만) */
  projects?: ProjectStore;
}

/** 프로젝트 목록 저장 (app.ts가 구현) */
export interface ProjectStore {
  list(): LocalProject[];
  save(project: LocalProject): Promise<void>;
  remove(id: string): Promise<void>;
  /** 목록이 바뀌었으니 다시 그리기 */
  changed(): void;
  /** 서버의 기본 프로젝트 표시 이름 반영 */
  setDefaultName?(name: string): Promise<void>;
}

/** 서버 오류 분류 (sync/api.ts가 ApiError/ServerUnavailable을 이 모양으로 전달) */
export interface SyncError {
  status?: number;
  code?: string;
  message: string;
  details?: Record<string, unknown>;
  offline?: boolean;
}
export function asSyncError(e: unknown): SyncError {
  if (e && typeof e === 'object' && 'message' in e) return e as SyncError;
  return { message: String(e) };
}

export type LeaseState =
  | { kind: 'not-needed' } // 아직 서버에 없는 메모, 동기화 꺼짐
  | { kind: 'held'; expiresAt: string }
  | { kind: 'denied'; holderLabel: string; expiresAt: string; message: string }
  | { kind: 'offline' }; // 서버 연결 불가 → 초안으로 편집

export type SyncPhase = 'idle' | 'syncing' | 'offline' | 'error';
export interface SyncStatus {
  phase: SyncPhase;
  pending: number; // 올리지 않은 메모 수
  conflicts: number;
  message?: string;
}

export type ConflictChoice = 'mine' | 'server' | 'both';

interface HeldLease {
  token: number;
  expiresAt: number;
  ttlMs: number;
  timer?: ReturnType<typeof setInterval>;
  editing: boolean; // 화면에서 편집 중 (false = 올리기용 임시 임대)
}

const snapshotOf = (n: { title: string; folder: string; format: NoteFormat; content: string }) =>
  JSON.stringify([n.title, n.folder, n.format, n.content]);

export function serverCopy(s: ServerNote): ServerCopy {
  return {
    version: s.version,
    title: s.title,
    folder: s.folder,
    format: s.format,
    content: s.content,
    modifiedAt: s.modifiedAt,
    deleted: s.deleted,
  };
}

export class SyncEngine {
  private cursor = 0;
  private leases = new Map<string, HeldLease>();
  private running: Promise<void> | null = null;
  private pushTimer: ReturnType<typeof setTimeout> | undefined;
  private stopped = false;
  private resyncing = false;
  private lastStatus: SyncStatus = { phase: 'idle', pending: 0, conflicts: 0 };

  constructor(
    private readonly api: SyncApi,
    private readonly store: SyncStore,
    private readonly opts: {
      clientId: string;
      label: string;
      onStatus?: (s: SyncStatus) => void;
      onLeaseLost?: (noteId: string, message: string) => void;
      /** 올리지 않을 '손대지 않은 기본 문서' (처음 실행 사용법). 서버에 같은 ID가 있으면 서버 것으로 맞춤 */
      pristine?: (note: Note) => boolean;
    },
  ) {}

  async init(): Promise<void> {
    this.cursor = (await this.store.getMeta<number>(SYNC_SETTINGS.cursorMetaKey)) ?? 0;
    if (this.store.projects && !(await this.store.getMeta<boolean>(SYNC_SETTINGS.projectsAwareMetaKey))) {
      this.cursor = 0; // 이미 받은 메모는 버전이 같아 다시 쓰지 않음
      this.resyncing = true;
    }
  }

  status(): SyncStatus {
    return this.lastStatus;
  }

  /** 진행 중인 동기화가 끝날 때까지 기다림 (디스크 다시 읽기와 겹치지 않게) */
  idle(): Promise<void> {
    return this.running ?? Promise.resolve();
  }

  /** 처음 계정에 연결할 때: 아직 올리지 않은 로컬 메모를 모두 올릴 대상으로 표시 */
  async adoptLocalNotes(): Promise<number> {
    const local = this.store.list().filter((n) => !n.sync && !this.opts.pristine?.(n));
    for (const note of local) {
      note.sync = { version: 0, dirty: true };
      await this.store.save(note);
    }
    return local.length;
  }

  /** 가져오기 → 올리기. 동시에 여러 번 불려도 한 번만 실행 */
  syncOnce(): Promise<void> {
    if (this.stopped) return Promise.resolve();
    this.running ??= this.run().finally(() => {
      this.running = null;
    });
    return this.running;
  }

  private async run(): Promise<void> {
    this.report('syncing');
    try {
      await this.pull();
      await this.push();
      this.report('idle');
    } catch (e) {
      const err = asSyncError(e);
      this.report(err.offline ? 'offline' : 'error', err.offline ? undefined : err.message);
    }
  }

  private report(phase: SyncPhase, message?: string) {
    const notes = this.store.list();
    this.lastStatus = {
      phase,
      pending:
        notes.filter((n) => n.sync?.dirty || n.sync?.deleting).length +
        (this.store.projects?.list().filter((p) => p.sync.dirty || p.sync.deleting).length ?? 0),
      conflicts: notes.filter((n) => n.sync?.conflict).length,
      message,
    };
    this.opts.onStatus?.(this.lastStatus);
  }

  async pull(): Promise<void> {
    for (;;) {
      const projects = this.store.projects;
      const page = await this.api.changes(this.cursor, !!projects);
      // 프로젝트를 먼저: 메모가 가리키는 프로젝트가 화면에 있게
      if (projects) for (const p of page.projects ?? []) await this.applyProject(projects, p);
      for (const server of page.notes) await this.apply(server);
      this.cursor = page.cursor;
      await this.store.setMeta(SYNC_SETTINGS.cursorMetaKey, this.cursor);
      if (page.defaultProjectName !== undefined && projects?.setDefaultName)
        await projects.setDefaultName(page.defaultProjectName);
      if (!page.hasMore) break;
    }
    if (this.resyncing) {
      this.resyncing = false;
      await this.store.setMeta(SYNC_SETTINGS.projectsAwareMetaKey, true);
    }
  }

  private async apply(server: ServerNote): Promise<void> {
    const local = this.store.list().find((n) => n.id === server.id);
    if (!local) {
      if (server.deleted) return;
      await this.store.add({
        id: server.id,
        title: server.title,
        folder: server.folder,
        format: server.format,
        content: server.content,
        createdAt: server.createdAt,
        modifiedAt: server.modifiedAt,
        sessions: [],
        undo: [],
        redo: [],
        sync: { version: server.version, dirty: false },
        ...(this.store.projects ? { projectId: server.projectId ?? null } : {}),
      });
      this.store.changed([server.id]);
      return;
    }
    // 손대지 않은 기본 문서는 서버 것으로 그냥 맞춤 (충돌로 보지 않음)
    const sync: NoteSync = local.sync ?? { version: 0, dirty: !this.opts.pristine?.(local) };
    if (server.version <= sync.version) return; // 이미 반영됨 (내가 올린 결과 등)
    // 편집 권한을 갖고 있는데 다른 기기가 새 버전을 저장함 = 권한을 빼앗김(여기서 편집하기) → 바로 읽기 전용으로
    if (this.leases.has(local.id) && !server.deleted) {
      this.dropLease(local.id);
      this.opts.onLeaseLost?.(local.id, 'lease_taken');
    }
    if (sync.deleting && !server.deleted) {
      // 지우려던 사이 다른 기기에서 고쳐짐 → 지우지 않고 최신 내용을 받음
      local.sync = { version: sync.version, dirty: false, error: DELETE_CANCELLED };
      sync.deleting = false;
      sync.dirty = false;
    }
    if (sync.dirty) {
      if (!server.deleted && snapshotOf(local) === snapshotOf(server)) {
        local.sync = { version: server.version, dirty: false }; // 결과가 같으면 충돌 아님
      } else {
        local.sync = { ...sync, conflict: serverCopy(server) }; // 덮어쓰지 않고 보관 → 사용자 선택
      }
      await this.store.save(local);
      this.store.changed([local.id]);
      return;
    }
    if (server.deleted) {
      await this.store.remove(local.id);
      this.store.changed([local.id]);
      return;
    }
    Object.assign(local, {
      title: server.title,
      folder: server.folder,
      format: server.format,
      content: server.content,
      modifiedAt: server.modifiedAt,
      ...(this.store.projects ? { projectId: server.projectId ?? null } : {}),
    });
    local.sync = { version: server.version, dirty: false };
    await this.store.save(local);
    this.store.changed([local.id]);
  }

  /** 서버의 프로젝트 변경을 반영 (ADR-0005) */
  private async applyProject(store: ProjectStore, server: ServerProject): Promise<void> {
    const local = store.list().find((p) => p.id === server.id);
    if (server.deleted) {
      if (!local) return;
      await store.remove(server.id);
      // 서버는 빈 프로젝트만 지우므로 남은 메모는 이 기기에서 오프라인으로 만든 것 → 기본으로 옮겨 잃지 않음
      const orphans = this.store.list().filter((n) => n.projectId === server.id);
      for (const note of orphans) {
        note.projectId = null;
        this.markDirty(note);
        await this.store.save(note);
      }
      store.changed();
      if (orphans.length) this.store.changed(orphans.map((n) => n.id));
      return;
    }
    if (local?.sync.dirty || local?.sync.deleting) return; // 내 변경(이름 바꾸기·지우기)을 올릴 때까지 유지
    await store.save({
      id: server.id,
      name: server.name,
      createdAt: server.createdAt,
      modifiedAt: server.modifiedAt,
      sync: { synced: true, dirty: false },
    });
    store.changed();
  }

  /**
   * 프로젝트 변경을 서버에. 순서: 만들기·이름 바꾸기(메모보다 먼저 — 메모가 새 프로젝트를 가리킬 수 있음)
   * → 메모 → 지우기(메모를 다른 프로젝트로 옮긴 뒤여야 빈 프로젝트)
   */
  private async pushProjects(store: ProjectStore, phase: 'upsert' | 'delete'): Promise<void> {
    if (!this.api.putProject || !this.api.deleteProject) return;
    // 안의 메모가 아직 삭제 대기 중인 프로젝트는 다음에 (그 메모가 다 지워져야 서버가 지우기를 받아 줌)
    const waiting = new Set(this.store.list().filter((n) => n.sync?.deleting && n.projectId).map((n) => n.projectId));
    const targets = store
      .list()
      .filter((p) =>
        phase === 'delete' ? p.sync.deleting && !waiting.has(p.id) : p.sync.dirty && !p.sync.deleting,
      );
    for (const project of targets) {
      try {
        if (project.sync.deleting) {
          if (project.sync.synced) await this.api.deleteProject(project.id);
          await store.remove(project.id);
        } else {
          await this.api.putProject(project.id, project.name);
          project.sync = { synced: true, dirty: false };
          await store.save(project);
        }
      } catch (e) {
        const err = asSyncError(e);
        if (err.offline) throw e;
        if (err.code === 'project_not_found' && project.sync.deleting) {
          await store.remove(project.id); // 이미 지워짐
        } else if (err.code === 'project_not_empty') {
          // 다른 기기가 메모를 넣음 → 지우지 않고 되살림 (그 메모는 다음 가져오기에 옴)
          project.sync = { ...project.sync, deleting: false, error: err.message };
          await store.save(project);
        } else {
          project.sync = { ...project.sync, error: err.message };
          await store.save(project);
        }
      }
      store.changed();
    }
  }

  async push(): Promise<void> {
    const projects = this.store.projects;
    if (projects) await this.pushProjects(projects, 'upsert');
    const targets = this.store
      .list()
      .filter((n) => n.sync?.dirty && !n.sync.deleting && !n.sync.conflict && !n.sync.error);
    for (const note of targets) await this.pushNote(note);
    for (const note of this.store.list().filter((n) => n.sync?.deleting)) await this.pushDelete(note);
    if (projects) await this.pushProjects(projects, 'delete');
  }

  private async pushNote(note: Note): Promise<void> {
    const base = note.sync?.version ?? 0;
    let token: number | undefined;
    const temporaryLease = base > 0 && !this.leases.get(note.id)?.editing;
    if (base > 0) {
      const lease = await this.ensureLease(note.id, false);
      if (lease.kind !== 'held') return; // 다른 기기가 편집 중 → 초안 유지, 다음에 다시
      token = this.leases.get(note.id)?.token;
    }
    const sent = snapshotOf(note);
    try {
      const saved = await this.api.put(note.id, {
        baseVersion: base,
        clientId: this.opts.clientId,
        fencingToken: token ?? null,
        title: note.title,
        folder: note.folder,
        format: note.format,
        content: note.content,
        createdAt: note.createdAt,
        modifiedAt: note.modifiedAt,
        // 프로젝트를 모르는 화면(데스크톱 3단계 전)은 보내지 않음 → 서버가 지금 프로젝트 유지
        ...(this.store.projects ? { projectId: note.projectId ?? null } : {}),
      });
      // 올리는 동안 또 편집했으면 계속 dirty
      note.sync = { version: saved.version, dirty: snapshotOf(note) !== sent };
      // 서버가 제목을 정리했으면 (예: 예약어) 반영
      if (!note.sync.dirty && (saved.title !== note.title || saved.folder !== note.folder)) {
        note.title = saved.title;
        note.folder = saved.folder;
      }
    } catch (e) {
      const err = asSyncError(e);
      if (err.offline) throw e;
      if (err.code === 'version_conflict') {
        const current = await this.api.get(note.id).catch(() => null);
        note.sync = {
          version: base,
          dirty: true,
          conflict: current
            ? serverCopy(current)
            : { version: Number(err.details?.currentVersion ?? 0), title: note.title, folder: note.folder,
                format: note.format, content: '', modifiedAt: note.modifiedAt, deleted: true },
        };
      } else if (err.status === 423) {
        this.dropLease(note.id);
        return; // 임대를 잃음 → 초안 유지
      } else {
        note.sync = { ...(note.sync ?? { version: base }), dirty: true, error: err.message };
      }
    } finally {
      if (temporaryLease) await this.release(note.id);
    }
    await this.store.save(note);
    this.store.changed([note.id]);
  }

  /** 삭제 대기 메모를 서버에서 지움. 다른 기기가 열고 있으면 다음 동기화 때 다시 (2026-11-13 사용자 확인) */
  private async pushDelete(note: Note): Promise<void> {
    const sync = note.sync!;
    if (sync.version <= 0) {
      await this.store.remove(note.id); // 서버에 없음 → 이 기기에서만
      this.store.changed([note.id]);
      return;
    }
    const result = await this.deleteRemote(note.id, sync.version);
    if (result === 'deleted' || result === 'gone') {
      await this.store.remove(note.id);
    } else if (result === 'changed') {
      note.sync = { version: sync.version, dirty: false, error: DELETE_CANCELLED };
      await this.store.save(note);
    } else {
      return; // busy: 다른 기기가 편집 중 → 대기 유지
    }
    this.store.changed([note.id]);
  }

  /** 입력할 때마다: 올리기 대상으로 표시하고 잠시 뒤 올리기 */
  markDirty(note: Note): void {
    note.sync = { version: note.sync?.version ?? 0, dirty: true, conflict: note.sync?.conflict ?? null };
    // 상태 표시를 바로 '올릴 메모 n'으로 (올리기 전에 '동기화됨'으로 보이지 않게)
    const phase = this.lastStatus.phase === 'error' ? 'idle' : this.lastStatus.phase;
    this.report(phase, this.lastStatus.message);
    clearTimeout(this.pushTimer);
    this.pushTimer = setTimeout(() => void this.syncOnce(), SYNC_SETTINGS.pushDebounceMs);
  }

  /** 편집 시작 (에디터에 들어갈 때). 다른 기기가 편집 중이면 denied → 읽기 전용 */
  async beginEdit(note: Note): Promise<LeaseState> {
    if (!note.sync || note.sync.version === 0) return { kind: 'not-needed' };
    return this.ensureLease(note.id, true);
  }

  /**
   * 편집 권한 강제로 가져오기 (2026-11-14 사용자 승인, ADR-0003 보완): 서버가 새 fencing token을 줌 →
   * 지금 편집 중이던 기기는 다음 갱신·저장 때 거절되어 읽기 전용이 되고, 그 기기의 올리지 못한 내용은 충돌 검토로 보관됨.
   * 가져오기 전에 서버의 최신 내용을 먼저 받음
   */
  async takeOver(note: Note): Promise<LeaseState> {
    if (!note.sync || note.sync.version === 0) return { kind: 'not-needed' };
    await this.refreshNote(note.id);
    try {
      const grant = await this.api.acquireLease(note.id, this.opts.clientId, this.opts.label, true);
      const held = this.leases.get(note.id);
      this.leases.set(note.id, {
        token: grant.fencingToken,
        expiresAt: Date.parse(grant.expiresAt),
        ttlMs: grant.ttlSeconds * 1000,
        editing: true,
        timer: held?.timer,
      });
      this.startRenew(note.id);
      return { kind: 'held', expiresAt: grant.expiresAt };
    } catch (e) {
      const err = asSyncError(e);
      if (err.offline) return { kind: 'offline' };
      if (err.status === 404) return { kind: 'not-needed' };
      throw e;
    }
  }

  /** 이 메모 하나의 최신 내용을 서버에서 바로 받음 (20초 주기를 기다리지 않고). 내 변경이 있으면 지금처럼 충돌 검토 */
  async refreshNote(noteId: string): Promise<void> {
    try {
      const server = await this.api.get(noteId);
      await this.apply(server);
    } catch (e) {
      const err = asSyncError(e);
      if (err.offline) throw e;
      if (err.status !== 404) throw e;
    }
  }

  /** 편집 끝 (다른 메모로 이동, 창 숨김, 오래 입력 없음) */
  async endEdit(noteId: string, keepalive = false): Promise<void> {
    if (this.leases.has(noteId)) await this.release(noteId, keepalive);
  }

  isHolding(noteId: string): boolean {
    const l = this.leases.get(noteId);
    return !!l && l.expiresAt > Date.now();
  }

  private async ensureLease(noteId: string, editing: boolean): Promise<LeaseState> {
    const held = this.leases.get(noteId);
    if (held && held.expiresAt - Date.now() > SYNC_SETTINGS.leaseSafetyMs) {
      if (editing && !held.editing) {
        held.editing = true;
        this.startRenew(noteId);
      }
      return { kind: 'held', expiresAt: new Date(held.expiresAt).toISOString() };
    }
    try {
      const grant = await this.api.acquireLease(noteId, this.opts.clientId, this.opts.label);
      const lease: HeldLease = {
        token: grant.fencingToken,
        expiresAt: Date.parse(grant.expiresAt),
        ttlMs: grant.ttlSeconds * 1000,
        editing: editing || !!held?.editing,
        timer: held?.timer,
      };
      this.leases.set(noteId, lease);
      if (lease.editing) this.startRenew(noteId);
      return { kind: 'held', expiresAt: grant.expiresAt };
    } catch (e) {
      const err = asSyncError(e);
      if (err.offline) return { kind: 'offline' };
      if (err.code === 'lease_held') {
        this.dropLease(noteId);
        return {
          kind: 'denied',
          holderLabel: String(err.details?.holderLabel ?? ''),
          expiresAt: String(err.details?.expiresAt ?? ''),
          message: err.message,
        };
      }
      if (err.status === 404) return { kind: 'not-needed' }; // 서버에서 삭제됨 → 다음 동기화에서 정리
      throw e;
    }
  }

  private startRenew(noteId: string) {
    const lease = this.leases.get(noteId);
    if (!lease || lease.timer) return;
    const every = Math.max(1_000, lease.ttlMs * SYNC_SETTINGS.leaseRenewFraction);
    lease.timer = setInterval(async () => {
      try {
        const grant = await this.api.acquireLease(noteId, this.opts.clientId, this.opts.label);
        const current = this.leases.get(noteId);
        if (current) current.expiresAt = Date.parse(grant.expiresAt);
      } catch (e) {
        const err = asSyncError(e);
        if (err.offline) return; // 연결이 돌아오면 다시 시도. 그 사이 만료되면 저장 때 다시 받음
        this.dropLease(noteId);
        this.opts.onLeaseLost?.(noteId, err.message);
      }
    }, every);
  }

  private dropLease(noteId: string) {
    const lease = this.leases.get(noteId);
    if (lease?.timer) clearInterval(lease.timer);
    this.leases.delete(noteId);
  }

  private async release(noteId: string, keepalive = false) {
    this.dropLease(noteId);
    await this.api.releaseLease(noteId, this.opts.clientId, keepalive).catch(() => undefined);
  }

  /** 충돌 해결: 내 것으로 덮어쓰기 / 서버 것 사용 / 둘 다 보관(내 것은 새 메모로). 'both'는 새 메모를 돌려줌 */
  async resolveConflict(note: Note, choice: ConflictChoice): Promise<Note | null> {
    const server = note.sync?.conflict;
    if (!server) return null;
    let copy: Note | null = null;
    if (choice === 'mine') {
      if (server.deleted) {
        // 서버에서 지워진 메모는 같은 ID로 되살릴 수 없으므로 새 메모로 올림
        copy = this.copyOf(note, note.title);
        await this.store.add(copy);
        await this.store.remove(note.id);
      } else {
        note.sync = { version: server.version, dirty: true, conflict: null };
        await this.store.save(note);
      }
    } else {
      if (choice === 'both') {
        copy = this.copyOf(note, `${note.title} (내 사본)`);
        await this.store.add(copy);
      }
      if (server.deleted) {
        await this.store.remove(note.id);
      } else {
        Object.assign(note, {
          title: server.title,
          folder: server.folder,
          format: server.format,
          content: server.content,
          modifiedAt: server.modifiedAt,
        });
        note.sync = { version: server.version, dirty: false, conflict: null };
        await this.store.save(note);
      }
    }
    this.store.changed([note.id, ...(copy ? [copy.id] : [])]);
    void this.syncOnce();
    return copy;
  }

  private copyOf(note: Note, title: string): Note {
    const now = new Date().toISOString();
    return {
      ...structuredClone(note),
      id: crypto.randomUUID(),
      title,
      createdAt: now,
      modifiedAt: now,
      sessions: [],
      undo: [],
      redo: [],
      sync: { version: 0, dirty: true },
    };
  }

  /**
   * 앱 밖에서 지운 메모를 서버에서도 삭제 (데스크톱, ADR-0004). baseVersion은 지우기 직전의 서버 버전.
   * 그 사이 다른 기기에서 바뀌었으면 지우지 않고 'changed'.
   */
  async deleteRemote(id: string, baseVersion: number): Promise<RemoteDeleteResult> {
    const lease = await this.ensureLease(id, false);
    if (lease.kind === 'offline') throw { offline: true, message: 'offline' } satisfies SyncError;
    if (lease.kind === 'denied') return 'busy';
    if (lease.kind === 'not-needed') return 'gone';
    try {
      await this.api.remove(id, baseVersion, this.opts.clientId, this.leases.get(id)?.token ?? null);
      return 'deleted';
    } catch (e) {
      const err = asSyncError(e);
      if (err.offline) throw e;
      if (err.status === 404) return 'gone';
      if (err.code === 'version_conflict') return err.details?.deleted ? 'gone' : 'changed';
      throw e;
    } finally {
      await this.release(id);
    }
  }

  /** 서버의 현재 내용으로 다시 받기 (바깥에서 지운 메모 되살리기). 서버에도 없으면 false */
  async restoreRemote(id: string): Promise<boolean> {
    let server: ServerNote;
    try {
      server = await this.api.get(id);
    } catch (e) {
      const err = asSyncError(e);
      if (err.status === 404) return false;
      throw e;
    }
    if (server.deleted) return false;
    if (this.store.list().some((n) => n.id === id)) return true;
    await this.apply(server);
    return true;
  }

  /** 로그아웃·계정 전환: 임대를 모두 반납하고 멈춤 */
  async stop(keepalive = false): Promise<void> {
    this.stopped = true;
    clearTimeout(this.pushTimer);
    await Promise.all([...this.leases.keys()].map((id) => this.release(id, keepalive)));
  }
}
