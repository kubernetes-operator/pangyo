import { describe, expect, it } from 'vitest';
import { folderMoveProblem, remap, subtreeExtraDepth } from './tree-ops';

const folders = ['업무', '업무/회의', '개인', '보관'];

describe('트리 편집 규칙', () => {
  it('경로 바꾸기: 자신과 그 아래만', () => {
    expect(remap('업무', '업무', '회사')).toBe('회사');
    expect(remap('업무/회의', '업무', '회사')).toBe('회사/회의');
    expect(remap('업무2', '업무', '회사')).toBeNull();
    expect(remap('개인', '업무', '회사')).toBeNull();
  });
  it('하위 폴더 깊이', () => {
    expect(subtreeExtraDepth('업무', folders)).toBe(1);
    expect(subtreeExtraDepth('개인', folders)).toBe(0);
  });
  it('폴더 이동: 2단계를 넘거나 자기 안으로는 불가', () => {
    expect(folderMoveProblem('개인', '보관', folders, 2)).toBeNull(); // 보관/개인
    expect(folderMoveProblem('업무', '보관', folders, 2)).toBe('too_deep'); // 보관/업무/회의 = 3단계
    expect(folderMoveProblem('업무', '업무/회의', folders, 2)).toBe('into_self');
    expect(folderMoveProblem('업무/회의', '업무', folders, 2)).toBe('same');
    expect(folderMoveProblem('업무/회의', '', folders, 2)).toBeNull(); // 루트로
    expect(folderMoveProblem('업무', '', folders, 2, false)).toBeNull(); // 다른 프로젝트의 루트로
  });
});
