// === 빌드 시 오프라인 서비스워커(sw.js) 생성 Vite 플러그인 ===
// 해시가 붙은 빌드 파일 목록을 캐시 대상으로 넣고, 목록이 바뀌면 캐시 이름도 바뀝니다.
// 사용자 메모는 IndexedDB에 있으므로 캐시하지 않습니다. 외부 출처 요청도 캐시하지 않습니다.
import { createHash } from 'node:crypto';
import type { Plugin } from 'vite';

const CACHE_PREFIX = 'memonest-static-'; // 이전 v0.1 캐시(memonest-v01-static-1)는 activate 시 삭제됨
const ALWAYS_CACHED = ['./', './index.html', './manifest.webmanifest']; // public/ 파일 포함

function serviceWorkerSource(cacheName: string, assets: string[]): string {
  return `// 자동 생성 파일 (apps/web/build/offline-sw.ts). 직접 수정하지 마세요.
const CACHE_NAME = ${JSON.stringify(cacheName)};
const ASSETS = ${JSON.stringify(assets)};
self.addEventListener('install', (event) => event.waitUntil(
  caches.open(CACHE_NAME).then((cache) => cache.addAll(ASSETS)).then(() => self.skipWaiting())));
self.addEventListener('activate', (event) => event.waitUntil(
  caches.keys().then((keys) => Promise.all(keys.filter((k) => k !== CACHE_NAME).map((k) => caches.delete(k))))
    .then(() => self.clients.claim())));
self.addEventListener('fetch', (event) => {
  if (event.request.method !== 'GET' || new URL(event.request.url).origin !== location.origin) return;
  // ignoreVary: Vite의 crossorigin 스크립트 요청(Origin 헤더)과 캐시 저장 요청이 'Vary: Origin'으로 어긋나지 않게 함.
  // 같은 출처 정적 파일만 캐시하므로 안전합니다.
  event.respondWith(caches.match(event.request, { ignoreVary: true }).then((cached) => cached || fetch(event.request)));
});
`;
}

export function offlineServiceWorker(): Plugin {
  return {
    name: 'memonest-offline-sw',
    apply: 'build',
    enforce: 'post',
    generateBundle(_options, bundle) {
      const built = Object.keys(bundle)
        .filter((f) => !f.endsWith('.map'))
        .map((f) => './' + f);
      const assets = [...new Set([...ALWAYS_CACHED, ...built])].sort();
      const digest = createHash('sha256');
      for (const [name, chunk] of Object.entries(bundle)) {
        digest.update(name);
        digest.update(chunk.type === 'chunk' ? chunk.code : chunk.source);
      }
      const cacheName = CACHE_PREFIX + digest.digest('hex').slice(0, 12);
      this.emitFile({ type: 'asset', fileName: 'sw.js', source: serviceWorkerSource(cacheName, assets) });
    },
  };
}
