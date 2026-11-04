//! 경로/파일명 규칙 (요구 21). 웹 `apps/web/src/domain/paths.ts`와 같은 결과를 냅니다.
//! 기준 데이터: `packages/protocol/fixtures/paths.json` (tests/parity.rs가 검사).
//! 길이는 모두 유니코드 코드 포인트 기준입니다.

use unicode_normalization::UnicodeNormalization;

use crate::{FALLBACK_TITLE, FILENAME_MAX, FOLDER_SEGMENT_MAX};

/// Windows 예약 이름 (대소문자 무시, 첫 '.' 앞부분 기준)
const WINDOWS_RESERVED: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FolderError {
    #[error("폴더는 최대 {max}단계까지 만들 수 있습니다")]
    TooDeep { max: usize },
    #[error("사용할 수 없는 폴더 이름입니다: {0:?}")]
    InvalidName(String),
}

/// 파일 이름에 쓸 수 없는 문자: \ / : * ? " < > | 와 제어 문자(U+0000–U+001F, U+007F)
fn is_forbidden(c: char) -> bool {
    matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
        || c <= '\u{1f}'
        || c == '\u{7f}'
}

/// 앞뒤 공백: 유니코드 White_Space + U+FEFF (JS `trim()` + U+0085와 같은 집합)
fn is_edge_space(c: char) -> bool {
    c.is_whitespace() || c == '\u{feff}'
}

fn is_reserved(name: &str) -> bool {
    let base = name.split('.').next().unwrap_or_default();
    WINDOWS_RESERVED
        .iter()
        .any(|r| r.eq_ignore_ascii_case(base))
}

fn strip_trailing_dots_and_spaces(s: &str) -> &str {
    s.trim_end_matches(['.', ' '])
}

/// `'Work/Project'` 형태 입력을 검증·정규화합니다. 빈 문자열은 루트입니다.
pub fn parse_folder(raw: &str, max_depth: usize) -> Result<String, FolderError> {
    let bits: Vec<String> = raw
        .replace('\\', "/")
        .split('/')
        .filter(|s| !s.is_empty())
        .map(|s| s.trim_matches(is_edge_space).nfc().collect())
        .collect();
    if bits.len() > max_depth {
        return Err(FolderError::TooDeep { max: max_depth });
    }
    for bit in &bits {
        let invalid = bit.is_empty()
            || bit.starts_with('.') // '.', '..', 숨김 폴더
            || bit.ends_with('.')
            || bit.chars().any(is_forbidden)
            || is_reserved(bit)
            || bit.chars().count() > FOLDER_SEGMENT_MAX;
        if invalid {
            return Err(FolderError::InvalidName(bit.clone()));
        }
    }
    Ok(bits.join("/"))
}

/// `"A/B"` → `["A", "A/B"]` : 상위 폴더부터 순서대로
pub fn folder_ancestors(path: &str) -> Vec<String> {
    if path.is_empty() {
        return Vec::new();
    }
    let bits: Vec<&str> = path.split('/').collect();
    (1..=bits.len()).map(|i| bits[..i].join("/")).collect()
}

/// 메모 제목을 모든 OS에서 안전한 파일 이름(확장자 제외)으로 바꿉니다.
pub fn safe_filename(name: &str) -> String {
    let replaced: String = name
        .trim_matches(is_edge_space)
        .nfc()
        .map(|c| if is_forbidden(c) { '_' } else { c })
        .collect();
    let mut s = strip_trailing_dots_and_spaces(&replaced).to_owned();
    if let Some(rest) = s.strip_prefix('.') {
        s = format!("_{rest}"); // 숨김 파일 방지
    }
    if !s.is_empty() && is_reserved(&s) {
        s.insert(0, '_');
    }
    let truncated: String = s.chars().take(FILENAME_MAX).collect();
    let result = strip_trailing_dots_and_spaces(&truncated);
    if result.is_empty() {
        FALLBACK_TITLE.to_owned()
    } else {
        result.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ancestors() {
        assert!(folder_ancestors("").is_empty());
        assert_eq!(folder_ancestors("A/B"), ["A", "A/B"]);
    }

    #[test]
    fn too_deep_reports_limit() {
        assert_eq!(
            parse_folder("a/b/c", 2),
            Err(FolderError::TooDeep { max: 2 })
        );
    }
}
