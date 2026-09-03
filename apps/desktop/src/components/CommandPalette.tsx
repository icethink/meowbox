import { Search } from 'lucide-react';
import { useAppStore } from '../store/app';
import { Modal } from './ui/Modal';

/** Ctrl+K の入れ物だけ。検索とコマンドの中身は P5 */
export function CommandPalette() {
  const open = useAppStore((s) => s.commandPaletteOpen);
  const setOpen = useAppStore((s) => s.setCommandPaletteOpen);

  return (
    <Modal
      open={open}
      onClose={() => setOpen(false)}
      labelledBy="command-palette-title"
      className="w-[560px] overflow-hidden"
    >
      <div className="flex items-center gap-[9px] border-b border-line px-[14px] py-[12px]">
        <Search size={15} strokeWidth={2} className="text-muted" aria-hidden="true" />
        {/* パレットは開いた瞬間に打ち始めるものなので autoFocus を付ける */}
        <input
          autoFocus
          id="command-palette-title"
          aria-label="コマンドと検索"
          placeholder="メールを検索、またはコマンドを実行…"
          className="selectable flex-1 bg-transparent text-md text-primary outline-none placeholder:text-placeholder"
        />
      </div>
      <div className="grid h-[180px] place-items-center text-base text-faint">
        {/* TODO(P5): 検索・案件切り替え・タスク作成をここに出す */}
        検索とコマンドは P5 で実装
      </div>
    </Modal>
  );
}
