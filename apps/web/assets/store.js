// === 0. 브라우저 저장 어댑터 변수 (실제 파일 API는 Tauri용 어댑터로 교체 예정) ===
const DB_NAME = 'memonest_local_v1'; // 앱 데이터용 IndexedDB; 디스크의 .md 파일은 아닙니다.
const DB_VERSION = 1;                // 스키마 변경 시 증가하고 upgrade migration 작성.
const STORES = { notes: 'notes', folders: 'folders', meta: 'meta' };

// IndexedDB 요청을 비동기 함수처럼 사용합니다.
function asPromise(request) {
  return new Promise((resolve, reject) => {
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
}

let connection;
async function db() {
  if (connection) return connection;
  connection = await new Promise((resolve, reject) => {
    const request = indexedDB.open(DB_NAME, DB_VERSION);
    request.onupgradeneeded = () => {
      const database = request.result;
      if (!database.objectStoreNames.contains(STORES.notes)) database.createObjectStore(STORES.notes, { keyPath: 'id' });
      if (!database.objectStoreNames.contains(STORES.folders)) database.createObjectStore(STORES.folders, { keyPath: 'path' });
      if (!database.objectStoreNames.contains(STORES.meta)) database.createObjectStore(STORES.meta, { keyPath: 'key' });
    };
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
  return connection;
}

// 단일 객체 read/write. 트랜잭션 완료를 기다려 저장 성공 상태를 확정합니다.
async function query(storeName, mode, operation) {
  const database = await db();
  return new Promise((resolve, reject) => {
    const tx = database.transaction(storeName, mode);
    const store = tx.objectStore(storeName);
    let result;
    const req = operation(store);
    req.onsuccess = () => { result = req.result; };
    tx.oncomplete = () => resolve(result);
    tx.onerror = () => reject(tx.error);
    tx.onabort = () => reject(tx.error);
  });
}
export const localStore = {
  notes: () => query(STORES.notes, 'readonly', (s) => s.getAll()),
  saveNote: (note) => query(STORES.notes, 'readwrite', (s) => s.put(structuredClone(note))),
  folders: () => query(STORES.folders, 'readonly', (s) => s.getAll()),
  saveFolder: (path) => query(STORES.folders, 'readwrite', (s) => s.put({ path })),
  getMeta: (key) => query(STORES.meta, 'readonly', (s) => s.get(key)),
  setMeta: (key, value) => query(STORES.meta, 'readwrite', (s) => s.put({ key, value })),
};
