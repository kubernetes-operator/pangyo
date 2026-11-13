// === 트리 편집 규칙 (2026-11-13 사용 의견: 확장 메뉴·드래그 앤 드롭으로 이동·이름 변경·삭제) ===
// 폴더는 따로 저장된 개체가 아니라 메모의 folder 경로(+ 빈 폴더 기록)이므로, 폴더 이동·이름 변경은
// 그 아래 메모들의 경로를 바꾸는 것입니다. 깊이 2단계(요구 21)를 넘지 않게 확인합니다.

export const depthOf = (path: string): number => (path ? path.split('/').length : 0);
export const lastSegment = (path: string): string => path.split('/').at(-1) ?? '';
export const parentOf = (path: string): string => path.split('/').slice(0, -1).join('/');
export const joinPath = (parent: string, name: string): string => (parent ? `${parent}/${name}` : name);

/** path가 folder 자신이거나 그 아래인지 */
export const isWithin = (path: string, folder: string): boolean =>
  !!folder && (path === folder || path.startsWith(folder + '/'));

/** folder 아래(자신 포함)에서 가장 깊은 폴더가 folder보다 몇 단계 아래인지 (자신만 있으면 0) */
export function subtreeExtraDepth(folder: string, allFolders: Iterable<string>): number {
  let extra = 0;
  for (const p of allFolders) if (isWithin(p, folder)) extra = Math.max(extra, depthOf(p) - depthOf(folder));
  return extra;
}

/** from 경로를 to로 옮겼을 때 path의 새 경로 (from 아래가 아니면 null) */
export function remap(path: string, from: string, to: string): string | null {
  if (path === from) return to;
  if (path.startsWith(from + '/')) return to + path.slice(from.length);
  return null;
}

export type FolderMoveProblem = 'same' | 'into_self' | 'too_deep' | null;

/**
 * folder를 targetParent 아래로 옮길 수 있는지 (같은 프로젝트 안). 프로젝트가 바뀌면 into_self 검사는 하지 않음.
 * 결과 경로의 깊이 + 하위 폴더 깊이가 maxDepth를 넘으면 too_deep
 */
export function folderMoveProblem(
  folder: string,
  targetParent: string,
  allFolders: Iterable<string>,
  maxDepth: number,
  sameProject = true,
): FolderMoveProblem {
  const folders = [...allFolders];
  if (sameProject && parentOf(folder) === targetParent) return 'same';
  if (sameProject && isWithin(targetParent, folder)) return 'into_self';
  const moved = joinPath(targetParent, lastSegment(folder));
  if (depthOf(moved) + subtreeExtraDepth(folder, folders) > maxDepth) return 'too_deep';
  return null;
}
