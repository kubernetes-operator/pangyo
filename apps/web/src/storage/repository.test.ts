// === 저장소 계약 테스트: 모든 어댑터가 같은 동작을 해야 화면 코드가 저장 방식과 무관해집니다 ===
// - IndexedDB 어댑터: fake-indexeddb로 실제 코드 경로를 실행
// - Tauri 어댑터: IPC를 메모리 가짜 백엔드로 대체해 명령 이름/인자 연결을 검증
//   (Rust 쪽 실제 파일 동작은 crates/core/tests/workspace.rs 가 검증)
import 'fake-indexeddb/auto';
import { clearMocks, mockIPC, mockWindows } from '@tauri-apps/api/mocks';
import { afterEach, describe, expect, it } from 'vitest';
import type { Note } from '../domain/types';
import type { NoteRepository } from './repository';

// Node 테스트 환경에는 window가 없으므로 Tauri 모의 객체가 쓸 전역을 연결합니다.
(globalThis as { window?: unknown }).window ??= globalThis;

const sampleNote = (id: string, patch: Partial<Note> = {}): Note => ({
  id,
  title: '회의',
  folder: 'Work',
  format: 'md',
  content: '# 안건\n',
  createdAt: '2026-11-04T00:00:00.000Z',
  modifiedAt: '2026-11-04T00:00:00.000Z',
  sessions: [{ id: 's1', startedAt: '2026-11-04T00:00:00.000Z', endedAt: null, activeMs: 1000, openMs: 5000 }],
  undo: [{ title: '회의', content: '', format: 'md' }],
  redo: [],
  ...patch,
});

/** Rust Workspace를 흉내 내는 최소 가짜 백엔드 */
function fakeTauriBackend() {
  const notes = new Map<string, Note>();
  const folders = new Set<string>();
  const meta = new Map<string, unknown>();
  const calls: string[] = [];
  mockWindows('main');
  mockIPC((cmd, args) => {
    calls.push(cmd);
    const a = (args ?? {}) as Record<string, unknown>;
    switch (cmd) {
      case 'notes_root':
        return '/home/test/MemoNest/Notes';
      case 'list_notes':
        return [...notes.values()];
      case 'save_note': {
        const note = a.note as Note;
        notes.set(note.id, structuredClone(note));
        return null;
      }
      case 'list_folders':
        return [...folders].sort();
      case 'create_folder':
        folders.add(a.path as string);
        return a.path;
      case 'get_meta':
        return meta.get(a.key as string) ?? null;
      case 'set_meta':
        meta.set(a.key as string, a.value);
        return null;
      default:
        throw new Error(`unknown command ${cmd}`);
    }
  });
  return { calls };
}

async function indexedDb(): Promise<NoteRepository> {
  return (await import('./indexeddb')).indexedDbRepository;
}
async function tauri(): Promise<NoteRepository> {
  fakeTauriBackend();
  return (await import('./tauri')).createTauriRepository();
}

afterEach(() => clearMocks());

describe.each([
  ['indexeddb', indexedDb],
  ['tauri', tauri],
] as const)('NoteRepository 계약: %s', (kind, open) => {
  it('메모 저장 후 목록에 같은 내용(세션·Undo 포함)으로 나타남', async () => {
    const repo = await open();
    expect(repo.kind).toBe(kind === 'tauri' ? 'files' : 'indexeddb');
    const id = crypto.randomUUID();
    await repo.saveNote(sampleNote(id));
    const found = (await repo.listNotes()).find((n) => n.id === id);
    expect(found).toEqual(sampleNote(id));
  });

  it('같은 id로 다시 저장하면 갱신 (새 메모가 생기지 않음)', async () => {
    const repo = await open();
    const id = crypto.randomUUID();
    await repo.saveNote(sampleNote(id));
    await repo.saveNote(sampleNote(id, { title: '회의록', content: '변경' }));
    const matches = (await repo.listNotes()).filter((n) => n.id === id);
    expect(matches).toHaveLength(1);
    expect(matches[0]).toMatchObject({ title: '회의록', content: '변경' });
  });

  it('폴더와 메타 값', async () => {
    const repo = await open();
    await repo.saveFolder('Work');
    await repo.saveFolder('Work/Project');
    expect((await repo.listFolders()).map((f) => f.path)).toEqual(
      expect.arrayContaining(['Work', 'Work/Project']),
    );
    expect(await repo.getMeta('missing-key')).toBeUndefined();
    await repo.setMeta('locale', 'en');
    expect(await repo.getMeta('locale')).toBe('en');
  });
});
