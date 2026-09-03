import type { Task } from '../types';

/**
 * この確度を超えた AI 抽出タスクは「確定」として実線で出す。
 * 下回るものは「候補」で、人間が確定を押すまでタスクとして数えない。
 */
// TODO(P3): move to mailcore once real extraction accuracy is known
export const CONFIDENT_AT = 0.8;

export function isConfident(task: Task): boolean {
  return task.confidence >= CONFIDENT_AT;
}

/** "2025-09-05T09:00:00Z" → "9/5 (金)" */
export function formatDue(due: string): string {
  const d = new Date(due);
  const week = ['日', '月', '火', '水', '木', '金', '土'][d.getDay()] ?? '';
  return `${d.getMonth() + 1}/${d.getDate()} (${week})`;
}
