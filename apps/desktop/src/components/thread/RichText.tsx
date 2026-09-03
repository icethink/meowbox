import { Fragment } from 'react';
import type { RichSpan } from '../../types.ui';

/**
 * 強調区間つきのテキスト。HTML を流し込まずに済むよう区間の配列で受ける
 * （メール本文由来の文字列をそのまま dangerouslySetInnerHTML に渡さないため）。
 * 改行は whitespace-pre-line で出す。
 */
export function RichText({
  spans,
  strongClassName = 'font-medium',
}: {
  spans: RichSpan[];
  strongClassName?: string;
}) {
  return (
    <span className="whitespace-pre-line">
      {spans.map((span, i) => (
        <Fragment key={i}>
          {span.strong ? <b className={strongClassName}>{span.text}</b> : span.text}
        </Fragment>
      ))}
    </span>
  );
}
