//! MemoNest 공통 도메인 (ADR-0002).
//!
//! - [`paths`]: 폴더(최대 2단계)·파일명 규칙. 웹(TypeScript)과 같은 결과를 내야 하며,
//!   기준 데이터는 `packages/protocol/fixtures/paths.json`입니다.
//! - [`store`]: 사람이 읽을 수 있는 `Notes/<폴더>/<제목>.md` 파일을 정본으로 쓰는 로컬 저장소.
//!
//! 동시 편집 잠금(요구 #28)과 동기화는 이 크레이트에 아직 없습니다 (승인 전 구현 금지).

// === 0. 공통 정책 변수 (config/client.example.yaml과 같은 값) ===
/// 루트에서 허용하는 폴더 깊이 (요구 21)
pub const MAX_FOLDER_DEPTH: usize = 2;
/// 폴더 이름 한 칸의 최대 길이 (코드 포인트)
pub const FOLDER_SEGMENT_MAX: usize = 70;
/// 메모 제목(확장자 제외)의 최대 길이 (코드 포인트)
pub const FILENAME_MAX: usize = 200;
/// 제목이 비었을 때 쓰는 이름
pub const FALLBACK_TITLE: &str = "Untitled";

pub mod error;
pub mod format;
pub mod paths;
pub mod store;

pub use error::{CoreError, Result};
pub use format::NoteFormat;
pub use paths::{folder_ancestors, parse_folder, safe_filename, FolderError};
pub use store::{FileStore, NoteLocation};
