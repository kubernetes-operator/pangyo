// === 편집 보조: 블록 들여쓰기(요구 26), 편집 시간 계산(요구 23·24) ===
import type { EditSession } from './types';

export interface IndentResult {
  start: number; // 바꿀 구간 시작 (선택 영역 첫 줄의 맨 앞)
  end: number; // 바꿀 구간 끝
  text: string; // 대체할 텍스트
}

/** 선택된 줄 묶음을 들여쓰기/내어쓰기 한 결과를 계산합니다. */
export function indentLines(
  value: string,
  selectionStart: number,
  selectionEnd: number,
  outdent: boolean,
  unit: string,
): IndentResult {
  const start = value.lastIndexOf('\n', selectionStart - 1) + 1;
  const selected = value.slice(start, selectionEnd);
  const text = selected
    .split('\n')
    .map((line) => (outdent ? line.replace(/^( {1,2}|\t)/, '') : unit + line))
    .join('\n');
  return { start, end: selectionEnd, text };
}

/** 마지막 입력 후 idleMs가 지났는지 */
export function isIdle(now: number, lastInputAt: number, idleMs: number): boolean {
  return now - lastInputAt > idleMs;
}

/** 타이머 한 번에 더할 시간. 절전/탭 정지로 크게 건너뛴 시간은 tick 2회분까지만 인정합니다. */
export function tickIncrement(now: number, lastTick: number, tickMs: number): number {
  return Math.max(0, Math.min(now - lastTick, tickMs * 2));
}

export function totalActiveMs(sessions: readonly EditSession[]): number {
  return sessions.reduce((sum, s) => sum + (s.activeMs || 0), 0);
}

/** 1234567ms → '00:20:34' */
export function timeText(milliseconds: number): string {
  const t = Math.floor(milliseconds / 1000);
  return [Math.floor(t / 3600), Math.floor(t / 60) % 60, t % 60]
    .map((x) => String(x).padStart(2, '0'))
    .join(':');
}
