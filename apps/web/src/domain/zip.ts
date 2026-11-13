// === ZIP 만들기 (프로젝트 전체 다운로드, 2026-11-13 사용 의견) ===
// 의존성 없이 '저장(무압축)' 방식 ZIP을 만듭니다. 메모는 작은 텍스트라 압축 이득이 적고, 어떤 OS에서도 열립니다.
// 파일 이름은 UTF-8(일반 목적 비트 11)이라 한글 경로가 그대로 보입니다. 폴더는 '/'로 끝나는 항목.

export interface ZipEntry {
  /** 'Work/Plan.md' 또는 빈 폴더 'Archive/' */
  path: string;
  data?: Uint8Array;
  modified?: Date;
}

const CRC_TABLE = (() => {
  const table = new Uint32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    table[n] = c >>> 0;
  }
  return table;
})();

export function crc32(data: Uint8Array): number {
  let c = 0xffffffff;
  for (const byte of data) c = CRC_TABLE[(c ^ byte) & 0xff]! ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

function dosTime(d: Date): { time: number; date: number } {
  const year = Math.max(1980, d.getFullYear());
  return {
    time: (d.getHours() << 11) | (d.getMinutes() << 5) | Math.floor(d.getSeconds() / 2),
    date: ((year - 1980) << 9) | ((d.getMonth() + 1) << 5) | d.getDate(),
  };
}

export function createZip(entries: ZipEntry[]): Uint8Array {
  const enc = new TextEncoder();
  const chunks: Uint8Array[] = [];
  const central: Uint8Array[] = [];
  let offset = 0;
  for (const entry of entries) {
    const name = enc.encode(entry.path);
    const data = entry.data ?? new Uint8Array();
    const crc = crc32(data);
    const { time, date } = dosTime(entry.modified ?? new Date());
    const local = new DataView(new ArrayBuffer(30));
    local.setUint32(0, 0x04034b50, true);
    local.setUint16(4, 20, true); // 필요한 버전 2.0
    local.setUint16(6, 0x0800, true); // UTF-8 이름
    local.setUint16(8, 0, true); // 저장(무압축)
    local.setUint16(10, time, true);
    local.setUint16(12, date, true);
    local.setUint32(14, crc, true);
    local.setUint32(18, data.length, true);
    local.setUint32(22, data.length, true);
    local.setUint16(26, name.length, true);
    local.setUint16(28, 0, true);
    chunks.push(new Uint8Array(local.buffer), name, data);
    const dir = new DataView(new ArrayBuffer(46));
    dir.setUint32(0, 0x02014b50, true);
    dir.setUint16(4, 20, true);
    dir.setUint16(6, 20, true);
    dir.setUint16(8, 0x0800, true);
    dir.setUint16(10, 0, true);
    dir.setUint16(12, time, true);
    dir.setUint16(14, date, true);
    dir.setUint32(16, crc, true);
    dir.setUint32(20, data.length, true);
    dir.setUint32(24, data.length, true);
    dir.setUint16(28, name.length, true);
    dir.setUint32(38, entry.path.endsWith('/') ? 0x10 : 0, true); // 폴더 속성
    dir.setUint32(42, offset, true);
    central.push(new Uint8Array(dir.buffer), name);
    offset += 30 + name.length + data.length;
  }
  const centralSize = central.reduce((n, c) => n + c.length, 0);
  const end = new DataView(new ArrayBuffer(22));
  end.setUint32(0, 0x06054b50, true);
  end.setUint16(8, entries.length, true);
  end.setUint16(10, entries.length, true);
  end.setUint32(12, centralSize, true);
  end.setUint32(16, offset, true);
  const all = [...chunks, ...central, new Uint8Array(end.buffer)];
  const out = new Uint8Array(all.reduce((n, c) => n + c.length, 0));
  let at = 0;
  for (const c of all) {
    out.set(c, at);
    at += c.length;
  }
  return out;
}

/** 시험·확인용: 무압축 ZIP의 (경로, 내용) 목록 */
export function readStoredZip(zip: Uint8Array): Array<{ path: string; text: string }> {
  const view = new DataView(zip.buffer, zip.byteOffset, zip.byteLength);
  const dec = new TextDecoder();
  const out: Array<{ path: string; text: string }> = [];
  let at = 0;
  while (at + 4 <= zip.length && view.getUint32(at, true) === 0x04034b50) {
    const size = view.getUint32(at + 18, true);
    const nameLen = view.getUint16(at + 26, true);
    const extra = view.getUint16(at + 28, true);
    const path = dec.decode(zip.subarray(at + 30, at + 30 + nameLen));
    const start = at + 30 + nameLen + extra;
    out.push({ path, text: dec.decode(zip.subarray(start, start + size)) });
    at = start + size;
  }
  return out;
}
