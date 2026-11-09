#!/usr/bin/env python3
"""FreeBSD arm64 가상 머신(QEMU)에서 memonest-bsd를 빌드·테스트·실행 확인 (로드맵 08).

- 원본 VM 이미지는 수정하지 않음 (매번 덮어쓰기용 qcow2 오버레이)
- 소스: tar를 읽기 전용 디스크(vtbd1)로 전달 / 결과: vtbd2에 tar로 받아 옴
- 시리얼 콘솔 자동 조작: root 로그인(공식 VM 이미지 기본: 암호 없음) 후 명령마다 종료 코드 확인
사용: python3 scripts/bsd-vm-verify.py   (x86 호스트에서는 ARM 에뮬레이션이라 수십 분 걸림)
"""
# === 0. 설정 변수 ===
VM_DIR = "/var/tmp/memonest-vm"                                    # 이미지·로그 폴더 (저장소 밖)
BASE_IMAGE = "FreeBSD-15.1-RELEASE-arm64-aarch64-ufs.qcow2"       # 공식 이미지 (SHA256 검증 후 압축 해제)
# 주의: pkg 저장소 패키지는 최신 마이너 버전 기준으로 빌드됨 → 15.0에서는 rust가 libc 버전 불일치로 실행 안 됨
UEFI = "/usr/share/qemu-efi-aarch64/QEMU_EFI.fd"
CPUS, MEMORY_MB, DISK_GB = 6, 4096, 24
SOURCES = ["Cargo.toml", "Cargo.lock", "crates", "apps/bsd", "apps/native/src-tauri", "packages/protocol", "db"]
TIMEOUTS = {"boot": 1800, "pkg": 3600, "build": 10800, "default": 600}

import os
import re
import subprocess
import sys
import tarfile
import time
from pathlib import Path

import pexpect

ROOT = Path(__file__).resolve().parent.parent
VM = Path(VM_DIR)
LOG = VM / "vm-console.log"
PROMPT_MARK = "__MEMONEST_RC="


def sh(child, cmd, timeout_key="default"):
    """명령 실행 후 종료 코드 반환 (출력은 콘솔 로그에 기록)"""
    print(f"[vm] $ {cmd}", flush=True)
    child.sendline(f"{cmd}; echo {PROMPT_MARK}$?__")
    child.expect(re.escape(PROMPT_MARK) + r"(\d+)__", timeout=TIMEOUTS[timeout_key])
    rc = int(child.match.group(1))
    print(f"[vm]   → exit {rc}", flush=True)
    return rc


def main() -> int:
    base = VM / BASE_IMAGE
    if not base.exists():
        print(f"VM 이미지 없음: {base}", file=sys.stderr)
        return 2
    overlay, source, results = VM / "run.qcow2", VM / "source.tar", VM / "results.img"
    # --reuse: 이전 실행의 VM 디스크(rust 설치·빌드 캐시 포함)를 그대로 써서 빠르게 재검증
    reuse = "--reuse" in sys.argv and overlay.exists()
    for f in ((source, results) if reuse else (overlay, source, results)):
        f.unlink(missing_ok=True)
    if not reuse:
        subprocess.run(["qemu-img", "create", "-q", "-f", "qcow2", "-F", "qcow2", "-b", str(base), str(overlay), f"{DISK_GB}G"], check=True)
    files = subprocess.run(["git", "ls-files", "-co", "--exclude-standard", *SOURCES], cwd=ROOT, check=True,
                           capture_output=True, text=True).stdout.split()
    with tarfile.open(source, "w") as tar:
        for f in files:
            if (ROOT / f).exists():
                tar.add(ROOT / f, arcname=f)
    with open(results, "wb") as r:
        r.truncate(64 * 1024 * 1024)
    print(f"[host] 소스 {len(files)}개 파일, 오버레이 {DISK_GB}G 준비", flush=True)

    cmd = ["qemu-system-aarch64", "-M", "virt", "-cpu", "cortex-a72",  # max는 pauth 에뮬레이션으로 매우 느림
            "-smp", str(CPUS), "-m", str(MEMORY_MB),
           "-bios", UEFI, "-nographic",
           "-drive", f"if=virtio,file={overlay},format=qcow2",
           "-drive", f"if=virtio,file={source},format=raw,readonly=on",
           "-drive", f"if=virtio,file={results},format=raw",
           "-netdev", "user,id=n0", "-device", "virtio-net-pci,netdev=n0,romfile="]  # 네트워크 부팅 ROM 불필요
    started = time.monotonic()
    with open(LOG, "w", encoding="utf-8", errors="replace") as log:
        child = pexpect.spawn(cmd[0], cmd[1:], encoding="utf-8", codec_errors="replace", timeout=TIMEOUTS["default"])
        child.logfile_read = log
        try:
            child.expect("login:", timeout=TIMEOUTS["boot"])
            print(f"[vm] 부팅 완료 ({time.monotonic() - started:.0f}초)", flush=True)
            child.sendline("root")
            time.sleep(3)
            child.sendline("export PAGER=cat ASSUME_ALWAYS_YES=yes LANG=C.UTF-8; set -o pipefail")  # 파이프 중간 실패도 실패로
            checks = {}
            checks["arch"] = sh(child, "uname -m -p -r && freebsd-version")
            checks["pkg_rust"] = sh(child, "pkg install -y rust >/dev/null && rustc --version && cargo --version", "pkg")
            checks["source"] = sh(child, "rm -rf /root/results && mkdir -p /root/src /root/results && tar -xf /dev/vtbd1 -C /root/src")
            checks["test"] = sh(child, "cd /root/src && cargo test -p memonest-bsd 2>&1 | tee /root/results/cargo-test.log | tail -15", "build")
            checks["build"] = sh(child, "cd /root/src && cargo build --release -p memonest-bsd 2>&1 | tail -3", "build")
            checks["version"] = sh(child, "/root/src/target/release/memonest-bsd --version && file /root/src/target/release/memonest-bsd | tee /root/results/binary.txt")
            checks["tui"] = sh(child, "sh /root/src/apps/bsd/vm/tui-smoke.sh 2>&1 | tee /root/results/tui-smoke.log | tail -4 && grep -q TUI_SMOKE_OK /root/results/tui-smoke.log", "default")
            # 키오스크 설치: 두 번 실행해도 같은 결과(멱등), 계정 셸·자동 로그인 설정 확인
            checks["kiosk"] = sh(child, "cd /root/src/apps/bsd/kiosk && sh install.sh /root/src/target/release/memonest-bsd --dry-run > /root/results/kiosk-dry.log"
                                        " && sh install.sh /root/src/target/release/memonest-bsd > /root/results/kiosk-install.log"
                                        " && sh install.sh /root/src/target/release/memonest-bsd > /root/results/kiosk-install2.log"
                                        " && pw usershow memonest | tee /root/results/kiosk-user.txt | grep -q ':/usr/local/bin/memonest-kiosk$'"
                                        " && grep -c '^memonest.autologin:' /etc/gettytab | grep -qx 1"
                                        " && grep '^ttyv0' /etc/ttys | tee /root/results/kiosk-ttys.txt | grep -q memonest.autologin")
            sh(child, "tar -cf /dev/vtbd2 -C /root results")
            child.sendline("shutdown -p now")
            child.expect(pexpect.EOF, timeout=300)
        except pexpect.ExceptionPexpect as e:
            print(f"[host] VM 조작 실패: {type(e).__name__} (로그: {LOG})", file=sys.stderr)
            child.terminate(force=True)
            return 1
    out = VM / "results"
    subprocess.run(["rm", "-rf", str(out)])
    out.mkdir()
    subprocess.run(["tar", "-xf", str(results), "-C", str(out)], check=False)
    print(f"[host] 총 {(time.monotonic() - started) / 60:.1f}분, 결과: {out}/results", flush=True)
    for name, rc in checks.items():
        print(("OK   " if rc == 0 else "FAIL ") + name)
    return 0 if all(rc == 0 for rc in checks.values()) else 1


if __name__ == "__main__":
    sys.exit(main())
