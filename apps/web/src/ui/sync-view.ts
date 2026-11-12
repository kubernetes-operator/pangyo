// === 동기화 화면 요소: 상단 상태 표시 + 메모 위 알림(읽기 전용 / 충돌 검토 / 오프라인 초안 / 오류) ===
import type { Locale } from '../settings';
import type { Note } from '../domain/types';
import type { ConflictChoice, LeaseState, SyncStatus } from '../sync/engine';

const TEXT: Record<Locale, Record<string, string>> = {
  ko: {
    off: '동기화 꺼짐',
    synced: '동기화됨',
    syncing: '동기화 중…',
    pending: '올릴 메모 {n}',
    offline: '오프라인 · 초안 {n}',
    conflicts: '충돌 {n}',
    error: '동기화 오류',
    locked: '{who}에서 편집 중이라 읽기 전용입니다',
    lockedUntil: '(~{time}, 편집이 끝나면 자동 해제)',
    retry: '다시 시도',
    conflict: '다른 기기에서 바뀐 내용과 충돌합니다. 어느 쪽도 지우지 않았습니다 — 선택하세요.',
    conflictDeleted: '다른 기기에서 이 메모를 삭제했습니다. 내 내용은 그대로 있습니다 — 선택하세요.',
    serverVersion: '서버 버전 보기',
    mine: '내 버전 유지',
    server: '서버 버전 사용',
    serverDeleted: '삭제 따르기',
    both: '둘 다 보관',
    draft: '오프라인 초안 — 연결되면 자동으로 올립니다',
    failed: '동기화 실패: {msg} (내용은 이 기기에 저장됨)',
    otherAccount: '이 기기의 메모는 다른 계정과 동기화되어 있어 동기화하지 않습니다',
    externalDeleted: '밖에서 삭제 {n} · 확인 필요',
    revoked: '장비 연결 해제됨 · 다시 연결 필요',
  },
  en: {
    off: 'Sync off',
    synced: 'Synced',
    syncing: 'Syncing…',
    pending: '{n} to upload',
    offline: 'Offline · {n} drafts',
    conflicts: '{n} conflicts',
    error: 'Sync error',
    locked: 'Read-only: being edited on {who}',
    lockedUntil: '(until ~{time}, released automatically)',
    retry: 'Retry',
    conflict: 'This note changed on another device too. Nothing was overwritten — choose:',
    conflictDeleted: 'This note was deleted on another device. Your text is kept — choose:',
    serverVersion: 'Show server version',
    mine: 'Keep mine',
    server: 'Use server',
    serverDeleted: 'Accept deletion',
    both: 'Keep both',
    draft: 'Offline draft — uploads when back online',
    failed: 'Sync failed: {msg} (saved on this device)',
    otherAccount: 'Notes on this device belong to another account; sync is off',
    externalDeleted: '{n} deleted outside · review',
    revoked: 'Device unlinked · link again',
  },
};

const fill = (s: string, vars: Record<string, string | number>) =>
  s.replace(/\{(\w+)\}/g, (_, k: string) => String(vars[k] ?? ''));

export function syncStatusText(
  status: SyncStatus | null,
  locale: Locale,
  blocked = false,
  externalDeleted = 0,
  revoked = false,
): string {
  const t = TEXT[locale];
  if (revoked) return t.revoked;
  const base = baseStatusText(status, t, blocked);
  return externalDeleted ? `${base} · ${fill(t.externalDeleted, { n: externalDeleted })}` : base;
}

function baseStatusText(status: SyncStatus | null, t: Record<string, string>, blocked: boolean): string {
  if (blocked) return t.otherAccount;
  if (!status) return t.off;
  if (status.conflicts) return fill(t.conflicts, { n: status.conflicts });
  if (status.phase === 'offline') return fill(t.offline, { n: status.pending });
  if (status.phase === 'error') return t.error;
  if (status.phase === 'syncing') return t.syncing;
  return status.pending ? fill(t.pending, { n: status.pending }) : t.synced;
}

function button(label: string, id: string, onClick: () => void): HTMLButtonElement {
  const b = document.createElement('button');
  b.type = 'button';
  b.id = id;
  b.textContent = label;
  b.onclick = onClick;
  return b;
}

/** 메모 위 알림을 다시 그립니다. 보여줄 것이 없으면 숨김. */
export function renderSyncBanner(
  banner: HTMLElement,
  args: {
    note: Note | null;
    lease: LeaseState | null;
    status: SyncStatus | null;
    locale: Locale;
    onRetry: () => void;
    onResolve: (choice: ConflictChoice) => void;
  },
): void {
  const { note, lease, status, locale } = args;
  const t = TEXT[locale];
  banner.replaceChildren();
  banner.className = 'sync-banner';
  const text = (s: string) => {
    const p = document.createElement('p');
    p.textContent = s;
    return p;
  };
  const conflict = note?.sync?.conflict;
  if (note && conflict) {
    banner.classList.add('conflict');
    banner.dataset.kind = 'conflict';
    const details = document.createElement('details');
    const summary = document.createElement('summary');
    summary.textContent = `${t.serverVersion} (v${conflict.version}, ${new Date(conflict.modifiedAt).toLocaleString()})`;
    const pre = document.createElement('pre');
    pre.id = 'conflict-server-content';
    pre.textContent = conflict.deleted ? '—' : `${conflict.title}\n\n${conflict.content}`;
    details.append(summary, pre);
    const actions = document.createElement('div');
    actions.className = 'toolbar';
    actions.append(
      button(t.mine, 'conflict-mine', () => args.onResolve('mine')),
      button(conflict.deleted ? t.serverDeleted : t.server, 'conflict-server', () => args.onResolve('server')),
      button(t.both, 'conflict-both', () => args.onResolve('both')),
    );
    banner.append(text(conflict.deleted ? t.conflictDeleted : t.conflict), details, actions);
  } else if (lease?.kind === 'denied') {
    banner.classList.add('locked');
    banner.dataset.kind = 'locked';
    const until = lease.expiresAt ? new Date(lease.expiresAt).toLocaleTimeString() : '';
    banner.append(
      text(`${fill(t.locked, { who: lease.holderLabel || '?' })} ${until ? fill(t.lockedUntil, { time: until }) : ''}`),
      button(t.retry, 'lease-retry', args.onRetry),
    );
  } else if (note?.sync?.error) {
    banner.classList.add('error');
    banner.dataset.kind = 'error';
    banner.append(text(fill(t.failed, { msg: note.sync.error })));
  } else if (note?.sync?.dirty && status?.phase === 'offline') {
    banner.dataset.kind = 'draft';
    banner.append(text(t.draft));
  } else {
    banner.dataset.kind = '';
    banner.hidden = true;
    return;
  }
  banner.hidden = false;
}
