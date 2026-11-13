import { describe, expect, it } from 'vitest';
import { crc32, createZip, readStoredZip } from './zip';

describe('ZIP (프로젝트 전체 다운로드)', () => {
  it('CRC32 표준값', () => {
    expect(crc32(new TextEncoder().encode('123456789')).toString(16)).toBe('cbf43926');
  });
  it('한글 경로·빈 폴더를 담고 다시 읽힘', () => {
    const enc = new TextEncoder();
    const zip = createZip([
      { path: '업무/회의.md', data: enc.encode('# 회의\n- 안건') },
      { path: '보관/' },
      { path: '메모.txt', data: enc.encode('plain') },
    ]);
    expect(readStoredZip(zip)).toEqual([
      { path: '업무/회의.md', text: '# 회의\n- 안건' },
      { path: '보관/', text: '' },
      { path: '메모.txt', text: 'plain' },
    ]);
    // 끝 레코드: 항목 3개
    const view = new DataView(zip.buffer);
    expect(view.getUint32(zip.length - 22, true)).toBe(0x06054b50);
    expect(view.getUint16(zip.length - 22 + 10, true)).toBe(3);
  });
});
