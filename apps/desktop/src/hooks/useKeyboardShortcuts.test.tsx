import { beforeEach, describe, expect, it } from 'vitest';
import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useAppStore } from '../store/app';
import { useKeyboardShortcuts } from './useKeyboardShortcuts';

const KEYS = ['th-prod-error', 'th-estimate', 'th-monthly-meeting'];

/** フックだけを載せた最小のハーネス。実 UI に依存させない */
function Harness() {
  const selectedThreadKey = useAppStore((s) => s.selectedThreadKey);
  const archived = useAppStore((s) => s.archivedKeys);
  useKeyboardShortcuts({
    threadKeys: KEYS.filter((k) => !archived.includes(k)),
    selectedThreadKey,
  });
  return (
    <div>
      <span data-testid="selected">{selectedThreadKey}</span>
      <textarea aria-label="返信本文" id="reply-input" />
    </div>
  );
}

describe('useKeyboardShortcuts', () => {
  beforeEach(() => {
    useAppStore.setState({
      selectedThreadKey: 'th-prod-error',
      archivedKeys: [],
      readKeys: [],
      rightPanelOpen: true,
      commandPaletteOpen: false,
    });
  });

  it('j で次、k で前のスレッドに移動する', async () => {
    render(<Harness />);

    await userEvent.keyboard('j');
    expect(screen.getByTestId('selected')).toHaveTextContent('th-estimate');

    await userEvent.keyboard('j');
    expect(screen.getByTestId('selected')).toHaveTextContent('th-monthly-meeting');

    await userEvent.keyboard('k');
    expect(screen.getByTestId('selected')).toHaveTextContent('th-estimate');
  });

  it('端で j / k を押しても選択は外れない', async () => {
    render(<Harness />);

    await userEvent.keyboard('k');
    expect(screen.getByTestId('selected')).toHaveTextContent('th-prod-error');

    await userEvent.keyboard('jjjj');
    expect(screen.getByTestId('selected')).toHaveTextContent('th-monthly-meeting');
  });

  it('e でアーカイブし、選択を次のスレッドへ送る', async () => {
    render(<Harness />);

    await userEvent.keyboard('e');

    expect(useAppStore.getState().archivedKeys).toEqual(['th-prod-error']);
    expect(screen.getByTestId('selected')).toHaveTextContent('th-estimate');
  });

  it('入力中はショートカットを拾わない', async () => {
    render(<Harness />);
    const input = screen.getByRole('textbox', { name: '返信本文' });

    await userEvent.click(input);
    await userEvent.keyboard('je');

    expect(screen.getByTestId('selected')).toHaveTextContent('th-prod-error');
    expect(useAppStore.getState().archivedKeys).toEqual([]);
    expect(input).toHaveValue('je');
  });

  it('r で返信欄にフォーカスする', async () => {
    render(<Harness />);

    await userEvent.keyboard('r');

    expect(screen.getByRole('textbox', { name: '返信本文' })).toHaveFocus();
  });

  it('Ctrl+\\ で右パネル、Ctrl+K でコマンドパレットが開く', async () => {
    render(<Harness />);

    await userEvent.keyboard('{Control>}\\{/Control}');
    expect(useAppStore.getState().rightPanelOpen).toBe(false);

    await userEvent.keyboard('{Control>}k{/Control}');
    expect(useAppStore.getState().commandPaletteOpen).toBe(true);
  });
});
