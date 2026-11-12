// === 계정 화면 (로드맵 05): 로그인/가입/로그아웃, 용량, 장비 등록 코드, 장비 목록 ===
// 메모 자체는 계속 이 기기에만 저장됩니다 (동기화는 로드맵 06).
import type { Locale } from '../settings';
import { accountApi, ApiError, downloadApi, formatBytes, ServerUnavailable, tokenApi, type Account } from './api';

// === 0. 문구 (ko/en) ===
const TEXT: Record<Locale, Record<string, string>> = {
  ko: {
    title: '계정',
    close: '닫기',
    loading: '확인 중…',
    unavailable: '서버에 연결되어 있지 않습니다. 메모는 이 기기에서 계속 쓸 수 있습니다.',
    note: '로그인하면 이 브라우저의 메모가 계정과 동기화됩니다. 데스크톱/모바일 앱은 아래 등록 코드로 연결하세요.',
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
    pairHelp: '데스크톱/모바일 앱의 "계정 → 장비 연결"에 서버 주소와 코드를 10분 안에 입력하세요.',
    serverAddress: '서버 주소',
    devices: '등록된 장비',
    noDevices: '등록된 장비가 없습니다.',
    revoke: '해제',
    revoked: '해제됨',
    googleSoon: 'Google 로그인은 준비 중입니다.',
    downloads: '설치 파일',
    downloadsNone: '아직 준비된 설치 파일이 없습니다.',
    download: '받기',
    downloadsHelp: '서명 전 버전이라 처음 실행할 때 보안 경고가 나올 수 있습니다.',
    tokens: '자동화 토큰 (n8n)',
    tokenName: '이름 (예: n8n 업무 자동화)',
    tokenWrite: '쓰기 허용 (메모 수정)',
    tokenTtl: '만료',
    tokenCreate: '토큰 만들기',
    tokenOnce: '지금만 보입니다. n8n의 Header Auth 자격 증명(Authorization: Bearer …)에 저장하세요.',
    tokenNone: '발급한 토큰이 없습니다.',
    days: '일',
    expired: '만료됨',
  },
  en: {
    title: 'Account',
    close: 'Close',
    loading: 'Checking…',
    unavailable: 'Not connected to a server. Notes keep working on this device.',
    note: 'While signed in, notes in this browser sync with your account. Link desktop/mobile apps with a pairing code below.',
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
    pairHelp: 'Enter the server address and code in the desktop/mobile app (Account → Device link) within 10 minutes.',
    serverAddress: 'Server address',
    devices: 'Devices',
    noDevices: 'No devices yet.',
    revoke: 'Revoke',
    revoked: 'revoked',
    googleSoon: 'Google sign-in is coming.',
    downloads: 'Installers',
    downloadsNone: 'No installers available yet.',
    download: 'Download',
    downloadsHelp: 'Unsigned builds: your OS may show a security warning on first launch.',
    tokens: 'Automation tokens (n8n)',
    tokenName: 'Name (e.g. n8n workflow)',
    tokenWrite: 'Allow writes (edit notes)',
    tokenTtl: 'Expires in',
    tokenCreate: 'Create token',
    tokenOnce: 'Shown only now. Save it in n8n as a Header Auth credential (Authorization: Bearer …).',
    tokenNone: 'No tokens yet.',
    days: 'days',
    expired: 'expired',
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
    const serverBox = el('p', { id: 'pairing-server', className: 'muted' });
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
        serverBox.textContent = `${t('serverAddress')}: ${location.origin}`;
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
      serverBox,
      message(t('pairHelp')),
      el('h3', {}, t('devices')),
      deviceList,
      ...downloadSection(),
      ...tokenSection(),
    );
    await renderDevices(deviceList);
  }

  /** 설치 파일: 누를 때 일회성 주소를 받아 바로 내려받음 (주소는 화면에 표시하지 않음) */
  function downloadSection(): Node[] {
    const list = el('ul', { className: 'device-list', id: 'download-list' });
    void (async () => {
      const result = await downloadApi.list().catch(() => null);
      if (!result || !result.files.length) {
        list.replaceChildren(el('li', { className: 'muted' }, t('downloadsNone')));
        return;
      }
      list.replaceChildren(
        ...result.files.map((f) => {
          const get = el('button', { type: 'button', dataset: { downloadId: f.id } }, t('download'));
          get.onclick = async () => {
            get.disabled = true;
            try {
              const link = await downloadApi.link(f.id);
              const a = el('a', { href: link.url, download: f.fileName });
              document.body.append(a);
              a.click();
              a.remove();
            } finally {
              get.disabled = false;
            }
          };
          return el('li', {}, `${f.platform} · ${f.label} (${formatBytes(f.size)}) `, get);
        }),
      );
    })();
    return [el('h3', {}, t('downloads')), list, message(t('downloadsHelp'))];
  }

  /** 자동화 토큰(n8n): 만들기(원문 1회 표시)·목록·해제 */
  function tokenSection(): Node[] {
    const list = el('ul', { className: 'device-list', id: 'token-list' });
    const name = el('input', { type: 'text', id: 'token-name', maxLength: 60, placeholder: t('tokenName') });
    const write = el('input', { type: 'checkbox', id: 'token-write' });
    const ttl = el('select', { id: 'token-ttl' });
    for (const d of [30, 90, 365]) ttl.append(el('option', { value: String(d), selected: d === 90 }, `${d} ${t('days')}`));
    const created = el('pre', { id: 'token-created', className: 'pairing-code', hidden: true });
    const createdHelp = message(t('tokenOnce'));
    createdHelp.hidden = true;
    const status = message('', 'error');
    const button = el('button', { type: 'button', id: 'token-create' }, t('tokenCreate'));
    button.onclick = async () => {
      status.textContent = '';
      try {
        const scopes = write.checked ? ['notes:read', 'notes:write'] : ['notes:read'];
        const result = await tokenApi.create(name.value || 'n8n', scopes, Number(ttl.value));
        created.textContent = result.token;
        created.hidden = false;
        createdHelp.hidden = false;
        name.value = '';
        await renderTokens(list);
      } catch (e) {
        status.textContent = e instanceof ApiError ? e.message : t('unavailable');
      }
    };
    void renderTokens(list);
    return [
      el('h3', {}, t('tokens')),
      el('div', { className: 'account-form' }, name, el('label', { className: 'inline' }, write, t('tokenWrite')),
        el('label', { className: 'inline' }, t('tokenTtl'), ttl), button, status),
      created,
      createdHelp,
      list,
    ];
  }

  async function renderTokens(list: HTMLUListElement) {
    const tokens = await tokenApi.list().catch(() => []);
    const live = tokens.filter((x) => !x.revokedAt);
    if (!live.length) {
      list.replaceChildren(el('li', { className: 'muted' }, t('tokenNone')));
      return;
    }
    list.replaceChildren(
      ...live.map((x) => {
        const expired = Date.parse(x.expiresAt) < Date.now();
        const revoke = el('button', { type: 'button' }, t('revoke'));
        revoke.onclick = async () => {
          await tokenApi.revoke(x.id).catch(() => undefined);
          await renderTokens(list);
        };
        const scope = x.scopes.includes('notes:write') ? 'read+write' : 'read';
        const until = expired ? t('expired') : `~${new Date(x.expiresAt).toLocaleDateString()}`;
        return el('li', { dataset: { tokenId: x.id } }, `${x.name} (${scope}, ${until}) `, revoke);
      }),
    );
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
