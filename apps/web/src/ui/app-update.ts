// === 앱 안 업데이트 (A안, 2026-11-13 사용자 선택): 새 버전 알림 + 받기 ===
// 앱에 들어간 빌드 커밋(MEMONEST_BUILD)과 서버에 게시한 설치 파일 목록의 build를 비교합니다.
// 새 버전이면 이 기기에 맞는 설치 파일의 일회용 주소를 받아 OS 브라우저/설치 화면으로 엽니다(서명 키 불필요).
// 로컬 개발 빌드(build 없음)나 build가 없는 예전 목록에서는 알리지 않습니다.
import type { DownloadFile } from '../account/api';

export interface AppInfo {
  version: string;
  /** 빌드 커밋 (CI 빌드만) */
  build: string | null;
  /** Rust std::env::consts::OS: android, windows, macos, linux, ios */
  platform: string;
  /** Windows 설치 방식: msi(Program Files) / nsis(사용자 폴더 .exe). 같은 방식의 파일로 업데이트 */
  installKind?: 'msi' | 'nsis' | null;
}

/** 이 기기에서 받을 설치 파일 (scripts/publish-downloads.sh의 id) */
export const UPDATE_FILE_ID: Record<string, string> = {
  android: 'android-apk',
  windows: 'windows-exe',
  macos: 'macos-dmg',
  linux: 'linux-appimage',
};

const short = (build: string) => build.trim().slice(0, 7).toLowerCase();

/** 게시된 빌드가 지금 앱과 다르면 새 버전 (둘 중 하나라도 모르면 알리지 않음) */
export function isNewBuild(current: string | null | undefined, published: string | null | undefined): boolean {
  if (!current || !published) return false;
  return short(current) !== short(published);
}

export interface UpdateState {
  info: AppInfo;
  /** 새 버전이면 받을 파일 */
  file: DownloadFile | null;
  publishedBuild: string | null;
}

/** 이 앱을 업데이트할 설치 파일 id. Windows는 설치 방식과 같은 파일 (msi로 설치한 앱을 .exe로 올리면 다른 폴더에 따로 설치됨) */
export function updateFileId(info: AppInfo): string | undefined {
  if (info.platform === 'windows' && info.installKind === 'msi') return 'windows-msi';
  return UPDATE_FILE_ID[info.platform];
}

export function pickUpdate(info: AppInfo, list: { build?: string; files: DownloadFile[] }): UpdateState {
  const id = updateFileId(info);
  const file = list.files.find((f) => f.id === id) ?? null;
  const fresh = isNewBuild(info.build, list.build);
  return { info, file: fresh ? file : null, publishedBuild: list.build ?? null };
}

export const buildLabel = (info: AppInfo) =>
  `${info.version}${info.build ? ` (${short(info.build)})` : ''}${info.installKind ? ` · ${info.installKind === 'msi' ? '.msi' : '.exe'}` : ''}`;

/** 지난 Windows 업데이트 기록 "exit=0 kind=msi" → 실패면 종료 코드, 성공이면 0, 알 수 없으면 null */
export function updateExitCode(log: string | null | undefined): number | null {
  const m = /exit=(-?\d+)/.exec(log ?? '');
  return m ? Number(m[1]) : null;
}
