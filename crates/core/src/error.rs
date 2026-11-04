//! 공통 오류 타입. 메시지는 사용자에게 그대로 보여줄 수 있도록 한국어로 씁니다.
use std::{io, path::PathBuf};

use crate::paths::FolderError;

pub type Result<T, E = CoreError> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error(transparent)]
    Folder(#[from] FolderError),
    #[error("사용할 수 없는 메모 위치입니다: {0}")]
    InvalidLocation(String),
    #[error("심볼릭 링크를 통한 접근은 허용하지 않습니다: {}", .0.display())]
    Symlink(PathBuf),
    #[error("UTF-8 텍스트 파일이 아닙니다: {}", .0.display())]
    NotUtf8(PathBuf),
    #[error("메모를 찾을 수 없습니다: {0}")]
    NotFound(String),
    #[error("같은 이름의 메모가 이미 있습니다: {0}")]
    Conflict(String),
    #[error("파일 입출력 오류: {0}")]
    Io(#[from] io::Error),
    #[cfg(feature = "workspace")]
    #[error("인덱스 데이터베이스 오류: {0}")]
    Index(#[from] rusqlite::Error),
    #[cfg(feature = "workspace")]
    #[error("인덱스 데이터 형식 오류: {0}")]
    Json(#[from] serde_json::Error),
}
