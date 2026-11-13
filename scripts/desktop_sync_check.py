#!/usr/bin/env python3
"""데스크톱 앱 ↔ 서버 동기화 실기 확인 (Linux, 로드맵 06 / ADR-0004).

로컬 테스트 서버(테스트 Postgres의 새 DB)에 계정을 만들고, Xvfb에서 실제 데스크톱 앱을 띄워
화면의 '장비 연결'로 등록한 뒤 다음을 확인합니다:
  1. 데스크톱 파일 → 서버 (처음 연결 시 기존 메모 올림)
  2. 서버(웹) 메모 → 데스크톱 .md 파일
  3. 앱 밖 편집 → 서버 반영
  4. 앱 밖 이름 변경 → 같은 ID로 서버 반영
  5. 앱 밖 삭제 → 서버에서 바로 지우지 않고 '확인 필요' → 화면에서 '서버에서도 삭제' → 서버 묘비
  6. 장비 토큰이 앱 데이터 폴더(인덱스·device.json)에 평문으로 남지 않음
  7. 웹에서 장비를 폐기하면 앱이 동기화를 멈추고 다시 연결하라고 안내 (메모 파일은 그대로)
  8. 프로젝트(ADR-0005): 웹 프로젝트 메모 → `@이름` 폴더, 앱 밖 `@폴더` → 서버 프로젝트, 웹에서 이름 바꾸면 폴더 이름도
사용자의 실제 메모 폴더·앱 데이터·키링은 건드리지 않습니다 (임시 HOME + 격리된 D-Bus 세션의 임시 키링).

사용: make desktop-sync-check  (make db-test-up 필요)
"""
# === 0. 설정 변수 ===
SERVER_PORT = 18091            # 이 확인 전용 서버 포트 (Playwright E2E 18090과 분리)
STEP_TIMEOUT = 90              # 각 단계 최대 대기(초). 앱 동기화 주기 20초 + 여유
POLL = 1.0
TEST_PG_CONTAINER = "memonest-test-pg"
DEFAULT_TEST_DB = "postgres://postgres@127.0.0.1:55432/postgres"
PASSWORD = "desktop-sync-check-1"  # 임시 DB의 임시 계정 (확인 후 DB 삭제)

import http.cookiejar
import json
import os
import re
import shutil
import signal
import sqlite3
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request
import uuid
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BASE = f"http://127.0.0.1:{SERVER_PORT}"


class Api:
    """웹 브라우저처럼 세션 쿠키로 서버 API 호출"""

    def __init__(self):
        self.opener = urllib.request.build_opener(urllib.request.HTTPCookieProcessor(http.cookiejar.CookieJar()))

    def call(self, method, path, body=None):
        data = None if body is None else json.dumps(body).encode()
        req = urllib.request.Request(BASE + "/api/v1" + path, data=data, method=method)
        req.add_header("Accept", "application/json")
        req.add_header("X-MemoNest-Request", "1")
        if data is not None:
            req.add_header("Content-Type", "application/json")
        try:
            with self.opener.open(req, timeout=10) as r:
                raw = r.read()
                return r.status, (json.loads(raw) if raw else None)
        except urllib.error.HTTPError as e:
            raw = e.read()
            return e.code, (json.loads(raw) if raw else None)

    def notes(self):
        status, data = self.call("GET", "/notes?since=0&projects=1")
        assert status == 200, (status, data)
        return {n["id"]: n for n in data["notes"]}


def wait(what, predicate, timeout=STEP_TIMEOUT):
    started = time.monotonic()
    while time.monotonic() - started < timeout:
        try:
            value = predicate()
        except Exception:  # 앱이 쓰는 중인 SQLite 등 → 다음 확인 때 다시
            value = None
        if value:
            print(f"OK   {what} ({time.monotonic() - started:.0f}초)")
            return value
        time.sleep(POLL)
    print(f"FAIL {what} ({timeout}초 초과)")
    return None


def main() -> int:
    for stream in (sys.stdout, sys.stderr):
        stream.reconfigure(encoding="utf-8", errors="replace")
    if not sys.platform.startswith("linux"):
        print("Linux 전용 확인입니다", file=sys.stderr)
        return 2
    for tool in ("xvfb-run", "dbus-run-session", "gnome-keyring-daemon", "docker"):
        if not shutil.which(tool):
            print(f"필요한 도구 없음: {tool}", file=sys.stderr)
            return 2
    base_db = os.environ.get("TEST_DATABASE_URL", DEFAULT_TEST_DB)
    db_name = f"memonest_desk_{os.getpid()}"
    if subprocess.run(["docker", "exec", TEST_PG_CONTAINER, "createdb", "-U", "postgres", db_name]).returncode:
        print("테스트 Postgres가 없습니다: make db-test-up", file=sys.stderr)
        return 2
    database_url = base_db.rsplit("/", 1)[0] + "/" + db_name

    work = Path(tempfile.mkdtemp(prefix="memonest-sync-check-"))
    server = app = None
    results = []
    try:
        server = subprocess.Popen(
            [str(ROOT / "target/debug/memonest-server")],
            env={**os.environ, "API_BIND_ADDRESS": f"127.0.0.1:{SERVER_PORT}", "DATABASE_URL": database_url,
                 "COOKIE_SECURE": "false", "LEASE_TTL_SECS": "10", "RUST_LOG": "warn"},
            stdout=(work / "server.log").open("wb"), stderr=subprocess.STDOUT, start_new_session=True)
        api = Api()

        def ready():
            try:
                with urllib.request.urlopen(BASE + "/readyz", timeout=2) as r:
                    return r.status == 200
            except Exception:
                return False
        results.append(wait("테스트 서버 준비", ready, 120))

        status, _ = api.call("POST", "/auth/register",
                             {"email": f"desk-{uuid.uuid4().hex[:8]}@example.test", "password": PASSWORD,
                              "displayName": "desk"})
        assert status in (200, 201), status
        status, pairing = api.call("POST", "/pairing/start")
        assert status == 201, (status, pairing)
        now = time.strftime("%Y-%m-%dT%H:%M:%S.000Z", time.gmtime())
        web_id = str(uuid.uuid4())
        status, _ = api.call("PUT", f"/notes/{web_id}", {
            "baseVersion": 0, "clientId": "web-check", "fencingToken": None, "title": "웹에서 씀", "folder": "",
            "format": "md", "content": "서버에서 내려온 내용", "createdAt": now, "modifiedAt": now})
        assert status in (200, 201), status

        notes = work / "Notes"
        (notes / "Old").mkdir(parents=True)
        (notes / "Inbox.md").write_text("데스크톱 원래 내용\n", encoding="utf-8")
        (notes / "Old" / "rename-me.md").write_text("이름이 바뀔 메모\n", encoding="utf-8")
        data_dir = work / "home" / ".local" / "share" / "dev.memonest.notes"
        (work / "home" / ".cache").mkdir(parents=True)  # 키링 데몬 소켓 폴더 (없으면 잠금 해제 데몬이 실패)
        index = data_dir / "index.sqlite"
        script = work / "e2e.js"
        script.write_text(E2E_JS.replace("__SERVER__", BASE).replace("__CODE__", pairing["code"]), encoding="utf-8")

        env = {**os.environ, "MEMONEST_NOTES_DIR": str(notes), "MEMONEST_INDEX_PATH": str(index),
               "MEMONEST_WEBVIEW_CONSOLE": "1", "MEMONEST_E2E_SCRIPT": str(script), "HOME": str(work / "home"),
               "XDG_DATA_HOME": str(work / "home" / ".local" / "share")}
        # 격리된 D-Bus 세션 + 임시 키링(임시 암호로 login 키링 생성·잠금 해제) → Secret Service 사용
        inner = ("printf 'check' | gnome-keyring-daemon --unlock --components=secrets >/dev/null 2>&1; "
                 f"exec xvfb-run -a '{ROOT / 'target/debug/memonest-native'}'")
        log_path = work / "app.log"
        app = subprocess.Popen(["dbus-run-session", "--", "sh", "-c", inner], env=env,
                               stdout=log_path.open("wb"), stderr=subprocess.STDOUT, cwd=work,
                               start_new_session=True)
        log = lambda: log_path.read_text(encoding="utf-8", errors="replace")

        results.append(wait("화면에서 장비 연결", lambda: "[e2e] paired" in log() or None))
        if not results[-1]:
            if "[e2e]" not in log():
                print("화면 조작 스크립트가 실행되지 않았습니다: e2e feature 빌드가 아닙니다. make desktop-sync-check 로 실행하세요")
            print("--- app.log ---\n" + "\n".join(l for l in log().splitlines() if not l.startswith("libEGL")))
            return 1
        by_title = lambda: {n["title"]: n for n in api.notes().values() if not n["deleted"]}
        results.append(wait("1. 데스크톱 파일 → 서버", lambda: {"Inbox", "rename-me"} <= set(by_title())))
        results.append(wait("2. 서버 메모 → 데스크톱 파일",
                            lambda: (notes / "웹에서 씀.md").exists()
                            and (notes / "웹에서 씀.md").read_text(encoding="utf-8") == "서버에서 내려온 내용"))

        (notes / "Inbox.md").write_text("바깥 편집기에서 고침\n", encoding="utf-8")
        results.append(wait("3. 앱 밖 편집 → 서버",
                            lambda: by_title().get("Inbox", {}).get("content") == "바깥 편집기에서 고침\n"))

        before = by_title().get("rename-me", {}).get("id")
        (notes / "Old" / "rename-me.md").rename(notes / "Old" / "renamed.md")
        results.append(wait("4. 앱 밖 이름 변경 → 같은 ID로 서버",
                            lambda: before and by_title().get("renamed", {}).get("id") == before
                            and "rename-me" not in by_title()))

        (notes / "웹에서 씀.md").unlink()

        def pending_deletion():
            db = sqlite3.connect(f"file:{index}?mode=ro", uri=True, timeout=2)
            try:
                return db.execute("select id from external_deletions").fetchall() == [(web_id,)]
            finally:
                db.close()
        results.append(wait("5a. 앱 밖 삭제 → '확인 필요'로 보관 (서버는 그대로)",
                            lambda: pending_deletion() and not api.notes()[web_id]["deleted"]))
        results.append(wait("5b. 화면에서 '서버에서도 삭제' → 서버 묘비",
                            lambda: api.notes()[web_id]["deleted"] and "[e2e] deletion: " in log()))

        # 8. 프로젝트 (ADR-0005)
        trip = str(uuid.uuid4())
        status, _ = api.call("PUT", f"/projects/{trip}", {"name": "여행"})
        assert status == 200, status
        status, _ = api.call("PUT", f"/notes/{uuid.uuid4()}", {
            "baseVersion": 0, "clientId": "web-check", "fencingToken": None, "title": "일정", "folder": "제주",
            "format": "md", "content": "3일", "createdAt": now, "modifiedAt": now, "projectId": trip})
        assert status == 200, status
        trip_note = lambda name: notes / f"@{name}" / "제주" / "일정.md"
        results.append(wait("8a. 웹 프로젝트 메모 → 데스크톱 @여행/제주/일정.md",
                            lambda: trip_note("여행").is_file() and trip_note("여행").read_text(encoding="utf-8") == "3일"))

        (notes / "@개인").mkdir()
        (notes / "@개인" / "일기.md").write_text("맑음\n", encoding="utf-8")

        def outside_project():
            status, projects = api.call("GET", "/projects")
            ids = {p["id"] for p in projects if p["name"] == "개인"}
            diary = by_title().get("일기")
            return ids and diary and diary.get("projectId") in ids
        results.append(wait("8b. 앱 밖 @개인 폴더 → 서버 프로젝트 '개인'과 그 메모", outside_project))

        status, _ = api.call("PUT", f"/projects/{trip}", {"name": "제주 여행"})
        assert status == 200, status
        results.append(wait("8c. 웹에서 프로젝트 이름 바꾸기 → 데스크톱 폴더 @제주 여행",
                            lambda: trip_note("제주 여행").is_file() and not (notes / "@여행").exists()))

        # 장비 토큰 = base64url 43자. 앱 데이터 폴더의 어떤 파일에도 그 모양의 값이 없어야 함
        device_json = json.loads((data_dir / "device.json").read_text(encoding="utf-8"))
        token_like = re.compile(rb"(?<![A-Za-z0-9_-])[A-Za-z0-9_-]{43}(?![A-Za-z0-9_-])")
        leaked = any("token" in k.lower() for k in device_json) or any(
            token_like.search(f.read_bytes()) for f in data_dir.rglob("*") if f.is_file())
        print(("OK   " if not leaked else "FAIL ") + "6. 장비 토큰이 device.json·인덱스에 없음 (OS 키링에만)")
        results.append(not leaked)

        status, _ = api.call("DELETE", f"/devices/{device_json['deviceId']}")
        assert status == 204, status
        notes_before = sorted(p.name for p in notes.rglob("*.md"))
        results.append(wait("7. 웹에서 장비 폐기 → 앱이 동기화를 멈추고 '다시 연결' 안내 (메모 파일 유지)",
                            lambda: "[e2e] revoked shown" in log()
                            and sorted(p.name for p in notes.rglob("*.md")) == notes_before))

        errors = [l for l in log().splitlines() if "CONSOLE ERROR" in l or "[e2e] error" in l]
        print(("OK   " if not errors else "FAIL ") + "웹뷰 콘솔 오류 없음")
        results.append(not errors)
        if not all(results):
            print("--- app.log ---\n" + "\n".join(l for l in log().splitlines() if not l.startswith("libEGL")))
            print("--- server.log ---\n" + (work / "server.log").read_text(errors="replace"))
        return 0 if all(results) else 1
    finally:
        for proc in (app, server):
            if proc and proc.poll() is None:
                try:
                    os.killpg(proc.pid, signal.SIGTERM)
                    proc.wait(timeout=10)
                except (ProcessLookupError, subprocess.TimeoutExpired):
                    os.killpg(proc.pid, signal.SIGKILL)
        subprocess.run(["docker", "exec", TEST_PG_CONTAINER, "dropdb", "-U", "postgres", "--if-exists", db_name])
        shutil.rmtree(work, ignore_errors=True)


# 웹뷰 안에서 실행: 실제 화면 조작 (장비 연결 → 바깥 삭제 처리)
E2E_JS = r"""
(async () => {
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  const waitFor = async (sel, ms = 60000) => {
    const end = Date.now() + ms;
    for (;;) {
      const node = document.querySelector(sel);
      if (node) return node;
      if (Date.now() > end) throw new Error('timeout: ' + sel);
      await sleep(200);
    }
  };
  const fill = (sel, value) => {
    const input = document.querySelector(sel);
    input.value = value;
    input.dispatchEvent(new Event('input', { bubbles: true }));
  };
  // 버튼 처리기는 앱이 장비 상태를 확인한 뒤 붙으므로 열릴 때까지 다시 누름
  for (let i = 0; !document.querySelector('#device-form'); i++) {
    if (i > 120) throw new Error('timeout: #device-form');
    document.querySelector('#account-btn')?.click();
    await sleep(500);
  }
  fill('#device-server', '__SERVER__');
  fill('#device-code', '__CODE__');
  fill('#device-name', 'sync-check');
  document.querySelector('#device-form').requestSubmit();
  try {
    await waitFor('#device-server-url');
  } catch (e) {
    throw new Error(e + ' / 화면: ' + (document.querySelector('#account-body')?.textContent ?? ''));
  }
  console.log('[e2e] paired');
  document.querySelector('#account-close').click();
  for (;;) {
    if ((document.querySelector('#sync-status')?.textContent ?? '').includes('밖에서 삭제')) break;
    await sleep(300);
  }
  console.log('[e2e] pending deletion shown');
  await sleep(5000); // 확인 스크립트가 '보관 중' 상태를 먼저 확인할 시간
  document.querySelector('#sync-status').click();
  (await waitFor('#external-deletions button[data-choice="delete"]')).click();
  const result = await waitFor('#external-deletion-result');
  console.log('[e2e] deletion: ' + result.textContent);
  document.querySelector('#account-close').click();
  for (;;) {
    if ((document.querySelector('#sync-status')?.textContent ?? '').includes('연결 해제됨')) break;
    await sleep(300);
  }
  console.log('[e2e] revoked shown');
})().catch((e) => console.log('[e2e] error ' + e));
"""

if __name__ == "__main__":
    sys.exit(main())
