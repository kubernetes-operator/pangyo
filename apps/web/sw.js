// === 0. 오프라인 캐시 버전: 변경 배포 시 증가 ===
const CACHE_NAME = 'memonest-v01-static-1'; // 정적 앱만 캐시; 사용자 메모는 IndexedDB에 보관.
const ASSETS = ['./', './index.html', './assets/main.css', './assets/app.js', './assets/store.js', './manifest.webmanifest'];
self.addEventListener('install',(event)=>event.waitUntil(caches.open(CACHE_NAME).then(cache=>cache.addAll(ASSETS)).then(()=>self.skipWaiting())));
self.addEventListener('activate',(event)=>event.waitUntil(caches.keys().then(keys=>Promise.all(keys.filter(k=>k!==CACHE_NAME).map(k=>caches.delete(k)))).then(()=>self.clients.claim())));
// 외부 API / 사용자 정보는 캐시하지 않으며 동일 출처 정적 파일만 캐시합니다.
self.addEventListener('fetch',event=>{if(event.request.method!=='GET'||new URL(event.request.url).origin!==location.origin)return;event.respondWith(caches.match(event.request).then(cached=>cached||fetch(event.request)));});
