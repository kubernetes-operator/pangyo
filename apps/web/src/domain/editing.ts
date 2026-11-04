// === 편집 보조: 블록 들여쓰기(요구 26), 편집 시간 계산(요구 23·24) ===
// 편집 시간 = 작성 시간(키 입력) + 열어둔 시간(창에 메모가 보이는 시간)을 각각 기록합니다.
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

/**
 * 작성 시간에 더할 값: 직전 입력과의 간격이 idleMs 이하이면 그 간격, 아니면 0.
 * 첫 입력(lastInputAt=0)과 오래 쉰 뒤의 입력은 0이므로 쉬는 시간은 작성 시간에 들어가지 않습니다.
 */
export function typingIncrement(now: number, lastInputAt: number, idleMs: number): number {
  if (lastInputAt <= 0) return 0;
  const gap = now - lastInputAt;
  return gap >= 0 && gap <= idleMs ? gap : 0;
}

/** 열어둔 시간 타이머 한 번에 더할 값. 절전 등으로 크게 건너뛴 시간은 tick 2회분까지만 인정합니다. */
export function tickIncrement(now: number, lastTick: number, tickMs: number): number {
  return Math.max(0, Math.min(now - lastTick, tickMs * 2));
}

export function totalActiveMs(sessions: readonly EditSession[]): number {
  return sessions.reduce((sum, s) => sum + (s.activeMs || 0), 0);
}

export function totalOpenMs(sessions: readonly EditSession[]): number {
  return sessions.reduce((sum, s) => sum + (s.openMs || 0), 0);
}

/** 입력 없이 잠깐 스쳐 간 세션(목록 넘기기 등)은 기록하지 않습니다. */
export function isNegligibleSession(session: EditSession, minOpenMs: number): boolean {
  return session.activeMs === 0 && session.openMs < minOpenMs;
}

/** 1234567ms → '00:20:34' */
export function timeText(milliseconds: number): string {
  const t = Math.floor(milliseconds / 1000);
  return [Math.floor(t / 3600), Math.floor(t / 60) % 60, t % 60]
    .map((x) => String(x).padStart(2, '0'))
    .join(':');
}
