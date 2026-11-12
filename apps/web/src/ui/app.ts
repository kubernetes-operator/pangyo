// === 화면 연결: 메모 편집(v0.1 동작) + 계정 + 동기화(로드맵 06, ADR-0003 A안) ===
import { SETTINGS, WORDS, PLACEHOLDERS, type Locale } from '../settings';
import type { NoteRepository } from '../storage/repository';
import { isTauri } from '../storage';
import { createAccountPanel } from '../account/panel';
import { accountApi, searchApi, ServerUnavailable, setTransport } from '../account/api';
import type { DeletionChoice } from '../account/device-panel';
import type { DeviceInfo, ExternalDeletion } from '../storage/tauri';
import { SyncEngine, type ConflictChoice, type LeaseState, type SyncStatus, type SyncStore } from '../sync/engine';
import { deviceLabel, newClientId, syncApi } from '../sync/api';
import { renderSyncBanner, syncStatusText } from './sync-view';
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
};

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
  a.modifiedAt === b.modifiedAt && JSON.stringify(a.sync ?? null) === JSON.stringify(b.sync ?? null);

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
      const { title, folder, format, content, modifiedAt, sync } = found;
      Object.assign(current, { title, folder, format, content, modifiedAt, sync });
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
  });
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
  state.sync?.markDirty(note); // 로그인 중이면 새 메모도 올리기 대상
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
