/**
 * アカウント追加ウィザード（IMAP のみ。Gmail / M365 は「近日対応」）。
 *
 * パスワードの扱い（厳守）:
 * パスワードはこのコンポーネントの useState にだけ置く。zustand の store・
 * localStorage・sessionStorage・URL には絶対に入れない。console.log にも出さず、
 * エラーメッセージにも含めない。モーダルを閉じるとき（完了・キャンセル・Esc の
 * いずれでも）は必ず空文字列に戻す。
 */
import { useEffect, useState, type ReactNode } from 'react';
import { addAccount, listAccounts, setAccountPassword, testConnection } from '../../api';
import type { NewAccountInput } from '../../types.api';
import { guessImapSettings, validateServerStep } from '../../lib/accountValidation';
import { Badge } from '../ui/Badge';
import { Modal } from '../ui/Modal';

type Step = 1 | 2 | 3;
type TestState = 'idle' | 'testing' | 'ok' | 'error';

const TITLES: Record<Step, string> = {
  1: '種別を選ぶ',
  2: 'サーバー設定',
  3: '案件タグ',
};

const inputClass =
  'w-full rounded-sm border border-line-input bg-elevated px-[10px] py-[6px] text-base text-primary outline-none placeholder:text-placeholder';

function errorMessage(e: unknown): string {
  if (
    e &&
    typeof e === 'object' &&
    'message' in e &&
    typeof (e as { message: unknown }).message === 'string'
  ) {
    return (e as { message: string }).message;
  }
  return '不明なエラーが発生しました';
}

function Field({
  label,
  id,
  error,
  children,
}: {
  label: string;
  id: string;
  error?: string;
  children: ReactNode;
}) {
  return (
    <div className="flex flex-col gap-[4px]">
      <label htmlFor={id} className="text-11 text-muted">
        {label}
      </label>
      {children}
      {error && (
        <span id={`${id}-error`} className="text-11 text-danger">
          {error}
        </span>
      )}
    </div>
  );
}

export function AccountWizard({
  open,
  onClose,
  onAdded,
}: {
  open: boolean;
  onClose: () => void;
  onAdded: (accountId: number) => void;
}) {
  const [step, setStep] = useState<Step>(1);

  const [name, setName] = useState('');
  const [email, setEmail] = useState('');
  const [username, setUsername] = useState('');
  const [usernameTouched, setUsernameTouched] = useState(false);
  const [host, setHost] = useState('');
  const [hostTouched, setHostTouched] = useState(false);
  const [port, setPort] = useState('');
  const [starttls, setStarttls] = useState(false);
  // パスワードはここだけに置く（ファイル先頭のコメント参照）。store / storage / URL には入れない
  const [password, setPassword] = useState('');
  const [touched, setTouched] = useState<Record<string, boolean>>({});

  const [testState, setTestState] = useState<TestState>('idle');
  const [testFolderCount, setTestFolderCount] = useState<number | null>(null);
  const [testError, setTestError] = useState<string | null>(null);

  const [existingTags, setExistingTags] = useState<string[]>([]);
  const [selectedTag, setSelectedTag] = useState<string | null>(null);
  const [addingNewTag, setAddingNewTag] = useState(false);
  const [newTagInput, setNewTagInput] = useState('');

  const [submitting, setSubmitting] = useState(false);
  const [submitError, setSubmitError] = useState<string | null>(null);

  useEffect(() => {
    if (step !== 3) return;
    let cancelled = false;
    void listAccounts().then((accounts) => {
      if (cancelled) return;
      const tags = Array.from(
        new Set(accounts.map((a) => a.project_tag).filter((t): t is string => !!t)),
      );
      setExistingTags(tags);
    });
    return () => {
      cancelled = true;
    };
  }, [step]);

  function resetTest() {
    setTestState('idle');
    setTestFolderCount(null);
    setTestError(null);
  }

  function resetAll() {
    setStep(1);
    setName('');
    setEmail('');
    setUsername('');
    setUsernameTouched(false);
    setHost('');
    setHostTouched(false);
    setPort('');
    setStarttls(false);
    setPassword('');
    setTouched({});
    resetTest();
    setExistingTags([]);
    setSelectedTag(null);
    setAddingNewTag(false);
    setNewTagInput('');
    setSubmitting(false);
    setSubmitError(null);
  }

  function handleClose() {
    resetAll();
    onClose();
  }

  function handleNameChange(v: string) {
    setName(v);
    setTouched((t) => ({ ...t, name: true }));
    resetTest();
  }

  function handleEmailChange(v: string) {
    setEmail(v);
    setTouched((t) => ({ ...t, email: true }));
    if (!usernameTouched) setUsername(v);
    if (!hostTouched) {
      const guess = guessImapSettings(v);
      if (guess) {
        setHost(guess.host);
        setPort(String(guess.port));
        setStarttls(guess.starttls);
      }
    }
    resetTest();
  }

  function handleUsernameChange(v: string) {
    setUsername(v);
    setUsernameTouched(true);
    setTouched((t) => ({ ...t, username: true }));
    resetTest();
  }

  function handleHostChange(v: string) {
    setHost(v);
    setHostTouched(true);
    setTouched((t) => ({ ...t, host: true }));
    resetTest();
  }

  function handlePortChange(v: string) {
    setPort(v);
    setTouched((t) => ({ ...t, port: true }));
    resetTest();
  }

  function handleStarttlsChange(v: boolean) {
    setStarttls(v);
    resetTest();
  }

  function handlePasswordChange(v: string) {
    setPassword(v);
    resetTest();
  }

  const stepErrors = validateServerStep({ name, email, username, host, port });
  const hasStepErrors = Object.keys(stepErrors).length > 0;
  const canTest = !hasStepErrors && password.trim() !== '' && testState !== 'testing';

  function buildInput(projectTag: string | null = null): NewAccountInput {
    return {
      name,
      kind: 'imap',
      email,
      project_tag: projectTag,
      host,
      port: Number(port),
      username,
      starttls,
    };
  }

  async function handleTest() {
    setTestState('testing');
    setTestError(null);
    try {
      const result = await testConnection(buildInput(), password);
      setTestFolderCount(result.folders.length);
      setTestState('ok');
    } catch (e) {
      setTestError(errorMessage(e));
      setTestState('error');
    }
  }

  function fieldError(field: keyof typeof stepErrors): string | undefined {
    return touched[field] ? stepErrors[field] : undefined;
  }

  function toggleTag(tag: string) {
    setAddingNewTag(false);
    setSelectedTag((current) => (current === tag ? null : tag));
  }

  function selectNewTag() {
    setSelectedTag(null);
    setAddingNewTag(true);
  }

  async function handleFinish() {
    setSubmitting(true);
    setSubmitError(null);

    const projectTag = addingNewTag ? newTagInput.trim() || null : selectedTag;
    const input = buildInput(projectTag);

    let accountId: number;
    try {
      const account = await addAccount(input);
      accountId = account.id;
    } catch (e) {
      setSubmitError(errorMessage(e));
      setSubmitting(false);
      return;
    }

    try {
      await setAccountPassword(accountId, password);
    } catch {
      setSubmitError('アカウントは作成しましたが、パスワードの保存に失敗しました');
      setSubmitting(false);
      return;
    }

    setSubmitting(false);
    onAdded(accountId);
    handleClose();
  }

  return (
    <Modal
      open={open}
      onClose={handleClose}
      labelledBy="account-wizard-title"
      className="w-[520px] p-[18px]"
    >
      <div className="mb-[12px] flex items-center justify-between">
        <h2 id="account-wizard-title" className="text-md font-bold text-primary">
          {TITLES[step]}
        </h2>
        <span className="text-11 text-faint">ステップ {step} / 3</span>
      </div>

      {step === 1 && (
        <div className="flex flex-col gap-[10px]">
          <div
            aria-pressed="true"
            className="rounded-token border border-accent bg-accent-bg-subtle px-[12px] py-[10px] text-left text-base text-primary"
          >
            IMAP / 一般のメールサーバー
          </div>
          <button
            type="button"
            disabled
            className="flex items-center justify-between rounded-token border border-line-soft px-[12px] py-[10px] text-left text-base text-disabled opacity-60"
          >
            Gmail
            <Badge tone="neutral">近日対応</Badge>
          </button>
          <button
            type="button"
            disabled
            className="flex items-center justify-between rounded-token border border-line-soft px-[12px] py-[10px] text-left text-base text-disabled opacity-60"
          >
            Microsoft 365
            <Badge tone="neutral">近日対応</Badge>
          </button>

          <div className="mt-[8px] flex justify-end">
            <button
              type="button"
              onClick={() => setStep(2)}
              className="rounded-token bg-accent px-[16px] py-[6px] text-12 font-bold text-accent-on transition-colors hover:bg-accent-hover"
            >
              次へ
            </button>
          </div>
        </div>
      )}

      {step === 2 && (
        <div className="flex flex-col gap-[10px]">
          <Field label="表示名" id="wizard-name" error={fieldError('name')}>
            <input
              id="wizard-name"
              value={name}
              onChange={(e) => handleNameChange(e.target.value)}
              aria-invalid={!!fieldError('name')}
              aria-describedby={fieldError('name') ? 'wizard-name-error' : undefined}
              className={inputClass}
            />
          </Field>

          <Field label="メールアドレス" id="wizard-email" error={fieldError('email')}>
            <input
              id="wizard-email"
              value={email}
              onChange={(e) => handleEmailChange(e.target.value)}
              aria-invalid={!!fieldError('email')}
              aria-describedby={fieldError('email') ? 'wizard-email-error' : undefined}
              className={inputClass}
            />
          </Field>

          <Field label="ユーザー名" id="wizard-username" error={fieldError('username')}>
            <input
              id="wizard-username"
              value={username}
              onChange={(e) => handleUsernameChange(e.target.value)}
              aria-invalid={!!fieldError('username')}
              aria-describedby={fieldError('username') ? 'wizard-username-error' : undefined}
              className={inputClass}
            />
          </Field>

          <div className="flex gap-[10px]">
            <div className="flex-1">
              <Field label="ホスト" id="wizard-host" error={fieldError('host')}>
                <input
                  id="wizard-host"
                  value={host}
                  onChange={(e) => handleHostChange(e.target.value)}
                  aria-invalid={!!fieldError('host')}
                  aria-describedby={fieldError('host') ? 'wizard-host-error' : undefined}
                  className={inputClass}
                />
              </Field>
            </div>
            <div className="w-[100px]">
              <Field label="ポート" id="wizard-port" error={fieldError('port')}>
                <input
                  id="wizard-port"
                  value={port}
                  onChange={(e) => handlePortChange(e.target.value)}
                  aria-invalid={!!fieldError('port')}
                  aria-describedby={fieldError('port') ? 'wizard-port-error' : undefined}
                  className={inputClass}
                />
              </Field>
            </div>
          </div>

          <label
            htmlFor="wizard-starttls"
            className="flex items-center gap-[8px] text-base text-secondary"
          >
            <input
              id="wizard-starttls"
              type="checkbox"
              checked={starttls}
              onChange={(e) => handleStarttlsChange(e.target.checked)}
            />
            STARTTLS を使う
          </label>

          <Field label="パスワード" id="wizard-password">
            <input
              id="wizard-password"
              type="password"
              value={password}
              onChange={(e) => handlePasswordChange(e.target.value)}
              className={inputClass}
            />
          </Field>

          <div className="flex items-center gap-[8px]">
            <button
              type="button"
              onClick={() => void handleTest()}
              disabled={!canTest}
              className="rounded-token border border-line-soft px-[12px] py-[6px] text-12 text-secondary transition-colors hover:bg-selected disabled:opacity-50"
            >
              {testState === 'testing' ? '接続中…' : '接続テスト'}
            </button>
            {testState === 'ok' && (
              <span className="text-11 text-ok">
                接続できました（INBOX を確認） · フォルダ {testFolderCount} 件
              </span>
            )}
            {testState === 'error' && <span className="text-11 text-danger">{testError}</span>}
          </div>

          <div className="mt-[8px] flex justify-between">
            <button
              type="button"
              onClick={() => setStep(1)}
              className="rounded-token border border-line-soft px-[14px] py-[6px] text-12 text-muted transition-colors hover:bg-selected"
            >
              戻る
            </button>
            <button
              type="button"
              onClick={() => setStep(3)}
              disabled={testState !== 'ok'}
              className="rounded-token bg-accent px-[16px] py-[6px] text-12 font-bold text-accent-on transition-colors hover:bg-accent-hover disabled:opacity-50"
            >
              次へ
            </button>
          </div>
        </div>
      )}

      {step === 3 && (
        <div className="flex flex-col gap-[10px]">
          <p className="text-base text-muted">
            案件タグを選ぶと、サイドバーでまとめて表示されます（空のままでも構いません）。
          </p>
          <div className="flex flex-wrap gap-[6px]">
            {existingTags.map((tag) => (
              <button
                key={tag}
                type="button"
                onClick={() => toggleTag(tag)}
                className={`rounded-pill border px-[10px] py-[4px] text-12 transition-colors ${
                  selectedTag === tag
                    ? 'border-accent bg-accent-bg text-accent'
                    : 'border-line-soft text-secondary hover:bg-selected'
                }`}
              >
                {tag}
              </button>
            ))}
            <button
              type="button"
              onClick={selectNewTag}
              className={`rounded-pill border px-[10px] py-[4px] text-12 transition-colors ${
                addingNewTag
                  ? 'border-accent bg-accent-bg text-accent'
                  : 'border-line-soft text-secondary hover:bg-selected'
              }`}
            >
              新しい案件
            </button>
          </div>

          {addingNewTag && (
            <Field label="新しい案件名" id="wizard-new-tag">
              <input
                id="wizard-new-tag"
                value={newTagInput}
                onChange={(e) => setNewTagInput(e.target.value)}
                className={inputClass}
              />
            </Field>
          )}

          {submitError && <p className="text-11 text-danger">{submitError}</p>}

          <div className="mt-[8px] flex justify-between">
            <button
              type="button"
              onClick={() => setStep(2)}
              className="rounded-token border border-line-soft px-[14px] py-[6px] text-12 text-muted transition-colors hover:bg-selected"
            >
              戻る
            </button>
            <button
              type="button"
              onClick={() => void handleFinish()}
              disabled={submitting}
              className="rounded-token bg-accent px-[16px] py-[6px] text-12 font-bold text-accent-on transition-colors hover:bg-accent-hover disabled:opacity-50"
            >
              {submitting ? '作成中…' : '完了'}
            </button>
          </div>
        </div>
      )}
    </Modal>
  );
}
