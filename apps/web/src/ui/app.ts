// === 화면 연결: v0.1 app.js의 동작을 그대로 옮긴 것 (서버 계정/동기화는 구현하지 않음) ===
import { SETTINGS, WORDS, PLACEHOLDERS, type Locale } from '../settings';
import type { NoteRepository } from '../storage/repository';
import { parseFolder, folderAncestors, safeFilename } from '../domain/paths';
import { remember, restore, type HistoryKind } from '../domain/history';
import {
  indentLines,
  isNegligibleSession,
  tickIncrement,
  timeText,
  totalActiveMs,
  totalOpenMs,
  typingIncrement,
} from '../domain/editing';
import type { EditSession, FolderRecord, Note, NoteFormat } from '../domain/types';

type Mode = 'split' | 'organizer' | 'editor';

/** bootstrap()에서 실행 환경에 맞게 주입됩니다 (IndexedDB 또는 파일). */
let repo: NoteRepository;

// === 1. 내부 UI 상태 ===
const state = {
  notes: [] as Note[],
  folders: [] as FolderRecord[],
  selectedId: null as string | null,
  selectedFolder: '',
  locale: 'ko' as Locale,
  note: null as Note | null,
  mode: 'split' as Mode,
  lastInputAt: 0, // 현재 세션의 마지막 키 입력 시각 (0 = 아직 없음)
  activeSessionId: null as string | null,
  lastOpenTick: Date.now(), // 열어둔 시간을 마지막으로 더한 시각
  lastSaveRequestAt: 0,
  saveTimer: undefined as ReturnType<typeof setTimeout> | undefined,
  saving: Promise.resolve() as Promise<unknown>,
};

function $<T extends HTMLElement = HTMLElement>(id: string): T {
  const node = document.getElementById(id);
  if (!node) throw new Error(`missing #${id}`);
  return node as T;
}
const nowISO = () => new Date().toISOString();
const uid = () => crypto.randomUUID();
const isSmallScreen = () => window.matchMedia(SETTINGS.smallScreenQuery).matches;
const editor = () => $<HTMLTextAreaElement>('editor');

function setLanguage(locale: Locale): void {
  state.locale = locale;
  document.documentElement.lang = locale;
  document.querySelectorAll<HTMLElement>('[data-i18n]').forEach((node) => {
    const key = node.dataset.i18n ?? '';
    node.textContent = WORDS[locale][key] ?? node.textContent;
  });
  $<HTMLInputElement>('search').placeholder = PLACEHOLDERS[locale].search;
  $<HTMLInputElement>('title').placeholder = PLACEHOLDERS[locale].title;
  editor().placeholder = PLACEHOLDERS[locale].editor;
  repo.setMeta('locale', locale).catch(console.error);
}

function updateConnectivity(): void {
  $('connectivity').textContent = navigator.onLine ? 'LOCAL · ONLINE' : 'LOCAL · OFFLINE';
  // 브라우저는 Wi-Fi 종류/BT 및 배터리 정보를 안정적으로 제공하지 않습니다.
  $('wifi').textContent = navigator.onLine ? 'NET: ONLINE' : 'NET: OFFLINE';
  $('battery').textContent = 'BAT: --';
  $('bluetooth').textContent = 'BT: --';
}

function renderClock(): void {
  $('clock').textContent = new Date().toLocaleString(state.locale === 'ko' ? 'ko-KR' : 'en-US');
}

function changeMode(mode: Mode): void {
  state.mode = mode;
  document.body.className = `mode-${mode}`;
}

function selectFolder(folder: string): void {
  state.selectedFolder = folder;
  $('folder-path').textContent = '/' + folder;
  displayTree();
}

function displayTree(): void {
  const tree = $('tree');
  tree.replaceChildren();
  const query = $<HTMLInputElement>('search').value.toLocaleLowerCase();
  const rows = state.notes.filter(
    (n) => !query || `${n.title} ${n.content}`.toLocaleLowerCase().includes(query),
  );
  const folderPaths = [
    ...new Set([...state.folders.map((x) => x.path), ...rows.flatMap((n) => folderAncestors(n.folder))]),
  ].sort();
  const makeButton = (
    title: string,
    cls: string,
    indent: number,
    selected: boolean,
    handler: () => void,
  ) => {
    const b = document.createElement('button');
    b.type = 'button';
    b.className = `${cls} tree-indent-${Math.min(indent, 3)}`;
    b.textContent = title;
    b.setAttribute('aria-selected', String(selected));
    b.addEventListener('click', handler);
    tree.append(b);
  };
  makeButton('▸ /', 'folder', 0, state.selectedFolder === '', () => selectFolder(''));
  // 폴더와 노트를 계층 경로 순서대로 표시합니다(파일 선택은 UUID 기준).
  const byFolder = new Map<string, Note[]>();
  for (const note of rows) {
    const key = note.folder || '';
    if (!byFolder.has(key)) byFolder.set(key, []);
    byFolder.get(key)!.push(note);
  }
  const showNotes = (folder: string, depth: number) => {
    const notes = (byFolder.get(folder) ?? []).sort((a, b) => b.modifiedAt.localeCompare(a.modifiedAt));
    for (const note of notes) {
      makeButton(`· ${note.title}.${note.format}`, 'note', depth, note.id === state.selectedId, () => {
        void selectNote(note.id);
      });
    }
  };
  showNotes('', 1);
  for (const folder of folderPaths) {
    const depth = folder.split('/').length;
    makeButton('▸ ' + folder.split('/').at(-1), 'folder', depth, state.selectedFolder === folder, () =>
      selectFolder(folder),
    );
    showNotes(folder, Math.min(depth + 1, 3));
  }
  $('count').textContent = `${rows.length} notes`;
}

function renderSessions(): void {
  const node = $('sessions');
  node.replaceChildren();
  const words = WORDS[state.locale];
  for (const s of (state.note?.sessions ?? []).slice(-SETTINGS.sessionListMax).reverse()) {
    const p = document.createElement('p');
    const end = s.endedAt ? new Date(s.endedAt).toLocaleString() : '...';
    p.textContent = `${new Date(s.startedAt).toLocaleString()} - ${end} (${words.typingTime} ${timeText(
      s.activeMs,
    )} / ${words.openTime} ${timeText(s.openMs)})`;
    node.append(p);
  }
}

function renderTimers(): void {
  const sessions = state.note?.sessions ?? [];
  $('timer-active').textContent = timeText(totalActiveMs(sessions));
  $('timer-open').textContent = timeText(totalOpenMs(sessions));
}

function renderEditor(): void {
  const note = state.note;
  if (!note) {
    $<HTMLInputElement>('title').value = '';
    editor().value = '';
    $('save-status').textContent = 'NO NOTE';
    return;
  }
  $<HTMLInputElement>('title').value = note.title;
  editor().value = note.content;
  $<HTMLSelectElement>('format').value = note.format;
  $('format-label').textContent = '.' + note.format;
  $('created-at').textContent = new Date(note.createdAt).toLocaleString();
  renderTimers();
  $('save-status').textContent = 'LOCAL SAVED';
  renderSessions();
}

async function persist(note: Note): Promise<void> {
  // 같은 파일의 입력 순서를 보장하도록 직렬화합니다.
  const copy = structuredClone(note);
  state.saving = state.saving.catch(console.error).then(() => repo.saveNote(copy));
  try {
    await state.saving;
    $('save-status').textContent = 'LOCAL SAVED';
  } catch (e) {
    $('save-status').textContent = 'SAVE FAILED';
    console.error(e);
    alert('로컬 저장이 실패했습니다. 공간/권한을 확인하세요.');
  }
}

function scheduleSave(): void {
  if (!state.note) return;
  $('save-status').textContent = 'SAVING...';
  state.lastSaveRequestAt = Date.now();
  clearTimeout(state.saveTimer);
  const target = state.note;
  state.saveTimer = setTimeout(() => void persist(target), SETTINGS.saveDebounceMs);
}

async function flushSave(): Promise<void> {
  clearTimeout(state.saveTimer);
  if (state.note) await persist(state.note);
}

function activeSession(): EditSession | undefined {
  return state.note?.sessions.find((s) => s.id === state.activeSessionId);
}

/** 메모가 열려 있고 창이 보일 때 세션을 시작합니다 (이미 진행 중이면 그대로). */
function startSession(): void {
  const note = state.note;
  if (!note || state.activeSessionId || document.visibilityState === 'hidden') return;
  const session: EditSession = { id: uid(), startedAt: nowISO(), endedAt: null, activeMs: 0, openMs: 0 };
  note.sessions.push(session);
  state.activeSessionId = session.id;
  state.lastInputAt = 0;
  state.lastOpenTick = Date.now();
  renderSessions();
}

/** 메모 전환·창 숨김·종료 시 세션을 닫습니다. 마지막 tick 이후의 열어둔 시간도 더합니다. */
function endSession(): void {
  const note = state.note;
  const session = activeSession();
  if (note && session && !session.endedAt) {
    session.openMs += tickIncrement(Date.now(), state.lastOpenTick, SETTINGS.timerTickMs);
    session.endedAt = nowISO();
    if (isNegligibleSession(session, SETTINGS.minSessionOpenMs)) {
      note.sessions = note.sessions.filter((s) => s !== session);
    }
    scheduleSave();
  }
  state.activeSessionId = null;
  state.lastInputAt = 0;
}

/** 키 입력(편집)마다 호출: 직전 입력과의 간격을 작성 시간에 더합니다. */
function editActivity(): void {
  if (!state.note) return;
  startSession();
  const session = activeSession();
  if (!session) return;
  const current = Date.now();
  session.activeMs += typingIncrement(current, state.lastInputAt, SETTINGS.idleAfterMs);
  state.lastInputAt = current;
  renderTimers();
  renderSessions();
}

function applyEdit(field: 'title' | 'content', value: string): void {
  const note = state.note;
  if (!note || note[field] === value) return;
  remember(note, SETTINGS.historyLimit);
  note[field] = value;
  note.modifiedAt = nowISO();
  editActivity();
  scheduleSave();
  displayTree();
}

function restoreHistory(kind: HistoryKind): void {
  const note = state.note;
  if (!note || !restore(note, kind, SETTINGS.historyLimit)) return;
  note.modifiedAt = nowISO();
  editActivity();
  scheduleSave();
  renderEditor();
  displayTree();
}

async function selectNote(id: string): Promise<void> {
  if (id === state.selectedId) return;
  endSession();
  await flushSave();
  state.selectedId = id;
  state.note = state.notes.find((n) => n.id === id) ?? null;
  startSession();
  renderEditor();
  displayTree();
  await repo.setMeta('selectedId', id);
  if (isSmallScreen()) changeMode('editor');
}

async function addNote(title: string, content = '', format: NoteFormat = 'md'): Promise<void> {
  const sanitized = safeFilename(title);
  const exists = state.notes.some(
    (n) => n.folder === state.selectedFolder && n.title === sanitized && n.format === format,
  );
  if (exists) {
    alert('같은 폴더에 같은 이름의 메모가 있습니다.');
    return;
  }
  const time = nowISO();
  const note: Note = {
    id: uid(),
    title: sanitized,
    folder: state.selectedFolder,
    format,
    content,
    createdAt: time,
    modifiedAt: time,
    sessions: [],
    undo: [],
    redo: [],
  };
  await repo.saveNote(note);
  state.notes.push(note);
  await selectNote(note.id);
}

async function addFolder(raw: string): Promise<void> {
  const path = parseFolder(raw, SETTINGS.maxFolderDepth);
  if (!path) return;
  for (const part of folderAncestors(path)) {
    if (!state.folders.some((f) => f.path === part)) {
      await repo.saveFolder(part);
      state.folders.push({ path: part });
    }
  }
  selectFolder(path);
}

function downloadNote(): void {
  const note = state.note;
  if (!note) return;
  const blob = new Blob([note.content], { type: 'text/plain;charset=utf-8' });
  const a = document.createElement('a');
  const url = URL.createObjectURL(blob);
  a.href = url;
  a.download = `${safeFilename(note.title)}.${note.format}`;
  a.click();
  setTimeout(() => URL.revokeObjectURL(url), 2500);
}

function indentSelection(outdent = false): void {
  const box = editor();
  if (!state.note) return;
  const r = indentLines(box.value, box.selectionStart, box.selectionEnd, outdent, SETTINGS.indentation);
  box.setRangeText(r.text, r.start, r.end, 'select');
  applyEdit('content', box.value);
}

/** 1초마다: 창이 보이는 동안 열어둔 시간을 더하고, 주기적으로 저장합니다. */
function updateTimer(): void {
  const session = activeSession();
  if (session && document.visibilityState !== 'hidden') {
    const current = Date.now();
    session.openMs += tickIncrement(current, state.lastOpenTick, SETTINGS.timerTickMs);
    state.lastOpenTick = current;
    if (current - state.lastSaveRequestAt >= SETTINGS.sessionSaveEveryMs) scheduleSave();
  }
  renderTimers();
}

function toggleFocus(): void {
  changeMode(state.mode === 'editor' ? 'organizer' : 'editor');
}

function bindEvents(): void {
  $('add-note').onclick = async () => {
    const value = prompt(state.locale === 'ko' ? '새 메모 제목' : 'Note title', 'Untitled');
    if (value !== null && value.trim()) await addNote(value);
  };
  $('add-folder').onclick = async () => {
    const value = prompt('폴더명 (예: Work/Project)', state.selectedFolder ? state.selectedFolder + '/' : '');
    if (value === null) return;
    try {
      await addFolder(value);
    } catch (e) {
      alert((e as Error).message);
    }
  };
  $<HTMLInputElement>('title').oninput = (e) =>
    applyEdit('title', safeFilename((e.target as HTMLInputElement).value));
  editor().oninput = (e) => applyEdit('content', (e.target as HTMLTextAreaElement).value);
  $<HTMLSelectElement>('format').onchange = (e) => {
    const note = state.note;
    if (!note) return;
    remember(note, SETTINGS.historyLimit);
    note.format = (e.target as HTMLSelectElement).value === 'txt' ? 'txt' : 'md';
    note.modifiedAt = nowISO();
    scheduleSave();
    renderEditor();
    displayTree();
  };
  $('search').oninput = displayTree;
  $('language-btn').onclick = () => setLanguage(state.locale === 'ko' ? 'en' : 'ko');
  $('toggle-btn').onclick = toggleFocus;
  $('split-btn').onclick = () => changeMode('split');
  $('undo-btn').onclick = () => restoreHistory('undo');
  $('redo-btn').onclick = () => restoreHistory('redo');
  $('indent-btn').onclick = () => indentSelection();
  $('outdent-btn').onclick = () => indentSelection(true);
  $('export-btn').onclick = downloadNote;
  $('import-btn').onclick = () => $('import-file').click();
  $<HTMLInputElement>('import-file').onchange = async (e) => {
    const input = e.target as HTMLInputElement;
    const file = input.files?.[0];
    if (!file) return;
    const format: NoteFormat = file.name.toLowerCase().endsWith('.txt') ? 'txt' : 'md';
    await addNote(file.name.replace(/\.(md|txt)$/i, ''), await file.text(), format);
    input.value = '';
  };
  editor().onkeydown = (e) => {
    if (e.key === 'Tab') {
      e.preventDefault();
      indentSelection(e.shiftKey);
    }
    const mod = e.ctrlKey || e.metaKey;
    if (mod && e.key.toLowerCase() === 'z') {
      e.preventDefault();
      restoreHistory(e.shiftKey ? 'redo' : 'undo');
    } else if (mod && e.key.toLowerCase() === 'y') {
      e.preventDefault();
      restoreHistory('redo');
    }
  };
  document.addEventListener('keydown', (e) => {
    if (e.key === 'F6') {
      e.preventDefault();
      toggleFocus();
    }
  });
  // 창을 숨기면 세션을 닫고, 다시 보이면 새 세션(재개)을 시작합니다.
  document.addEventListener('visibilitychange', () => {
    if (document.visibilityState === 'hidden') {
      endSession();
      void flushSave();
    } else {
      startSession();
    }
  });
  // 종료 직전 저장은 최선 노력. 네이티브 어댑터는 내구성 있는 저널을 사용할 예정.
  window.addEventListener('pagehide', () => {
    endSession();
    void flushSave();
  });
  window.addEventListener('online', updateConnectivity);
  window.addEventListener('offline', updateConnectivity);
}

export async function bootstrap(repository: NoteRepository): Promise<void> {
  repo = repository;
  $('hint').textContent = `Local-first · ${repo.describe()} · Sync / login not implemented`;
  const locale = await repo.getMeta<string>('locale');
  state.notes = await repo.listNotes();
  state.folders = await repo.listFolders();
  setLanguage(locale === 'en' ? 'en' : 'ko');
  const selected = await repo.getMeta<string>('selectedId');
  const initial = state.notes.find((n) => n.id === selected) ?? state.notes[0];
  if (initial) {
    state.selectedId = initial.id;
    state.note = initial;
    startSession();
  }
  renderEditor();
  displayTree();
  renderClock();
  updateConnectivity();
  bindEvents();
  setInterval(updateTimer, SETTINGS.timerTickMs);
  setInterval(renderClock, SETTINGS.clockTickMs);
}

export function reportStorageFailure(e: unknown): void {
  console.error(e);
  $('save-status').textContent = 'STORAGE UNAVAILABLE';
  alert('브라우저 IndexedDB를 사용할 수 없습니다. 보안 모드/저장공간을 확인하세요.');
}
