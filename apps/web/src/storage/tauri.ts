// === 데스크톱(Tauri) 파일 저장소 어댑터 ===
// Rust 쪽(apps/native/src-tauri, crates/core Workspace)이 실제 .md/.txt 파일과 SQLite 보조 인덱스를 관리합니다.
// 명령 이름은 apps/native/src-tauri/src/lib.rs 의 #[tauri::command] 함수와 같아야 합니다.
import { invoke } from '@tauri-apps/api/core';
import { ServerUnavailable, type Transport } from '../account/api';
import { normalizeNote, type Note, type NoteFormat, type NoteSync } from '../domain/types';
import type { NoteRepository } from './repository';

export const COMMANDS = {
  root: 'notes_root',
  listNotes: 'list_notes',
  saveNote: 'save_note',
  saveSessions: 'save_sessions',
  deleteNote: 'delete_note',
  listFolders: 'list_folders',
  createFolder: 'create_folder',
  deleteFolder: 'delete_folder',
  getMeta: 'get_meta',
  setMeta: 'set_meta',
  // 바깥 삭제 확인 (ADR-0004)
  listExternalDeletions: 'list_external_deletions',
  forgetExternalDeletion: 'forget_external_deletion',
  // 장비 등록·서버 연결 (토큰은 Rust에만 있음)
  deviceStatus: 'device_status',
  deviceHostname: 'device_hostname',
  // 앱 안 업데이트 (A안)
  appInfo: 'app_info',
  openDownload: 'open_download',
  saveDownload: 'save_download',
  installUpdate: 'install_update',
  topInset: 'top_inset',
  lastUpdateResult: 'last_update_result',
  bottomInset: 'bottom_inset',
  devicePair: 'device_pair',
  deviceUnpair: 'device_unpair',
  serverRequest: 'server_request',
} as const;

/** 앱 밖에서 지워진, 서버에 올라간 적 있는 메모 (crates/core ExternalDeletion) */
export interface ExternalDeletion {
  id: string;
  title: string;
  folder: string;
  format: NoteFormat;
  sync: NoteSync;
  detectedAt: string;
}

/** 이 기기의 등록 정보 (토큰 제외) */
export interface DeviceInfo {
  serverUrl: string;
  deviceId: string;
  deviceName: string;
  pairedAt: string;
}

/** Rust 명령 오류 문자열: 'offline: …' 또는 'code: 메시지' */
function commandError(e: unknown): { code: string; message: string } {
  const text = String(e);
  const at = text.indexOf(':');
  if (at > 0 && /^[a-z_]+$/.test(text.slice(0, at))) {
    return { code: text.slice(0, at), message: text.slice(at + 1).trim() };
  }
  return { code: 'error', message: text };
}

export class DeviceError extends Error {
  constructor(
    readonly code: string,
    message: string,
  ) {
    super(message);
  }
}

/** 앱 버전·빌드, 받은 일회용 다운로드 주소 열기 (앱 안 업데이트 A안) */
export const appApi = {
  info: () => invoke<import('../ui/app-update').AppInfo>(COMMANDS.appInfo),
  openDownload: (url: string) => invoke<void>(COMMANDS.openDownload, { url }),
  /** 내보낸 파일(프로젝트 .zip)을 OS 다운로드 폴더에 저장 → 저장한 경로 */
  saveDownload: (fileName: string, data: number[]) => invoke<string>(COMMANDS.saveDownload, { fileName, data }),
  /** 지난 Windows 업데이트 기록 (한 번 읽으면 지움) */
  lastUpdateResult: () => invoke<string | null>(COMMANDS.lastUpdateResult).catch(() => null),
  /** 화면 위쪽 여백 (Android 상태바 높이, CSS px) */
  topInset: () => invoke<number>(COMMANDS.topInset).catch(() => 0),
  /** 화면 아래쪽 여백 (Android 내비게이션 막대 높이, CSS px) */
  bottomInset: () =>
    invoke<{ insets: number; navBar: number }>(COMMANDS.bottomInset).catch(() => ({ insets: 0, navBar: 0 })),
  /** Windows: 받아서 검증 → 조용히 설치 → 다시 실행 (다른 OS는 'unsupported') */
  installUpdate: (url: string, sha256: string) => invoke<void>(COMMANDS.installUpdate, { url, sha256 }),
};

export const deviceApi = {
  status: () => invoke<DeviceInfo | null>(COMMANDS.deviceStatus).catch(() => null),
  /** 이 기기의 호스트 이름 (없으면 null) */
  hostname: () => invoke<string | null>(COMMANDS.deviceHostname).catch(() => null),
  async pair(serverUrl: string, code: string, deviceName: string): Promise<DeviceInfo> {
    try {
      return await invoke<DeviceInfo>(COMMANDS.devicePair, { serverUrl, code, deviceName, now: new Date().toISOString() });
    } catch (e) {
      const err = commandError(e);
      throw new DeviceError(err.code, err.message);
    }
  },
  unpair: () => invoke<void>(COMMANDS.deviceUnpair),
  externalDeletions: () => invoke<ExternalDeletion[]>(COMMANDS.listExternalDeletions),
  forgetExternalDeletion: (id: string) => invoke<void>(COMMANDS.forgetExternalDeletion, { id }),
};

/**
 * 장비 토큰 전송: 화면 → Rust → 서버. 401(장비 폐기·토큰 없음)이면 onUnauthorized 호출.
 * Rust 쪽에서 허용한 경로(/notes, /search, /account, /info)만 보낼 수 있습니다.
 */
export function deviceTransport(onUnauthorized: () => void): Transport {
  return async (method, path, body) => {
    let response: { status: number; body: unknown };
    try {
      response = await invoke(COMMANDS.serverRequest, { method, path, body: body ?? null });
    } catch (e) {
      throw new ServerUnavailable(String(e)); // 연결 실패·미등록 → 오프라인과 같게 (메모는 이 기기에 계속 저장)
    }
    if (response.status === 401) onUnauthorized();
    return {
      status: response.status,
      json: response.status === 204 || response.body !== null,
      data: response.body ?? undefined,
    };
  };
}

export async function createTauriRepository(): Promise<NoteRepository> {
  const root = await invoke<string>(COMMANDS.root);
  return {
    kind: 'files',
    describe: () => root,
    async listNotes() {
      const notes = await invoke<Array<Partial<Note> & { id: string }>>(COMMANDS.listNotes);
      return notes.map(normalizeNote);
    },
    async saveNote(note) {
      await invoke(COMMANDS.saveNote, { note });
    },
    async saveSessions(note) {
      await invoke(COMMANDS.saveSessions, { id: note.id, sessions: note.sessions });
    },
    async deleteNote(id) {
      await invoke(COMMANDS.deleteNote, { id });
    },
    async listFolders() {
      const paths = await invoke<string[]>(COMMANDS.listFolders);
      return paths.map((path) => ({ path }));
    },
    async saveFolder(path) {
      await invoke(COMMANDS.createFolder, { path });
    },
    async deleteFolder(path) {
      await invoke(COMMANDS.deleteFolder, { path });
    },
    async getMeta<T>(key: string) {
      return ((await invoke<T | null>(COMMANDS.getMeta, { key })) ?? undefined) as T | undefined;
    },
    async setMeta(key, value) {
      await invoke(COMMANDS.setMeta, { key, value });
    },
  };
}
