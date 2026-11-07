#!/usr/bin/env python3
"""데스크톱 앱 실행 확인 (Linux/Windows/macOS 공통).

임시 폴더에 메모 2개를 넣고 앱을 띄운 뒤, 화면(웹뷰)이 Rust 명령을 호출해
인덱스(SQLite)에 목록·설정·열어둔 시간을 남겼는지 검사합니다.
사용자의 실제 메모 폴더와 앱 데이터 폴더는 건드리지 않습니다.

사용: python3 scripts/desktop_smoke.py <앱 실행 파일 경로> [--seconds 15]
"""
# === 0. 설정 변수 ===
DEFAULT_SECONDS = 60        # 최대 대기 시간. 조건을 모두 만족하면 바로 끝납니다 (느린 CI/에뮬레이션 대비)
POLL_SECONDS = 1            # 인덱스 확인 간격
STARTUP_SECONDS = 20        # Linux: 이 시간 안에 웹뷰 페이지 로드가 시작되지 않으면 다시 실행
STARTUP_RETRIES = 2         # Linux 재실행 횟수. 개발 서버(AMD+Mesa 26.0.8+Xvfb)에서 Mesa 초기화가
                            # 간헐적으로 멈추는 문제(docs/12_TESTPLAN.md 알려진 문제) 대응. 재시도는 결과에 표시
MIN_OPEN_MS = 5_000         # 기록돼야 하는 최소 열어둔 시간
EXPECTED_NOTES = [("BSD/device.md",), ("Inbox.md",)]

import argparse
import os
import signal
import shutil
import sqlite3
import subprocess
import sys
import tempfile
import time
from pathlib import Path


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("binary", type=Path)
    parser.add_argument("--seconds", type=int, default=DEFAULT_SECONDS)
    args = parser.parse_args()
    if not args.binary.exists():
        print(f"실행 파일 없음: {args.binary}", file=sys.stderr)
        return 2

    work = Path(tempfile.mkdtemp(prefix="memonest-smoke-"))
    try:
        notes = work / "Notes"
        (notes / "BSD").mkdir(parents=True)
        (notes / "Inbox.md").write_text("# 받은 메모\n", encoding="utf-8")
        (notes / "BSD" / "device.md").write_text("from device\n", encoding="utf-8")  # 바깥에서 만든 파일
        index = work / "data" / "index.sqlite"

        env = dict(os.environ)
        env.update(
            MEMONEST_NOTES_DIR=str(notes),
            MEMONEST_INDEX_PATH=str(index),
            MEMONEST_WEBVIEW_CONSOLE="1",
            APPIMAGE_EXTRACT_AND_RUN="1",  # AppImage를 FUSE 없이 실행
            HOME=str(work / "home") if sys.platform.startswith("linux") else env.get("HOME", ""),
            XDG_DATA_HOME=str(work / "home" / ".local" / "share"),
        )
        command = [str(args.binary.resolve())]
        if sys.platform.startswith("linux") and not env.get("DISPLAY"):
            if not shutil.which("xvfb-run"):
                print("화면이 없는 리눅스에서는 xvfb-run 필요", file=sys.stderr)
                return 2
            command = ["xvfb-run", "-a", *command]

        print(f"[desktop-smoke] 최대 {args.seconds}초: {args.binary.name} ({sys.platform})")
        linux = sys.platform.startswith("linux")
        retries = STARTUP_RETRIES if linux else 0
        for attempt in range(retries + 1):
            log_path = work / f"app-{attempt}.log"
            started = time.monotonic()
            stalled = False
            with log_path.open("wb") as log:
                # 새 프로세스 그룹으로 실행해 xvfb-run 아래 앱·WebKit까지 함께 종료 (고아 프로세스 방지)
                group = {"start_new_session": True} if os.name == "posix" else {
                    "creationflags": subprocess.CREATE_NEW_PROCESS_GROUP}
                proc = subprocess.Popen(command, env=env, stdout=log, stderr=subprocess.STDOUT, cwd=work, **group)
                state = read_index(index)
                while time.monotonic() - started < args.seconds and proc.poll() is None:
                    time.sleep(POLL_SECONDS)
                    state = read_index(index)
                    if index_ok(state):
                        break
                    if linux and time.monotonic() - started > STARTUP_SECONDS and \
                            "page Started" not in log_path.read_text(encoding="utf-8", errors="replace"):
                        stalled = True
                        break
                alive = proc.poll() is None
                stop(proc)
            if not stalled:
                break
            print(f"[desktop-smoke] 재시도 {attempt + 1}/{retries}: {STARTUP_SECONDS}초 안에 웹뷰가 시작되지 않음 (Mesa 초기화 지연 의심)")
        elapsed = time.monotonic() - started
        output = log_path.read_text(encoding="utf-8", errors="replace")
        print("\n".join(l for l in output.splitlines() if not l.startswith("libEGL")))
        print(f"[desktop-smoke] {elapsed:.1f}초 경과")

        note_rows, meta, open_ms = state
        checks = {"앱이 확인 끝까지 살아 있음": alive, "웹뷰 시작됨": not stalled}
        checks["웹뷰 콘솔 오류 없음 (Linux만 수집)"] = "CONSOLE ERROR" not in output and "SECURITY ERROR" not in output
        checks["바깥 파일 2개가 인덱스에 등록됨 (화면→list_notes)"] = note_rows == EXPECTED_NOTES
        checks["화면 언어 설정 저장됨 (화면→set_meta)"] = meta.get("locale") == '"ko"'
        checks[f"열어둔 시간 {MIN_OPEN_MS // 1000}초 이상 기록 (화면→save_note)"] = open_ms >= MIN_OPEN_MS
        for name, ok in checks.items():
            print(("OK   " if ok else "FAIL ") + name)
        return 0 if all(checks.values()) else 1
    finally:
        shutil.rmtree(work, ignore_errors=True)


def read_index(index: Path):
    """(메모 경로 목록, 메타, 열어둔 시간 합). 앱이 쓰는 중이면 다음 확인 때 다시 읽습니다."""
    if not index.exists():
        return [], {}, 0
    try:
        db = sqlite3.connect(f"file:{index}?mode=ro", uri=True, timeout=2)
        try:
            notes = db.execute("select path from notes order by path").fetchall()
            meta = dict(db.execute("select key, value_json from meta").fetchall())
            open_ms = db.execute("select coalesce(sum(open_ms),0) from edit_sessions").fetchone()[0]
            return notes, meta, open_ms
        finally:
            db.close()
    except sqlite3.Error:
        return [], {}, 0


def index_ok(state) -> bool:
    notes, meta, open_ms = state
    return notes == EXPECTED_NOTES and meta.get("locale") == '"ko"' and open_ms >= MIN_OPEN_MS


def stop(proc: subprocess.Popen) -> None:
    """프로세스 그룹 전체 종료 (POSIX) / 프로세스 종료 (Windows: WebView2 자식은 부모와 함께 종료)"""
    try:
        if os.name == "posix":
            os.killpg(proc.pid, signal.SIGTERM)
        else:
            proc.terminate()
        proc.wait(timeout=10)
    except (ProcessLookupError, subprocess.TimeoutExpired):
        if os.name == "posix":
            try:
                os.killpg(proc.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
        else:
            proc.kill()


if __name__ == "__main__":
    sys.exit(main())
