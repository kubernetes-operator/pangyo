// === 동기화 엔진 단위 테스트: 서버와 같은 규칙의 메모리 가짜 서버 + 두 기기(엔진 2개) ===
// 실제 서버 규칙은 crates/server/tests/notes.rs, 브라우저 전체 흐름은 e2e/sync.spec.ts 가 검사합니다.
import { afterEach, describe, expect, it } from 'vitest';
import type { Note } from '../domain/types';
import { SyncEngine, type LeaseGrant, type ServerNote, type SyncApi, type SyncError, type SyncStore } from './engine';

const LEASE_TTL = 600; // 테스트 중 자동 갱신이 돌지 않도록 길게

class FakeServer {
  notes = new Map<string, ServerNote>();
  leases = new Map<string, { holder: string; label: string; token: number; expires: number }>();
  epochs = new Map<string, number>();
  seq = 0;
  offline = false;

  private fail(status: number, code: string, message: string, details?: Record<string, unknown>): never {
    throw { status, code, message, details } satisfies SyncError;
  }
  private guard() {
    if (this.offline) throw { offline: true, message: 'offline' } satisfies SyncError;
  }
  private leaseOk(id: string, holder: string, token: unknown) {
    const l = this.leases.get(id);
    if (l && l.expires > Date.now() && l.holder === holder && l.token === token) return;
    if (l && l.expires > Date.now() && l.holder !== holder) this.fail(423, 'lease_held', `${l.label}에서 편집 중`, { holderLabel: l.label });
    this.fail(423, 'lease_required', '임대 필요');
  }

  api(): SyncApi {
    return {
      changes: async (since) => {
        this.guard();
        const notes = [...this.notes.values()].filter((n) => n.changeSeq > since).sort((a, b) => a.changeSeq - b.changeSeq);
        return { notes: structuredClone(notes), cursor: notes.at(-1)?.changeSeq ?? since, hasMore: false };
      },
      get: async (id) => {
        this.guard();
        const n = this.notes.get(id);
        if (!n) this.fail(404, 'note_not_found', 'not found');
        return structuredClone(n);
      },
      put: async (id, body) => {
        this.guard();
        const current = this.notes.get(id);
        if (current) {
          this.leaseOk(id, String(body.clientId), body.fencingToken);
          if (body.baseVersion !== current.version || current.deleted)
            this.fail(409, 'version_conflict', '충돌', { currentVersion: current.version });
        } else if (body.baseVersion !== 0) this.fail(409, 'version_conflict', '충돌');
        const next: ServerNote = {
          id,
          title: String(body.title),
          folder: String(body.folder),
          format: body.format as 'md' | 'txt',
          content: String(body.content),
          version: (current?.version ?? 0) + 1,
          createdAt: String(body.createdAt),
          modifiedAt: String(body.modifiedAt),
          deleted: false,
          changeSeq: ++this.seq,
        };
        this.notes.set(id, next);
        return structuredClone(next);
      },
      acquireLease: async (id, clientId, label): Promise<LeaseGrant> => {
        this.guard();
        if (!this.notes.get(id) || this.notes.get(id)!.deleted) this.fail(404, 'note_not_found', 'not found');
        const l = this.leases.get(id);
        if (l && l.expires > Date.now() && l.holder !== clientId)
          this.fail(423, 'lease_held', `${l.label}에서 편집 중입니다`, { holderLabel: l.label, expiresAt: new Date(l.expires).toISOString() });
        const token = l && l.expires > Date.now() ? l.token : (this.epochs.get(id) ?? 0) + 1;
        this.epochs.set(id, token);
        const expires = Date.now() + LEASE_TTL * 1000;
        this.leases.set(id, { holder: clientId, label, token, expires });
        return { fencingToken: token, expiresAt: new Date(expires).toISOString(), ttlSeconds: LEASE_TTL };
      },
      releaseLease: async (id, clientId) => {
        this.guard();
        if (this.leases.get(id)?.holder === clientId) this.leases.delete(id);
      },
      remove: async (id, baseVersion, clientId, fencingToken) => {
        this.guard();
        const current = this.notes.get(id);
        if (!current) this.fail(404, 'note_not_found', 'not found');
        if (current.deleted) this.fail(409, 'version_conflict', '삭제됨', { currentVersion: current.version, deleted: true });
        this.leaseOk(id, clientId, fencingToken);
        if (baseVersion !== current.version)
          this.fail(409, 'version_conflict', '충돌', { currentVersion: current.version, deleted: false });
        this.deleteOnServer(id);
        this.leases.delete(id);
        return structuredClone(this.notes.get(id)!);
      },
    };
  }

  deleteOnServer(id: string) {
    const n = this.notes.get(id)!;
    this.notes.set(id, { ...n, deleted: true, content: '', version: n.version + 1, changeSeq: ++this.seq });
  }
}

class MemoryStore implements SyncStore {
  notes: Note[] = [];
  meta = new Map<string, unknown>();
  list = () => this.notes;
  save = async () => undefined;
  add = async (n: Note) => void this.notes.push(n);
  remove = async (id: string) => void (this.notes = this.notes.filter((n) => n.id !== id));
  getMeta = async <T,>(k: string) => this.meta.get(k) as T | undefined;
  setMeta = async (k: string, v: unknown) => void this.meta.set(k, v);
  changed = () => undefined;
  find = (id: string) => this.notes.find((n) => n.id === id)!;
}

function note(title: string, content: string): Note {
  const t = '2026-11-09T00:00:00.000Z';
  return { id: crypto.randomUUID(), title, folder: '', format: 'md', content, createdAt: t, modifiedAt: t, sessions: [], undo: [], redo: [] };
}

const engines: SyncEngine[] = [];
async function device(server: FakeServer, clientId: string, label: string) {
  const store = new MemoryStore();
  const engine = new SyncEngine(server.api(), store, { clientId, label });
  await engine.init();
  engines.push(engine);
  return { store, engine };
}
afterEach(async () => {
  await Promise.all(engines.splice(0).map((e) => e.stop()));
});

/** 편집: 화면에서 입력할 때와 같은 순서 (내용 변경 → markDirty) */
function edit(engine: SyncEngine, n: Note, content: string) {
  n.content = content;
  engine.markDirty(n);
}

describe('동기화 엔진 (ADR-0003 A안)', () => {
  it('처음 연결하면 로컬 메모를 올리고, 다른 기기가 받아감', async () => {
    const server = new FakeServer();
    const a = await device(server, 'laptop', '노트북');
    a.store.notes.push(note('회의', '안건'));
    expect(await a.engine.adoptLocalNotes()).toBe(1);
    a.engine.markDirty(a.store.notes[0]!);
    expect(a.engine.status().pending).toBe(1); // 올리기 전에는 '대기'로 표시
    await a.engine.syncOnce();
    expect(a.engine.status()).toMatchObject({ phase: 'idle', pending: 0 });
    const b = await device(server, 'phone', '휴대폰');
    await b.engine.syncOnce();
    expect(b.store.notes.map((n) => [n.title, n.content, n.sync?.version])).toEqual([['회의', '안건', 1]]);
  });

  it('편집 중인 메모는 다른 기기에서 읽기 전용(denied), 반납 후 편집 가능', async () => {
    const server = new FakeServer();
    const a = await device(server, 'laptop', '노트북');
    a.store.notes.push(note('공유', 'v1'));
    await a.engine.adoptLocalNotes();
    await a.engine.syncOnce();
    const b = await device(server, 'phone', '휴대폰');
    await b.engine.syncOnce();

    const n = a.store.notes[0]!;
    expect((await a.engine.beginEdit(n)).kind).toBe('held');
    const denied = await b.engine.beginEdit(b.store.notes[0]!);
    expect(denied).toMatchObject({ kind: 'denied', holderLabel: '노트북' });

    edit(a.engine, n, 'v2 노트북');
    await a.engine.syncOnce();
    await b.engine.syncOnce();
    expect(b.store.notes[0]!.content).toBe('v2 노트북'); // 읽기 전용 화면도 최신 내용으로

    await a.engine.endEdit(n.id);
    expect((await b.engine.beginEdit(b.store.notes[0]!)).kind).toBe('held');
  });

  it('오프라인 초안과 서버 변경이 겹치면 덮어쓰지 않고 충돌로 보관 → 둘 다 보관', async () => {
    const server = new FakeServer();
    const a = await device(server, 'laptop', '노트북');
    a.store.notes.push(note('계획', 'v1'));
    await a.engine.adoptLocalNotes();
    await a.engine.syncOnce();
    const b = await device(server, 'phone', '휴대폰');
    await b.engine.syncOnce();

    server.offline = true; // 휴대폰 오프라인
    edit(b.engine, b.store.notes[0]!, '휴대폰 초안');
    await b.engine.syncOnce();
    expect(b.engine.status()).toMatchObject({ phase: 'offline', pending: 1 });
    server.offline = false;

    edit(a.engine, a.store.notes[0]!, '노트북 수정'); // 그 사이 노트북이 저장
    await a.engine.syncOnce();

    await b.engine.syncOnce(); // 다시 연결
    const bn = b.store.notes[0]!;
    expect(bn.content).toBe('휴대폰 초안'); // 로컬 초안 보존
    expect(bn.sync?.conflict).toMatchObject({ content: '노트북 수정', version: 2 });
    expect(server.notes.get(bn.id)!.content).toBe('노트북 수정'); // 서버도 보존
    expect(b.engine.status().conflicts).toBe(1);

    const copy = await b.engine.resolveConflict(bn, 'both');
    expect(bn.content).toBe('노트북 수정');
    expect(copy).toMatchObject({ title: '계획 (내 사본)', content: '휴대폰 초안' });
    await b.engine.syncOnce();
    expect([...server.notes.values()].map((n) => n.content).sort()).toEqual(['노트북 수정', '휴대폰 초안']);
  });

  it('충돌에서 "내 버전"을 고르면 서버 최신 버전 위에 내 내용을 올림', async () => {
    const server = new FakeServer();
    const a = await device(server, 'laptop', '노트북');
    a.store.notes.push(note('x', 'v1'));
    await a.engine.adoptLocalNotes();
    await a.engine.syncOnce();
    const b = await device(server, 'phone', '휴대폰');
    await b.engine.syncOnce();
    server.offline = true;
    edit(b.engine, b.store.notes[0]!, '내 것');
    await b.engine.syncOnce();
    server.offline = false;
    edit(a.engine, a.store.notes[0]!, '서버 것');
    await a.engine.syncOnce();
    await b.engine.syncOnce();
    await b.engine.resolveConflict(b.store.notes[0]!, 'mine');
    await b.engine.syncOnce();
    expect(server.notes.get(b.store.notes[0]!.id)).toMatchObject({ content: '내 것', version: 3 });
  });

  it('충돌에서 "서버 버전"을 고르면 로컬을 서버 내용으로 바꾸고 올릴 것 없음', async () => {
    const server = new FakeServer();
    const a = await device(server, 'laptop', '노트북');
    a.store.notes.push(note('y', 'v1'));
    await a.engine.adoptLocalNotes();
    await a.engine.syncOnce();
    const b = await device(server, 'phone', '휴대폰');
    await b.engine.syncOnce();
    server.offline = true;
    edit(b.engine, b.store.notes[0]!, '버릴 초안');
    await b.engine.syncOnce();
    server.offline = false;
    edit(a.engine, a.store.notes[0]!, '정본');
    await a.engine.syncOnce();
    await b.engine.syncOnce();
    await b.engine.resolveConflict(b.store.notes[0]!, 'server');
    expect(b.store.notes[0]).toMatchObject({ content: '정본', sync: { version: 2, dirty: false } });
  });

  it('다른 기기가 임대 중이면 오프라인 초안은 올리지 않고 대기 (덮어쓰기 없음)', async () => {
    const server = new FakeServer();
    const a = await device(server, 'laptop', '노트북');
    a.store.notes.push(note('z', 'v1'));
    await a.engine.adoptLocalNotes();
    await a.engine.syncOnce();
    const b = await device(server, 'phone', '휴대폰');
    await b.engine.syncOnce();
    await a.engine.beginEdit(a.store.notes[0]!); // 노트북 편집 중
    edit(b.engine, b.store.notes[0]!, '휴대폰 초안');
    await b.engine.syncOnce();
    expect(b.store.notes[0]!.sync?.dirty).toBe(true);
    expect(server.notes.get(b.store.notes[0]!.id)!.content).toBe('v1');
  });

  it('서버에서 삭제된 메모는 로컬에서도 사라짐 (로컬 변경이 없을 때)', async () => {
    const server = new FakeServer();
    const a = await device(server, 'laptop', '노트북');
    a.store.notes.push(note('지움', 'x'));
    await a.engine.adoptLocalNotes();
    await a.engine.syncOnce();
    server.deleteOnServer(a.store.notes[0]!.id);
    await a.engine.syncOnce();
    expect(a.store.notes).toHaveLength(0);
  });

  it('서버 오류(경로 중복 등)는 메모에 표시하고 다시 편집할 때까지 재시도하지 않음', async () => {
    const server = new FakeServer();
    const api = server.api();
    const failing: SyncApi = { ...api, put: async () => { throw { status: 409, code: 'path_conflict', message: '같은 이름' } satisfies SyncError; } };
    const store = new MemoryStore();
    const engine = new SyncEngine(failing, store, { clientId: 'c', label: 'l' });
    engines.push(engine);
    store.notes.push(note('dup', ''));
    await engine.adoptLocalNotes();
    await engine.syncOnce();
    expect(store.notes[0]!.sync).toMatchObject({ dirty: true, error: '같은 이름' });
    edit(engine, store.notes[0]!, 'changed');
    expect(store.notes[0]!.sync?.error).toBeUndefined();
  });

  describe('앱 밖에서 지운 메모 (데스크톱, ADR-0004)', () => {
    async function setup() {
      const server = new FakeServer();
      const desk = await device(server, 'desk', '데스크톱');
      desk.store.notes.push(note('바깥삭제', 'x'));
      await desk.engine.adoptLocalNotes();
      await desk.engine.syncOnce();
      const n = desk.store.notes[0]!;
      const version = n.sync!.version;
      desk.store.notes = []; // 파일이 바깥에서 지워짐 (core가 external_deletions로 보관)
      return { server, desk, id: n.id, version };
    }

    it('서버에서도 삭제 → 다른 기기에서 사라짐', async () => {
      const { server, desk, id, version } = await setup();
      const phone = await device(server, 'phone', '휴대폰');
      await phone.engine.syncOnce();
      expect(phone.store.notes).toHaveLength(1);
      expect(await desk.engine.deleteRemote(id, version)).toBe('deleted');
      expect(server.notes.get(id)!.deleted).toBe(true);
      expect(server.leases.has(id)).toBe(false);
      await phone.engine.syncOnce();
      expect(phone.store.notes).toHaveLength(0);
      await desk.engine.syncOnce();
      expect(desk.store.notes).toHaveLength(0); // 묘비는 되살리지 않음
    });

    it('지운 뒤 다른 기기가 고쳤으면 삭제하지 않음(changed) → 다시 받기', async () => {
      const { server, desk, id, version } = await setup();
      const phone = await device(server, 'phone', '휴대폰');
      await phone.engine.syncOnce();
      edit(phone.engine, phone.store.notes[0]!, '휴대폰에서 고침');
      await phone.engine.syncOnce();
      expect(await desk.engine.deleteRemote(id, version)).toBe('changed');
      expect(server.notes.get(id)!.deleted).toBe(false);
      expect(await desk.engine.restoreRemote(id)).toBe(true);
      expect(desk.store.notes.map((n) => [n.id, n.content, n.sync?.dirty])).toEqual([[id, '휴대폰에서 고침', false]]);
    });

    it('다른 기기가 편집 중이면 busy, 이미 서버에서 지워졌으면 gone', async () => {
      const { server, desk, id, version } = await setup();
      const phone = await device(server, 'phone', '휴대폰');
      await phone.engine.syncOnce();
      await phone.engine.beginEdit(phone.store.notes[0]!);
      expect(await desk.engine.deleteRemote(id, version)).toBe('busy');
      await phone.engine.endEdit(id);
      server.deleteOnServer(id);
      expect(await desk.engine.deleteRemote(id, version)).toBe('gone');
      expect(await desk.engine.restoreRemote(id)).toBe(false);
    });

    it('오프라인이면 오류(나중에 다시)', async () => {
      const { server, desk, id, version } = await setup();
      server.offline = true;
      await expect(desk.engine.deleteRemote(id, version)).rejects.toMatchObject({ offline: true });
    });
  });
});

describe('삭제 대기 (2026-11-13 사용자 확인: 메모 삭제 시 "사용 중")', () => {
  it('다른 기기가 편집 중이면 대기했다가, 그 기기가 닫으면 자동으로 서버·두 기기에서 지워짐', async () => {
    const server = new FakeServer();
    const a = await device(server, 'laptop', '노트북');
    a.store.notes.push(note('지울 메모', '본문'));
    await a.engine.adoptLocalNotes();
    await a.engine.syncOnce();
    const b = await device(server, 'phone', '휴대폰');
    await b.engine.syncOnce();
    expect((await b.engine.beginEdit(b.store.notes[0]!)).kind).toBe('held');

    const n = a.store.notes[0]!;
    n.sync = { ...n.sync!, deleting: true };
    await a.engine.syncOnce();
    expect(a.store.notes.map((x) => x.sync?.deleting)).toEqual([true]); // 대기
    expect(a.engine.status().pending).toBe(1);
    expect(server.notes.get(n.id)!.deleted).toBe(false);

    await b.engine.endEdit(b.store.notes[0]!.id);
    await a.engine.syncOnce();
    expect(a.store.notes).toEqual([]);
    expect(server.notes.get(n.id)!.deleted).toBe(true);
    await b.engine.syncOnce();
    expect(b.store.notes).toEqual([]);
  });

  it('대기 중 다른 기기가 고치면 지우지 않고 최신 내용을 받음', async () => {
    const server = new FakeServer();
    const a = await device(server, 'laptop', '노트북');
    a.store.notes.push(note('남길 메모', 'v1'));
    await a.engine.adoptLocalNotes();
    await a.engine.syncOnce();
    const b = await device(server, 'phone', '휴대폰');
    await b.engine.syncOnce();
    const bn = b.store.notes[0]!;
    expect((await b.engine.beginEdit(bn)).kind).toBe('held');
    const n = a.store.notes[0]!;
    n.sync = { ...n.sync!, deleting: true };
    await a.engine.syncOnce(); // 대기
    edit(b.engine, bn, 'v2 휴대폰에서 고침');
    await b.engine.syncOnce();
    await b.engine.endEdit(bn.id);
    await a.engine.syncOnce();
    expect(a.store.notes.map((x) => [x.content, !!x.sync?.deleting])).toEqual([['v2 휴대폰에서 고침', false]]);
    expect(server.notes.get(n.id)!.deleted).toBe(false);
  });
});

