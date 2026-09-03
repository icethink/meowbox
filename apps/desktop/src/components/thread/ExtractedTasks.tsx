import { SquareCheck } from 'lucide-react';
import type { Task } from '../../types';
import { useAppStore } from '../../store/app';
import { isConfident } from '../../lib/tasks';
import { TaskRow } from './TaskRow';

export function ExtractedTasks({ tasks }: { tasks: Task[] }) {
  const decisions = useAppStore((s) => s.taskDecisions);
  const decide = useAppStore((s) => s.decideTask);

  const visible = tasks.filter((t) => decisions[t.id] !== 'dismissed');
  if (visible.length === 0) return null;

  return (
    <section className="flex flex-col gap-[6px]">
      <h3 className="flex items-center gap-[6px] text-11 font-medium tracking-w6 text-faint">
        <SquareCheck size={11} strokeWidth={2} className="text-ai" aria-hidden="true" />
        AI 抽出タスク
      </h3>
      {visible.map((task) => {
        const decision = decisions[task.id];
        // 人間が「確定」を押したものは、確度が低くても確定行として扱う
        const asConfirmed = decision === 'confirmed' || decision === 'done' || isConfident(task);
        return (
          <TaskRow
            key={task.id}
            task={asConfirmed ? { ...task, confidence: 1 } : task}
            checked={decision === 'done'}
            onToggle={(next) => decide(task.id, next ? 'done' : 'confirmed')}
            onConfirm={() => decide(task.id, 'confirmed')}
            onDismiss={() => decide(task.id, 'dismissed')}
          />
        );
      })}
    </section>
  );
}
