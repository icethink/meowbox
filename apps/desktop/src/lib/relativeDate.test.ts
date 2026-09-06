/**
 * タイムゾーン依存で落ちないよう、日時は必ずローカルのコンストラクタ
 * `new Date(2026, 8, 6, 10, 24)` で作り、関数に渡すときは `.toISOString()` する。
 * 文字列リテラルの `'2026-09-06T10:24:00Z'` を直接使うと UTC 解釈になり、
 * 実行環境のタイムゾーンによってテスト結果が変わってしまうため避ける。
 */
import { describe, expect, it } from 'vitest';
import { formatDueDate, formatMessageTime, formatRelativeDate } from './relativeDate';

describe('formatRelativeDate', () => {
  const now = new Date(2026, 8, 6, 10, 24); // 2026-09-06 10:24 ローカル

  it('今日の 9:41 は時がゼロ埋めされない', () => {
    expect(formatRelativeDate(new Date(2026, 8, 6, 9, 41).toISOString(), now)).toBe('9:41');
  });

  it('今日の 0:05 は分だけゼロ埋めされる', () => {
    expect(formatRelativeDate(new Date(2026, 8, 6, 0, 5).toISOString(), now)).toBe('0:05');
  });

  it('昨日は「昨日」', () => {
    expect(formatRelativeDate(new Date(2026, 8, 5, 20, 0).toISOString(), now)).toBe('昨日');
  });

  it('8 日前（同年）は M/D', () => {
    expect(formatRelativeDate(new Date(2026, 7, 29, 12, 0).toISOString(), now)).toBe('8/29');
  });

  it('前年は YYYY/M/D', () => {
    expect(formatRelativeDate(new Date(2025, 7, 29, 12, 0).toISOString(), now)).toBe('2025/8/29');
  });

  it('未来（同年）は M/D。「明日」とは書かない', () => {
    expect(formatRelativeDate(new Date(2026, 8, 12, 9, 0).toISOString(), now)).toBe('9/12');
  });

  it('空文字は空文字列', () => {
    expect(formatRelativeDate('', now)).toBe('');
  });

  it('不正な文字列は空文字列', () => {
    expect(formatRelativeDate('not a date', now)).toBe('');
  });

  it('月をまたぐ「昨日」', () => {
    const base = new Date(2026, 8, 1, 10, 0); // 2026-09-01
    expect(formatRelativeDate(new Date(2026, 7, 31, 20, 0).toISOString(), base)).toBe('昨日');
  });

  it('年をまたぐ「昨日」', () => {
    const base = new Date(2026, 0, 1, 10, 0); // 2026-01-01
    expect(formatRelativeDate(new Date(2025, 11, 31, 20, 0).toISOString(), base)).toBe('昨日');
  });
});

describe('formatMessageTime', () => {
  const now = new Date(2026, 8, 6, 10, 24);

  it('今日', () => {
    expect(formatMessageTime(new Date(2026, 8, 6, 9, 41).toISOString(), now)).toBe('今日 9:41');
  });

  it('昨日', () => {
    expect(formatMessageTime(new Date(2026, 8, 5, 9, 41).toISOString(), now)).toBe('昨日 9:41');
  });

  it('同年', () => {
    expect(formatMessageTime(new Date(2026, 8, 1, 17, 20).toISOString(), now)).toBe('9/1 17:20');
  });

  it('前年', () => {
    expect(formatMessageTime(new Date(2025, 8, 1, 17, 20).toISOString(), now)).toBe(
      '2025/9/1 17:20',
    );
  });
});

describe('formatDueDate', () => {
  const now = new Date(2026, 8, 6, 10, 24);

  it('今日', () => {
    expect(formatDueDate(new Date(2026, 8, 6, 9, 0).toISOString(), now)).toBe('今日');
  });

  it('明日', () => {
    expect(formatDueDate(new Date(2026, 8, 7, 9, 0).toISOString(), now)).toBe('明日');
  });

  it('同年の過去日は M/D。「昨日」とは書かない', () => {
    expect(formatDueDate(new Date(2026, 8, 5, 9, 0).toISOString(), now)).toBe('9/5');
  });

  it('前年は YYYY/M/D', () => {
    expect(formatDueDate(new Date(2025, 8, 5, 9, 0).toISOString(), now)).toBe('2025/9/5');
  });

  it('不正な文字列は空文字列', () => {
    expect(formatDueDate('not a date', now)).toBe('');
  });
});
