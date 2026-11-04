// === 진입점: 스타일, 앱 부팅, 오프라인 서비스워커 등록 ===
import './ui/main.css';
import { bootstrap, reportStorageFailure } from './ui/app';
import { isTauri, openRepository } from './storage';

openRepository()
  .then(bootstrap)
  .catch(reportStorageFailure);

// 서비스워커는 웹 빌드 결과물에서만 등록합니다 (개발 서버·Tauri 앱은 제외).
if (import.meta.env.PROD && !isTauri() && 'serviceWorker' in navigator) {
  navigator.serviceWorker.register('./sw.js').catch(console.error);
}
