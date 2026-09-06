import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { render } from '@testing-library/react';
import { useAutoRefresh, useRefreshOnFocus } from './useRefreshOnFocus';

function Harness({ refresh }: { refresh: () => void }) {
  useRefreshOnFocus(refresh);
  return null;
}

describe('useRefreshOnFocus', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('ウィンドウがフォーカスを得ると refresh が呼ばれる', () => {
    const refresh = vi.fn();
    render(<Harness refresh={refresh} />);

    window.dispatchEvent(new Event('focus'));

    expect(refresh).toHaveBeenCalledTimes(1);
  });

  it('2 秒以内の連続フォーカスでは 1 回しか呼ばれない', () => {
    const refresh = vi.fn();
    render(<Harness refresh={refresh} />);

    window.dispatchEvent(new Event('focus'));
    vi.advanceTimersByTime(500);
    window.dispatchEvent(new Event('focus'));
    vi.advanceTimersByTime(500);
    window.dispatchEvent(new Event('focus'));

    expect(refresh).toHaveBeenCalledTimes(1);

    vi.advanceTimersByTime(1100);
    window.dispatchEvent(new Event('focus'));

    expect(refresh).toHaveBeenCalledTimes(2);
  });

  it('アンマウントするとリスナが外れる', () => {
    const refresh = vi.fn();
    const { unmount } = render(<Harness refresh={refresh} />);

    unmount();
    window.dispatchEvent(new Event('focus'));

    expect(refresh).not.toHaveBeenCalled();
  });
});

function AutoRefreshHarness({
  refresh,
  intervalMs,
  enabled,
}: {
  refresh: () => void;
  intervalMs: number;
  enabled: boolean;
}) {
  useAutoRefresh(refresh, intervalMs, enabled);
  return null;
}

describe('useAutoRefresh', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('既定オフ: enabled=false のときは時間を進めても refresh が呼ばれない', () => {
    const refresh = vi.fn();
    render(<AutoRefreshHarness refresh={refresh} intervalMs={1000} enabled={false} />);

    vi.advanceTimersByTime(10000);

    expect(refresh).not.toHaveBeenCalled();
  });

  it('enabled=true のとき、インターバル経過ごとに refresh が呼ばれる', () => {
    const refresh = vi.fn();
    render(<AutoRefreshHarness refresh={refresh} intervalMs={1000} enabled={true} />);

    vi.advanceTimersByTime(1000);
    expect(refresh).toHaveBeenCalledTimes(1);

    vi.advanceTimersByTime(1000);
    expect(refresh).toHaveBeenCalledTimes(2);
  });

  it('アンマウントすると clearInterval され、以後 refresh は呼ばれない', () => {
    const refresh = vi.fn();
    const { unmount } = render(
      <AutoRefreshHarness refresh={refresh} intervalMs={1000} enabled={true} />,
    );

    unmount();
    vi.advanceTimersByTime(10000);

    expect(refresh).not.toHaveBeenCalled();
  });

  it('refresh が毎レンダー新しい関数になってもインターバルは張り直されない', () => {
    let calls = 0;
    const { rerender } = render(
      <AutoRefreshHarness
        refresh={() => {
          calls += 1;
        }}
        intervalMs={1000}
        enabled={true}
      />,
    );

    rerender(
      <AutoRefreshHarness
        refresh={() => {
          calls += 1;
        }}
        intervalMs={1000}
        enabled={true}
      />,
    );
    rerender(
      <AutoRefreshHarness
        refresh={() => {
          calls += 1;
        }}
        intervalMs={1000}
        enabled={true}
      />,
    );

    vi.advanceTimersByTime(1000);

    expect(calls).toBe(1);
  });
});
