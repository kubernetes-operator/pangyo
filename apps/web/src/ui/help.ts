// === 도움말: 처음 실행할 때 보여 주는 MemoNest·Markdown 사용법 (2026-11-13 사용 의견) ===
// 메모로 자동 저장하지 않습니다(여러 기기에서 처음 실행할 때 같은 메모가 겹쳐 올라가지 않게).
// '메모로 만들기'를 누르면 지금 프로젝트에 메모로 저장합니다.
import type { Locale } from '../settings';

export const HELP_TITLE: Record<Locale, string> = { ko: 'MemoNest 사용법', en: 'MemoNest guide' };

/**
 * 처음 실행 때 만드는 첫 문서(사용법)의 고정 ID (2026-11-13 사용 의견: 첫 문서로 남기기).
 * 모든 기기가 같은 ID를 써서, 처음 실행한 기기가 여럿이어도 계정에는 한 개만 남습니다.
 */
export const WELCOME_NOTE_ID = 'a0e1c0de-0000-4000-8000-000000000001';

/** 손대지 않은 사용법 문서인지 (그러면 서버에 올리지 않고, 서버에 같은 문서가 있으면 그것으로 맞춤) */
export const isPristineWelcome = (note: { id: string; title: string; content: string }): boolean =>
  note.id === WELCOME_NOTE_ID &&
  (Object.keys(HELP_MARKDOWN) as Locale[]).some((l) => note.content === HELP_MARKDOWN[l] && note.title === HELP_TITLE[l]);

export const HELP_MARKDOWN: Record<Locale, string> = {
  ko: `# MemoNest 사용법

## 기본
- **+ 메모**: 새 메모. 제목은 \`날짜 시분 Memo\`로 채워지며 바로 바꿀 수 있습니다. Enter = 만들기
- **+ 폴더**: 폴더 안에 폴더 한 번 더 (\`업무/회의\`, 2단계까지)
- **프로젝트**: 맨 위 프로젝트 줄에서 고르기·만들기. 프로젝트마다 폴더와 메모가 따로 보입니다
- 메모는 입력하는 즉시 이 기기에 저장되고, 로그인(앱은 장비 연결)하면 다른 기기와 동기화됩니다
- 본문 아래: 프로젝트 이동 · 폴더 이동 · 메모 삭제 · 다운로드

## Markdown 쓰는 법
제목은 줄 앞에 \`#\`을 붙입니다.

# 제목 1
## 제목 2
### 제목 3

**굵게** 는 \`**굵게**\`, *기울임* 은 \`*기울임*\`, ~~취소선~~ 은 \`~~취소선~~\`

- 목록은 \`- \`로 시작
  - Tab 키로 들여쓰기, Shift+Tab으로 내어쓰기
1. 번호 목록은 \`1. \`
- [ ] 할 일은 \`- [ ] \`, 끝낸 일은 \`- [x] \`

> 인용은 \`> \`로 시작

링크: \`[MemoNest](https://example.com)\`

코드는 \`\` \`한 줄\` \`\` 또는 백틱 세 개로 감쌉니다:

\`\`\`
여러 줄 코드
\`\`\`

표:

| 항목 | 내용 |
|---|---|
| 회의 | 월요일 10시 |

---
가로줄은 \`---\` 입니다.

## 단축키
- **F6**: 메모만 보기 ↔ 목록만 보기
- **Ctrl+Z / Ctrl+Y**: 실행 취소 / 다시 실행 (메모마다 250번까지 기억)
- **Tab / Shift+Tab**: 들여쓰기 / 내어쓰기

이 도움말은 상단 **도움말**에서 언제든 다시 볼 수 있습니다.
`,
  en: `# MemoNest guide

## Basics
- **+ Note**: new note. The title starts as \`date time Memo\`; edit it and press Enter
- **+ Folder**: one folder inside another (\`Work/Meetings\`, 2 levels)
- **Projects**: pick or create one in the project row at the top; each project has its own folders and notes
- Notes are saved on this device as you type and sync once you sign in (apps: link the device)
- Below the editor: move to project · move to folder · delete · download

## Markdown
Start a line with \`#\` for headings.

# Heading 1
## Heading 2

**bold** is \`**bold**\`, *italic* is \`*italic*\`, ~~strike~~ is \`~~strike~~\`

- Lists start with \`- \` (Tab / Shift+Tab to indent)
1. Numbered lists with \`1. \`
- [ ] Tasks with \`- [ ] \`, done with \`- [x] \`

> Quotes start with \`> \`

Links: \`[MemoNest](https://example.com)\`, code: \`\` \`inline\` \`\` or three backticks.

| Item | Note |
|---|---|
| Meeting | Mon 10:00 |

---
## Shortcuts
- **F6**: note only ↔ list only
- **Ctrl+Z / Ctrl+Y**: undo / redo (250 steps per note)
- **Tab / Shift+Tab**: indent / outdent

Open this guide again from **Help** at the top.
`,
};
