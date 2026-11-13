// === 화면 연결: 메모 편집(v0.1 동작) + 계정 + 동기화(로드맵 06, ADR-0003 A안) ===
import { SETTINGS, WORDS, PLACEHOLDERS, type Locale } from '../settings';
import type { NoteRepository } from '../storage/repository';
import { isTauri } from '../storage';
import { createAccountPanel } from '../account/panel';
import { accountApi, ApiError, downloadApi, searchApi, ServerUnavailable, setTransport } from '../account/api';
import type { DeletionChoice } from '../account/device-panel';
import type { DeviceInfo, ExternalDeletion } from '../storage/tauri';
import { SyncEngine, type ConflictChoice, type LeaseState, type SyncStatus, type SyncStore } from '../sync/engine';
import { deviceLabel, newClientId, syncApi } from '../sync/api';
import { renderSyncBanner, syncStatusText } from './sync-view';
import { parseFolder, folderAncestors, safeFilename, FolderError } from '../domain/paths';
import { askConfirm, askMove, askName, type MoveTarget } from './name-dialog';
import { depthOf, folderMoveProblem, isWithin, joinPath, lastSegment, parentOf, remap } from '../domain/tree-ops';
import { HELP_MARKDOWN, HELP_TITLE } from './help';
import { buildLabel, pickUpdate, type UpdateState } from './app-update';
import {
  CURRENT_PROJECT_META_KEY,
  DEFAULT_PROJECT,
  PROJECTS_META_KEY,
  folderKey,
  inProject,
  normalizeProjectName,
  parseFolderKey,
  projectOf,
  type LocalProject,
} from '../domain/projects';
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
  // 동기화 (로그인 시)
  sync: null as SyncEngine | null,
  syncTimer: undefined as ReturnType<typeof setInterval> | undefined,
  syncStatus: null as SyncStatus | null,
  syncBlocked: false, // 이 기기의 메모가 다른 계정에 묶여 있음
  lease: null as LeaseState | null, // 선택한 메모의 편집 임대 상태
  leaseRequest: null as Promise<void> | null,
  lastEditAt: 0, // 마지막 편집 시각 (임대 자동 반납 판단)
  syncTick: null as Promise<void> | null,
  /** 본문·제목·형식·Undo를 바꿨지만 아직 저장하지 않은 메모 (아니면 세션 기록만 저장) */
  unsaved: new Set<string>(),
  // 프로젝트 (ADR-0005). 데스크톱·모바일 앱은 파일 배치(3단계) 전까지 기본 프로젝트만
  projects: [] as LocalProject[],
  project: DEFAULT_PROJECT,
  /** 기본 프로젝트 표시 이름 (서버 계정 설정, '' = "기본") */
  defaultProjectName: '',
};
const DEFAULT_PROJECT_NAME_META_KEY = 'defaultProjectName';
/** 프로젝트 화면 사용 여부 (ADR-0005: 웹·데스크톱·모바일 모두. 데스크톱은 `@프로젝트` 폴더) */
const projectsEnabled = () => true;

// 데스크톱·모바일 앱의 장비 연결 상태 (ADR-0004). 웹에서는 쓰지 않음
const device = {
  info: null as DeviceInfo | null,
  revoked: false, // 서버가 401: 장비 폐기됨
  deletions: [] as ExternalDeletion[], // 앱 밖에서 지워진 동기화 메모
  retry: undefined as ReturnType<typeof setTimeout> | undefined,
  api: null as typeof import('../storage/tauri').deviceApi | null,
  panel: null as { open(): Promise<void>; refresh(): Promise<void> } | null,
};
/** 이 탭의 동기화 클라이언트 ID (탭마다 달라서 같은 브라우저의 두 탭도 동시에 편집하지 못함) */
const CLIENT_ID = newClientId();

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
  if (repo) renderSyncInfo();
  updateConnectivity();
  renderModeButtons();
  if (repo) renderProjects();
  editor().placeholder = PLACEHOLDERS[locale].editor;
  repo.setMeta('locale', locale).catch(console.error);
}

function updateConnectivity(): void {
  $('connectivity').textContent = navigator.onLine ? 'LOCAL · ONLINE' : 'LOCAL · OFFLINE';
  // 기기 상태 줄 (요구 09). 브라우저는 Wi-Fi 종류·블루투스 상태를 주지 않음 → '--'.
  // 배터리는 Battery Status API가 있는 브라우저(Chrome, Android)에서만 표시
  const w = WORDS[state.locale];
  $('wifi').textContent = `${w.net}: ${navigator.onLine ? w.online : w.offline}`;
  $('bluetooth').textContent = `${w.bluetooth}: --`;
  if (battery.level === null) $('battery').textContent = `${w.battery}: --`;
  else $('battery').textContent = `${w.battery}: ${Math.round(battery.level * 100)}%${battery.charging ? ' ⚡' : ''}`;
}

const battery = { level: null as number | null, charging: false };
type BatteryManager = EventTarget & { level: number; charging: boolean };
async function watchBattery(): Promise<void> {
  const nav = navigator as Navigator & { getBattery?: () => Promise<BatteryManager> };
  if (!nav.getBattery) return;
  try {
    const b = await nav.getBattery();
    const update = () => {
      battery.level = b.level;
      battery.charging = b.charging;
      updateConnectivity();
    };
    b.addEventListener('levelchange', update);
    b.addEventListener('chargingchange', update);
    update();
  } catch {
    /* 지원하지 않거나 권한 없음 → '--' 유지 */
  }
}

function renderClock(): void {
  $('clock').textContent = new Date().toLocaleString(state.locale === 'ko' ? 'ko-KR' : 'en-US');
}

function changeMode(mode: Mode): void {
  state.mode = mode;
  document.body.className = `mode-${mode}`;
  renderModeButtons();
}

/** 상단 버튼 문구: F6이 지금 무엇을 할지 그대로 보여 줌 (2026-11-13 사용 의견: '집중 모드'·'2패널'이 어려움) */
function renderModeButtons(): void {
  const w = WORDS[state.locale];
  $('toggle-btn').textContent = state.mode === 'editor' ? w.showList : w.showEditor;
  $('split-btn').hidden = state.mode === 'split';
}

/** 화면 설정이라 기기(브라우저)마다: localStorage (E2E는 미리 설정해 건너뜀) */
const HELP_SHOWN_KEY = 'memonest.helpShown';
function helpShown(): boolean {
  try {
    return localStorage.getItem(HELP_SHOWN_KEY) === '1';
  } catch {
    return true; // 저장소를 쓸 수 없으면 매번 띄우지 않음
  }
}
function markHelpShown(): void {
  try {
    localStorage.setItem(HELP_SHOWN_KEY, '1');
  } catch {
    /* 무시 */
  }
}

function openHelp(): void {
  const dialog = $<HTMLDialogElement>('help-dialog');
  $('help-title').textContent = HELP_TITLE[state.locale];
  $('help-text').textContent = HELP_MARKDOWN[state.locale];
  $('help-close').textContent = WORDS[state.locale].close;
  if (!dialog.open) dialog.showModal();
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
    (n) =>
      !n.sync?.deleting &&
      inProject(n, state.project) && (!query || `${n.title} ${n.content}`.toLocaleLowerCase().includes(query)),
  );
  const projectFolders = state.folders
    .map((x) => parseFolderKey(x.path))
    .filter((f) => f.project === state.project)
    .map((f) => f.path);
  const folderPaths = [...new Set([...projectFolders, ...rows.flatMap((n) => folderAncestors(n.folder))])].sort();
  /** 한 행 = 항목 버튼 + ⋯ 메뉴 버튼. 오른쪽 클릭(모바일은 길게 누르기)도 메뉴. 드래그해서 폴더에 놓으면 이동 */
  const makeButton = (
    title: string,
    cls: string,
    indent: number,
    selected: boolean,
    handler: () => void,
    item: TreeItem,
  ) => {
    const row = document.createElement('div');
    row.className = 'tree-row';
    const b = document.createElement('button');
    b.type = 'button';
    b.className = `${cls} tree-indent-${Math.min(indent, 3)}`;
    b.textContent = title;
    b.setAttribute('aria-selected', String(selected));
    b.addEventListener('click', handler);
    row.append(b);
    if (item.kind !== 'root') {
      b.draggable = true;
      b.addEventListener('dragstart', (e) => {
        dragging = item;
        e.dataTransfer?.setData('text/plain', title);
        if (e.dataTransfer) e.dataTransfer.effectAllowed = 'move';
      });
      b.addEventListener('dragend', () => (dragging = null));
      b.addEventListener('contextmenu', (e) => {
        e.preventDefault();
        openTreeMenu(item, b);
      });
      const menu = document.createElement('button');
      menu.type = 'button';
      menu.className = 'row-menu';
      menu.textContent = '⋯';
      menu.title = WORDS[state.locale].menu;
      menu.setAttribute('aria-label', `${WORDS[state.locale].menu}: ${title}`);
      menu.addEventListener('click', () => openTreeMenu(item, menu));
      row.append(menu);
    }
    if (item.kind !== 'note') {
      const target = item.kind === 'root' ? '' : item.path;
      b.addEventListener('dragover', (e) => {
        if (!dragging || !canDrop(dragging, target)) return;
        e.preventDefault();
        row.classList.add('drop-target');
      });
      b.addEventListener('dragleave', () => row.classList.remove('drop-target'));
      b.addEventListener('drop', (e) => {
        e.preventDefault();
        row.classList.remove('drop-target');
        const moving = dragging;
        dragging = null;
        if (moving) void dropOnFolder(moving, target);
      });
    }
    tree.append(row);
  };
  makeButton('▸ /', 'folder', 0, state.selectedFolder === '', () => selectFolder(''), { kind: 'root' });
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
      makeButton(
        `· ${note.title}.${note.format}`,
        'note',
        depth,
        note.id === state.selectedId,
        () => void selectNote(note.id),
        { kind: 'note', id: note.id },
      );
    }
  };
  showNotes('', 1);
  for (const folder of folderPaths) {
    const depth = folder.split('/').length;
    makeButton(
      '▸ ' + folder.split('/').at(-1),
      'folder',
      depth,
      state.selectedFolder === folder,
      () => selectFolder(folder),
      { kind: 'folder', path: folder },
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
    renderNoteProject();
    renderNoteFolder();
    return;
  }
  $<HTMLInputElement>('title').value = note.title;
  editor().value = note.content;
  renderNoteProject();
  renderNoteFolder();
  $<HTMLSelectElement>('format').value = note.format;
  $('format-label').textContent = '.' + note.format;
  $('created-at').textContent = new Date(note.createdAt).toLocaleString();
  renderTimers();
  $('save-status').textContent = 'LOCAL SAVED';
  renderSessions();
  setReadOnly(state.lease?.kind === 'denied');
  renderSyncInfo();
  renderRelated();
}

/** 연관 메모 (로드맵 07): 동기화 중이고 서버에 있는 메모일 때만. 펼쳤을 때만 서버에 요청 */
function renderRelated(): void {
  const panel = $<HTMLDetailsElement>('related-panel');
  const note = state.note;
  const available = !!state.sync && !!note?.sync && note.sync.version > 0;
  panel.hidden = !available;
  if (available && panel.open) void loadRelated();
}

async function loadRelated(): Promise<void> {
  const note = state.note;
  const list = $('related-list');
  if (!note) return;
  const hits = await searchApi.similar(note.id).catch(() => []);
  if (state.note !== note) return;
  list.replaceChildren(
    ...hits
      .filter((h) => state.notes.some((n) => n.id === h.id))
      .map((h) => {
        const li = document.createElement('li');
        const b = document.createElement('button');
        b.type = 'button';
        b.textContent = `${h.folder ? h.folder + '/' : ''}${h.title}.${h.format}`;
        const small = document.createElement('small');
        small.textContent = h.snippet.slice(0, 80);
        b.append(small);
        b.onclick = () => void selectNote(h.id);
        li.append(b);
        return li;
      }),
  );
  if (!list.childElementCount) {
    const li = document.createElement('li');
    li.textContent = '—';
    list.append(li);
  }
}

// ---------- 동기화 (로드맵 06) ----------

function setReadOnly(locked: boolean): void {
  editor().readOnly = locked;
  $<HTMLInputElement>('title').readOnly = locked;
  $<HTMLSelectElement>('format').disabled = locked;
  document.body.classList.toggle('note-locked', locked);
}

function renderSyncInfo(): void {
  $('sync-status').textContent = state.sync || state.syncBlocked || device.revoked
    ? syncStatusText(state.syncStatus, state.locale, state.syncBlocked, device.deletions.length, device.revoked)
    : '';
  renderSyncBanner($('sync-banner'), {
    note: state.note,
    lease: state.lease,
    status: state.syncStatus,
    locale: state.locale,
    onRetry: () => void requestEdit(),
    onResolve: (choice) => void resolveConflict(choice),
  });
}

function syncStore(): SyncStore {
  return {
    list: () => state.notes,
    save: (note) => persist(note, true),
    add: async (note) => {
      await repo.saveNote(note); // 먼저 저장: 디스크 다시 읽기(refreshFromDisk)가 저장 전 메모를 지우지 않게
      state.notes.push(note);
    },
    remove: async (id) => {
      state.notes = state.notes.filter((n) => n.id !== id);
      await repo.deleteNote(id);
      if (state.selectedId === id) {
        state.selectedId = null;
        state.note = null;
        state.lease = null;
      }
    },
    getMeta: (key) => repo.getMeta(key),
    setMeta: (key, value) => repo.setMeta(key, value),
    ...(projectsEnabled()
      ? {
          projects: {
            list: () => state.projects,
            save: async (project: LocalProject) => {
              const at = state.projects.findIndex((p) => p.id === project.id);
              if (at < 0) state.projects.push(project);
              else state.projects[at] = project;
              await saveProjects();
            },
            remove: async (id: string) => {
              state.projects = state.projects.filter((p) => p.id !== id);
              await saveProjects();
              if (state.project === id) await switchProject(DEFAULT_PROJECT);
            },
            changed: () => {
              renderProjects();
              displayTree();
            },
            setDefaultName: async (name: string) => {
              if (name === state.defaultProjectName) return;
              state.defaultProjectName = name;
              await repo.setMeta(DEFAULT_PROJECT_NAME_META_KEY, name);
              renderProjects();
            },
          },
        }
      : {}),
    changed: (ids) => {
      displayTree();
      const selected = state.note;
      // 내가 편집 중(임대 보유)인 메모는 입력 위치를 흔들지 않도록 다시 그리지 않음
      if (selected && ids.includes(selected.id) && state.lease?.kind !== 'held') renderEditor();
      else if (!selected && ids.length) renderEditor();
      else renderSyncInfo();
    },
  };
}

/** 로그인(웹) 또는 장비 연결(앱) 확인 시 동기화 시작 (이미 실행 중이면 무시) */
async function startSync(accountId: string, label: string): Promise<void> {
  if (state.sync) return;
  const boundTo = await repo.getMeta<string>('syncAccountId');
  if (boundTo && boundTo !== accountId) {
    state.syncBlocked = true; // 다른 계정의 메모를 섞지 않음
    renderSyncInfo();
    return;
  }
  const engine = new SyncEngine(syncApi, syncStore(), {
    clientId: CLIENT_ID,
    label,
    onStatus: (status) => {
      state.syncStatus = status;
      renderSyncInfo();
    },
    onLeaseLost: (noteId) => {
      if (state.note?.id === noteId) {
        state.lease = null;
        void requestEdit(); // 다시 받아 보고, 다른 기기가 가져갔으면 읽기 전용
      }
    },
  });
  await engine.init();
  state.sync = engine;
  state.syncBlocked = false;
  if (!boundTo) {
    await repo.setMeta('syncAccountId', accountId);
    await engine.adoptLocalNotes(); // 이 기기의 기존 메모를 계정에 올림
  }
  await syncTick();
  state.syncTimer = setInterval(() => void syncTick(), SETTINGS.syncIntervalMs);
  renderSyncInfo();
}

/**
 * 동기화 한 번. 앱(파일 저장소)은 그 전에 디스크를 다시 읽어 바깥 편집·이름 변경·새 파일을 반영하고,
 * 끝난 뒤 바깥 삭제 목록을 갱신합니다. 겹쳐 실행하지 않음.
 */
function syncTick(): Promise<void> {
  const engine = state.sync;
  if (!engine) return Promise.resolve();
  state.syncTick ??= (async () => {
    if (isTauri()) {
      await engine.idle();
      await refreshFromDisk();
      await engine.adoptLocalNotes(); // 바깥에서 새로 만든 파일도 올리기 대상
    }
    await engine.syncOnce();
    if (isTauri()) await refreshDeletions();
  })()
    .catch(console.error)
    .finally(() => {
      state.syncTick = null;
    });
  return state.syncTick;
}

const sameNote = (a: Note, b: Note) =>
  a.title === b.title && a.folder === b.folder && a.format === b.format && a.content === b.content &&
  a.modifiedAt === b.modifiedAt && JSON.stringify(a.sync ?? null) === JSON.stringify(b.sync ?? null) &&
  (a.projectId ?? null) === (b.projectId ?? null);

/** 파일 저장소를 다시 읽어 화면 상태에 반영 (편집 중인 메모는 건드리지 않음) */
async function refreshFromDisk(): Promise<void> {
  await state.saving.catch(() => undefined);
  const before = new Set(state.notes.map((n) => n.id));
  const disk = await repo.listNotes();
  const fresh = new Map(disk.map((n) => [n.id, n]));
  // 저장하지 않은 편집이 있거나 편집 중(임대 보유)인 메모는 화면 쪽이 최신 → 디스크 내용으로 바꾸지 않음
  const busy = (n: Note) => state.unsaved.has(n.id) || (n === state.note && state.lease?.kind === 'held');
  const changed: string[] = [];
  const next: Note[] = [];
  for (const current of state.notes) {
    const found = fresh.get(current.id);
    fresh.delete(current.id);
    if (!found) {
      // 디스크에 없음: 다시 읽기 전부터 있던 메모면 바깥에서 지워진 것 (읽는 동안 새로 생긴 메모는 유지)
      if (before.has(current.id) && !busy(current)) changed.push(current.id);
      else next.push(current);
    } else if (sameNote(current, found) || busy(current)) {
      next.push(current);
    } else if (current === state.note) {
      // 선택한 메모는 같은 객체를 유지 (진행 중인 편집 세션 기록이 이 객체에 있음)
      const { title, folder, format, content, modifiedAt, sync, projectId } = found;
      Object.assign(current, { title, folder, format, content, modifiedAt, sync, projectId });
      next.push(current);
      changed.push(current.id);
    } else {
      next.push(found);
      changed.push(found.id);
    }
  }
  for (const added of fresh.values()) {
    next.push(added);
    changed.push(added.id);
  }
  state.notes = next;
  state.folders = await repo.listFolders();
  // 앱 밖에서 만든 `@이름` 폴더는 core가 새 프로젝트로 등록 → 목록 다시 읽기 (ADR-0005)
  const projects = await repo.getMeta<LocalProject[]>(PROJECTS_META_KEY);
  if (projects && projects.length !== state.projects.length) {
    state.projects = projects;
    renderProjects();
    displayTree();
  }
  if (state.note && !next.includes(state.note)) {
    state.selectedId = null;
    state.note = null;
    state.lease = null;
    state.activeSessionId = null;
  }
  if (changed.length) {
    displayTree();
    if (!state.note || changed.includes(state.note.id)) renderEditor();
  }
}

/** 앱 밖에서 지워진 동기화 메모 목록 갱신 → 상단에 '확인 필요' 표시 */
async function refreshDeletions(): Promise<void> {
  if (!device.api) return;
  device.deletions = await device.api.externalDeletions().catch(() => device.deletions);
  renderSyncInfo();
}

/** 바깥 삭제 처리: 서버에서도 삭제 / 다시 받기 */
async function resolveDeletion(item: ExternalDeletion, choice: DeletionChoice): Promise<string> {
  const engine = state.sync;
  if (!engine || !device.api) return 'notSynced';
  let outcome: string;
  try {
    if (choice === 'restore') {
      outcome = (await engine.restoreRemote(item.id)) ? 'restored' : 'missing';
    } else {
      const result = await engine.deleteRemote(item.id, item.sync.version);
      outcome = result;
      // 지운 뒤 다른 기기에서 바뀌었으면 지우지 않고 서버 내용을 다시 받음 (어느 쪽도 잃지 않음)
      if (result === 'changed') await engine.restoreRemote(item.id);
    }
  } catch (e) {
    return (e as { offline?: boolean }).offline ? 'offline' : String((e as Error).message ?? e);
  }
  if (outcome !== 'busy') await device.api.forgetExternalDeletion(item.id).catch(console.error);
  await refreshDeletions();
  displayTree();
  return outcome;
}

/** 앱: 장비 연결이 확인되면 계정을 확인하고 동기화 시작 (오프라인이면 이전에 묶인 계정으로) */
async function startDeviceSync(info: DeviceInfo): Promise<void> {
  device.info = info;
  device.revoked = false;
  clearTimeout(device.retry);
  let accountId: string | undefined;
  try {
    const account = await accountApi.me();
    if (!account) return onDeviceUnauthorized();
    accountId = account.id;
  } catch (e) {
    if (!(e instanceof ServerUnavailable)) console.error(e);
    accountId = await repo.getMeta<string>('syncAccountId');
  }
  if (!accountId) {
    // 처음 연결했는데 서버에 닿지 않음 → 잠시 뒤 다시
    device.retry = setTimeout(() => void startDeviceSync(info), SETTINGS.syncIntervalMs);
    renderSyncInfo();
    return;
  }
  await startSync(accountId, info.deviceName);
  void checkAppUpdate().catch(() => undefined);
  clearInterval(appUpdate.timer);
  appUpdate.timer = setInterval(() => void checkAppUpdate().catch(() => undefined), UPDATE_CHECK_MS);
}

async function stopDeviceSync(): Promise<void> {
  device.info = null;
  device.revoked = false;
  clearTimeout(device.retry);
  await stopSync();
}

/** 서버가 장비 토큰을 거부(401): 동기화를 멈추고 다시 연결하도록 안내 (메모는 이 기기에 그대로) */
function onDeviceUnauthorized(): void {
  if (device.revoked) return;
  device.revoked = true;
  void stopSync().then(() => device.panel?.refresh());
}

/** 앱: 서버 요청을 장비 토큰 전송으로 바꾸고 장비 연결 화면을 붙임 */
// === 앱 안 업데이트 (A안) ===
const appUpdate = {
  state: null as UpdateState | null,
  timer: undefined as ReturnType<typeof setInterval> | undefined,
};
const UPDATE_CHECK_MS = 6 * 60 * 60 * 1000; // 6시간마다 (앱을 켜 두는 경우)

/** 서버에 게시한 설치 파일 목록과 이 앱의 빌드를 비교 → 새 버전이면 상단 버튼 표시. 결과 문구를 돌려줌 */
async function checkAppUpdate(): Promise<string> {
  const ko = state.locale === 'ko';
  const { appApi } = await import('../storage/tauri');
  const info = await appApi.info();
  if (!info.build) return ko ? '개발 빌드라 업데이트를 확인하지 않습니다.' : 'Development build: updates are not checked.';
  if (!device.info) return ko ? '장비를 연결하면 업데이트를 확인할 수 있습니다.' : 'Link this device to check for updates.';
  try {
    appUpdate.state = pickUpdate(info, await downloadApi.list());
  } catch {
    return ko ? '서버에 연결할 수 없어 확인하지 못했습니다.' : 'Could not reach the server.';
  }
  $('update-btn').hidden = !appUpdate.state.file;
  if (appUpdate.state.file) return WORDS[state.locale].updateAvailable + ` (${appUpdate.state.publishedBuild?.slice(0, 7)})`;
  return ko ? '최신 버전입니다.' : 'You are up to date.';
}

async function installAppUpdate(): Promise<void> {
  const update = appUpdate.state;
  const server = device.info?.serverUrl;
  if (!update?.file || !server) return;
  const w = WORDS[state.locale];
  const ok = await askConfirm({
    message: w.updateConfirm.replace('{build}', update.publishedBuild?.slice(0, 7) ?? ''),
    ok: w.updateGet,
    cancel: w.cancel,
  });
  if (!ok) return;
  try {
    const { appApi } = await import('../storage/tauri');
    const link = await downloadApi.link(update.file.id);
    await appApi.openDownload(server + link.url);
    $('project-status').textContent = w.updateOpened;
  } catch (e) {
    console.error(e);
    $('project-status').textContent = w.updateFailed;
  }
}

async function setupDevice(): Promise<void> {
  const tauri = await import('../storage/tauri');
  const { createDevicePanel } = await import('../account/device-panel');
  setTransport(tauri.deviceTransport(onDeviceUnauthorized));
  device.api = tauri.deviceApi;
  const panel = createDevicePanel($<HTMLDialogElement>('account-dialog'), () => state.locale, {
    onPaired: (info) => startDeviceSync(info),
    onUnpaired: () => stopDeviceSync(),
    statusText: () => syncStatusText(state.syncStatus, state.locale, state.syncBlocked, device.deletions.length, device.revoked),
    resolveDeletion,
    revoked: () => device.revoked,
    appVersion: async () => buildLabel(await tauri.appApi.info()),
    checkUpdate: checkAppUpdate,
  });
  $('update-btn').onclick = () => void installAppUpdate();
  device.panel = panel;
  $('account-btn').onclick = () => void panel.open();
  $('sync-status').onclick = () => void panel.open();
  const info = await tauri.deviceApi.status();
  if (info) await startDeviceSync(info);
  await refreshDeletions();
}

async function stopSync(): Promise<void> {
  clearInterval(state.syncTimer);
  const engine = state.sync;
  state.sync = null;
  state.syncStatus = null;
  state.lease = null;
  setReadOnly(false);
  await engine?.stop();
  renderSyncInfo();
}

/** 편집 시작(에디터/제목에 들어갈 때, 임대 없이 입력할 때): 임대를 받고, 다른 기기가 편집 중이면 읽기 전용 */
function requestEdit(): Promise<void> {
  const note = state.note;
  const engine = state.sync;
  if (!note || !engine) return Promise.resolve();
  state.leaseRequest ??= engine
    .beginEdit(note)
    .then((lease) => {
      if (state.note !== note) return void engine.endEdit(note.id);
      state.lease = lease;
      state.lastEditAt = Date.now();
      setReadOnly(lease.kind === 'denied');
      renderSyncInfo();
      // 읽기 전용이 되면 상대가 방금 저장한 최신 내용을 바로 가져와 보여줌
      if (lease.kind === 'denied') void engine.syncOnce();
    })
    .catch(console.error)
    .finally(() => {
      state.leaseRequest = null;
    });
  return state.leaseRequest;
}

function releaseEdit(keepalive = false): void {
  const id = state.note?.id;
  state.lease = null;
  if (id) void state.sync?.endEdit(id, keepalive);
}

/** 내용이 바뀔 때마다: 동기화 대상 표시 + 임대가 없으면 받기 */
function noteChanged(note: Note): void {
  if (!state.sync) return;
  state.lastEditAt = Date.now();
  state.sync.markDirty(note);
  if (note.sync && note.sync.version > 0 && !state.sync.isHolding(note.id)) void requestEdit();
  renderSyncInfo();
}

async function resolveConflict(choice: ConflictChoice): Promise<void> {
  const note = state.note;
  if (!note || !state.sync) return;
  await state.sync.resolveConflict(note, choice);
  renderEditor();
  displayTree();
}

/**
 * 저장. 본문을 바꾸지 않은 메모는(full=false) 세션 기록만 저장해, 열어 둔 사이 바깥 편집기에서 고친 파일을
 * 덮어쓰지 않습니다 (파일 저장소만 해당). 동기화가 바꾼 메모는 full=true.
 */
async function persist(note: Note, full = false): Promise<void> {
  // 같은 파일의 입력 순서를 보장하도록 직렬화합니다.
  const copy = structuredClone(note);
  const sessionsOnly = !full && !state.unsaved.has(note.id) && !!repo.saveSessions;
  state.unsaved.delete(note.id);
  state.saving = state.saving
    .catch(console.error)
    .then(() => (sessionsOnly ? repo.saveSessions!(copy) : repo.saveNote(copy)));
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
  state.unsaved.add(note.id);
  editActivity();
  noteChanged(note);
  scheduleSave();
  displayTree();
}

function restoreHistory(kind: HistoryKind): void {
  const note = state.note;
  if (!note || !restore(note, kind, SETTINGS.historyLimit)) return;
  note.modifiedAt = nowISO();
  state.unsaved.add(note.id);
  editActivity();
  noteChanged(note);
  scheduleSave();
  renderEditor();
  displayTree();
}

async function selectNote(id: string): Promise<void> {
  if (id === state.selectedId) return;
  releaseEdit();
  setReadOnly(false);
  endSession();
  await flushSave();
  state.selectedId = id;
  state.note = state.notes.find((n) => n.id === id) ?? null;
  // 다른 프로젝트의 메모(연관 메모 등)를 열면 그 프로젝트로 이동
  if (state.note && projectsEnabled() && projectOf(state.note) !== state.project) {
    state.project = projectOf(state.note);
    void repo.setMeta(CURRENT_PROJECT_META_KEY, state.project);
    renderProjects();
  }
  // 강조는 지금 연 메모와 그 메모가 있는 폴더만 (2026-11-13 사용 의견)
  if (state.note) {
    state.selectedFolder = state.note.folder;
    $('folder-path').textContent = '/' + state.note.folder;
  }
  startSession();
  renderEditor();
  displayTree();
  await repo.setMeta('selectedId', id);
  if (isSmallScreen()) changeMode('editor');
}

// === 프로젝트 (ADR-0005) ===
const liveProjects = () => state.projects.filter((p) => !p.sync.deleting);
const projectName = (id: string) =>
  id
    ? (state.projects.find((p) => p.id === id)?.name ?? '?')
    : state.defaultProjectName || WORDS[state.locale].defaultProject;
/** 목록 표시: 서버에 아직 못 올린 프로젝트는 ↑, 서버가 거절한 프로젝트는 ⚠ */
const projectLabel = (id: string) => {
  const p = state.projects.find((x) => x.id === id);
  const mark = p?.sync.error ? ' ⚠' : p && p.sync.dirty && state.sync ? ' ↑' : '';
  return projectName(id) + mark;
};

async function saveProjects(): Promise<void> {
  await repo.setMeta(PROJECTS_META_KEY, state.projects);
}

function projectOptions(select: HTMLSelectElement, current: string): void {
  const options = [DEFAULT_PROJECT, ...liveProjects().sort((a, b) => a.name.localeCompare(b.name)).map((p) => p.id)];
  select.replaceChildren(
    ...options.map((id) => {
      const o = document.createElement('option');
      o.value = id;
      o.textContent = projectLabel(id);
      o.selected = id === current;
      return o;
    }),
  );
}

function renderProjects(): void {
  if (!projectsEnabled()) return;
  projectOptions($<HTMLSelectElement>('project-select'), state.project);
  const isDefault = state.project === DEFAULT_PROJECT;
  $<HTMLButtonElement>('delete-project').disabled = isDefault;
  const error = state.projects.find((p) => p.id === state.project)?.sync.error;
  $('project-status').textContent = error ?? '';
  renderNoteProject();
}

function renderNoteProject(): void {
  if (!projectsEnabled()) return;
  const select = $<HTMLSelectElement>('note-project');
  const note = state.note;
  select.disabled = !note || state.lease?.kind === 'denied';
  if (!note) return select.replaceChildren();
  projectOptions(select, projectOf(note));
  for (const o of select.options) o.textContent = WORDS[state.locale].moveTo + o.textContent;
}

async function switchProject(id: string): Promise<void> {
  if (id === state.project) return;
  state.project = id;
  state.selectedFolder = '';
  $('folder-path').textContent = '/';
  await repo.setMeta(CURRENT_PROJECT_META_KEY, id);
  renderProjects();
  displayTree();
  // 지금 연 메모가 다른 프로젝트면 그 프로젝트의 최근 메모로 (없으면 빈 화면)
  if (state.note && !inProject(state.note, id)) {
    const next = state.notes
      .filter((n) => inProject(n, id))
      .sort((a, b) => b.modifiedAt.localeCompare(a.modifiedAt))[0];
    if (next) await selectNote(next.id);
    else {
      releaseEdit();
      endSession();
      await flushSave();
      state.selectedId = null;
      state.note = null;
      state.lease = null;
      renderEditor();
      displayTree();
    }
  }
}

function markProjectDirty(project: LocalProject): void {
  project.sync = { ...project.sync, dirty: true, error: null };
  project.modifiedAt = nowISO();
}

async function addProject(name: string): Promise<void> {
  const time = nowISO();
  const project: LocalProject = { id: uid(), name, createdAt: time, modifiedAt: time, sync: { synced: false, dirty: true } };
  state.projects.push(project);
  await saveProjects();
  await switchProject(project.id);
  void state.sync?.syncOnce();
}

async function renameProject(id: string, name: string): Promise<void> {
  const project = state.projects.find((p) => p.id === id);
  if (!project) return;
  project.name = name;
  markProjectDirty(project);
  await saveProjects();
  renderProjects();
  void state.sync?.syncOnce();
}

/** 프로젝트 지우기: 안에 메모가 있으면 경고 후 폴더·메모까지 (메모는 삭제 대기 → 다 지워진 뒤 프로젝트) */
async function deleteProject(id: string): Promise<void> {
  const project = state.projects.find((p) => p.id === id);
  if (!project) return;
  const w = WORDS[state.locale];
  const notes = visibleNotes().filter((n) => inProject(n, id));
  if (notes.length) {
    const ok = await askConfirm({
      message: w.deleteProjectConfirm.replace('{name}', project.name).replace('{n}', String(notes.length)),
      ok: w.deleteNoteOk,
      cancel: w.cancel,
      danger: true,
    });
    if (!ok || !(await deleteNotes(notes))) return;
  }
  // 이 프로젝트의 빈 폴더 기록도 지움
  for (const key of state.folders.map((f) => f.path).filter((k) => parseFolderKey(k).project === id))
    await repo.deleteFolder(key).catch(console.error);
  state.folders = state.folders.filter((f) => parseFolderKey(f.path).project !== id);
  if (project.sync.synced) project.sync = { ...project.sync, deleting: true };
  else state.projects = state.projects.filter((p) => p.id !== id);
  await saveProjects();
  await switchProject(DEFAULT_PROJECT);
  renderProjects();
  void state.sync?.syncOnce();
}

/** 메모를 다른 프로젝트로 (폴더 경로는 그대로) */
async function moveNoteToProject(target: string): Promise<void> {
  const note = state.note;
  if (!note || projectOf(note) === target) return;
  const clash = state.notes.some(
    (n) =>
      n.id !== note.id &&
      inProject(n, target) &&
      n.folder === note.folder &&
      n.title === note.title &&
      n.format === note.format,
  );
  if (clash) {
    $('project-status').textContent = WORDS[state.locale].moveConflict;
    renderNoteProject();
    return;
  }
  note.projectId = target || null;
  note.modifiedAt = nowISO();
  state.sync?.markDirty(note);
  await repo.saveNote(note);
  state.project = target;
  await repo.setMeta(CURRENT_PROJECT_META_KEY, target);
  state.selectedFolder = note.folder;
  renderProjects();
  displayTree();
}

const projectNameValidator = (selfId?: string) => (v: string) => {
  try {
    normalizeProjectName(v, state.projects, selfId);
    return null;
  } catch (e) {
    return e instanceof FolderError ? e.message : String(e);
  }
};

/** 새 메모 기본 제목: "2026-11-13 1530 Memo" (':'는 파일 이름에 쓸 수 없어 시분을 붙여 씀) */
function defaultMemoTitle(now = new Date()): string {
  const p = (n: number) => String(n).padStart(2, '0');
  const date = `${now.getFullYear()}-${p(now.getMonth() + 1)}-${p(now.getDate())}`;
  return `${date} ${p(now.getHours())}${p(now.getMinutes())} ${WORDS[state.locale].memo}`;
}

/** 지금 프로젝트의 폴더 목록 (빈 폴더 포함, 루트 '' 먼저) */
function projectFolderPaths(): string[] {
  const own = state.folders.map((x) => parseFolderKey(x.path)).filter((f) => f.project === state.project);
  const fromNotes = state.notes.filter((n) => inProject(n, state.project)).flatMap((n) => folderAncestors(n.folder));
  return ['', ...[...new Set([...own.map((f) => f.path), ...fromNotes])].filter(Boolean).sort()];
}

function renderNoteFolder(): void {
  const select = $<HTMLSelectElement>('note-folder');
  const del = $<HTMLButtonElement>('delete-note');
  const note = state.note;
  const locked = !note || state.lease?.kind === 'denied';
  select.disabled = locked;
  del.disabled = locked;
  if (!note) return select.replaceChildren();
  select.replaceChildren(
    ...projectFolderPaths().map((path) => {
      const o = document.createElement('option');
      o.value = path;
      o.textContent = WORDS[state.locale].moveFolder + '/' + path;
      o.selected = path === note.folder;
      return o;
    }),
  );
}

/** 메모를 같은 프로젝트의 다른 폴더로 (같은 이름이 있으면 거절) */
async function moveNoteToFolder(target: string): Promise<void> {
  const note = state.note;
  if (!note || note.folder === target) return;
  const clash = state.notes.some(
    (n) =>
      n.id !== note.id &&
      inProject(n, projectOf(note)) &&
      n.folder === target &&
      n.title === note.title &&
      n.format === note.format,
  );
  if (clash) {
    $('project-status').textContent = WORDS[state.locale].moveConflict;
    renderNoteFolder();
    return;
  }
  note.folder = target;
  note.modifiedAt = nowISO();
  state.sync?.markDirty(note);
  await repo.saveNote(note);
  state.selectedFolder = target;
  $('folder-path').textContent = '/' + target;
  displayTree();
  renderNoteFolder();
}

/** 메모 삭제(확인 창). 서버에 올라간 메모는 삭제 대기로 → 동기화가 서버에서도 지움 */
async function deleteCurrentNote(): Promise<void> {
  const note = state.note;
  if (note) await deleteNoteWithConfirm(note);
}

async function deleteNoteWithConfirm(note: Note): Promise<void> {
  const w = WORDS[state.locale];
  const ok = await askConfirm({
    message: w.deleteNoteConfirm.replace('{title}', `${note.title}.${note.format}`),
    ok: w.deleteNoteOk,
    cancel: w.cancel,
    danger: true,
  });
  if (ok) await deleteNotes([note]);
}

/**
 * 메모 여러 개 지우기 (확인은 호출 쪽). 동기화된 메모는 삭제 대기(화면에서 숨김), 아니면 바로 지움.
 * 동기화 중이 아닌데 서버에 올라간 메모가 있으면 지우지 않고 false
 */
async function deleteNotes(notes: Note[]): Promise<boolean> {
  const w = WORDS[state.locale];
  const synced = (n: Note) => (n.sync?.version ?? 0) > 0;
  if (notes.some(synced) && !state.sync) {
    $('project-status').textContent = w.deleteOffline; // 로그인·장비 연결 전에는 서버 기록을 지울 수 없음
    return false;
  }
  const current = state.note;
  if (current && notes.includes(current)) {
    releaseEdit();
    endSession();
    await flushSave();
  }
  for (const note of notes) {
    state.unsaved.delete(note.id);
    if (synced(note)) {
      note.sync = { ...note.sync!, deleting: true, conflict: null, error: null };
      await repo.saveNote(note);
    } else {
      state.notes = state.notes.filter((n) => n.id !== note.id);
      await repo.deleteNote(note.id);
    }
  }
  if (state.sync && notes.some(synced)) {
    void state.sync.syncOnce().then(() => {
      if (state.notes.some((n) => notes.includes(n) && n.sync?.deleting))
        $('project-status').textContent = w.deleteQueued;
    });
  }
  if (current && notes.includes(current)) {
    state.selectedId = null;
    state.note = null;
    state.lease = null;
    const next = state.notes
      .filter((n) => inProject(n, state.project) && !n.sync?.deleting)
      .sort((a, b) => b.modifiedAt.localeCompare(a.modifiedAt))[0];
    if (next) await selectNote(next.id);
    else renderEditor();
  }
  displayTree();
  return true;
}

// === 트리 확장 메뉴·드래그 앤 드롭 (2026-11-13 사용 의견) ===
type TreeItem = { kind: 'root' } | { kind: 'note'; id: string } | { kind: 'folder'; path: string };
let dragging: TreeItem | null = null;

const visibleNotes = () => state.notes.filter((n) => !n.sync?.deleting);
const notesIn = (project: string, folder: string) =>
  visibleNotes().filter((n) => inProject(n, project) && isWithin(n.folder, folder));

/** 프로젝트의 폴더 목록 (빈 폴더 기록 + 메모의 폴더) */
function foldersOf(project: string): string[] {
  const own = state.folders.map((x) => parseFolderKey(x.path)).filter((f) => f.project === project);
  const fromNotes = visibleNotes()
    .filter((n) => inProject(n, project))
    .flatMap((n) => folderAncestors(n.folder));
  return [...new Set([...own.map((f) => f.path), ...fromNotes])].filter(Boolean).sort();
}

function canDrop(item: TreeItem, target: string): boolean {
  if (item.kind === 'note') {
    const note = state.notes.find((n) => n.id === item.id);
    return !!note && note.folder !== target;
  }
  if (item.kind === 'folder')
    return folderMoveProblem(item.path, target, foldersOf(state.project), SETTINGS.maxFolderDepth) === null;
  return false;
}

async function dropOnFolder(item: TreeItem, target: string): Promise<void> {
  if (item.kind === 'note') {
    const note = state.notes.find((n) => n.id === item.id);
    if (note) await moveNotesTo([note], projectOf(note), target, note.folder);
  } else if (item.kind === 'folder') {
    await moveFolder(item.path, state.project, target);
  }
}

function closeTreeMenu(): void {
  document.getElementById('tree-menu')?.remove();
}

function openTreeMenu(item: TreeItem, anchor: HTMLElement): void {
  closeTreeMenu();
  const w = WORDS[state.locale];
  const menu = document.createElement('div');
  menu.id = 'tree-menu';
  menu.className = 'tree-menu';
  menu.setAttribute('role', 'menu');
  const action = (label: string, id: string, run: () => Promise<void> | void, danger = false) => {
    const b = document.createElement('button');
    b.type = 'button';
    b.id = id;
    b.setAttribute('role', 'menuitem');
    b.textContent = label;
    if (danger) b.className = 'danger-text';
    b.addEventListener('click', () => {
      closeTreeMenu();
      void run();
    });
    menu.append(b);
  };
  action(w.renameItem, 'menu-rename', () => renameItem(item));
  action(w.moveItem, 'menu-move', () => moveItem(item));
  action(w.deleteItem, 'menu-delete', () => deleteItem(item), true);
  const rect = anchor.getBoundingClientRect();
  menu.style.top = `${Math.min(rect.bottom + 2, window.innerHeight - 130)}px`;
  menu.style.left = `${Math.max(4, Math.min(rect.left, window.innerWidth - 180))}px`;
  document.body.append(menu);
  (menu.firstElementChild as HTMLButtonElement | null)?.focus();
  const close = (e: Event) => {
    if (e instanceof KeyboardEvent ? e.key === 'Escape' : !menu.contains(e.target as Node)) {
      closeTreeMenu();
      document.removeEventListener('pointerdown', close, true);
      document.removeEventListener('keydown', close, true);
    }
  };
  setTimeout(() => {
    document.addEventListener('pointerdown', close, true);
    document.addEventListener('keydown', close, true);
  });
}

async function renameItem(item: TreeItem): Promise<void> {
  const w = WORDS[state.locale];
  if (item.kind === 'note') {
    const note = state.notes.find((n) => n.id === item.id);
    if (!note) return;
    const value = await askName({ label: w.renameNote, initial: note.title, ok: w.rename, cancel: w.cancel });
    if (!value) return;
    const title = safeFilename(value);
    const clash = visibleNotes().find(
      (n) => n.id !== note.id && inProject(n, projectOf(note)) && n.folder === note.folder && n.title === title && n.format === note.format,
    );
    if (clash) {
      $('project-status').textContent = w.noteClash.replace('{name}', `${title}.${note.format}`);
      return;
    }
    note.title = title;
    note.modifiedAt = nowISO();
    state.sync?.markDirty(note);
    await repo.saveNote(note);
    if (note === state.note) renderEditor();
    displayTree();
  } else if (item.kind === 'folder') {
    const value = await askName({
      label: w.renameFolder,
      initial: lastSegment(item.path),
      ok: w.rename,
      cancel: w.cancel,
      validate: (v) => {
        try {
          if (v.includes('/') || v.includes('\\')) throw new FolderError('invalid', w.folderHint);
          parseFolder(v, 1);
          return null;
        } catch (e) {
          return e instanceof FolderError ? e.message : String(e);
        }
      },
    });
    if (!value) return;
    const target = joinPath(parentOf(item.path), parseFolder(value, 1));
    if (target === item.path) return;
    await relocateFolder(item.path, state.project, target);
  }
}

async function moveItem(item: TreeItem): Promise<void> {
  const w = WORDS[state.locale];
  const note = item.kind === 'note' ? state.notes.find((n) => n.id === item.id) : undefined;
  if (item.kind === 'root' || (item.kind === 'note' && !note)) return;
  const fromProject = note ? projectOf(note) : state.project;
  const initial: MoveTarget = { project: fromProject, folder: note ? note.folder : parentOf((item as { path: string }).path) };
  const target = await askMove({
    title: w.moveTitle,
    projectLabel: w.project,
    folderLabel: w.folderLabel,
    projects: [DEFAULT_PROJECT, ...liveProjects().map((p) => p.id)].map((id) => ({ id, label: projectName(id) })),
    folders: (project) =>
      ['', ...foldersOf(project)].map((path) => ({
        path,
        label: path ? '/' + path : w.rootFolder,
        // 폴더 이동: 자기 안이나 2단계를 넘는 곳은 고를 수 없음
        disabled:
          item.kind === 'folder' &&
          folderMoveProblem(item.path, path, foldersOf(project), SETTINGS.maxFolderDepth, project === fromProject) !== null &&
          !(project === fromProject && parentOf(item.path) === path),
      })),
    initial,
    ok: w.moveTo2,
    cancel: w.cancel,
  });
  if (!target) return;
  if (note) await moveNotesTo([note], target.project, target.folder, note.folder);
  else if (item.kind === 'folder') await moveFolder(item.path, target.project, target.folder);
}

async function deleteItem(item: TreeItem): Promise<void> {
  const w = WORDS[state.locale];
  if (item.kind === 'note') {
    const note = state.notes.find((n) => n.id === item.id);
    if (note) await deleteNoteWithConfirm(note);
  } else if (item.kind === 'folder') {
    const notes = notesIn(state.project, item.path);
    const ok = await askConfirm({
      message: w.deleteFolderConfirm.replace('{name}', item.path).replace('{n}', String(notes.length)),
      ok: w.deleteNoteOk,
      cancel: w.cancel,
      danger: true,
    });
    if (!ok) return;
    if (!(await deleteNotes(notes))) return;
    await forgetFolders(state.project, item.path);
    if (isWithin(state.selectedFolder, item.path)) selectFolder(parentOf(item.path));
    displayTree();
  }
}

/** 폴더 기록(빈 폴더)을 folder와 그 아래까지 지움. 파일 저장소는 비어 있는 폴더만 실제로 지움 */
async function forgetFolders(project: string, folder: string): Promise<void> {
  const keys = state.folders
    .map((f) => f.path)
    .filter((key) => {
      const k = parseFolderKey(key);
      return k.project === project && isWithin(k.path, folder);
    });
  state.folders = state.folders.filter((f) => !keys.includes(f.path));
  // 깊은 것부터 (파일 저장소에서 상위 폴더가 먼저 지워지지 않게)
  for (const key of [...keys, folderKey(project, folder)].sort((a, b) => b.length - a.length))
    await repo.deleteFolder(key).catch(console.error);
}

/** 메모들을 project/folder로. 같은 이름이 있으면 하나도 옮기지 않음 */
async function moveNotesTo(notes: Note[], project: string, folder: string, _from: string): Promise<boolean> {
  const w = WORDS[state.locale];
  const ids = new Set(notes.map((n) => n.id));
  for (const note of notes) {
    const clash = visibleNotes().find(
      (n) => !ids.has(n.id) && inProject(n, project) && n.folder === folder && n.title === note.title && n.format === note.format,
    );
    if (clash) {
      $('project-status').textContent = w.noteClash.replace('{name}', `${note.title}.${note.format}`);
      return false;
    }
  }
  for (const note of notes) {
    note.projectId = project || null;
    note.folder = folder;
    note.modifiedAt = nowISO();
    state.sync?.markDirty(note);
    await repo.saveNote(note);
  }
  if (state.note && ids.has(state.note.id)) {
    // 연 메모를 옮겼으면 강조 폴더도 그 메모의 새 위치로
    if (project !== state.project) await switchProject(project);
    state.selectedFolder = state.note.folder;
    $('folder-path').textContent = '/' + state.selectedFolder;
    renderEditor();
  }
  displayTree();
  return true;
}

/** 폴더 이동: targetParent 아래로 (다른 프로젝트 가능) */
async function moveFolder(folder: string, project: string, targetParent: string): Promise<void> {
  const w = WORDS[state.locale];
  const sameProject = project === state.project;
  const problem = folderMoveProblem(folder, targetParent, foldersOf(state.project), SETTINGS.maxFolderDepth, sameProject);
  if (problem === 'same') return;
  if (problem) {
    $('project-status').textContent = problem === 'into_self' ? w.folderIntoSelf : w.folderTooDeep;
    return;
  }
  await relocateFolder(folder, project, joinPath(targetParent, lastSegment(folder)));
}

/** folder(지금 프로젝트)를 project의 target 경로로: 아래 메모·빈 폴더 기록까지 */
async function relocateFolder(folder: string, project: string, target: string): Promise<void> {
  const w = WORDS[state.locale];
  const from = state.project;
  if (foldersOf(project).includes(target) && !(project === from && target === folder)) {
    $('project-status').textContent = w.folderExists;
    return;
  }
  if (depthOf(target) > SETTINGS.maxFolderDepth) {
    $('project-status').textContent = w.folderTooDeep;
    return;
  }
  const notes = notesIn(from, folder);
  const ids = new Set(notes.map((n) => n.id));
  for (const note of notes) {
    const next = remap(note.folder, folder, target)!;
    const clash = visibleNotes().find(
      (n) => !ids.has(n.id) && inProject(n, project) && n.folder === next && n.title === note.title && n.format === note.format,
    );
    if (clash) {
      $('project-status').textContent = w.noteClash.replace('{name}', `${note.title}.${note.format}`);
      return;
    }
  }
  // 새 폴더 기록 먼저 (빈 하위 폴더 유지)
  const subfolders = foldersOf(from).filter((p) => isWithin(p, folder));
  for (const path of subfolders) {
    const key = folderKey(project, remap(path, folder, target)!);
    if (!state.folders.some((f) => f.path === key)) {
      await repo.saveFolder(key);
      state.folders.push({ path: key });
    }
  }
  for (const note of notes) {
    note.folder = remap(note.folder, folder, target)!;
    note.projectId = project || null;
    note.modifiedAt = nowISO();
    state.sync?.markDirty(note);
    await repo.saveNote(note);
  }
  await forgetFolders(from, folder);
  if (isWithin(state.selectedFolder, folder) && project === from)
    state.selectedFolder = remap(state.selectedFolder, folder, target)!;
  if (project !== from) await switchProject(project);
  if (state.note && ids.has(state.note.id)) state.selectedFolder = state.note.folder;
  $('folder-path').textContent = '/' + state.selectedFolder;
  displayTree();
  if (state.note && ids.has(state.note.id)) renderEditor();
}

async function addNote(title: string, content = '', format: NoteFormat = 'md'): Promise<void> {
  const sanitized = safeFilename(title);
  const exists = state.notes.some(
    (n) =>
      inProject(n, state.project) && n.folder === state.selectedFolder && n.title === sanitized && n.format === format,
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
    ...(projectsEnabled() ? { projectId: state.project || null } : {}),
  };
  state.sync?.markDirty(note); // 로그인 중이면 새 메모도 올리기 대상
  await repo.saveNote(note);
  state.notes.push(note);
  await selectNote(note.id);
}

async function addFolder(raw: string): Promise<void> {
  const path = parseFolder(raw, SETTINGS.maxFolderDepth);
  if (!path) return;
  for (const part of folderAncestors(path)) {
    const key = folderKey(state.project, part);
    if (!state.folders.some((f) => f.path === key)) {
      await repo.saveFolder(key);
      state.folders.push({ path: key });
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
  // 오래 입력하지 않으면 임대를 반납해 다른 기기가 편집할 수 있게 (다시 입력하면 자동으로 다시 받음)
  if (state.lease?.kind === 'held' && Date.now() - state.lastEditAt > SETTINGS.leaseIdleReleaseMs) {
    releaseEdit();
    renderSyncInfo();
  }
  renderTimers();
}

function toggleFocus(): void {
  changeMode(state.mode === 'editor' ? 'organizer' : 'editor');
}

function bindEvents(): void {
  const w = () => WORDS[state.locale];
  $('add-note').onclick = async () => {
    const value = await askName({
      label: w().newNoteTitle,
      hint: state.selectedFolder ? `${w().saveIn}: /${state.selectedFolder}` : undefined,
      initial: defaultMemoTitle(),
      ok: w().ok,
      cancel: w().cancel,
    });
    if (value) await addNote(value);
  };
  $('add-folder').onclick = async () => {
    const value = await askName({
      label: w().newFolderName,
      hint: w().folderHint,
      initial: state.selectedFolder ? state.selectedFolder + '/' : '',
      selectAll: false,
      ok: w().ok,
      cancel: w().cancel,
      validate: (v) => {
        try {
          parseFolder(v, SETTINGS.maxFolderDepth);
          return null;
        } catch (e) {
          return e instanceof FolderError ? e.message : String(e);
        }
      },
    });
    if (value) await addFolder(value);
  };
  $<HTMLSelectElement>('project-select').onchange = (e) =>
    void switchProject((e.target as HTMLSelectElement).value);
  $('add-project').onclick = async () => {
    const value = await askName({
      label: w().newProjectName,
      hint: w().projectHint,
      ok: w().ok,
      cancel: w().cancel,
      validate: projectNameValidator(),
    });
    if (value) await addProject(normalizeProjectName(value, state.projects));
  };
  $('rename-project').onclick = async () => {
    const id = state.project;
    if (!id) {
      // 기본 프로젝트: 행이 없으므로 계정 설정의 표시 이름만 바꿈 (모든 기기, 서버 필요)
      const value = await askName({
        label: w().renameProjectTo,
        initial: projectName(''),
        hint: w().renameDefaultHint,
        ok: w().rename,
        cancel: w().cancel,
        validate: (v) => (v === w().defaultProject ? null : projectNameValidator('')(v)),
      });
      if (value === null) return;
      const name = value === w().defaultProject ? '' : normalizeProjectName(value, state.projects, '');
      try {
        const account = await accountApi.update({ defaultProjectName: name });
        state.defaultProjectName = account.defaultProjectName ?? name;
        await repo.setMeta(DEFAULT_PROJECT_NAME_META_KEY, state.defaultProjectName);
        renderProjects();
      } catch (e) {
        $('project-status').textContent = e instanceof ApiError ? e.message : w().needServer;
      }
      return;
    }
    const value = await askName({
      label: w().renameProjectTo,
      initial: projectName(id),
      ok: w().rename,
      cancel: w().cancel,
      validate: projectNameValidator(id),
    });
    if (value) await renameProject(id, normalizeProjectName(value, state.projects, id));
  };
  $('delete-project').onclick = () => void deleteProject(state.project);
  $<HTMLSelectElement>('note-project').onchange = (e) =>
    void moveNoteToProject((e.target as HTMLSelectElement).value);
  $<HTMLSelectElement>('note-folder').onchange = (e) =>
    void moveNoteToFolder((e.target as HTMLSelectElement).value);
  $('delete-note').onclick = () => void deleteCurrentNote();
  // 제목에서 Enter → 본문으로 이동 (한글 조합 중 Enter는 제외)
  $<HTMLInputElement>('title').addEventListener('keydown', (e) => {
    if (e.key !== 'Enter' || e.isComposing) return;
    e.preventDefault();
    editor().focus();
  });
  $<HTMLInputElement>('title').oninput = (e) =>
    applyEdit('title', safeFilename((e.target as HTMLInputElement).value));
  editor().oninput = (e) => applyEdit('content', (e.target as HTMLTextAreaElement).value);
  $<HTMLSelectElement>('format').onchange = (e) => {
    const note = state.note;
    if (!note) return;
    remember(note, SETTINGS.historyLimit);
    note.format = (e.target as HTMLSelectElement).value === 'txt' ? 'txt' : 'md';
    note.modifiedAt = nowISO();
    state.unsaved.add(note.id);
    noteChanged(note);
    scheduleSave();
    renderEditor();
    displayTree();
  };
  $('search').oninput = displayTree;
  $('language-btn').onclick = () => setLanguage(state.locale === 'ko' ? 'en' : 'ko');
  $('toggle-btn').onclick = toggleFocus;
  $('split-btn').onclick = () => changeMode('split');
  $('help-btn').onclick = openHelp;
  $('help-close').onclick = () => $<HTMLDialogElement>('help-dialog').close();
  $('help-to-note').onclick = async () => {
    $<HTMLDialogElement>('help-dialog').close();
    await addNote(HELP_TITLE[state.locale], HELP_MARKDOWN[state.locale]);
  };
  // 계정 화면: 웹(서버와 같은 출처). 데스크톱/모바일 앱은 장비 연결 화면(setupDevice)
  if (!isTauri()) {
    const panel = createAccountPanel($<HTMLDialogElement>('account-dialog'), () => state.locale, {
      onSignedIn: (account) => void startSync(account.id, deviceLabel()),
      onSignedOut: () => void stopSync(),
    });
    $('account-btn').onclick = () => void panel.open();
  }
  $<HTMLDetailsElement>('related-panel').addEventListener('toggle', () => renderRelated());
  // 편집 시작 = 에디터/제목에 들어갈 때 임대 요청 (열기만 해서는 잠그지 않음)
  editor().addEventListener('focus', () => void requestEdit());
  $<HTMLInputElement>('title').addEventListener('focus', () => void requestEdit());
  window.addEventListener('online', () => void syncTick());
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
      releaseEdit();
      endSession();
      void flushSave();
    } else {
      startSession();
      void syncTick();
    }
  });
  // 종료 직전 저장은 최선 노력. 네이티브 어댑터는 내구성 있는 저널을 사용할 예정.
  window.addEventListener('pagehide', () => {
    releaseEdit(true); // keepalive: 탭을 닫아도 반납 요청이 전달되게
    endSession();
    void flushSave();
  });
  window.addEventListener('online', updateConnectivity);
  window.addEventListener('offline', updateConnectivity);
}

export async function bootstrap(repository: NoteRepository): Promise<void> {
  repo = repository;
  $('hint').textContent = `Local-first · ${repo.describe()}`;
  const locale = await repo.getMeta<string>('locale');
  state.notes = await repo.listNotes();
  state.folders = await repo.listFolders();
  if (projectsEnabled()) {
    state.projects = (await repo.getMeta<LocalProject[]>(PROJECTS_META_KEY)) ?? [];
    state.defaultProjectName = (await repo.getMeta<string>(DEFAULT_PROJECT_NAME_META_KEY)) ?? '';
    const current = (await repo.getMeta<string>(CURRENT_PROJECT_META_KEY)) ?? DEFAULT_PROJECT;
    state.project = state.projects.some((p) => p.id === current && !p.sync.deleting) ? current : DEFAULT_PROJECT;
    $('project-row').hidden = false;
    $('note-project').hidden = false;
  }
  setLanguage(locale === 'en' ? 'en' : 'ko');
  renderProjects();
  const selected = await repo.getMeta<string>('selectedId');
  const initial =
    state.notes.find((n) => n.id === selected && inProject(n, state.project)) ??
    state.notes.find((n) => inProject(n, state.project));
  if (initial) {
    state.selectedId = initial.id;
    state.note = initial;
    startSession();
  }
  renderEditor();
  displayTree();
  renderClock();
  updateConnectivity();
  void watchBattery();
  bindEvents();
  // 처음 실행(메모 없음): 사용법을 먼저 보여 줌 (한 번만)
  if (!state.notes.length && !helpShown()) {
    markHelpShown();
    openHelp();
  }
  setInterval(updateTimer, SETTINGS.timerTickMs);
  setInterval(renderClock, SETTINGS.clockTickMs);
  // 이미 로그인한 브라우저 / 연결된 장비면 바로 동기화 (서버가 없으면 조용히 로컬 전용)
  if (isTauri()) {
    void setupDevice().catch(console.error);
  } else {
    accountApi
      .me()
      .then((account) => account && startSync(account.id, deviceLabel()))
      .catch(() => undefined);
  }
}

export function reportStorageFailure(e: unknown): void {
  console.error(e);
  $('save-status').textContent = 'STORAGE UNAVAILABLE';
  alert('브라우저 IndexedDB를 사용할 수 없습니다. 보안 모드/저장공간을 확인하세요.');
}
