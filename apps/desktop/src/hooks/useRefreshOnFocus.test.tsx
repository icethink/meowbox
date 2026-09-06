import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { render } from '@testing-library/react';
import { useRefreshOnFocus } from './useRefreshOnFocus';

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
