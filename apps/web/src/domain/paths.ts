// === 경로/파일명 규칙 (요구 21: 폴더 최대 2단계) ===
// 단일 기준: packages/protocol/fixtures/paths.json — Rust(crates/core/src/paths.rs)와 같은 결과여야 합니다.
// 길이는 모두 유니코드 코드 포인트 기준입니다 (UTF-16 길이 아님).

export const FOLDER_SEGMENT_MAX = 70; // 폴더 이름 한 칸의 최대 길이
export const FILENAME_MAX = 200; // 메모 제목(파일명, 확장자 제외) 최대 길이
export const FALLBACK_TITLE = 'Untitled'; // 비어 있을 때 쓰는 제목

/** Windows에서 파일/폴더 이름으로 쓸 수 없는 예약어 (대소문자 무시, 첫 '.' 앞부분 기준) */
const WINDOWS_RESERVED = new Set([
  'CON', 'PRN', 'AUX', 'NUL',
  ...Array.from({ length: 9 }, (_, i) => `COM${i + 1}`),
  ...Array.from({ length: 9 }, (_, i) => `LPT${i + 1}`),
]);
// 금지 문자: \ / : * ? " < > | 와 제어 문자(U+0000–U+001F, U+007F)
// eslint 미사용: 제어 문자 범위를 의도적으로 정규식에 포함합니다.
const FORBIDDEN = /[\\/:*?"<>|\u0000-\u001f\u007f]/gu;
const HAS_FORBIDDEN = /[\\/:*?"<>|\u0000-\u001f\u007f]/u;
// 앞뒤 공백: 유니코드 White_Space + U+FEFF (Rust: char::is_whitespace || '\u{FEFF}')
const EDGE_SPACE = /^[\s\u0085﻿]+|[\s\u0085﻿]+$/gu;

export type FolderErrorKind = 'too_deep' | 'invalid';
export class FolderError extends Error {
  constructor(
    readonly kind: FolderErrorKind,
    message: string,
  ) {
    super(message);
  }
}

const trimEdges = (s: string) => s.replace(EDGE_SPACE, '');
const codePoints = (s: string) => Array.from(s);
const isReserved = (name: string) => WINDOWS_RESERVED.has(name.split('.')[0]!.toUpperCase());
const stripTrailingDotsAndSpaces = (s: string) => s.replace(/[. ]+$/u, '');

/** 'Work/Project' 형태 입력을 검증·정규화합니다. 빈 문자열은 루트입니다. */
export function parseFolder(raw: string, maxDepth: number): string {
  const bits = raw
    .replaceAll('\\', '/')
    .split('/')
    .filter(Boolean)
    .map((x) => trimEdges(x).normalize('NFC'));
  if (bits.length > maxDepth) {
    throw new FolderError('too_deep', `폴더는 최대 ${maxDepth}단계까지 만들 수 있습니다.`);
  }
  for (const bit of bits) {
    const invalid =
      !bit ||
      bit.startsWith('.') || // '.', '..', 숨김 폴더
      bit.endsWith('.') ||
      HAS_FORBIDDEN.test(bit) ||
      isReserved(bit) ||
      codePoints(bit).length > FOLDER_SEGMENT_MAX;
    if (invalid) {
      throw new FolderError(
        'invalid',
        `사용할 수 없는 폴더 이름입니다: "${bit}" (상대 경로, 특수문자 \\ / : * ? " < > |, 예약어, ${FOLDER_SEGMENT_MAX}자 초과 불가)`,
      );
    }
  }
  return bits.join('/');
}

/** 'A/B' → ['A', 'A/B'] : 상위 폴더부터 순서대로 */
export function folderAncestors(path: string): string[] {
  if (!path) return [];
  const bits = path.split('/');
  return bits.map((_, i) => bits.slice(0, i + 1).join('/'));
}

/** 메모 제목을 모든 OS에서 안전한 파일 이름(확장자 제외)으로 바꿉니다. */
export function safeFilename(name: string): string {
  let s = trimEdges(name).normalize('NFC').replace(FORBIDDEN, '_');
  s = stripTrailingDotsAndSpaces(s);
  if (s.startsWith('.')) s = '_' + s.slice(1); // 숨김 파일 방지
  if (s && isReserved(s)) s = '_' + s;
  s = stripTrailingDotsAndSpaces(codePoints(s).slice(0, FILENAME_MAX).join(''));
  return s || FALLBACK_TITLE;
}
