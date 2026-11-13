// === 계정 화면 (로드맵 05): 로그인/가입/로그아웃, 용량, 장비 등록 코드, 장비 목록 ===
// 메모 자체는 계속 이 기기에만 저장됩니다 (동기화는 로드맵 06).
import type { Locale } from '../settings';
import { accountApi, adminApi, ApiError, downloadApi, formatBytes, ServerUnavailable, tokenApi, type Account, type AdminAccount } from './api';

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
    passwordConfirm: '비밀번호 확인 (한 번 더)',
    passwordMismatch: '비밀번호가 서로 다릅니다. 다시 확인하세요.',
    displayName: '이름 (선택)',
    toRegister: '계정이 없나요? 가입',
    toLogin: '이미 계정이 있나요? 로그인',
    closed: '현재 가입을 받지 않습니다.',
    logout: '로그아웃',
    storage: '저장 공간',
    pair: '장비 등록 코드 만들기',
    pairTitle: '장비(데스크톱·모바일 앱) 추가 방법',
    pairStep1: '아래 "장비 등록 코드 만들기"를 누릅니다.',
    pairStep2: '앱에서 "계정 → 장비 연결"을 엽니다.',
    pairStep3: '화면에 나온 서버 주소와 코드(10분, 한 번만)를 입력하고 "연결"을 누릅니다.',
    forget: '기록 지우기',
    copy: '복사',
    copied: '복사됨',
    pairCode: '등록 코드',
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
    tokenNew: '새 토큰 만들기 (눌러서 입력)',
    admin: '시스템 (관리자)',
    adminDefault: '계정당 기본 저장공간 (MB)',
    adminSave: '저장',
    adminApplied: '저장했습니다. 따로 정하지 않은 계정 {n}개에 바로 적용했습니다.',
    adminAccounts: '계정별 저장공간',
    adminCustom: '따로 정함',
    adminApply: '적용',
    adminReset: '기본값으로',
    adminHelp: '계정별로 따로 정한 용량은 기본값을 바꿔도 그대로입니다. 사용량보다 작게 정하면 그 계정은 메모를 더 늘릴 수 없습니다.',
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
    passwordConfirm: 'Confirm password',
    passwordMismatch: 'Passwords do not match.',
    displayName: 'Name (optional)',
    toRegister: 'No account? Sign up',
    toLogin: 'Have an account? Log in',
    closed: 'Sign-up is closed.',
    logout: 'Log out',
    storage: 'Storage',
    pair: 'Create device pairing code',
    pairTitle: 'How to add a device (desktop/mobile app)',
    pairStep1: 'Press "Create device pairing code" below.',
    pairStep2: 'In the app, open "Account → Device link".',
    pairStep3: 'Enter the server address and code shown here (10 minutes, single use) and press "Link".',
    forget: 'Remove',
    copy: 'Copy',
    copied: 'Copied',
    pairCode: 'Pairing code',
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
    tokenNew: 'New token (click to fill in)',
    admin: 'System (admin)',
    adminDefault: 'Default storage per account (MB)',
    adminSave: 'Save',
    adminApplied: 'Saved. Applied right away to {n} accounts without their own limit.',
    adminAccounts: 'Storage per account',
    adminCustom: 'custom',
    adminApply: 'Apply',
    adminReset: 'Use default',
    adminHelp: 'Accounts with their own limit keep it when the default changes. A limit below current usage blocks that account from growing.',
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
    const confirm = el('input', {
      type: 'password',
      name: 'passwordConfirm',
      required: true,
      minLength: 10,
      autocomplete: 'new-password',
    });
    // 가입 시 비밀번호 두 번 입력: 다르면 제출 막기 (오타로 모르는 비밀번호가 되는 것 방지)
    const checkConfirm = () =>
      confirm.setCustomValidity(confirm.value && confirm.value !== password.value ? t('passwordMismatch') : '');
    confirm.addEventListener('input', checkConfirm);
    password.addEventListener('input', checkConfirm);
    const name = el('input', { type: 'text', name: 'displayName', maxLength: 80, autocomplete: 'name' });
    const submit = el('button', { type: 'submit', className: 'primary' }, t(mode));
    const form = el(
      'form',
      { className: 'account-form', id: `account-${mode}-form` },
      el('label', {}, t('email'), email),
      el('label', {}, t('password'), password),
      ...(mode === 'register'
        ? [el('label', {}, t('passwordConfirm'), confirm), el('label', {}, t('displayName'), name)]
        : []),
      submit,
      status,
    );
    form.addEventListener('submit', async (event) => {
      event.preventDefault();
      if (mode === 'register' && confirm.value !== password.value) {
        status.textContent = t('passwordMismatch');
        confirm.focus();
        return;
      }
      submit.disabled = true;
      status.textContent = '';
      try {
        const account =
          mode === 'register'
            ? await accountApi.register(email.value, password.value, name.value)
            : await accountApi.login(email.value, password.value);
        password.value = '';
        confirm.value = '';
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
    const serverBox = el('div', { id: 'pairing-server', className: 'pairing-server' });
    /** 크게 보이는 값 + 복사 버튼 (2026-11-13 사용 의견: 서버 주소가 너무 작게 보임) */
    const copyable = (label: string, value: string, id: string) => {
      const copy = el('button', { type: 'button', className: 'copy' }, t('copy'));
      copy.onclick = async () => {
        try {
          await navigator.clipboard.writeText(value);
          copy.textContent = t('copied');
        } catch {
          /* 복사 권한 없음: 직접 선택 */
        }
      };
      return el('div', { className: 'pairing-value' }, el('small', {}, label), el('code', { id }, value), copy);
    };
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
        serverBox.replaceChildren(
          copyable(t('serverAddress'), location.origin, 'pairing-server-url'),
          copyable(t('pairCode'), p.code, 'pairing-code-value'),
        );
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
      el(
        'div',
        { className: 'callout', id: 'pairing-howto' },
        el('strong', {}, t('pairTitle')),
        el('ol', {}, el('li', {}, t('pairStep1')), el('li', {}, t('pairStep2')), el('li', {}, t('pairStep3'))),
      ),
      el('div', { className: 'toolbar' }, pair, logout),
      codeBox,
      serverBox,
      el('h3', {}, t('devices')),
      deviceList,
      ...downloadSection(),
      ...tokenSection(),
      ...(account.isAdmin ? adminSection() : []),
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
      // 눌렀을 때만 입력란 표시 (평소에는 발급된 토큰 목록만)
      el(
        'details',
        { className: 'token-new', id: 'token-new' },
        el('summary', {}, t('tokenNew')),
        el('div', { className: 'account-form' }, name, el('label', { className: 'inline' }, write, t('tokenWrite')),
          el('label', { className: 'inline' }, t('tokenTtl'), ttl), button, status),
      ),
      created,
      createdHelp,
      list,
    ];
  }

  /** 관리자: 기본 저장공간과 계정별 저장공간 (ADMIN_EMAILS에 있는 계정만 보임, 서버도 다시 확인) */
  function adminSection(): Node[] {
    const status = el('p', { className: 'muted', role: 'status', id: 'admin-status' });
    const input = el('input', { type: 'number', id: 'admin-default-quota', min: '1', max: '1048576', step: '1', required: true });
    const save = el('button', { type: 'submit', className: 'primary', id: 'admin-default-save' }, t('adminSave'));
    const list = el('ul', { className: 'device-list', id: 'admin-accounts' });
    const form = el('form', { className: 'account-form', id: 'admin-default-form' }, el('label', {}, t('adminDefault'), input), save, status);
    const fail = (e: unknown) => (status.textContent = e instanceof ApiError ? e.message : t('unavailable'));
    form.addEventListener('submit', async (event) => {
      event.preventDefault();
      save.disabled = true;
      try {
        const r = await adminApi.setDefaultQuota(Number(input.value));
        status.textContent = t('adminApplied').replace('{n}', String(r.updatedAccounts));
        await renderAdminAccounts(list, status);
      } catch (e) {
        fail(e);
      } finally {
        save.disabled = false;
      }
    });
    void (async () => {
      try {
        input.value = String((await adminApi.settings()).defaultQuotaMb);
        await renderAdminAccounts(list, status);
      } catch (e) {
        fail(e);
      }
    })();
    return [el('h3', {}, t('admin')), form, el('h3', {}, t('adminAccounts')), message(t('adminHelp')), list];
  }

  async function renderAdminAccounts(list: HTMLUListElement, status: HTMLElement) {
    const { accounts } = await adminApi.accounts();
    const row = (a: AdminAccount) => {
      const quota = el('input', {
        type: 'number',
        min: '1',
        max: '1048576',
        step: '1',
        value: String(Math.round(a.quotaBytes / (1024 * 1024))),
        className: 'admin-quota',
      });
      quota.setAttribute('aria-label', `${a.email ?? a.id} MB`);
      const apply = el('button', { type: 'button' }, t('adminApply'));
      const reset = el('button', { type: 'button', disabled: !a.quotaCustom }, t('adminReset'));
      const update = async (value: number | null) => {
        try {
          await adminApi.setQuota(a.id, value);
          await renderAdminAccounts(list, status);
        } catch (e) {
          status.textContent = e instanceof ApiError ? e.message : t('unavailable');
        }
      };
      apply.onclick = () => void update(Number(quota.value));
      reset.onclick = () => void update(null);
      const usage = `${formatBytes(a.usedBytes)} / ${formatBytes(a.quotaBytes)}${a.quotaCustom ? ` · ${t('adminCustom')}` : ''}`;
      return el(
        'li',
        { className: 'admin-account', dataset: { accountId: a.id } },
        el('span', {}, el('strong', {}, a.email ?? a.displayName), el('br'), el('small', {}, usage)),
        el('span', { className: 'admin-actions' }, quota, apply, reset),
      );
    };
    list.replaceChildren(...accounts.map(row));
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
        // 폐기 전: '폐기', 폐기 후: '기록 지우기' (목록에서 없앰)
        const action = el('button', { type: 'button', className: d.revokedAt ? 'forget' : '' }, d.revokedAt ? t('forget') : t('revoke'));
        action.onclick = async () => {
          action.disabled = true;
          await (d.revokedAt ? accountApi.forgetDevice(d.id) : accountApi.revokeDevice(d.id)).catch(() => undefined);
          await renderDevices(list);
        };
        return el(
          'li',
          { dataset: { deviceId: d.id } },
          el('span', {}, `${d.name} (${d.platform})`, ...(d.revokedAt ? [el('small', { className: 'muted' }, ` · ${t('revoked')}`)] : [])),
          action,
        );
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
