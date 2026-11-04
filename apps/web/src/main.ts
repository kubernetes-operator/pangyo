// === 진입점: 스타일, 앱 부팅, 오프라인 서비스워커 등록 ===
import './ui/main.css';
import { bootstrap, reportStorageFailure } from './ui/app';

bootstrap().catch(reportStorageFailure);

// 서비스워커는 빌드 결과물에서만 등록합니다 (개발 서버에서는 캐시가 개발을 방해함).
if (import.meta.env.PROD && 'serviceWorker' in navigator) {
  navigator.serviceWorker.register('./sw.js').catch(console.error);
}
