// === 계정 화면 (로드맵 05): 로그인/가입/로그아웃, 용량, 장비 등록 코드, 장비 목록 ===
// 메모 자체는 계속 이 기기에만 저장됩니다 (동기화는 로드맵 06).
import type { Locale } from '../settings';
import { accountApi, ApiError, formatBytes, ServerUnavailable, type Account } from './api';

// === 0. 문구 (ko/en) ===
const TEXT: Record<Locale, Record<string, string>> = {
  ko: {
    title: '계정',
    close: '닫기',
    loading: '확인 중…',
    unavailable: '서버에 연결되어 있지 않습니다. 메모는 이 기기에서 계속 쓸 수 있습니다.',
    note: '계정이 있어도 메모는 아직 이 기기에만 저장됩니다 (동기화는 준비 중).',
    login: '로그인',
    register: '가입',
    email: '이메일',
    password: '비밀번호 (10자 이상)',
    displayName: '이름 (선택)',
    toRegister: '계정이 없나요? 가입',
    toLogin: '이미 계정이 있나요? 로그인',
    closed: '현재 가입을 받지 않습니다.',
    logout: '로그아웃',
    storage: '저장 공간',
    pair: '장비 등록 코드 만들기',
    pairHelp: '데스크톱/모바일 앱에서 10분 안에 입력하세요 (앱 연결은 준비 중).',
    devices: '등록된 장비',
    noDevices: '등록된 장비가 없습니다.',
    revoke: '해제',
    revoked: '해제됨',
    googleSoon: 'Google 로그인은 준비 중입니다.',
  },
  en: {
    title: 'Account',
    close: 'Close',
    loading: 'Checking…',
    unavailable: 'Not connected to a server. Notes keep working on this device.',
    note: 'Notes are still stored only on this device (sync is coming).',
    login: 'Log in',
    register: 'Sign up',
    email: 'Email',
    password: 'Password (10+ characters)',
    displayName: 'Name (optional)',
    toRegister: 'No account? Sign up',
    toLogin: 'Have an account? Log in',
    closed: 'Sign-up is closed.',
    logout: 'Log out',
    storage: 'Storage',
    pair: 'Create device pairing code',
    pairHelp: 'Enter it in the desktop/mobile app within 10 minutes (app linking coming).',
    devices: 'Devices',
    noDevices: 'No devices yet.',
    revoke: 'Revoke',
    revoked: 'revoked',
    googleSoon: 'Google sign-in is coming.',
  },
};

type Mode = 'login' | 'register';

function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  props: Partial<HTMLElementTagNameMap[K]> & { dataset?: Record<string, string> } = {},
  ...children: Array<Node | string>
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  const { dataset, ...rest } = props;
  Object.assign(node, rest);
  if (dataset) Object.assign(node.dataset, dataset);
  node.append(...children);
  return node;
}

export interface AccountHooks {
  /** 로그인 상태가 확인될 때마다 (가입·로그인·화면 열기) — 동기화 시작 */
  onSignedIn?: (account: Account) => void;
  /** 로그아웃 직후 — 동기화 중지 */
  onSignedOut?: () => void;
}

export function createAccountPanel(dialog: HTMLDialogElement, getLocale: () => Locale, hooks: AccountHooks = {}) {
  let mode: Mode = 'login';
  const t = (key: string) => TEXT[getLocale()][key] ?? key;
  const body = () => dialog.querySelector<HTMLElement>('#account-body')!;

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

  function message(text: string, cls = 'muted') {
    return el('p', { className: cls, role: cls === 'error' ? 'alert' : 'status' }, text);
  }

  async function render() {
    frame(message(t('loading')));
    try {
      const [providers, account] = await Promise.all([accountApi.providers(), accountApi.me()]);
      if (!providers.local) throw new ServerUnavailable('local auth disabled');
      if (account) await renderAccount(account);
      else renderForm(providers.registration === 'open');
    } catch (e) {
      body().replaceChildren(message(e instanceof ServerUnavailable ? t('unavailable') : String(e), 'error'));
    }
  }

  function renderForm(registrationOpen: boolean) {
    if (!registrationOpen) mode = 'login';
    const status = message('', 'error');
    const email = el('input', { type: 'email', name: 'email', required: true, autocomplete: 'email' });
    const password = el('input', {
      type: 'password',
      name: 'password',
      required: true,
      minLength: mode === 'register' ? 10 : 1,
      autocomplete: mode === 'register' ? 'new-password' : 'current-password',
    });
    const name = el('input', { type: 'text', name: 'displayName', maxLength: 80, autocomplete: 'name' });
    const submit = el('button', { type: 'submit', className: 'primary' }, t(mode));
    const form = el(
      'form',
      { className: 'account-form', id: `account-${mode}-form` },
      el('label', {}, t('email'), email),
      el('label', {}, t('password'), password),
      ...(mode === 'register' ? [el('label', {}, t('displayName'), name)] : []),
      submit,
      status,
    );
    form.addEventListener('submit', async (event) => {
      event.preventDefault();
      submit.disabled = true;
      status.textContent = '';
      try {
        const account =
          mode === 'register'
            ? await accountApi.register(email.value, password.value, name.value)
            : await accountApi.login(email.value, password.value);
        password.value = '';
        await renderAccount(account);
      } catch (e) {
        status.textContent = e instanceof ApiError ? e.message : t('unavailable');
        submit.disabled = false;
      }
    });
    const switcher = registrationOpen
      ? el(
          'button',
          {
            type: 'button',
            className: 'link',
            id: 'account-switch',
            onclick: () => {
              mode = mode === 'login' ? 'register' : 'login';
              renderForm(registrationOpen);
            },
          },
          t(mode === 'login' ? 'toRegister' : 'toLogin'),
        )
      : message(t('closed'));
    body().replaceChildren(form, switcher, message(t('googleSoon')), message(t('note')));
    email.focus();
  }

  async function renderAccount(account: Account) {
    hooks.onSignedIn?.(account);
    const percent = account.quotaBytes > 0 ? Math.min(100, (account.usedBytes / account.quotaBytes) * 100) : 0;
    const meter = el('meter', { min: 0, max: account.quotaBytes, value: account.usedBytes });
    const codeBox = el('p', { id: 'pairing-code', className: 'pairing-code' });
    const deviceList = el('ul', { className: 'device-list', id: 'device-list' });
    const logout = el('button', { type: 'button', id: 'account-logout' }, t('logout'));
    logout.onclick = async () => {
      hooks.onSignedOut?.();
      await accountApi.logout().catch(() => undefined);
      mode = 'login';
      await render();
    };
    const pair = el('button', { type: 'button', id: 'pairing-start' }, t('pair'));
    pair.onclick = async () => {
      try {
        const p = await accountApi.pairingStart();
        codeBox.textContent = `${p.code} · ~${new Date(p.expiresAt).toLocaleTimeString()}`;
      } catch (e) {
        codeBox.textContent = e instanceof ApiError ? e.message : t('unavailable');
      }
    };
    body().replaceChildren(
      el('p', { className: 'account-who', id: 'account-email' }, account.displayName ? `${account.displayName} · ` : '', account.email ?? ''),
      el(
        'div',
        { className: 'quota' },
        el('span', {}, `${t('storage')} (${account.tier})`),
        meter,
        el('span', { id: 'account-quota' }, `${formatBytes(account.usedBytes)} / ${formatBytes(account.quotaBytes)} (${percent.toFixed(0)}%)`),
      ),
      message(t('note')),
      el('div', { className: 'toolbar' }, pair, logout),
      codeBox,
      message(t('pairHelp')),
      el('h3', {}, t('devices')),
      deviceList,
    );
    await renderDevices(deviceList);
  }

  async function renderDevices(list: HTMLUListElement) {
    const devices = await accountApi.devices().catch(() => []);
    if (!devices.length) {
      list.replaceChildren(el('li', { className: 'muted' }, t('noDevices')));
      return;
    }
    list.replaceChildren(
      ...devices.map((d) => {
        const revoke = el('button', { type: 'button', disabled: !!d.revokedAt }, d.revokedAt ? t('revoked') : t('revoke'));
        revoke.onclick = async () => {
          await accountApi.revokeDevice(d.id).catch(() => undefined);
          await renderDevices(list);
        };
        return el('li', {}, `${d.name} (${d.platform}) `, revoke);
      }),
    );
  }

  return {
    async open() {
      if (!dialog.open) dialog.showModal();
      await render();
    },
  };
}
