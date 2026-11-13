// 장비 등록 기본 이름: 호스트 이름(없으면 Android 모델)을 앞에 둠 (2026-11-13 사용 의견)
import { describe, expect, it } from 'vitest';
import { androidModel, defaultDeviceName } from './device-panel';

const WIN = 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0 Safari/537.36 Edg/130.0';
const ANDROID_WV =
  'Mozilla/5.0 (Linux; Android 14; SM-S918N Build/UP1A.231005.007; wv) AppleWebKit/537.36 (KHTML, like Gecko) Version/4.0 Chrome/130.0 Mobile Safari/537.36';
const ANDROID_REDUCED = 'Mozilla/5.0 (Linux; Android 10; K) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0 Mobile Safari/537.36';

describe('defaultDeviceName', () => {
  it('호스트 이름을 앞에, OS를 뒤에', () => {
    expect(defaultDeviceName('DESKTOP-AB12', WIN)).toBe('DESKTOP-AB12 (Windows)');
  });
  it('Android는 호스트 이름이 없으면 기기 모델', () => {
    expect(androidModel(ANDROID_WV)).toBe('SM-S918N');
    expect(defaultDeviceName(null, ANDROID_WV)).toBe('SM-S918N (Android)');
  });
  it('모델을 알 수 없으면(축약 UA "K") 예전 형식', () => {
    expect(androidModel(ANDROID_REDUCED)).toBe('');
    expect(defaultDeviceName(null, ANDROID_REDUCED)).toBe('MemoNest · Android');
  });
  it('빈 호스트 이름은 무시', () => {
    expect(defaultDeviceName('  ', WIN)).toBe('MemoNest · Windows');
  });
});
