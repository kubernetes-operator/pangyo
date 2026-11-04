// === 도메인 단위 테스트: 요구 21, 23, 26, 27과 v0.1 데이터 호환 ===
import { describe, expect, it } from 'vitest';
import { FolderError, folderAncestors, parseFolder, safeFilename } from './paths';
import { remember, restore } from './history';
import { indentLines, isIdle, tickIncrement, timeText, totalActiveMs } from './editing';
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
  it('60초 초과 무입력이면 idle', () => {
    expect(isIdle(60_000, 0, 60_000)).toBe(false);
    expect(isIdle(60_001, 0, 60_000)).toBe(true);
  });
  it('절전 등으로 크게 건너뛴 시간은 tick 2회분까지만 인정', () => {
    expect(tickIncrement(1000, 0, 1000)).toBe(1000);
    expect(tickIncrement(3_600_000, 0, 1000)).toBe(2000);
    expect(tickIncrement(0, 1000, 1000)).toBe(0);
  });
  it('누적 시간과 표시 형식', () => {
    expect(totalActiveMs([{ id: 'a', startedAt: '', endedAt: null, activeMs: 1500 }])).toBe(1500);
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
  it('알 수 없는 format은 md로', () => {
    expect(normalizeNote({ id: 'x', format: 'docx' as never }).format).toBe('md');
  });
});
