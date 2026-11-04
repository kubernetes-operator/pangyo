//! 메모 형식: 기본 Markdown, 선택 Plain Text (요구 12)
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub enum NoteFormat {
    #[default]
    Md,
    Txt,
}

impl NoteFormat {
    pub const ALL: [NoteFormat; 2] = [NoteFormat::Md, NoteFormat::Txt];

    /// 파일 확장자 (점 제외)
    pub fn extension(self) -> &'static str {
        match self {
            NoteFormat::Md => "md",
            NoteFormat::Txt => "txt",
        }
    }

    /// 확장자로 형식을 찾습니다 (대소문자 무시). 지원하지 않으면 None.
    pub fn from_extension(ext: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|f| f.extension().eq_ignore_ascii_case(ext))
    }
}

impl fmt::Display for NoteFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.extension())
    }
}
