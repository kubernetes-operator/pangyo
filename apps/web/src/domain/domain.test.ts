// === 도메인 단위 테스트: 요구 21, 23, 26, 27과 v0.1 데이터 호환 ===
import { describe, expect, it } from 'vitest';
import { FolderError, folderAncestors, parseFolder, safeFilename } from './paths';
import { remember, restore } from './history';
import {
  indentLines,
  isNegligibleSession,
  tickIncrement,
  timeText,
  totalActiveMs,
  totalOpenMs,
  typingIncrement,
} from './editing';
import { normalizeNote, type Note } from './types';

const LIMIT = 250;
const newNote = (content = ''): Note => ({
  id: '00000000-0000-4000-8000-000000000000',
  title: 'T',
  content,
  format: 'md',
  folder: '',
  createdAt: '2026-01-01T00:00:00.000Z',
  modifiedAt: '2026-01-01T00:00:00.000Z',
  sessions: [],
  undo: [],
  redo: [],
});

describe('parseFolder (요구 21, T-02)', () => {
  it('0~2단계를 허용하고 구분자를 정규화', () => {
    expect(parseFolder('', 2)).toBe('');
    expect(parseFolder('Work', 2)).toBe('Work');
    expect(parseFolder(' Work / Project ', 2)).toBe('Work/Project');
    expect(parseFolder('Work\\Project/', 2)).toBe('Work/Project');
  });
  it('3단계 이상은 거부', () => {
    expect(() => parseFolder('Work/Project/Deep', 2)).toThrow(FolderError);
  });
  it('상대 경로, 콜론, 너무 긴 이름은 거부', () => {
    for (const bad of ['..', 'Work/..', './a', 'C:', 'x'.repeat(71)]) {
      expect(() => parseFolder(bad, 2), bad).toThrow(FolderError);
    }
  });
  it('상위 폴더 목록', () => {
    expect(folderAncestors('')).toEqual([]);
    expect(folderAncestors('A/B')).toEqual(['A', 'A/B']);
  });
});

describe('safeFilename', () => {
  it('금지 문자 치환, NFC 정규화, 빈 이름 대체', () => {
    expect(safeFilename('a/b:c*?')).toBe('a_b_c__');
    expect(safeFilename('  ')).toBe('Untitled');
    expect(safeFilename('한글'.normalize('NFD'))).toBe('한글'.normalize('NFC'));
    expect(safeFilename('x'.repeat(300))).toHaveLength(200);
  });
});

describe('Undo/Redo (요구 27, T-04)', () => {
  it('250개를 넘으면 가장 오래된 기록을 버림', () => {
    const note = newNote('0');
    for (let i = 1; i <= 260; i++) {
      remember(note, LIMIT);
      note.content = String(i);
    }
    expect(note.undo).toHaveLength(LIMIT);
    let steps = 0;
    while (restore(note, 'undo', LIMIT)) steps++;
    expect(steps).toBe(LIMIT);
    expect(note.content).toBe('10'); // 0~9는 한도 초과로 사라짐
    expect(restore(note, 'redo', LIMIT)).toBe(true);
    expect(note.content).toBe('11');
  });
  it('새 편집은 redo를 비움', () => {
    const note = newNote('a');
    remember(note, LIMIT);
    note.content = 'b';
    restore(note, 'undo', LIMIT);
    expect(note.redo).toHaveLength(1);
    remember(note, LIMIT);
    expect(note.redo).toHaveLength(0);
  });
});

describe('indentLines (요구 26)', () => {
  it('선택된 줄 전체를 들여쓰기', () => {
    const value = 'a\nbc\nd';
    const r = indentLines(value, 3, 4, false, '  '); // 'bc'의 'c'만 선택해도 줄 전체가 대상
    expect(r).toEqual({ start: 2, end: 4, text: '  bc' });
    const all = indentLines(value, 0, value.length, false, '  ');
    expect(all.text).toBe('  a\n  bc\n  d');
  });
  it('내어쓰기는 공백 2칸 또는 탭 1개 제거', () => {
    const value = '    a\n\tb\nc';
    expect(indentLines(value, 0, value.length, true, '  ').text).toBe('  a\nb\nc');
  });
});

describe('편집 시간 (요구 23, 24, T-05)', () => {
  it('작성 시간: 입력 간격이 한도 이하일 때만 더하고, 첫 입력과 오래 쉰 뒤 입력은 0', () => {
    expect(typingIncrement(5_000, 0, 60_000)).toBe(0); // 세션 첫 입력
    expect(typingIncrement(10_000, 5_000, 60_000)).toBe(5_000);
    expect(typingIncrement(65_000, 5_000, 60_000)).toBe(60_000); // 정확히 한도
    expect(typingIncrement(65_001, 5_000, 60_000)).toBe(0); // 쉬는 시간 제외
    expect(typingIncrement(4_000, 5_000, 60_000)).toBe(0); // 시계 역행
  });
  it('입력 없이 잠깐 연 세션은 기록 제외', () => {
    const s = { id: 'a', startedAt: '', endedAt: null, activeMs: 0, openMs: 1_000 };
    expect(isNegligibleSession(s, 2_000)).toBe(true);
    expect(isNegligibleSession({ ...s, openMs: 2_000 }, 2_000)).toBe(false);
    expect(isNegligibleSession({ ...s, activeMs: 1 }, 2_000)).toBe(false);
  });
  it('절전 등으로 크게 건너뛴 시간은 tick 2회분까지만 인정', () => {
    expect(tickIncrement(1000, 0, 1000)).toBe(1000);
    expect(tickIncrement(3_600_000, 0, 1000)).toBe(2000);
    expect(tickIncrement(0, 1000, 1000)).toBe(0);
  });
  it('누적 시간과 표시 형식', () => {
    const sessions = [
      { id: 'a', startedAt: '', endedAt: null, activeMs: 1500, openMs: 4000 },
      { id: 'b', startedAt: '', endedAt: null, activeMs: 500, openMs: 1000 },
    ];
    expect(totalActiveMs(sessions)).toBe(2000);
    expect(totalOpenMs(sessions)).toBe(5000);
    expect(timeText(3_723_000)).toBe('01:02:03');
  });
});

describe('normalizeNote (v0.1 데이터 호환)', () => {
  it('undo/redo/sessions가 없는 옛 레코드에 기본값을 채움', () => {
    const n = normalizeNote({ id: 'x', title: 'Old', content: 'body', format: 'md', folder: 'Work' });
    expect(n.undo).toEqual([]);
    expect(n.redo).toEqual([]);
    expect(n.sessions).toEqual([]);
    expect(n.content).toBe('body');
  });
  it('openMs가 없는 v0.1 세션은 0으로 보정', () => {
    const n = normalizeNote({
      id: 'x',
      sessions: [{ id: 's', startedAt: 't', endedAt: null, activeMs: 60_000 } as never],
    });
    expect(n.sessions[0]).toEqual({ id: 's', startedAt: 't', endedAt: null, activeMs: 60_000, openMs: 0 });
  });
  it('알 수 없는 format은 md로', () => {
    expect(normalizeNote({ id: 'x', format: 'docx' as never }).format).toBe('md');
  });
});
