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

  it('timeZone を指定すると Asia/Tokyo の壁時計で表示する', () => {
    // 2025-09-02T00:30:00Z は Asia/Tokyo では 2025-09-02 09:30。
    const base = new Date('2025-09-02T10:00:00+09:00');
    expect(formatRelativeDate('2025-09-02T00:30:00Z', base, 'Asia/Tokyo')).toBe('9:30');
  });

  it('timeZone に UTC を指定すると UTC の壁時計で表示する', () => {
    const base = new Date('2025-09-02T10:00:00+09:00');
    expect(formatRelativeDate('2025-09-02T00:30:00Z', base, 'UTC')).toBe('0:30');
  });

  it('日付境界をまたぐ場合、timeZone によって「今日」か「昨日」かが変わる', () => {
    // 2025-09-01T15:30:00Z は UTC では 9/1 だが、Asia/Tokyo では 9/2 0:30。
    const targetIso = '2025-09-01T15:30:00Z';
    const base = new Date('2025-09-02T10:00:00+09:00'); // Asia/Tokyo の 9/2

    expect(formatRelativeDate(targetIso, base, 'Asia/Tokyo')).toBe('0:30');
    expect(formatRelativeDate(targetIso, base, 'UTC')).toBe('昨日');
  });

  it('壊れたタイムゾーン名でも例外を投げずローカル時刻にフォールバックする', () => {
    expect(() =>
      formatRelativeDate(new Date(2026, 8, 6, 9, 41).toISOString(), now, 'Not/AZone'),
    ).not.toThrow();
    expect(
      typeof formatRelativeDate(new Date(2026, 8, 6, 9, 41).toISOString(), now, 'Not/AZone'),
    ).toBe('string');
  });

  it('テストのタイムゾーンが UTC に固定されている', () => {
    // JST（+09:00）のマシンなら 9 になるはずのものが 0 になれば、TZ=UTC 固定が効いている。
    expect(new Date('2025-09-02T00:30:00Z').getHours()).toBe(0);
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

  it('timeZone を指定すると Asia/Tokyo の壁時計で表示する', () => {
    const base = new Date('2025-09-02T10:00:00+09:00');
    expect(formatMessageTime('2025-09-02T00:30:00Z', base, 'Asia/Tokyo')).toBe('今日 9:30');
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

  it('timeZone を指定すると Asia/Tokyo の壁時計で「今日」判定する', () => {
    // 2025-09-01T15:30:00Z は Asia/Tokyo では 2025-09-02。
    const base = new Date('2025-09-02T10:00:00+09:00');
    expect(formatDueDate('2025-09-01T15:30:00Z', base, 'Asia/Tokyo')).toBe('今日');
  });
});
