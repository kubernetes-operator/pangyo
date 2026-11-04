// === 경로/파일명 규칙 (요구 21: 폴더 최대 2단계) ===
// 3단계(crates/core)에서 Rust와 같은 규칙으로 통일할 예정이므로 테스트를 기준으로 삼습니다.

export const FOLDER_SEGMENT_MAX = 70; // 폴더 이름 한 칸의 최대 길이
export const FILENAME_MAX = 200; // 메모 제목(파일명) 최대 길이

export class FolderError extends Error {}

/** 'Work/Project' 형태 입력을 검증·정규화합니다. 빈 문자열은 루트입니다. */
export function parseFolder(raw: string, maxDepth: number): string {
  const bits = raw
    .replaceAll('\\', '/')
    .split('/')
    .filter(Boolean)
    .map((x) => x.trim());
  const invalid = bits.some(
    (x) => x === '.' || x === '..' || !x || x.includes(':') || x.length > FOLDER_SEGMENT_MAX,
  );
  if (bits.length > maxDepth || invalid) {
    throw new FolderError('폴더는 최대 2단계이고 상대 경로 기호를 사용할 수 없습니다.');
  }
  return bits.join('/');
}

/** 'A/B' → ['A', 'A/B'] : 상위 폴더부터 순서대로 */
export function folderAncestors(path: string): string[] {
  if (!path) return [];
  const bits = path.split('/');
  return bits.map((_, i) => bits.slice(0, i + 1).join('/'));
}

/** OS 파일명으로 쓸 수 없는 문자를 '_'로 바꾸고 NFC 정규화합니다. */
export function safeFilename(name: string): string {
  return (
    name
      .normalize('NFC')
      .trim()
      .replace(/[\\/:*?"<>|\u0000-\u001f]/g, '_')
      .slice(0, FILENAME_MAX) || 'Untitled'
  );
}
