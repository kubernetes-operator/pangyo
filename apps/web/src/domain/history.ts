// === Undo/Redo 스냅샷 기록 (요구 27: 250회 영속화) ===
import type { Note, Snapshot } from './types';

export type HistoryKind = 'undo' | 'redo';

export function snapshot(note: Snapshot): Snapshot {
  return { title: note.title, content: note.content, format: note.format };
}

/** 변경 직전 상태를 undo에 넣고 redo를 비웁니다. limit을 넘으면 가장 오래된 기록을 버립니다. */
export function remember(note: Note, limit: number): void {
  note.undo.push(snapshot(note));
  if (note.undo.length > limit) note.undo.shift();
  note.redo = [];
}

/** undo 또는 redo 한 칸을 적용합니다. 적용할 기록이 없으면 false. */
export function restore(note: Note, kind: HistoryKind, limit: number): boolean {
  const source = note[kind];
  const previous = source.pop();
  if (!previous) return false;
  const target = kind === 'undo' ? note.redo : note.undo;
  target.push(snapshot(note));
  if (target.length > limit) target.shift();
  Object.assign(note, previous);
  return true;
}
