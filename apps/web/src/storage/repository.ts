// === 저장소 계약 (로드맵 03단계): 화면(ui/)은 이 인터페이스만 사용합니다 ===
// - 웹 브라우저: IndexedDB 어댑터 (storage/indexeddb.ts)
// - 데스크톱(Tauri): 파일 어댑터 (storage/tauri.ts) → Rust crates/core Workspace → 실제 .md/.txt 파일
// 두 어댑터 모두 같은 계약 테스트(repository.contract.ts)를 통과해야 합니다.
import type { FolderRecord, Note } from '../domain/types';

export type RepositoryKind = 'indexeddb' | 'files';

export interface NoteRepository {
  readonly kind: RepositoryKind;
  /** 화면 하단에 보여줄 저장 위치 설명 (예: "IndexedDB" 또는 실제 폴더 경로) */
  describe(): string;
  listNotes(): Promise<Note[]>;
  /** 새 메모 저장 또는 기존 메모 갱신. 파일 저장소에서는 제목/폴더/형식이 바뀌면 파일 이름도 바뀝니다. */
  saveNote(note: Note): Promise<void>;
  /**
   * (선택) 편집 세션 기록만 저장. 파일 저장소에서 열어 두기만 한 메모의 주기 저장이
   * 바깥 편집기에서 고친 본문을 덮어쓰지 않게 합니다 (ADR-0004). 없으면 saveNote를 씁니다.
   */
  saveSessions?(note: Note): Promise<void>;
  /** 메모 삭제 (다른 기기에서 삭제된 메모를 동기화로 반영할 때) */
  deleteNote(id: string): Promise<void>;
  listFolders(): Promise<FolderRecord[]>;
  /** 정규화된 폴더 경로 하나를 저장 (상위 폴더는 호출 쪽에서 순서대로 저장) */
  saveFolder(path: string): Promise<void>;
  getMeta<T>(key: string): Promise<T | undefined>;
  setMeta(key: string, value: unknown): Promise<void>;
}
