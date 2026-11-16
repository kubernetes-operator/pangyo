// === 서버 연결 실패 원인 안내 (앱) ===
// 앱(Rust net.rs)은 연결 실패를 `offline: <원인> | <상세>`로 알려 줍니다.
// 2026-11-16 사용 의견: 회사 Windows 서버에서 "서버에 연결할 수 없습니다"만 보여 원인을 알 수 없었음.
import type { Locale } from '../settings';

export type OfflineReason = 'dns' | 'timeout' | 'tls' | 'proxy' | 'refused' | 'network';

const HINTS: Record<Locale, Record<OfflineReason, string>> = {
  ko: {
    dns: '서버 주소를 찾지 못했습니다. 주소를 확인하세요.',
    timeout: '응답 시간이 지났습니다. 방화벽·프록시 설정을 확인하세요.',
    tls: '서버 인증서를 확인하지 못했습니다. 보안 장비의 HTTPS 검사나 이 기기의 날짜·시간을 확인하세요.',
    proxy: '프록시를 거쳐 연결하지 못했습니다. 프록시 설정을 확인하세요.',
    refused: '서버가 연결을 거부했습니다. 주소를 확인하세요.',
    network: '네트워크 연결이 끊겼거나 막혔습니다. 방화벽·프록시 설정을 확인하세요.',
  },
  en: {
    dns: 'The server address could not be found. Check the address.',
    timeout: 'The server did not respond in time. Check the firewall and proxy settings.',
    tls: "The server certificate could not be verified. Check HTTPS inspection by security software or this device's date and time.",
    proxy: 'Could not connect through the proxy. Check the proxy settings.',
    refused: 'The server refused the connection. Check the address.',
    network: 'The network connection was dropped or blocked. Check the firewall and proxy settings.',
  },
};

const PATTERN = /(?:^|offline:\s*)(dns|timeout|tls|proxy|refused|network) \| ([\s\S]*)$/;

/** `offline: tls | …` 또는 (코드를 뗀) `tls | …` → 원인·상세. 형식이 아니면 null */
export function parseOffline(text: string): { reason: OfflineReason; detail: string } | null {
  const m = PATTERN.exec(text.trim());
  return m ? { reason: m[1] as OfflineReason, detail: m[2].trim() } : null;
}

/** 화면에 붙일 안내 (예: "서버 인증서를 확인하지 못했습니다. … (io: invalid peer certificate: UnknownIssuer)"). 없으면 '' */
export function offlineHint(text: string, locale: Locale): string {
  const parsed = parseOffline(text);
  if (!parsed) return '';
  const hint = HINTS[locale][parsed.reason];
  return parsed.detail ? `${hint} (${parsed.detail})` : hint;
}
