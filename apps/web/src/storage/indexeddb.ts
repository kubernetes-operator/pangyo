// === 0. 브라우저 저장 어댑터 변수 (네이티브는 Tauri 파일 어댑터로 교체 예정) ===
// 주의: 아래 세 값은 v0.1 JS 프로토타입과 동일해야 기존 사용자 메모를 그대로 읽습니다.
export const DB_NAME = 'memonest_local_v1'; // 앱 데이터용 IndexedDB; 디스크의 .md 파일은 아닙니다.
export const DB_VERSION = 1; // 스키마 변경 시 증가하고 onupgradeneeded에 migration 작성.
export const STORES = { notes: 'notes', folders: 'folders', meta: 'meta' } as const;

import { normalizeNote, type FolderRecord, type Note } from '../domain/types';

type StoreName = (typeof STORES)[keyof typeof STORES];

let connection: Promise<IDBDatabase> | undefined;

function db(): Promise<IDBDatabase> {
  connection ??= new Promise((resolve, reject) => {
    const request = indexedDB.open(DB_NAME, DB_VERSION);
    request.onupgradeneeded = () => {
      const database = request.result;
      if (!database.objectStoreNames.contains(STORES.notes))
        database.createObjectStore(STORES.notes, { keyPath: 'id' });
      if (!database.objectStoreNames.contains(STORES.folders))
        database.createObjectStore(STORES.folders, { keyPath: 'path' });
      if (!database.objectStoreNames.contains(STORES.meta))
        database.createObjectStore(STORES.meta, { keyPath: 'key' });
    };
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
  return connection;
}

/** 단일 요청 트랜잭션. 트랜잭션 완료(oncomplete)를 기다려 저장 성공을 확정합니다. */
async function query<T>(
  storeName: StoreName,
  mode: IDBTransactionMode,
  operation: (store: IDBObjectStore) => IDBRequest,
): Promise<T> {
  const database = await db();
  return new Promise((resolve, reject) => {
    const tx = database.transaction(storeName, mode);
    const req = operation(tx.objectStore(storeName));
    let result: T;
    req.onsuccess = () => {
      result = req.result as T;
    };
    tx.oncomplete = () => resolve(result);
    tx.onerror = () => reject(tx.error);
    tx.onabort = () => reject(tx.error);
  });
}

export interface MetaRecord<T = unknown> {
  key: string;
  value: T;
}

export const localStore = {
  async notes(): Promise<Note[]> {
    const raw = await query<Array<Partial<Note> & { id: string }>>(STORES.notes, 'readonly', (s) =>
      s.getAll(),
    );
    return raw.map(normalizeNote);
  },
  saveNote: (note: Note) =>
    query<IDBValidKey>(STORES.notes, 'readwrite', (s) => s.put(structuredClone(note))),
  folders: () => query<FolderRecord[]>(STORES.folders, 'readonly', (s) => s.getAll()),
  saveFolder: (path: string) =>
    query<IDBValidKey>(STORES.folders, 'readwrite', (s) => s.put({ path })),
  getMeta: <T>(key: string) =>
    query<MetaRecord<T> | undefined>(STORES.meta, 'readonly', (s) => s.get(key)),
  setMeta: (key: string, value: unknown) =>
    query<IDBValidKey>(STORES.meta, 'readwrite', (s) => s.put({ key, value })),
};
