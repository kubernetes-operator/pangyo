# 공유 프로토콜 v1 (초안)
- 클라이언트 종류와 무관하게 note UUID / version / format / folder(0~2)를 사용.
- 스키마 변경 시 `/api/v1` 계약 버전을 유지하거나 명시적으로 올릴 것.
- `schemas/note.schema.json`은 API 계약의 후보이며 서버 검증에 아직 연결되지 않음.
