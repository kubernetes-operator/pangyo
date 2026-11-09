// === 데스크톱(Tauri) 파일 저장소 어댑터 ===
// Rust 쪽(apps/native/src-tauri, crates/core Workspace)이 실제 .md/.txt 파일과 SQLite 보조 인덱스를 관리합니다.
// 명령 이름은 apps/native/src-tauri/src/lib.rs 의 #[tauri::command] 함수와 같아야 합니다.
import { invoke } from '@tauri-apps/api/core';
import { normalizeNote, type Note } from '../domain/types';
import type { NoteRepository } from './repository';

export const COMMANDS = {
  root: 'notes_root',
  listNotes: 'list_notes',
  saveNote: 'save_note',
  deleteNote: 'delete_note',
  listFolders: 'list_folders',
  createFolder: 'create_folder',
  getMeta: 'get_meta',
  setMeta: 'set_meta',
} as const;

export async function createTauriRepository(): Promise<NoteRepository> {
  const root = await invoke<string>(COMMANDS.root);
  return {
    kind: 'files',
    describe: () => root,
    async listNotes() {
      const notes = await invoke<Array<Partial<Note> & { id: string }>>(COMMANDS.listNotes);
      return notes.map(normalizeNote);
    },
    async saveNote(note) {
      await invoke(COMMANDS.saveNote, { note });
    },
    async deleteNote(id) {
      await invoke(COMMANDS.deleteNote, { id });
    },
    async listFolders() {
      const paths = await invoke<string[]>(COMMANDS.listFolders);
      return paths.map((path) => ({ path }));
    },
    async saveFolder(path) {
      await invoke(COMMANDS.createFolder, { path });
    },
    async getMeta<T>(key: string) {
      return ((await invoke<T | null>(COMMANDS.getMeta, { key })) ?? undefined) as T | undefined;
    },
    async setMeta(key, value) {
      await invoke(COMMANDS.setMeta, { key, value });
    },
  };
}
