// === 동기화 엔진 (로드맵 06, ADR-0003 A안) ===
// - 가져오기(pull): 서버 변경 목록을 커서로 받아 로컬에 반영. 로컬에 올리지 않은 변경이 있으면 덮어쓰지 않고 '충돌'로 보관
// - 올리기(push): 바뀐 메모를 baseVersion과 함께 저장. 기존 메모는 편집 임대 + fencing token 필요
// - 편집 임대: 편집을 시작할 때 받고, 주기적으로 갱신, 끝나면 반납. 다른 기기가 갖고 있으면 읽기 전용
// - 오프라인: 임대 없이 로컬 '초안'(dirty)으로만 저장 → 다시 연결되면 올리기 시도
// 화면(DOM)과 저장소에 직접 의존하지 않아 단위 테스트로 검증합니다 (engine.test.ts).
import type { Note, NoteFormat, NoteSync, ServerCopy } from '../domain/types';

// === 0. 동기화 정책 변수 ===
export const SYNC_SETTINGS = Object.freeze({
  pushDebounceMs: 1_500, // 입력이 멈춘 뒤 올리기까지 대기
  leaseRenewFraction: 1 / 3, // 임대 유지 시간의 1/3마다 갱신
  leaseSafetyMs: 5_000, // 만료 이만큼 전이면 저장 전에 미리 갱신
  cursorMetaKey: 'syncCursor', // 마지막으로 받은 변경 커서 (계정별 기기 저장)
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
}

export interface LeaseGrant {
  fencingToken: number;
  expiresAt: string;
  ttlSeconds: number;
}

/** 서버 API (sync/api.ts가 실제 구현, 테스트는 가짜 구현) */
export interface SyncApi {
  changes(since: number): Promise<{ notes: ServerNote[]; cursor: number; hasMore: boolean }>;
  get(id: string): Promise<ServerNote>;
  put(id: string, body: Record<string, unknown>): Promise<ServerNote>;
  acquireLease(id: string, clientId: string, label: string): Promise<LeaseGrant>;
  releaseLease(id: string, clientId: string, keepalive?: boolean): Promise<void>;
}

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
  private lastStatus: SyncStatus = { phase: 'idle', pending: 0, conflicts: 0 };

  constructor(
    private readonly api: SyncApi,
    private readonly store: SyncStore,
    private readonly opts: {
      clientId: string;
      label: string;
      onStatus?: (s: SyncStatus) => void;
      onLeaseLost?: (noteId: string, message: string) => void;
    },
  ) {}

  async init(): Promise<void> {
    this.cursor = (await this.store.getMeta<number>(SYNC_SETTINGS.cursorMetaKey)) ?? 0;
  }

  status(): SyncStatus {
    return this.lastStatus;
  }

  /** 처음 계정에 연결할 때: 아직 올리지 않은 로컬 메모를 모두 올릴 대상으로 표시 */
  async adoptLocalNotes(): Promise<number> {
    const local = this.store.list().filter((n) => !n.sync);
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
      pending: notes.filter((n) => n.sync?.dirty).length,
      conflicts: notes.filter((n) => n.sync?.conflict).length,
      message,
    };
    this.opts.onStatus?.(this.lastStatus);
  }

  async pull(): Promise<void> {
    for (;;) {
      const page = await this.api.changes(this.cursor);
      for (const server of page.notes) await this.apply(server);
      this.cursor = page.cursor;
      await this.store.setMeta(SYNC_SETTINGS.cursorMetaKey, this.cursor);
      if (!page.hasMore) break;
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
      });
      this.store.changed([server.id]);
      return;
    }
    const sync: NoteSync = local.sync ?? { version: 0, dirty: true };
    if (server.version <= sync.version) return; // 이미 반영됨 (내가 올린 결과 등)
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
    });
    local.sync = { version: server.version, dirty: false };
    await this.store.save(local);
    this.store.changed([local.id]);
  }

  async push(): Promise<void> {
    const targets = this.store.list().filter((n) => n.sync?.dirty && !n.sync.conflict && !n.sync.error);
    for (const note of targets) await this.pushNote(note);
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

  /** 입력할 때마다: 올리기 대상으로 표시하고 잠시 뒤 올리기 */
  markDirty(note: Note): void {
    note.sync = { version: note.sync?.version ?? 0, dirty: true, conflict: note.sync?.conflict ?? null };
    clearTimeout(this.pushTimer);
    this.pushTimer = setTimeout(() => void this.syncOnce(), SYNC_SETTINGS.pushDebounceMs);
  }

  /** 편집 시작 (에디터에 들어갈 때). 다른 기기가 편집 중이면 denied → 읽기 전용 */
  async beginEdit(note: Note): Promise<LeaseState> {
    if (!note.sync || note.sync.version === 0) return { kind: 'not-needed' };
    return this.ensureLease(note.id, true);
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

  /** 로그아웃·계정 전환: 임대를 모두 반납하고 멈춤 */
  async stop(keepalive = false): Promise<void> {
    this.stopped = true;
    clearTimeout(this.pushTimer);
    await Promise.all([...this.leases.keys()].map((id) => this.release(id, keepalive)));
  }
}
