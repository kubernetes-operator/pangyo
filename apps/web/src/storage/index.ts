// === 실행 환경에 맞는 저장소 선택 ===
// Tauri 데스크톱 앱 안이면 파일 저장소, 아니면 브라우저 IndexedDB.
import type { NoteRepository } from './repository';

/** Tauri 2 웹뷰는 이 전역 객체를 주입합니다. */
export function isTauri(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

export async function openRepository(): Promise<NoteRepository> {
  if (isTauri()) {
    const { createTauriRepository } = await import('./tauri');
    return createTauriRepository();
  }
  const { indexedDbRepository } = await import('./indexeddb');
  return indexedDbRepository;
}

export type { NoteRepository } from './repository';
