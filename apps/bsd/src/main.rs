// === 0. 설정 변수: 경로는 환경변수로 바꿀 수 있으며 자격 증명은 다루지 않음 ===
/// 기본 메모 폴더 ($HOME 기준). v0.1 콘솔 예제와 같은 위치라 기존 메모를 그대로 읽습니다.
const APP_FOLDER: &str = "MemoNest/Notes";
/// 메모 폴더를 직접 지정하는 환경변수
const NOTES_DIR_ENV: &str = "MEMONEST_NOTES_DIR";
/// 입력 프롬프트
const PROMPT: &str = "> ";
/// 처음 열리는 메모 제목
const DEFAULT_TITLE: &str = "Untitled";
const HELP: &str = "\
명령:
  :new <[폴더/]제목[.txt]>   새 메모 (기본 .md, 예: :new Work/Project/회의록)
  :open <[폴더/]제목[.txt]>  메모 열기
  :mkdir <폴더>             폴더 만들기 (최대 2단계, 예: Work/Project)
  :list                     폴더/메모 목록
  :show                     현재 본문 보기
  :save                     저장
  :quit                     종료 (저장 안 된 변경이 있으면 경고, :quit! 는 강제 종료)
  :help                     도움말
그 밖의 줄은 본문에 추가됩니다. ':'로 시작하는 줄을 쓰려면 '\\:'로 시작하세요.";

use std::{
    env,
    io::{self, BufRead, Write},
    path::PathBuf,
};

use memonest_core::{CoreError, FileStore, NoteLocation};

fn default_root() -> PathBuf {
    env::var_os(NOTES_DIR_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env::var_os("HOME").unwrap_or_else(|| ".".into())).join(APP_FOLDER)
        })
}

#[derive(Debug, PartialEq, Eq)]
enum Flow {
    Continue,
    Quit,
}

/// 한 줄 단위 콘솔 편집기. 입출력을 주입받아 테스트할 수 있습니다.
struct Editor {
    store: FileStore,
    current: NoteLocation,
    body: String,
    dirty: bool,
    quit_warned: bool,
}

impl Editor {
    fn new(store: FileStore) -> Self {
        let current = NoteLocation::parse(DEFAULT_TITLE).expect("기본 제목은 항상 유효");
        Self {
            store,
            current,
            body: String::new(),
            dirty: false,
            quit_warned: false,
        }
    }

    fn handle(&mut self, line: &str, out: &mut impl Write) -> io::Result<Flow> {
        let (command, arg) = match line.strip_prefix(':') {
            Some(rest) => rest.split_once(' ').unwrap_or((rest, "")),
            None => {
                let text = line
                    .strip_prefix("\\:")
                    .map_or(line.to_owned(), |t| format!(":{t}"));
                self.body.push_str(&text);
                self.body.push('\n');
                self.dirty = true;
                return Ok(Flow::Continue);
            }
        };
        if command != "quit" {
            self.quit_warned = false;
        }
        let arg = arg.trim();
        match command {
            "quit" if self.dirty && !self.quit_warned => {
                self.quit_warned = true;
                writeln!(out, "저장하지 않은 변경이 있습니다. :save 후 종료하거나, 다시 :quit 또는 :quit! 로 버립니다.")?;
            }
            "quit" | "quit!" => return Ok(Flow::Quit),
            "help" => writeln!(out, "{HELP}")?,
            "list" => self.list(out)?,
            "show" => write!(out, "{}", self.body)?,
            "save" => match self.store.write(&self.current, &self.body) {
                Ok(()) => {
                    self.dirty = false;
                    writeln!(out, "저장됨: {}", self.current.display_path())?;
                }
                Err(e) => writeln!(out, "저장 실패: {e}")?,
            },
            "mkdir" => match self.store.create_folder(arg) {
                Ok(folder) if folder.is_empty() => writeln!(out, "폴더 이름을 입력하세요")?,
                Ok(folder) => writeln!(out, "폴더: {folder}/")?,
                Err(e) => writeln!(out, "{e}")?,
            },
            "new" | "open" if arg.is_empty() => {
                writeln!(out, "사용법: :{command} [폴더/]제목[.txt]")?
            }
            "new" => match NoteLocation::parse(arg) {
                Ok(location) if self.store.exists(&location) => writeln!(
                    out,
                    "이미 있는 메모입니다: {} (:open 으로 여세요)",
                    location.display_path()
                )?,
                Ok(location) => {
                    self.switch(location, String::new(), out)?;
                    self.dirty = true; // 새 메모는 :save 해야 파일이 생깁니다
                }
                Err(e) => writeln!(out, "{e}")?,
            },
            "open" => {
                match NoteLocation::parse(arg).and_then(|l| self.store.read(&l).map(|b| (l, b))) {
                    Ok((location, body)) => {
                        self.switch(location, body, out)?;
                        write!(out, "{}", self.body)?;
                    }
                    Err(e) => writeln!(out, "열 수 없음: {e}")?,
                }
            }
            other => writeln!(out, "알 수 없는 명령 :{other} (:help)")?,
        }
        Ok(Flow::Continue)
    }

    fn switch(
        &mut self,
        location: NoteLocation,
        body: String,
        out: &mut impl Write,
    ) -> io::Result<()> {
        if self.dirty {
            writeln!(
                out,
                "주의: {}의 저장하지 않은 변경을 버렸습니다",
                self.current.display_path()
            )?;
        }
        self.current = location;
        self.body = body;
        self.dirty = false;
        writeln!(out, "편집 중: {}", self.current.display_path())
    }

    fn list(&self, out: &mut impl Write) -> io::Result<()> {
        let listing = self
            .store
            .folders()
            .and_then(|f| Ok((f, self.store.list()?)));
        let (folders, notes) = match listing {
            Ok(v) => v,
            Err(e) => return writeln!(out, "목록 실패: {e}"),
        };
        writeln!(out, "/")?;
        let print_notes = |out: &mut dyn Write, folder: &str, depth: usize| -> io::Result<()> {
            for n in notes.iter().filter(|n| n.folder == folder) {
                writeln!(out, "{}{}", "  ".repeat(depth), n.file_name())?;
            }
            Ok(())
        };
        print_notes(out, "", 1)?;
        for folder in &folders {
            let depth = folder.split('/').count();
            let name = folder.rsplit('/').next().unwrap_or(folder);
            writeln!(out, "{}{name}/", "  ".repeat(depth))?;
            print_notes(out, folder, depth + 1)?;
        }
        Ok(())
    }
}

fn run(store: FileStore, input: impl BufRead, mut out: impl Write) -> io::Result<()> {
    writeln!(
        out,
        "MEMONEST BSD v{} | {}",
        env!("CARGO_PKG_VERSION"),
        store.root().display()
    )?;
    writeln!(out, ":help 로 명령 보기")?;
    let mut editor = Editor::new(store);
    writeln!(out, "편집 중: {}", editor.current.display_path())?;
    let mut lines = input.lines();
    loop {
        write!(out, "{PROMPT}")?;
        out.flush()?;
        let Some(line) = lines.next() else { break };
        let line = line?;
        if editor.handle(line.trim_end_matches('\r'), &mut out)? == Flow::Quit {
            break;
        }
    }
    Ok(())
}

fn main() {
    let result = FileStore::open(default_root())
        .map_err(|e: CoreError| io::Error::other(e.to_string()))
        .and_then(|store| run(store, io::stdin().lock(), io::stdout().lock()));
    if let Err(err) = result {
        eprintln!("MemoNest error: {err}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 입력 줄들을 실행하고 (출력, 저장소 루트)를 돌려줍니다.
    fn session(lines: &[&str]) -> (String, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let store = FileStore::open(dir.path()).unwrap();
        let mut out = Vec::new();
        run(store, lines.join("\n").as_bytes(), &mut out).unwrap();
        (String::from_utf8(out).unwrap(), dir)
    }

    #[test]
    fn writes_note_into_folder_as_plain_file() {
        let (out, dir) = session(&[
            ":mkdir Work/Project",
            ":new Work/Project/회의록",
            "# 안건",
            "\\:콜론 줄",
            ":save",
            ":quit",
        ]);
        assert!(out.contains("저장됨: Work/Project/회의록.md"), "{out}");
        let text = std::fs::read_to_string(dir.path().join("Work/Project/회의록.md")).unwrap();
        assert_eq!(text, "# 안건\n:콜론 줄\n");
    }

    #[test]
    fn reopens_existing_note_including_v01_root_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Old.md"), "v0.1 본문\n").unwrap(); // v0.1 예제가 만든 파일
        let store = FileStore::open(dir.path()).unwrap();
        let mut out = Vec::new();
        run(store, ":open Old\n추가\n:save\n:quit".as_bytes(), &mut out).unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.path().join("Old.md")).unwrap(),
            "v0.1 본문\n추가\n"
        );
    }

    #[test]
    fn rejects_third_level_folder_and_keeps_running() {
        let (out, _dir) = session(&[":mkdir A/B/C", ":new A/B/C/x", ":help", ":quit"]);
        assert_eq!(
            out.matches("최대 2단계까지 만들 수 있습니다").count(),
            2,
            "{out}"
        );
        assert!(out.contains(":mkdir <폴더>"));
    }

    #[test]
    fn quit_warns_once_when_unsaved() {
        let (out, _dir) = session(&["본문", ":quit", ":quit"]);
        assert_eq!(out.matches("저장하지 않은 변경").count(), 1, "{out}");
        let (out, _dir) = session(&["본문", ":quit!", ":help"]);
        assert!(
            !out.contains(":mkdir <폴더>"),
            "강제 종료 후 명령이 실행되면 안 됨"
        );
    }

    #[test]
    fn list_shows_tree_and_txt_format() {
        let (out, _dir) = session(&[
            ":new Inbox",
            ":save",
            ":new Work/Plan.txt",
            ":save",
            ":list",
            ":quit",
        ]);
        assert!(
            out.contains("/\n  Inbox.md\n  Work/\n    Plan.txt\n"),
            "{out}"
        );
    }

    #[test]
    fn new_refuses_to_overwrite_existing_note() {
        let (out, dir) = session(&[":new Keep", "원본", ":save", ":new Keep", ":quit"]);
        assert!(out.contains("이미 있는 메모입니다: Keep.md"), "{out}");
        assert_eq!(
            std::fs::read_to_string(dir.path().join("Keep.md")).unwrap(),
            "원본\n"
        );
    }
}
