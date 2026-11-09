//! 웹(TypeScript)과 같은 기준 데이터로 경로 규칙을 검사합니다.
//! 기준: packages/protocol/fixtures/paths.json (apps/web/src/domain/domain.test.ts도 같은 파일 사용)
use memonest_core::{parse_folder, safe_filename, FolderError};
use serde::Deserialize;

const FIXTURES: &str = include_str!("../../../packages/protocol/fixtures/paths.json");

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Fixtures {
    max_depth: usize,
    parse_folder: Vec<FolderCase>,
    safe_filename: Vec<NameCase>,
}

#[derive(Deserialize)]
struct FolderCase {
    input: String,
    ok: Option<String>,
    error: Option<String>,
}

#[derive(Deserialize)]
struct NameCase {
    input: String,
    output: String,
}

fn fixtures() -> Fixtures {
    serde_json::from_str(FIXTURES).expect("paths.json 형식 오류")
}

#[test]
fn parse_folder_matches_fixtures() {
    let f = fixtures();
    let mut failures = Vec::new();
    for case in &f.parse_folder {
        let got = parse_folder(&case.input, f.max_depth);
        let ok = match (&got, &case.ok, case.error.as_deref()) {
            (Ok(v), Some(expected), None) => v == expected,
            (Err(FolderError::TooDeep { .. }), None, Some("too_deep")) => true,
            (Err(FolderError::InvalidName(_)), None, Some("invalid")) => true,
            _ => false,
        };
        if !ok {
            failures.push(format!("{:?} → {:?}", case.input, got));
        }
    }
    assert!(failures.is_empty(), "불일치:\n{}", failures.join("\n"));
}

#[test]
fn safe_filename_matches_fixtures() {
    let mut failures = Vec::new();
    for case in &fixtures().safe_filename {
        let got = safe_filename(&case.input);
        if got != case.output {
            failures.push(format!(
                "{:?} → {:?} (기대 {:?})",
                case.input, got, case.output
            ));
        }
    }
    assert!(failures.is_empty(), "불일치:\n{}", failures.join("\n"));
}
