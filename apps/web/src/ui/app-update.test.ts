import { describe, expect, it } from 'vitest';
import { isNewBuild, pickUpdate, buildLabel } from './app-update';

const files = [
  { id: 'android-apk', platform: 'Android', label: 'APK', fileName: 'a.apk', size: 1, sha256: 'x' },
  { id: 'windows-exe', platform: 'Windows', label: 'exe', fileName: 'a.exe', size: 1, sha256: 'x' },
];

describe('앱 안 업데이트 (A안)', () => {
  it('빌드가 다르면 새 버전, 같거나 모르면 아님 (앞 7자리 비교)', () => {
    expect(isNewBuild('df68a25aaaa', 'b1c2d3e4')).toBe(true);
    expect(isNewBuild('df68a25aaaa', 'DF68A25')).toBe(false);
    expect(isNewBuild(null, 'b1c2d3e')).toBe(false); // 로컬 개발 빌드
    expect(isNewBuild('df68a25', undefined)).toBe(false); // build 없는 예전 목록
  });
  it('이 기기에 맞는 설치 파일을 고름', () => {
    const android = { version: '0.1.0', build: 'aaaaaaa1', platform: 'android' };
    expect(pickUpdate(android, { build: 'bbbbbbb', files }).file?.id).toBe('android-apk');
    expect(pickUpdate(android, { build: 'aaaaaaa', files }).file).toBeNull();
    expect(pickUpdate({ ...android, platform: 'ios' }, { build: 'bbbbbbb', files }).file).toBeNull();
    expect(buildLabel(android)).toBe('0.1.0 (aaaaaaa)');
  });
});
