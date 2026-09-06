/**
 * API は RFC 3339 の日時だけを返す。表示用ラベル（「10:24」「昨日」「8/29」等）は
 * ここで組み立てる。`now` を引数に取れるようにしているのはテストを固定するため
 * （`Date.now()` を直接呼ぶとテスト実行時刻に結果が左右される）。
 *
 * `timeZone` を明示したときだけ `Intl.DateTimeFormat` を使ってそのタイムゾーンでの
 * 壁時計を取り出す。省略時は実行環境のローカル時刻をそのまま使う（ロケール差の
 * 影響を受けない）。`timeZone` を受け取れるのは主にテストを固定するためで、
 * アプリは省略して実行環境のローカル時刻を使う。
 *
 * 「今日/昨日/同じ年か」はすべて `timeZone`（省略時はローカル時刻）の**カレンダー日**で
 * 比較する。UTC の日付境界で比較すると、日本時間の日付表示とずれるため。
 */

/** パースできない日時なら true。空文字・不正な文字列を弾くのに使う */
function isInvalid(d: Date): boolean {
  // 実データに壊れた日付が混ざってもここで弾き、画面を落とさないようにする。
  return Number.isNaN(d.getTime());
}

/** 壁時計の年・月（1-12）・日・時・分 */
interface WallClock {
  year: number;
  month: number;
  day: number;
  hour: number;
  minute: number;
}

/** 指定タイムゾーン（省略時は実行環境のローカル）での壁時計の各要素を取り出す。 */
function wallClock(d: Date, timeZone?: string): WallClock {
  if (timeZone === undefined) {
    return {
      year: d.getFullYear(),
      month: d.getMonth() + 1,
      day: d.getDate(),
      hour: d.getHours(),
      minute: d.getMinutes(),
    };
  }

  try {
    // hour12: false ではなく hourCycle: 'h23' を使う。環境によっては hour12: false でも
    // 深夜 0 時が "24" になることがあるため。
    const parts = new Intl.DateTimeFormat('en-US', {
      timeZone,
      year: 'numeric',
      month: 'numeric',
      day: 'numeric',
      hour: 'numeric',
      minute: '2-digit',
      hourCycle: 'h23',
    }).formatToParts(d);

    const get = (type: string): number => Number(parts.find((p) => p.type === type)?.value);
    const year = get('year');
    const month = get('month');
    const day = get('day');
    const hour = get('hour');
    const minute = get('minute');

    if (
      Number.isNaN(year) ||
      Number.isNaN(month) ||
      Number.isNaN(day) ||
      Number.isNaN(hour) ||
      Number.isNaN(minute)
    ) {
      // 壊れたタイムゾーン名などで値が取れなかった場合はローカル時刻にフォールバックする。
      return wallClock(d, undefined);
    }
    return { year, month, day, hour, minute };
  } catch {
    // 不正なタイムゾーン名で Intl が例外を投げても画面を落とさない。
    return wallClock(d, undefined);
  }
}

/** 年・月・日が一致するか */
function isSameCalendarDay(a: WallClock, b: WallClock): boolean {
  return a.year === b.year && a.month === b.month && a.day === b.day;
}

/** `today` を基準にした `date` の日数差（`date` が翌日なら 1、前日なら -1） */
function dayDiff(date: WallClock, today: WallClock): number {
  // ローカルの `new Date(y, m, d)` ではなく `Date.UTC` で比較する。タイムゾーンをまたいでも
  // 差分の計算が安全（DST などの影響を受けない）ため。
  const a = Date.UTC(date.year, date.month - 1, date.day);
  const b = Date.UTC(today.year, today.month - 1, today.day);
  return Math.round((a - b) / (24 * 60 * 60 * 1000));
}

/**
 * "H:MM" を組み立てる（24 時間表記、時はゼロ埋めしない、分は 2 桁ゼロ埋め）。
 * `Intl.DateTimeFormat` は使わず数値から組み立てる。実行環境のロケール設定に
 * 表示が左右されないようにするため。
 */
function formatHourMinute(w: WallClock): string {
  const minute = String(w.minute).padStart(2, '0');
  return `${w.hour}:${minute}`;
}

/** "M/D" を組み立てる（ゼロ埋めしない） */
function formatMonthDay(w: WallClock): string {
  return `${w.month}/${w.day}`;
}

/** "YYYY/M/D" を組み立てる */
function formatYearMonthDay(w: WallClock): string {
  return `${w.year}/${formatMonthDay(w)}`;
}

/** 一覧の 1 行に出す時刻ラベル。「10:24」「昨日」「8/29」「2025/8/29」 */
export function formatRelativeDate(date: string, now?: Date, timeZone?: string): string {
  const d = new Date(date);
  if (isInvalid(d)) return '';
  const base = now ?? new Date();
  const dw = wallClock(d, timeZone);
  const bw = wallClock(base, timeZone);

  if (isSameCalendarDay(dw, bw)) return formatHourMinute(dw);
  if (dayDiff(dw, bw) === -1) return '昨日';
  if (dw.year === bw.year) return formatMonthDay(dw);
  return formatYearMonthDay(dw);
}

/** スレッド内の 1 通に出す時刻ラベル。「今日 9:41」「昨日 9:41」「9/1 17:20」「2025/9/1 17:20」 */
export function formatMessageTime(date: string, now?: Date, timeZone?: string): string {
  const d = new Date(date);
  if (isInvalid(d)) return '';
  const base = now ?? new Date();
  const dw = wallClock(d, timeZone);
  const bw = wallClock(base, timeZone);
  const time = formatHourMinute(dw);

  if (isSameCalendarDay(dw, bw)) return `今日 ${time}`;
  if (dayDiff(dw, bw) === -1) return `昨日 ${time}`;
  if (dw.year === bw.year) return `${formatMonthDay(dw)} ${time}`;
  return `${formatYearMonthDay(dw)} ${time}`;
}

/** タスクの期限ラベル。「今日」「明日」「9/5」「2025/9/5」 */
export function formatDueDate(date: string, now?: Date, timeZone?: string): string {
  const d = new Date(date);
  if (isInvalid(d)) return '';
  const base = now ?? new Date();
  const dw = wallClock(d, timeZone);
  const bw = wallClock(base, timeZone);
  const diff = dayDiff(dw, bw);

  if (diff === 0) return '今日';
  if (diff === 1) return '明日';
  if (dw.year === bw.year) return formatMonthDay(dw);
  return formatYearMonthDay(dw);
}
