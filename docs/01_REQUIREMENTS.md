# 01. 요구사항 추적 표 v0.1

상태: **P**=설계, **D**=브라우저에서 시연, **S**=골격만, **A**=승인 대기. 시연은 제품 완료를 의미하지 않습니다.

| ID | 요청 | 목표 위치 | 상태 |
|---|---|---|---|
| 01 | 가벼운 앱 | `apps/web`, `apps/native` | D |
| 02 | 로컬 저장 및 선택 클라우드 동기화 | `packages/protocol`, `crates/server` | P |
| 03 | 짧거나 긴 노트 | `apps/web` | D |
| 04 | Linux/Windows/Android/iOS | `apps/native` | S |
| 05 | FreeBSD ARM64 장비 | `apps/bsd` | S |
| 06 | BSD 셸/Wi-Fi/BT | `config/device.freebsd.example.toml` | P |
| 07 | BSD 단일 목적 키오스크 | `docs/10_BSD.md` | P |
| 08 | 관리/작성 2화면 | `apps/web`, `docs/03_UX.md` | D |
| 09 | 트리/시간/배터리/Wi-Fi/BT 상태 | `apps/web`, `docs/03_UX.md` | S |
| 10 | 메모 전용 화면 | `apps/web` | D |
| 11 | 한 화면일 때 회전 전환 | `apps/web` | D |
| 12 | 기본 MD, 선택 TXT | `apps/web` | D |
| 13 | 한국어/영어 기본 | `apps/web` | D |
| 14 | 다른 입력 언어 추가 | `docs/03_UX.md` | P |
| 15 | 장비 간 자유로운 파일 전송 | `docs/05_SYNC_API.md` | P |
| 16 | LAN/인터넷 인증 연결 | `crates/server` 계정·장비 등록 API | D (인터넷 로그인·장비 토큰. LAN 발견은 후속) |
| 17 | 계정 서버 및 유료 저장 용량/999 사용자 | `crates/server` | D (가입/로그인, 등급별 용량 표시, 999명 상한. 결제·사용량 집계는 후속) |
| 18 | 선택적 AI 개인 계정/API | `docs/06_SECURITY.md` | P |
| 19 | n8n 읽기/쓰기 API | `crates/server` 자동화 토큰, `docs/14_N8N.md` | D (범위·만료 토큰, 읽기/검색/임대·저장) |
| 20 | 유사도 서버 검색 | `crates/server` 검색·연관 메모 (pg_trgm) | D (한국어 포함. 형태소/임베딩은 후속) |
| 21 | 디렉터리 최대 2단계 | `apps/web` | D |
| 22 | 트리/방사형 연결도 | `docs/03_UX.md` | P |
| 23 | 문서 편집 누적 시간 = **작성 시간(키 입력)** + **열어둔 시간(창에 메모가 보이는 시간)** 각각 기록 (2026-11-04 확정) | `apps/web` | D |
| 24 | 재개 세션별 시간 이력 (세션 = 메모 열기 ~ 전환/창 숨김/종료) | `apps/web` | D |
| 25 | 복사/붙여넣기 | `apps/web` | D |
| 26 | 블록 들여쓰기/내어쓰기 | `apps/web` | D |
| 27 | 250회 Undo/Redo 영속화 | `apps/web` | D |
| 28 | 동시 작성 불가/승인 필요 | `docs/07_LOCK_APPROVAL.md` | A |
| 29 | 웹 브라우저에서 작성 | `apps/web` | D |

> 웹 시연에서 250회는 **전체 텍스트 스냅샷**이므로 대형 문서에서는 용량이 큽니다. 최종 구현 시 델타 압축 + 한도 관리가 필요합니다.
