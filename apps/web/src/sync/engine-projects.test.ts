// === 프로젝트 동기화 (ADR-0005) 단위 테스트: 프로젝트를 아는 작은 가짜 서버 + 두 기기 ===
// 실제 서버 규칙은 crates/server/tests/projects.rs 가 검사합니다.
import { afterEach, describe, expect, it } from 'vitest';
import type { LocalProject } from '../domain/projects';
import type { Note } from '../domain/types';
import {
  SyncEngine,
  type ProjectStore,
  type ServerNote,
  type ServerProject,
  type SyncApi,
  type SyncError,
  type SyncStore,
} from './engine';

class ProjectServer {
  notes = new Map<string, ServerNote>();
  projects = new Map<string, ServerProject>();
  seq = 0;
  private fail(status: number, code: string, message = code): never {
    throw { status, code, message } satisfies SyncError;
  }
  api(): SyncApi {
    return {
      changes: async (since, aware) => {
        const notes = [...this.notes.values()].filter((n) => n.changeSeq > since);
        const projects = aware ? [...this.projects.values()].filter((p) => p.changeSeq > since) : [];
        const legacy = notes.map((n) => (aware || !n.projectId ? n : { ...n, deleted: true, content: '', projectId: null }));
        const seqs = [...notes, ...projects].map((x) => x.changeSeq);
        return { notes: structuredClone(legacy), projects: structuredClone(projects), cursor: Math.max(since, ...seqs), hasMore: false };
      },
      get: async (id) => structuredClone(this.notes.get(id)!),
      put: async (id, body) => {
        const current = this.notes.get(id);
        if ((current?.version ?? 0) !== body.baseVersion) this.fail(409, 'version_conflict');
        const projectId = 'projectId' in body ? (body.projectId as string | null) : (current?.projectId ?? null);
        if (projectId && (!this.projects.has(projectId) || this.projects.get(projectId)!.deleted)) this.fail(400, 'invalid_project');
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
          projectId,
        };
        this.notes.set(id, next);
        return structuredClone(next);
      },
      acquireLease: async () => ({ fencingToken: 1, expiresAt: new Date(Date.now() + 600_000).toISOString(), ttlSeconds: 600 }),
      releaseLease: async () => undefined,
      remove: async () => this.fail(501, 'not_needed'),
      putProject: async (id, name) => {
        const current = this.projects.get(id);
        if (current?.deleted) this.fail(404, 'project_not_found');
        const taken = [...this.projects.values()].some((p) => p.id !== id && !p.deleted && p.name === name);
        if (taken) this.fail(409, 'project_name_taken');
        const t = '2026-11-13T00:00:00.000Z';
        const next = { id, name, createdAt: current?.createdAt ?? t, modifiedAt: t, deleted: false, changeSeq: ++this.seq };
        this.projects.set(id, next);
        return structuredClone(next);
      },
      deleteProject: async (id) => {
        const current = this.projects.get(id);
        if (!current || current.deleted) this.fail(404, 'project_not_found');
        if ([...this.notes.values()].some((n) => n.projectId === id && !n.deleted)) this.fail(409, 'project_not_empty');
        const next = { ...current, deleted: true, changeSeq: ++this.seq };
        this.projects.set(id, next);
        return structuredClone(next);
      },
    };
  }
}

class Device implements SyncStore {
  notes: Note[] = [];
  projectList: LocalProject[] = [];
  meta = new Map<string, unknown>();
  list = () => this.notes;
  save = async () => undefined;
  add = async (n: Note) => void this.notes.push(n);
  remove = async (id: string) => void (this.notes = this.notes.filter((n) => n.id !== id));
  getMeta = async <T,>(k: string) => this.meta.get(k) as T | undefined;
  setMeta = async (k: string, v: unknown) => void this.meta.set(k, v);
  changed = () => undefined;
  projects?: ProjectStore;
  constructor(aware: boolean) {
    if (aware)
      this.projects = {
        list: () => this.projectList,
        save: async (p) => {
          this.projectList = [...this.projectList.filter((x) => x.id !== p.id), p];
        },
        remove: async (id) => {
          this.projectList = this.projectList.filter((x) => x.id !== id);
        },
        changed: () => undefined,
      };
  }
}

const engines: SyncEngine[] = [];
async function device(server: ProjectServer, aware = true) {
  const store = new Device(aware);
  const engine = new SyncEngine(server.api(), store, { clientId: crypto.randomUUID().slice(0, 8), label: 'test' });
  await engine.init();
  engines.push(engine);
  return { store, engine };
}
afterEach(async () => {
  await Promise.all(engines.splice(0).map((e) => e.stop()));
});

const T = '2026-11-13T00:00:00.000Z';
function note(title: string, projectId: string | null): Note {
  return { id: crypto.randomUUID(), title, folder: '', format: 'md', content: title, createdAt: T, modifiedAt: T, sessions: [], undo: [], redo: [], projectId };
}
function project(name: string): LocalProject {
  return { id: crypto.randomUUID(), name, createdAt: T, modifiedAt: T, sync: { synced: false, dirty: true } };
}

describe('프로젝트 동기화 (ADR-0005)', () => {
  it('새 프로젝트와 그 메모를 올리고(프로젝트 먼저), 다른 기기가 같은 프로젝트로 받음', async () => {
    const server = new ProjectServer();
    const a = await device(server);
    const work = project('업무');
    a.store.projectList.push(work);
    a.store.notes.push(note('회의', work.id), note('일기', null));
    await a.engine.adoptLocalNotes();
    await a.engine.syncOnce();
    expect(a.engine.status()).toMatchObject({ phase: 'idle', pending: 0 });
    expect(a.store.projectList[0]!.sync).toMatchObject({ synced: true, dirty: false });

    const b = await device(server);
    await b.engine.syncOnce();
    expect(b.store.projectList.map((p) => p.name)).toEqual(['업무']);
    const byTitle = Object.fromEntries(b.store.notes.map((n) => [n.title, n.projectId]));
    expect(byTitle).toEqual({ 회의: work.id, 일기: null });
  });

  it('메모 옮기기와 프로젝트 이름 바꾸기가 다른 기기에 전달', async () => {
    const server = new ProjectServer();
    const a = await device(server);
    const work = project('업무');
    a.store.projectList.push(work);
    const n = note('계획', null);
    a.store.notes.push(n);
    await a.engine.adoptLocalNotes();
    await a.engine.syncOnce();
    const b = await device(server);
    await b.engine.syncOnce();

    n.projectId = work.id;
    a.engine.markDirty(n);
    const live = a.store.projectList.find((p) => p.id === work.id)!;
    live.name = '회사';
    live.sync = { ...live.sync, dirty: true };
    await a.engine.syncOnce();
    await b.engine.syncOnce();
    expect(b.store.projectList[0]!.name).toBe('회사');
    expect(b.store.notes[0]!.projectId).toBe(work.id);
  });

  it('메모가 있는 프로젝트는 서버가 지우기를 거절 → 되살림, 비면 지워지고 다른 기기에서도 사라짐', async () => {
    const server = new ProjectServer();
    const a = await device(server);
    const work = project('업무');
    a.store.projectList.push(work);
    await a.engine.syncOnce();
    const b = await device(server);
    await b.engine.syncOnce();
    // b가 그 사이 메모를 넣음
    b.store.notes.push(note('b의 메모', work.id));
    await b.engine.adoptLocalNotes();
    await b.engine.syncOnce();

    // 목록의 객체는 서버에서 받을 때 교체되므로 ID로 찾음 (화면도 같은 방식)
    const live = () => a.store.projectList.find((p) => p.id === work.id)!;
    live().sync = { ...live().sync, deleting: true };
    await a.engine.syncOnce();
    expect(a.store.projectList[0]!.sync).toMatchObject({ deleting: false });
    expect(a.store.projectList[0]!.sync.error).toBeTruthy();
    expect(a.store.notes.map((x) => x.title)).toEqual(['b의 메모']);

    // 메모를 기본으로 옮긴 뒤 지우면 성공 → b에서도 사라짐
    const moved = a.store.notes[0]!;
    moved.projectId = null;
    a.engine.markDirty(moved);
    live().sync = { ...live().sync, deleting: true, error: null };
    await a.engine.syncOnce();
    expect(a.store.projectList).toEqual([]);
    await b.engine.syncOnce();
    expect(b.store.projectList).toEqual([]);
    expect(b.store.notes[0]!.projectId).toBeNull();
  });

  it('프로젝트를 모르는 기기(데스크톱 3단계 전)는 projectId를 보내지 않아 서버가 프로젝트를 유지하고, 다른 프로젝트 메모는 받지 않음', async () => {
    const server = new ProjectServer();
    const a = await device(server);
    const work = project('업무');
    a.store.projectList.push(work);
    a.store.notes.push(note('업무 메모', work.id), note('기본 메모', null));
    await a.engine.adoptLocalNotes();
    await a.engine.syncOnce();

    const legacy = await device(server, false);
    await legacy.engine.syncOnce();
    expect(legacy.store.notes.map((x) => x.title)).toEqual(['기본 메모']);
    const basic = legacy.store.notes[0]!;
    basic.content = '고침';
    legacy.engine.markDirty(basic);
    await legacy.engine.syncOnce();
    expect([...server.notes.values()].find((x) => x.title === '업무 메모')!.projectId).toBe(work.id);
    expect([...server.notes.values()].find((x) => x.title === '기본 메모')!.content).toBe('고침');
  });
});
