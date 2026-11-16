// 앱 연결 실패 원인 안내 (2026-11-16 사용 의견: 회사 Windows 서버에서 원인을 알 수 없었음)
import { describe, expect, it } from 'vitest';
import { offlineHint, parseOffline } from './offline-reason';

describe('parseOffline', () => {
  it('reads the reason and detail from the app error', () => {
    expect(parseOffline('offline: tls | io: invalid peer certificate: UnknownIssuer')).toEqual({
      reason: 'tls',
      detail: 'io: invalid peer certificate: UnknownIssuer',
    });
    expect(parseOffline('dns | host not found')).toEqual({ reason: 'dns', detail: 'host not found' });
    expect(parseOffline('offline: network | io: connection reset (proxy proxy.example:8080)')?.reason).toBe('network');
  });

  it('ignores other errors (web fetch, old app builds)', () => {
    expect(parseOffline('server unreachable')).toBeNull();
    expect(parseOffline('offline: io: connection refused')).toBeNull();
    expect(parseOffline('not_paired')).toBeNull();
  });
});

describe('offlineHint', () => {
  it('explains the reason with the detail', () => {
    const ko = offlineHint('offline: tls | io: invalid peer certificate: UnknownIssuer', 'ko');
    expect(ko).toContain('인증서');
    expect(ko).toContain('(io: invalid peer certificate: UnknownIssuer)');
    expect(offlineHint('proxy | 407', 'en')).toBe('Could not connect through the proxy. Check the proxy settings. (407)');
  });

  it('is empty when there is no reason', () => {
    expect(offlineHint('server unreachable', 'ko')).toBe('');
  });
});
