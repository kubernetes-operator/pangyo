import { describe, expect, it } from 'vitest';
import { daysLeft, expiresAtOf, pruneTrash, restoredName, type LocalTrashItem } from './trash';

describe('휴지통 규칙', () => {
  const now = Date.parse('2026-11-13T00:00:00Z');
  const note = (deletedAt: string): LocalTrashItem => ({ kind: 'note', note: { id: deletedAt } as never, deletedAt });
  it('7일이 지나면 뺌', () => {
    const kept = pruneTrash([note('2026-11-10T00:00:00Z'), note('2026-11-05T23:00:00Z')], now);
    expect(kept.map((i) => i.deletedAt)).toEqual(['2026-11-10T00:00:00Z']);
  });
  it('남은 날', () => {
    expect(daysLeft(expiresAtOf('2026-11-10T00:00:00Z'), now)).toBe(4);
    expect(daysLeft('2026-11-01T00:00:00Z', now)).toBe(0);
  });
  it('되살릴 때 이름이 겹치면 (복구)', () => {
    const taken = new Set(['회의', '회의 (복구)']);
    expect(restoredName('회의', (n) => taken.has(n))).toBe('회의 (복구 2)');
    expect(restoredName('새것', (n) => taken.has(n))).toBe('새것');
  });
});
