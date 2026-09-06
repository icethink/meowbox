import { describe, expect, it } from 'vitest';
import {
  guessImapSettings,
  validateEmail,
  validateHost,
  validatePort,
  validateRequired,
  validateServerStep,
} from './accountValidation';

describe('validateRequired', () => {
  it('値があれば null', () => {
    expect(validateRequired('自社', '表示名')).toBeNull();
  });

  it('空文字はエラー', () => {
    expect(validateRequired('', '表示名')).toBe('表示名を入力してください');
  });

  it('空白のみはエラー', () => {
    expect(validateRequired('   ', '表示名')).toBe('表示名を入力してください');
  });
});

describe('validateEmail', () => {
  it('妥当なアドレスは null', () => {
    expect(validateEmail('you@example.com')).toBeNull();
  });

  it('空はエラー', () => {
    expect(validateEmail('')).toBe('メールアドレスを入力してください');
  });

  it('形式不正はエラー', () => {
    expect(validateEmail('not-an-email')).toBe('メールアドレスの形式が正しくありません');
  });
});

describe('validateHost', () => {
  it('妥当なホストは null', () => {
    expect(validateHost('imap.example.com')).toBeNull();
  });

  it('空はエラー', () => {
    expect(validateHost('')).toBe('ホストを入力してください');
  });

  it('空白を含むとエラー', () => {
    expect(validateHost('imap. example.com')).toBe(
      'サーバー名だけを入力してください（例: imap.example.com）',
    );
  });

  it('スキーマを含むとエラー', () => {
    expect(validateHost('http://imap.example.com')).toBe(
      'サーバー名だけを入力してください（例: imap.example.com）',
    );
  });
});

describe('validatePort', () => {
  it('妥当な範囲は null', () => {
    expect(validatePort('993')).toBeNull();
  });

  it('空はエラー', () => {
    expect(validatePort('')).toBe('ポートを入力してください');
  });

  it('数字以外はエラー', () => {
    expect(validatePort('abc')).toBe('数字で入力してください');
  });

  it('範囲外はエラー', () => {
    expect(validatePort('0')).toBe('ポート番号は 1〜65535 です');
    expect(validatePort('65536')).toBe('ポート番号は 1〜65535 です');
  });
});

describe('guessImapSettings', () => {
  it('通常のアドレスからホストとポートを仮置きする', () => {
    expect(guessImapSettings('you@example.com')).toEqual({
      host: 'imap.example.com',
      port: 993,
      starttls: false,
    });
  });

  it('@ が無ければ null', () => {
    expect(guessImapSettings('not-an-email')).toBeNull();
  });

  it('サブドメイン付きでもそのままぶら下げる', () => {
    expect(guessImapSettings('you@mail.example.com')).toEqual({
      host: 'imap.mail.example.com',
      port: 993,
      starttls: false,
    });
  });
});

describe('validateServerStep', () => {
  it('全部妥当なら空オブジェクト', () => {
    expect(
      validateServerStep({
        name: '自社',
        email: 'you@example.com',
        username: 'you@example.com',
        host: 'imap.example.com',
        port: '993',
      }),
    ).toEqual({});
  });

  it('不正な項目だけエラーが入る', () => {
    const errors = validateServerStep({
      name: '',
      email: 'you@example.com',
      username: 'you@example.com',
      host: 'imap.example.com',
      port: 'abc',
    });
    expect(errors).toEqual({
      name: '表示名を入力してください',
      port: '数字で入力してください',
    });
  });
});
