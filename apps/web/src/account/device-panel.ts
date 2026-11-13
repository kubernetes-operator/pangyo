// === 장비 등록 화면 (데스크톱·모바일 앱, 로드맵 06 / ADR-0004) ===
// 웹 계정 화면에서 만든 일회용 코드 + 서버 주소를 입력해 이 기기를 계정에 연결합니다.
// 장비 토큰은 Rust 쪽 보안 저장소에만 있고 이 화면에는 오지 않습니다.
// 앱 밖에서 지워진 메모(바깥 삭제)는 여기서 '서버에서도 삭제' 또는 '다시 받기'를 고릅니다.
import type { Locale } from '../settings';
import { DeviceError, deviceApi, type DeviceInfo, type ExternalDeletion } from '../storage/tauri';

// === 0. 문구 (ko/en) ===
const TEXT: Record<Locale, Record<string, string>> = {
  ko: {
    title: '장비 연결',
    close: '닫기',
    intro: '웹 계정 화면에서 "장비 등록 코드 만들기"를 누르면 서버 주소와 코드가 나옵니다. 코드는 10분 동안 한 번만 쓸 수 있습니다.',
    server: '서버 주소',
    code: '등록 코드',
    name: '이 장비 이름',
    pair: '연결',
    pairing: '연결 중…',
    paired: '연결됨',
    serverLabel: '서버',
    nameLabel: '장비',
    since: '연결한 때',
    unpair: '이 기기 연결 해제',
    unpairHelp: '연결을 해제해도 이 기기의 메모 파일은 그대로 남습니다. 서버의 장비 기록은 웹 계정 화면에서 폐기하세요.',
    revoked: '서버에서 이 장비 연결이 폐기되었거나 만료되었습니다. 다시 연결하세요.',
    deletions: '앱 밖에서 지워진 메모',
    deletionsHelp: '파일 탐색기 등에서 지운 메모입니다. 서버와 다른 기기에는 아직 남아 있습니다.',
    noDeletions: '없음',
    deleteRemote: '서버에서도 삭제',
    restore: '다시 받기',
    working: '처리 중…',
    secure: 'OS 보안 저장소를 쓸 수 없어 연결 정보를 저장하지 못했습니다. (Linux: 키링/Secret Service 실행 여부 확인)',
    offline: '서버에 연결할 수 없습니다. 주소와 네트워크를 확인하세요.',
    outcome_deleted: '서버와 다른 기기에서도 삭제했습니다.',
    outcome_gone: '이미 서버에서 지워진 메모라 목록에서 뺐습니다.',
    outcome_changed: '지운 뒤 다른 기기에서 고쳐져 삭제하지 않고 최신 내용을 다시 받았습니다.',
    outcome_busy: '다른 기기에서 편집 중이라 지금은 지울 수 없습니다. 잠시 뒤 다시 시도하세요.',
    outcome_restored: '서버에서 다시 받았습니다.',
    outcome_missing: '서버에도 없는 메모라 목록에서 뺐습니다.',
    outcome_offline: '서버에 연결할 수 없습니다. 연결되면 다시 시도하세요.',
    outcome_notSynced: '동기화가 켜져 있지 않습니다. 먼저 장비를 연결하세요.',
    appVersion: '앱 버전',
    checkUpdate: '업데이트 확인',
  },
  en: {
    title: 'Device link',
    close: 'Close',
    intro: 'On the web Account screen, press "Create device pairing code" to get the server address and a code (single use, 10 minutes).',
    server: 'Server address',
    code: 'Pairing code',
    name: 'Device name',
    pair: 'Link',
    pairing: 'Linking…',
    paired: 'Linked',
    serverLabel: 'Server',
    nameLabel: 'Device',
    since: 'Linked at',
    unpair: 'Unlink this device',
    unpairHelp: 'Unlinking keeps the note files on this device. Revoke the device record on the web Account screen.',
    revoked: 'This device was revoked or expired on the server. Link it again.',
    deletions: 'Notes deleted outside the app',
    deletionsHelp: 'Deleted with a file manager or another tool. They still exist on the server and other devices.',
    noDeletions: 'None',
    deleteRemote: 'Delete on server too',
    restore: 'Restore',
    working: 'Working…',
    secure: 'Could not save the link: OS secure storage is unavailable (Linux: is a keyring/Secret Service running?)',
    offline: 'Cannot reach the server. Check the address and network.',
    outcome_deleted: 'Deleted on the server and other devices.',
    outcome_gone: 'Already deleted on the server; removed from this list.',
    outcome_changed: 'Changed on another device after you deleted it, so it was kept and downloaded again.',
    outcome_busy: 'Being edited on another device. Try again later.',
    outcome_restored: 'Downloaded again from the server.',
    outcome_missing: 'Not on the server either; removed from this list.',
    outcome_offline: 'Cannot reach the server. Try again when online.',
    outcome_notSynced: 'Sync is off. Link this device first.',
    appVersion: 'App version',
    checkUpdate: 'Check for updates',
  },
};

export type DeletionChoice = 'delete' | 'restore';

export interface DeviceHooks {
  /** 연결 성공 → 동기화 시작 */
  onPaired: (info: DeviceInfo) => void | Promise<void>;
  /** 연결 해제 → 동기화 중지 */
  onUnpaired: () => void | Promise<void>;
  /** 상단과 같은 동기화 상태 문구 */
  statusText: () => string;
  /** 바깥 삭제 처리. 결과 코드(deleted, gone, changed, busy, restored, missing, offline, notSynced)를 돌려줌 */
  resolveDeletion: (item: ExternalDeletion, choice: DeletionChoice) => Promise<string>;
  /** 서버가 이 장비를 거부(401)했는지 */
  revoked: () => boolean;
  /** 앱 버전 표시 (예: "0.1.0 (df68a25)") */
  appVersion?: () => Promise<string>;
  /** 업데이트 확인 → 결과 문구 (새 버전이면 상단 '새 버전 있음' 버튼도 켬) */
  checkUpdate?: () => Promise<string>;
}

function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  props: Partial<HTMLElementTagNameMap[K]> = {},
  ...children: Array<Node | string>
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  Object.assign(node, props);
  node.append(...children);
  return node;
}

/** 이 기기의 OS 이름 (user agent 기준) */
function osName(ua = navigator.userAgent): string {
  return /Android/.test(ua) ? 'Android' : /iPhone|iPad/.test(ua) ? 'iOS' : /Windows/.test(ua) ? 'Windows'
    : /Mac OS/.test(ua) ? 'macOS' : /Linux/.test(ua) ? 'Linux' : '';
}

/** Android 기기 모델 (예: "SM-S918N"). user agent의 "Android 14; <모델>" 부분 */
export function androidModel(ua = navigator.userAgent): string {
  const m = /Android [^;)]*; ([^;)]+?)(?: Build\/[^;)]*)?[;)]/.exec(ua);
  const model = m?.[1]?.trim() ?? '';
  return model && model !== 'K' && !/^wv$/i.test(model) ? model : '';
}

/**
 * 기본 장비 이름: 호스트 이름(또는 Android 모델)을 앞에 둠 (2026-11-13 사용 의견).
 * 예: "DESKTOP-AB12 (Windows)", "SM-S918N (Android)", 둘 다 없으면 "MemoNest · Windows"
 */
export function defaultDeviceName(hostname: string | null = null, ua = navigator.userAgent): string {
  const os = osName(ua);
  const host = hostname?.trim() || (os === 'Android' ? androidModel(ua) : '');
  if (host) return os ? `${host} (${os})` : host;
  return os ? `MemoNest · ${os}` : 'MemoNest';
}

export function createDevicePanel(dialog: HTMLDialogElement, getLocale: () => Locale, hooks: DeviceHooks) {
  const t = (key: string) => TEXT[getLocale()][key] ?? key;
  const message = (text: string, cls = 'muted') =>
    el('p', { className: cls, role: cls === 'error' ? 'alert' : 'status' }, text);

  function frame(...content: Node[]) {
    dialog.replaceChildren(
      el(
        'div',
        { className: 'account-head' },
        el('h2', {}, t('title')),
        el('button', { type: 'button', id: 'account-close', onclick: () => dialog.close() }, t('close')),
      ),
      el('div', { id: 'account-body' }, ...content),
    );
  }

  async function render() {
    const info = await deviceApi.status();
    if (info) await renderPaired(info);
    else await renderForm();
  }

  async function renderForm() {
    const status = message('', 'error');
    const server = el('input', { type: 'url', id: 'device-server', required: true, placeholder: 'https://', autocomplete: 'off' });
    const code = el('input', {
      type: 'text',
      id: 'device-code',
      required: true,
      maxLength: 16,
      autocomplete: 'one-time-code',
      autocapitalize: 'characters',
    });
    const name = el('input', {
      type: 'text',
      id: 'device-name',
      required: true,
      maxLength: 80,
      value: defaultDeviceName(await deviceApi.hostname()),
    });
    const submit = el('button', { type: 'submit', className: 'primary', id: 'device-pair' }, t('pair'));
    const form = el(
      'form',
      { className: 'account-form', id: 'device-form' },
      el('label', {}, t('server'), server),
      el('label', {}, t('code'), code),
      el('label', {}, t('name'), name),
      submit,
      status,
    );
    form.addEventListener('submit', async (event) => {
      event.preventDefault();
      submit.disabled = true;
      submit.textContent = t('pairing');
      status.textContent = '';
      try {
        const info = await deviceApi.pair(server.value, code.value, name.value);
        code.value = '';
        await hooks.onPaired(info);
        await renderPaired(info);
      } catch (e) {
        const err = e instanceof DeviceError ? e : new DeviceError('error', String(e));
        status.textContent =
          err.code === 'offline' ? t('offline') : err.code === 'secure_storage' ? `${t('secure')} — ${err.message}` : err.message;
        submit.disabled = false;
        submit.textContent = t('pair');
      }
    });
    frame(...(hooks.revoked() ? [message(t('revoked'), 'error')] : []), message(t('intro')), form);
    server.focus();
  }

  async function renderPaired(info: DeviceInfo) {
    const unpair = el('button', { type: 'button', id: 'device-unpair' }, t('unpair'));
    unpair.onclick = async () => {
      unpair.disabled = true;
      await hooks.onUnpaired();
      await deviceApi.unpair().catch(console.error);
      await renderForm();
    };
    const deletions = el('ul', { className: 'device-list', id: 'external-deletions' });
    const version = el('dd', { id: 'app-version' }, '…');
    void hooks.appVersion?.().then((v) => (version.textContent = v));
    const updateResult = el('small', { role: 'status', id: 'update-result' });
    const updateBtn = el('button', { type: 'button', id: 'check-update' }, t('checkUpdate'));
    updateBtn.onclick = async () => {
      updateBtn.disabled = true;
      updateResult.textContent = '…';
      updateResult.textContent = (await hooks.checkUpdate?.().catch((e: unknown) => String(e))) ?? '';
      updateBtn.disabled = false;
    };
    frame(
      ...(hooks.revoked() ? [message(t('revoked'), 'error')] : []),
      el(
        'dl',
        { className: 'device-info' },
        el('dt', {}, t('serverLabel')),
        el('dd', { id: 'device-server-url' }, info.serverUrl),
        el('dt', {}, t('nameLabel')),
        el('dd', {}, info.deviceName),
        el('dt', {}, t('since')),
        el('dd', {}, new Date(info.pairedAt).toLocaleString()),
        el('dt', {}, t('appVersion')),
        version,
      ),
      ...(hooks.checkUpdate ? [el('div', { className: 'toolbar' }, updateBtn, updateResult)] : []),
      message(hooks.statusText()),
      el('h3', {}, t('deletions')),
      message(t('deletionsHelp')),
      deletions,
      el('div', { className: 'toolbar' }, unpair),
      message(t('unpairHelp')),
    );
    await renderDeletions(deletions);
  }

  async function renderDeletions(list: HTMLUListElement) {
    const items = await deviceApi.externalDeletions().catch(() => []);
    if (!items.length) {
      list.replaceChildren(el('li', {}, t('noDeletions')));
      return;
    }
    list.replaceChildren(
      ...items.map((item) => {
        const result = el('small', { role: 'status' });
        const choices: DeletionChoice[] = ['delete', 'restore'];
        const buttons = choices.map((choice) => {
          const button = el('button', { type: 'button' }, t(choice === 'delete' ? 'deleteRemote' : 'restore'));
          button.dataset.choice = choice;
          button.onclick = async () => {
            buttons.forEach((b) => (b.disabled = true));
            result.textContent = t('working');
            const outcome = await hooks.resolveDeletion(item, choice).catch((e: unknown) => String(e));
            await renderDeletions(list);
            const text = TEXT[getLocale()][`outcome_${outcome}`] ?? outcome;
            list.prepend(el('li', { role: 'status', className: 'muted', id: 'external-deletion-result' }, text));
          };
          return button;
        });
        return el(
          'li',
          { className: 'external-deletion' },
          el('span', {}, `${item.folder ? item.folder + '/' : ''}${item.title}.${item.format}`),
          ...buttons,
          result,
        );
      }),
    );
  }

  return {
    open: async () => {
      if (!dialog.open) dialog.showModal();
      await render();
    },
    /** 열려 있으면 다시 그림 (바깥 삭제 목록·상태 갱신) */
    refresh: async () => {
      if (dialog.open) await render();
    },
  };
}
