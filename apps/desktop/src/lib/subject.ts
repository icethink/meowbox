/**
 * 件名の先頭にある【至急】【重要】のような角括弧マーカーを切り出す。
 * 差出人が付けた印なので、表示は人間側の色（--danger）で出す。
 */
export function splitSubjectMarker(subject: string): { marker: string | null; rest: string } {
  const m = /^【[^】]{1,8}】/.exec(subject);
  if (!m) return { marker: null, rest: subject };
  return { marker: m[0], rest: subject.slice(m[0].length) };
}
