# 08. 단계별 개발 계획(권장 10단계)

| 단계 | 목표 / 완료 기준 | 선행 조건 | 현재 |
|---|---|---|---|
| 01 | 요구사항·아키텍처·변수·Git 규칙 확정 | 없음 | 설계 초안 |
| 02 | Web 로컬 편집기, 2단계 폴더, MD/TXT, 시간, Undo 테스트 | 01 | 시연용 초안 |
| 03 | 공통 도메인/로컬 파일 어댑터와 브라우저 저장소 분리 | 02 | **완료(Linux)**: 웹 `NoteRepository`(IndexedDB/파일), `crates/core` Workspace, Tauri 연결 |
| 04 | Tauri Windows/Linux/Android/iOS 패키징 및 테스트 | 03 | **완료(서명 제외)**: Linux·Windows·macOS 설치 파일 빌드 + 실제 실행 확인, Android 디버그 APK·iOS 시뮬레이터 빌드 (CI `package.yml`, [실행 기록](https://github.com/kubernetes-operator/pangyo/actions/runs/42343557275)). 서명·스토어·모바일 실기기 실행은 후속 |
| 05 | OIDC, 사용자, 장비 등록, 저장 공간 계정별 제한 | 03 | **완료(Google 제외)**: DB 로그인·세션·장비 등록·용량·999명 상한, 2026-11-09 Kubernetes(`memonest`) 배포·도메인 확인. Google(OIDC)은 OAuth 클라이언트 준비 후 |
| 06 | 증분 동기화, 버전 검사, 충돌 보존, 사용자 승인된 잠금 | 05 + 승인 | **구현(웹)**: A안 승인(ADR-0003), 서버 동기화·임대 API, 웹 동기화 엔진·읽기 전용·충돌 검토. 데스크톱/모바일 연결은 페어링 화면 후 |
| 07 | 서버 검색, n8n 및 클라우드 어댑터 | 06 | 미착수 |
| 08 | FreeBSD ARM64 TUI/kiosk/무선/한글 키보드 프로토타입 | 하드웨어 선정 | 골격 |
| 09 | API 별 AI provider + 보안 저장소 구현 | 05 + 동의 UX | 미착수 |
| 10 | 운영 하드닝/백업복구/과금/사용성/모니터링 | 05~09 | 미착수 |

## 1차 Release Gate
- 오프라인에서 앱 재시작 후 노트 보존.
- 0/1/2단계 폴더, 긴 본문, MD/TXT 다운로드 테스트.
- 250회 Undo/Redo 및 세션 누적 확인.
- Git, Kubernetes, 클라우드 키 없이도 로컬 메모 작성 가능.
- 서버 미구현 기능은 사용자에게 ‘미지원’으로 분명히 표시.
