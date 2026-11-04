# 공유 프로토콜 v1 (초안)
- 클라이언트 종류와 무관하게 note UUID / version / format / folder(0~2)를 사용.
- 스키마 변경 시 `/api/v1` 계약 버전을 유지하거나 명시적으로 올릴 것.
- `schemas/note.schema.json`은 API 계약의 후보이며 서버 검증에 아직 연결되지 않음.
- `openapi.json`: 서버 API 계약 원본(OpenAPI 3.1). `x-memonest-status`로 구현/계획 구분, 서버 계약 테스트가 검사.
- `fixtures/paths.json`: 폴더/파일명 규칙 기준 데이터 (웹 TS, Rust core 공통 테스트).
- TS/Rust 타입 자동 생성은 실제 데이터 API(로드맵 05~06단계)를 구현할 때 도입.
