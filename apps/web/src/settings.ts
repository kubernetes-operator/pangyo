// === 0. 최상단 설정 변수: 동일 정책을 추후 config/client.example.yaml로부터 생성 ===
export const SETTINGS = Object.freeze({
  historyLimit: 250, // 각 노트에 영속화할 최대 Undo/Redo 기록 개수
  maxFolderDepth: 2, // 루트부터 허용하는 폴더 깊이(폴더 최대 2개)
  idleAfterMs: 60_000, // 키 입력 간격이 이보다 길면 그 사이는 작성 시간에서 제외
  saveDebounceMs: 300, // 저장 요청 지연, 너무 잦은 IndexedDB 작업 방지
  timerTickMs: 1000, // 열어둔 시간 갱신 간격
  sessionSaveEveryMs: 10_000, // 입력이 없어도 열어둔 시간을 이 간격으로 저장
  minSessionOpenMs: 2_000, // 입력 없이 이보다 짧게 연 세션은 기록하지 않음
  clockTickMs: 10_000, // 상단 시계 갱신 간격
  sessionListMax: 20, // 화면에 보여줄 최근 편집 세션 수
  indentation: '  ', // Markdown 블록 들여쓰기: 공백 두 칸
  smallScreenQuery: '(max-width:760px)', // 이 폭 이하는 단일 화면으로 취급
  syncIntervalMs: 20_000, // 로그인 상태에서 주기적 동기화 간격
  leaseIdleReleaseMs: 120_000, // 이 시간 동안 입력이 없으면 편집 임대를 반납 (다른 기기가 편집할 수 있게)
});

export type Locale = 'ko' | 'en';

// === 1. UI 문구 (요구 13: 한국어/영어) ===
export const WORDS: Record<Locale, Record<string, string>> = {
  ko: {
    folders: '디렉터리',
    newFolder: '+ 폴더',
    newNote: '+ 메모',
    search: '검색',
    trees: '연관 그래프: 준비 중',
    depthHint: '폴더 2단계까지',
    helpSummary: '표시 설명',
    helpDepth: '폴더 2단계까지: 폴더 안에 폴더를 한 번 더 만들 수 있습니다(예: 업무/회의). 한 폴더 안의 폴더 개수 제한은 없습니다.',
    helpDevice: '배터리·네트워크·블루투스: 이 기기의 상태입니다. 브라우저가 알려 주지 않는 값은 --로 표시합니다.',
    helpTree: '아래 목록: 루트(/) → 폴더 → 메모 순서로 보입니다. 굵게 강조된 것이 지금 연 메모와 그 폴더입니다.',
    battery: '배터리',
    net: '네트워크',
    bluetooth: '블루투스',
    online: '온라인',
    offline: '오프라인',
    newNoteTitle: '새 메모 제목',
    newFolderName: '새 폴더 이름',
    folderHint: '폴더 안에 폴더는 "상위/하위"로 입력 (2단계까지). Enter로 만들기',
    saveIn: '저장 위치',
    ok: '만들기',
    cancel: '취소',
    writing: '메모',
    focus: '집중 모드 (F6)',
    split: '2패널',
    undo: '실행취소',
    redo: '다시실행',
    export: '다운로드',
    import: '가져오기',
    sessions: '편집 세션 기록',
    typingTime: '작성',
    openTime: '열림',
    account: '계정',
    related: '연관 메모',
  },
  en: {
    folders: 'Folders',
    newFolder: '+ Folder',
    newNote: '+ Note',
    search: 'Search',
    trees: 'Related graph: coming soon',
    depthHint: 'Up to 2 folder levels',
    helpSummary: 'What these mean',
    helpDepth: 'Up to 2 folder levels: you can create one folder inside another (e.g. Work/Meetings). No limit on how many folders a folder holds.',
    helpDevice: 'Battery · Network · Bluetooth: status of this device. Values the browser does not provide show as --.',
    helpTree: 'List below: root (/) → folders → notes. Bold highlight = the note you have open and its folder.',
    battery: 'Battery',
    net: 'Network',
    bluetooth: 'Bluetooth',
    online: 'online',
    offline: 'offline',
    newNoteTitle: 'New note title',
    newFolderName: 'New folder name',
    folderHint: 'For a folder inside a folder type "parent/child" (2 levels). Press Enter to create',
    saveIn: 'Save in',
    ok: 'Create',
    cancel: 'Cancel',
    writing: 'Writer',
    focus: 'Focus (F6)',
    split: 'Split view',
    undo: 'Undo',
    redo: 'Redo',
    export: 'Download',
    import: 'Import',
    sessions: 'Editing sessions',
    typingTime: 'Typing',
    openTime: 'Open',
    account: 'Account',
    related: 'Related notes',
  },
};

export const PLACEHOLDERS: Record<Locale, { search: string; title: string; editor: string }> = {
  ko: {
    search: '제목 또는 본문',
    title: '메모 제목',
    editor: '여기에 생각을 작성하세요. Markdown을 그대로 입력합니다.',
  },
  en: { search: 'Title or body', title: 'Note title', editor: 'Write here. Plain Markdown text.' },
};
