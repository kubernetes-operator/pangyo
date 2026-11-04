// === 0. 설정 변수: 환경변수에서 경로를 읽으며 외부 의존성 없음 ===
const APP_FOLDER: &str = "MemoNest/Notes"; // 전용 메모 데이터의 기본 위치
const HELP: &str = "Commands: :new <title>, :open <title>, :list, :save, :quit";

use std::{
    env, fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

// 경로 탐색 문자 및 제어 문자를 제거해 파일명으로 변환합니다.
fn safe_title(input: &str) -> String {
    let result: String = input
        .trim()
        .chars()
        .filter(|c| !matches!(c, '/' | '\\' | ':' | '\0' | '\n' | '\r') && !c.is_control())
        .take(100)
        .collect();
    if result.is_empty() || result == "." || result == ".." {
        "Untitled".to_owned()
    } else {
        result
    }
}
fn default_root() -> PathBuf {
    env::var("MEMONEST_NOTES_DIR")
        .ok()
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env::var("HOME").unwrap_or_else(|_| ".".into())).join(APP_FOLDER)
        })
}
fn save_atomic(root: &Path, title: &str, content: &str) -> io::Result<()> {
    // 같은 디렉터리 임시 파일에 쓴 뒤 이름을 교체하여 일부 덮어쓰기를 줄입니다.
    let path = root.join(format!("{}.md", title));
    let temp = root.join(format!("{}.md.tmp", title));
    fs::write(&temp, content.as_bytes())?;
    fs::rename(&temp, &path)
}
fn run() -> io::Result<()> {
    let root = default_root();
    fs::create_dir_all(&root)?;
    let mut title = String::from("Untitled");
    let mut body = String::new();
    println!("MEMONEST BSD v0.1 | {} | {}", root.display(), HELP);
    println!("Edit [{}] - Enter plain lines, :save to write", title);
    loop {
        print!("> ");
        io::stdout().flush()?;
        let mut line = String::new();
        if io::stdin().read_line(&mut line)? == 0 {
            break;
        }
        let trimmed = line.trim_end_matches(['\n', '\r']);
        if trimmed == ":quit" {
            break;
        } else if trimmed == ":list" {
            for item in fs::read_dir(&root)? {
                let path = item?.path();
                if path.extension().is_some_and(|x| x == "md") {
                    println!(
                        "  {}",
                        path.file_name().unwrap_or_default().to_string_lossy()
                    );
                }
            }
        } else if let Some(name) = trimmed.strip_prefix(":new ") {
            title = safe_title(name);
            body.clear();
            println!("New: {}", title);
        } else if let Some(name) = trimmed.strip_prefix(":open ") {
            let desired = safe_title(name);
            let path = root.join(format!("{}.md", desired));
            match fs::read_to_string(&path) {
                Ok(text) => {
                    title = desired;
                    body = text;
                    println!("Loaded [{}]:\n{}", title, body)
                }
                Err(e) => eprintln!("Cannot open: {}", e),
            }
        } else if trimmed == ":save" {
            save_atomic(&root, &title, &body)?;
            println!("Saved {}.md", title);
        } else {
            body.push_str(trimmed);
            body.push('\n');
        }
    }
    Ok(())
}
fn main() {
    if let Err(err) = run() {
        eprintln!("MemoNest error: {err}");
        std::process::exit(1)
    }
}
