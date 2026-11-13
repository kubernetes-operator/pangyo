// === 이름 입력 창 (새 메모 제목, 새 폴더) — 2026-11-13 사용 의견 ===
// 브라우저 기본 prompt() 대신 한 줄 입력 창을 씁니다.
// - Enter = 확인 (모바일 앱의 기본 입력 창은 Enter가 줄 바꿈으로 들어가는 문제가 있었음)
// - 한글 조합 중 Enter는 글자 확정만 하고 제출하지 않음
// - 잘못된 값이면 창을 닫지 않고 바로 아래에 이유를 보여 줌 (alert 없음)

export interface NameDialogOptions {
  label: string;
  initial?: string;
  hint?: string;
  ok: string;
  cancel: string;
  /** false면 전체 선택 대신 커서를 끝에 둠 (예: 'Work/' 뒤에 이어 쓰기) */
  selectAll?: boolean;
  /** 오류 문구를 돌려주면 제출을 막음 */
  validate?: (value: string) => string | null;
}

export function askName(opts: NameDialogOptions): Promise<string | null> {
  const dialog = document.createElement('dialog');
  dialog.className = 'name-dialog';
  dialog.id = 'name-dialog';
  const form = document.createElement('form');
  const label = document.createElement('label');
  label.textContent = opts.label;
  const input = document.createElement('input');
  input.type = 'text';
  input.id = 'name-input';
  input.autocomplete = 'off';
  input.enterKeyHint = 'done';
  input.value = opts.initial ?? '';
  label.append(input);
  const hint = document.createElement('p');
  hint.className = 'hint';
  hint.textContent = opts.hint ?? '';
  hint.hidden = !opts.hint;
  const error = document.createElement('p');
  error.className = 'error';
  error.id = 'name-error';
  error.setAttribute('role', 'alert');
  const ok = document.createElement('button');
  ok.type = 'submit';
  ok.className = 'primary';
  ok.id = 'name-ok';
  ok.textContent = opts.ok;
  const cancel = document.createElement('button');
  cancel.type = 'button';
  cancel.id = 'name-cancel';
  cancel.textContent = opts.cancel;
  const bar = document.createElement('div');
  bar.className = 'toolbar';
  bar.append(cancel, ok);
  form.append(label, hint, error, bar);
  dialog.append(form);
  document.body.append(dialog);

  return new Promise((resolve) => {
    let result: string | null = null;
    const finish = (value: string | null) => {
      result = value;
      dialog.close();
    };
    dialog.addEventListener('close', () => {
      dialog.remove();
      resolve(result);
    });
    input.addEventListener('keydown', (e) => {
      // 한글 등 조합 중 Enter는 글자 확정용 → 제출하지 않음
      if (e.key === 'Enter' && e.isComposing) e.preventDefault();
    });
    input.addEventListener('input', () => (error.textContent = ''));
    cancel.addEventListener('click', () => finish(null));
    form.addEventListener('submit', (e) => {
      e.preventDefault();
      const value = input.value.trim();
      if (!value) {
        input.focus();
        return;
      }
      const problem = opts.validate?.(value) ?? null;
      if (problem) {
        error.textContent = problem;
        input.focus();
        return;
      }
      finish(value);
    });
    dialog.showModal();
    input.focus();
    if (opts.selectAll === false) input.setSelectionRange(input.value.length, input.value.length);
    else input.select();
  });
}

/** 확인 창 (브라우저 confirm()은 앱 웹뷰에서 동작하지 않을 수 있어 직접 그림). 확인 = true */
export function askConfirm(opts: { message: string; ok: string; cancel: string; danger?: boolean }): Promise<boolean> {
  const dialog = document.createElement('dialog');
  dialog.className = 'name-dialog';
  dialog.id = 'confirm-dialog';
  const form = document.createElement('form');
  const text = document.createElement('p');
  text.textContent = opts.message;
  const ok = document.createElement('button');
  ok.type = 'submit';
  ok.className = opts.danger ? 'primary danger' : 'primary';
  ok.id = 'confirm-ok';
  ok.textContent = opts.ok;
  const cancel = document.createElement('button');
  cancel.type = 'button';
  cancel.id = 'confirm-cancel';
  cancel.textContent = opts.cancel;
  const bar = document.createElement('div');
  bar.className = 'toolbar';
  bar.append(cancel, ok);
  form.append(text, bar);
  dialog.append(form);
  document.body.append(dialog);
  return new Promise((resolve) => {
    let result = false;
    dialog.addEventListener('close', () => {
      dialog.remove();
      resolve(result);
    });
    cancel.addEventListener('click', () => dialog.close());
    form.addEventListener('submit', (e) => {
      e.preventDefault();
      result = true;
      dialog.close();
    });
    dialog.showModal();
    cancel.focus(); // 실수로 Enter를 눌러도 지우지 않게
  });
}

export interface MoveTarget {
  project: string;
  folder: string;
}

/** 이동 대상 고르기 (프로젝트 + 그 프로젝트의 폴더). 폴더 목록은 프로젝트를 바꿀 때마다 다시 계산 */
export function askMove(opts: {
  title: string;
  projectLabel: string;
  folderLabel: string;
  projects: Array<{ id: string; label: string }>;
  folders: (project: string) => Array<{ path: string; label: string; disabled?: boolean }>;
  initial: MoveTarget;
  ok: string;
  cancel: string;
}): Promise<MoveTarget | null> {
  const dialog = document.createElement('dialog');
  dialog.className = 'name-dialog';
  dialog.id = 'move-dialog';
  const form = document.createElement('form');
  const heading = document.createElement('p');
  heading.textContent = opts.title;
  const label = (text: string, control: HTMLElement) => {
    const l = document.createElement('label');
    l.append(text, control);
    return l;
  };
  const project = document.createElement('select');
  project.id = 'move-project';
  for (const p of opts.projects) project.append(new Option(p.label, p.id, false, p.id === opts.initial.project));
  const folder = document.createElement('select');
  folder.id = 'move-folder';
  const fillFolders = (preferred: string) => {
    folder.replaceChildren(
      ...opts.folders(project.value).map((f) => {
        const o = new Option(f.label, f.path, false, f.path === preferred);
        o.disabled = !!f.disabled;
        return o;
      }),
    );
    if (folder.selectedOptions[0]?.disabled) {
      const first = [...folder.options].find((o) => !o.disabled);
      if (first) first.selected = true;
    }
  };
  fillFolders(opts.initial.folder);
  project.addEventListener('change', () => fillFolders(''));
  const ok = document.createElement('button');
  ok.type = 'submit';
  ok.className = 'primary';
  ok.id = 'move-ok';
  ok.textContent = opts.ok;
  const cancel = document.createElement('button');
  cancel.type = 'button';
  cancel.id = 'move-cancel';
  cancel.textContent = opts.cancel;
  const bar = document.createElement('div');
  bar.className = 'toolbar';
  bar.append(cancel, ok);
  form.append(heading, label(opts.projectLabel, project), label(opts.folderLabel, folder), bar);
  dialog.append(form);
  document.body.append(dialog);
  return new Promise((resolve) => {
    let result: MoveTarget | null = null;
    dialog.addEventListener('close', () => {
      dialog.remove();
      resolve(result);
    });
    cancel.addEventListener('click', () => dialog.close());
    form.addEventListener('submit', (e) => {
      e.preventDefault();
      if (!folder.value && folder.selectedOptions[0]?.disabled) return;
      result = { project: project.value, folder: folder.value };
      dialog.close();
    });
    dialog.showModal();
    folder.focus();
  });
}
