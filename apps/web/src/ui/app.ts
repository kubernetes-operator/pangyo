// === 화면 연결: v0.1 app.js의 동작을 그대로 옮긴 것 (서버 계정/동기화는 구현하지 않음) ===
import { SETTINGS, WORDS, PLACEHOLDERS, type Locale } from '../settings';
import { localStore } from '../storage/indexeddb';
import { parseFolder, folderAncestors, safeFilename } from '../domain/paths';
import { remember, restore, type HistoryKind } from '../domain/history';
import {
  indentLines,
  isIdle,
  tickIncrement,
  totalActiveMs,
  timeText,
} from '../domain/editing';
import type { EditSession, FolderRecord, Note, NoteFormat } from '../domain/types';

type Mode = 'split' | 'organizer' | 'editor';

// === 1. 내부 UI 상태 ===
const state = {
  notes: [] as Note[],
  folders: [] as FolderRecord[],
  selectedId: null as string | null,
  selectedFolder: '',
  locale: 'ko' as Locale,
  note: null as Note | null,
  mode: 'split' as Mode,
  lastInputAt: 0,
  activeSessionId: null as string | null,
  lastTimerTick: Date.now(),
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
  localStore.setMeta('locale', locale).catch(console.error);
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
  for (const s of (state.note?.sessions ?? []).slice(-SETTINGS.sessionListMax).reverse()) {
    const p = document.createElement('p');
    const end = s.endedAt ? new Date(s.endedAt).toLocaleString() : '...';
    p.textContent = `${new Date(s.startedAt).toLocaleString()} - ${end} (${timeText(s.activeMs || 0)})`;
    node.append(p);
  }
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
  $('timer').textContent = timeText(totalActiveMs(note.sessions));
  $('save-status').textContent = 'LOCAL SAVED';
  renderSessions();
}

async function persist(note: Note): Promise<void> {
  // 같은 파일의 입력 순서를 보장하도록 직렬화합니다.
  const copy = structuredClone(note);
  state.saving = state.saving.catch(console.error).then(() => localStore.saveNote(copy));
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

function endSession(): void {
  const session = activeSession();
  if (session && !session.endedAt) {
    session.endedAt = nowISO();
    scheduleSave();
  }
  state.activeSessionId = null;
}

function editActivity(): void {
  const note = state.note;
  if (!note) return;
  const current = Date.now();
  if (state.activeSessionId && isIdle(current, state.lastInputAt, SETTINGS.idleAfterMs)) endSession();
  if (!state.activeSessionId) {
    const session: EditSession = { id: uid(), startedAt: nowISO(), endedAt: null, activeMs: 0 };
    note.sessions.push(session);
    state.activeSessionId = session.id;
  }
  state.lastInputAt = current;
  state.lastTimerTick = current;
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
  state.activeSessionId = null;
  state.lastInputAt = 0;
  renderEditor();
  displayTree();
  await localStore.setMeta('selectedId', id);
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
  await localStore.saveNote(note);
  state.notes.push(note);
  await selectNote(note.id);
}

async function addFolder(raw: string): Promise<void> {
  const path = parseFolder(raw, SETTINGS.maxFolderDepth);
  if (!path) return;
  for (const part of folderAncestors(path)) {
    if (!state.folders.some((f) => f.path === part)) {
      await localStore.saveFolder(part);
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

function updateTimer(): void {
  const current = Date.now();
  const session = activeSession();
  if (session) {
    if (isIdle(current, state.lastInputAt, SETTINGS.idleAfterMs) || document.visibilityState === 'hidden') {
      endSession();
    } else {
      session.activeMs += tickIncrement(current, state.lastTimerTick, SETTINGS.timerTickMs);
      state.lastTimerTick = current;
      scheduleSave();
    }
  }
  if (state.note) $('timer').textContent = timeText(totalActiveMs(state.note.sessions));
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
  document.addEventListener('visibilitychange', () => {
    if (document.visibilityState === 'hidden') {
      endSession();
      void flushSave();
    }
  });
  // 디바운스 저장이 대부분 끝난 상태. 네이티브 어댑터는 내구성 있는 저널을 사용할 예정.
  window.addEventListener('beforeunload', endSession);
  window.addEventListener('online', updateConnectivity);
  window.addEventListener('offline', updateConnectivity);
}

export async function bootstrap(): Promise<void> {
  const locale = await localStore.getMeta<string>('locale');
  state.notes = await localStore.notes();
  state.folders = await localStore.folders();
  setLanguage(locale?.value === 'en' ? 'en' : 'ko');
  const selected = await localStore.getMeta<string>('selectedId');
  const initial = state.notes.find((n) => n.id === selected?.value) ?? state.notes[0];
  if (initial) {
    state.selectedId = initial.id;
    state.note = initial;
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
