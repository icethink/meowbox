/**
 * API は RFC 3339 の日時だけを返す。表示用ラベル（「10:24」「昨日」「8/29」等）は
 * ここで組み立てる。`now` を引数に取れるようにしているのはテストを固定するため
 * （`Date.now()` を直接呼ぶとテスト実行時刻に結果が左右される）。
 *
 * 「今日/昨日/同じ年か」はすべて**ローカル時刻のカレンダー日**で比較する。UTC の
 * 日付境界で比較すると、日本時間の日付表示とずれるため。
 */

/** パースできない日時なら true。空文字・不正な文字列を弾くのに使う */
function isInvalid(d: Date): boolean {
  // 実データに壊れた日付が混ざってもここで弾き、画面を落とさないようにする。
  return Number.isNaN(d.getTime());
}

/** 年・月・日が一致するか。`getTime()` の差分では日付境界をまたぐ判定を誤るため使わない */
function isSameCalendarDay(a: Date, b: Date): boolean {
  return (
    a.getFullYear() === b.getFullYear() &&
    a.getMonth() === b.getMonth() &&
    a.getDate() === b.getDate()
  );
}

/** `today` を基準にした `date` の日数差（`date` が翌日なら 1、前日なら -1） */
function dayDiff(date: Date, today: Date): number {
  const a = new Date(date.getFullYear(), date.getMonth(), date.getDate()).getTime();
  const b = new Date(today.getFullYear(), today.getMonth(), today.getDate()).getTime();
  return Math.round((a - b) / (24 * 60 * 60 * 1000));
}

/**
 * "H:MM" を組み立てる（24 時間表記、時はゼロ埋めしない、分は 2 桁ゼロ埋め）。
 * `Intl.DateTimeFormat` は使わず数値から組み立てる。実行環境のロケール設定に
 * 表示が左右されないようにするため。
 */
function formatHourMinute(d: Date): string {
  const minute = String(d.getMinutes()).padStart(2, '0');
  return `${d.getHours()}:${minute}`;
}

/** "M/D" を組み立てる（ゼロ埋めしない） */
function formatMonthDay(d: Date): string {
  return `${d.getMonth() + 1}/${d.getDate()}`;
}

/** "YYYY/M/D" を組み立てる */
function formatYearMonthDay(d: Date): string {
  return `${d.getFullYear()}/${formatMonthDay(d)}`;
}

/** 一覧の 1 行に出す時刻ラベル。「10:24」「昨日」「8/29」「2025/8/29」 */
export function formatRelativeDate(date: string, now?: Date): string {
  const d = new Date(date);
  if (isInvalid(d)) return '';
  const base = now ?? new Date();

  if (isSameCalendarDay(d, base)) return formatHourMinute(d);
  if (dayDiff(d, base) === -1) return '昨日';
  if (d.getFullYear() === base.getFullYear()) return formatMonthDay(d);
  return formatYearMonthDay(d);
}

/** スレッド内の 1 通に出す時刻ラベル。「今日 9:41」「昨日 9:41」「9/1 17:20」「2025/9/1 17:20」 */
export function formatMessageTime(date: string, now?: Date): string {
  const d = new Date(date);
  if (isInvalid(d)) return '';
  const base = now ?? new Date();
  const time = formatHourMinute(d);

  if (isSameCalendarDay(d, base)) return `今日 ${time}`;
  if (dayDiff(d, base) === -1) return `昨日 ${time}`;
  if (d.getFullYear() === base.getFullYear()) return `${formatMonthDay(d)} ${time}`;
  return `${formatYearMonthDay(d)} ${time}`;
}

/** タスクの期限ラベル。「今日」「明日」「9/5」「2025/9/5」 */
export function formatDueDate(date: string, now?: Date): string {
  const d = new Date(date);
  if (isInvalid(d)) return '';
  const base = now ?? new Date();
  const diff = dayDiff(d, base);

  if (diff === 0) return '今日';
  if (diff === 1) return '明日';
  if (d.getFullYear() === base.getFullYear()) return formatMonthDay(d);
  return formatYearMonthDay(d);
}
