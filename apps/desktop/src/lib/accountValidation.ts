/**
 * アカウント追加ウィザードの入力チェック。
 *
 * 外部依存（zod 等）は増やさず、素の関数で書く。戻り値は
 * 「エラーメッセージ、問題なければ null」で統一する。
 */

/** 空文字・空白のみを弾く */
export function validateRequired(value: string, label: string): string | null {
  if (value.trim() === '') return `${label}を入力してください`;
  return null;
}

// メールアドレスの形式チェックは厳密な RFC 準拠にしない。
// 厳密にやると実在する正しいアドレスまで弾いてしまうことがあり、
// 最終的な正否は「接続テスト」で分かるため、ここでは大まかな形だけ見る。
const EMAIL_RE = /^[^\s@]+@[^\s@.]+(\.[^\s@.]+)+$/;

export function validateEmail(value: string): string | null {
  const required = validateRequired(value, 'メールアドレス');
  if (required) return required;
  if (!EMAIL_RE.test(value.trim())) return 'メールアドレスの形式が正しくありません';
  return null;
}

export function validateHost(value: string): string | null {
  const required = validateRequired(value, 'ホスト');
  if (required) return required;
  const trimmed = value.trim();
  if (/\s/.test(trimmed) || /^[a-zA-Z]+:\/\//.test(trimmed)) {
    return 'サーバー名だけを入力してください（例: imap.example.com）';
  }
  return null;
}

export function validatePort(value: string): string | null {
  const required = validateRequired(value, 'ポート');
  if (required) return required;
  const trimmed = value.trim();
  if (!/^\d+$/.test(trimmed)) return '数字で入力してください';
  const port = Number(trimmed);
  if (port < 1 || port > 65535) return 'ポート番号は 1〜65535 です';
  return null;
}

/**
 * メールアドレスのドメインから IMAP 設定を仮置きする。ユーザーは後から直せる。
 * ドメインが取れない（`@` が無い、ドメインが空）場合は null を返す。
 */
export function guessImapSettings(
  email: string,
): { host: string; port: number; starttls: boolean } | null {
  const at = email.indexOf('@');
  if (at < 0) return null;
  const domain = email.slice(at + 1).trim();
  if (domain === '') return null;
  return { host: `imap.${domain}`, port: 993, starttls: false };
}

/** ステップ 2 の全項目。フィールド名 → エラーメッセージ。空オブジェクトなら OK */
export function validateServerStep(v: {
  name: string;
  email: string;
  username: string;
  host: string;
  port: string;
}): Record<string, string> {
  const errors: Record<string, string> = {};

  const name = validateRequired(v.name, '表示名');
  if (name) errors.name = name;

  const email = validateEmail(v.email);
  if (email) errors.email = email;

  const username = validateRequired(v.username, 'ユーザー名');
  if (username) errors.username = username;

  const host = validateHost(v.host);
  if (host) errors.host = host;

  const port = validatePort(v.port);
  if (port) errors.port = port;

  return errors;
}
