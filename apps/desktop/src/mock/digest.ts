import type { Digest } from '../types.ui';

/** 右パネル「今日のダイジェスト」。文言はデザインのまま */
export const mockDigest: Digest = {
  date_label: '9/2 (火)',
  summary: [
    { text: '要対応 4 件。最優先は ' },
    { text: '自社 の本番エラー対応', strong: true },
    { text: '、次いで ' },
    { text: '案件A の見積（9/5 期限）', strong: true },
    { text: '。案件B は 9/12 定例の資料準備を今週中に。' },
  ],
  groups: [
    {
      project_tag: '自社',
      items: [
        {
          id: 201,
          title: '本番エラーの一次対応連絡',
          due_label: '今日',
          due_tone: 'danger',
          is_candidate: false,
        },
      ],
    },
    {
      project_tag: '案件A',
      items: [
        {
          id: 101,
          title: '見積書を送る',
          due_label: '9/5',
          due_tone: 'accent',
          is_candidate: false,
        },
        {
          id: 102,
          title: 'ヒアリング日程調整',
          due_label: null,
          due_tone: 'neutral',
          is_candidate: true,
        },
      ],
    },
    {
      project_tag: '案件B',
      items: [
        {
          id: 301,
          title: '9/12 14:00 定例参加',
          due_label: '9/12',
          due_tone: 'neutral',
          is_candidate: false,
        },
      ],
    },
  ],
};
