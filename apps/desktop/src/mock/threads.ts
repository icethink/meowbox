import type { Task } from '../types';
import type { ThreadDetail, ThreadListItem } from '../types.ui';

/**
 * デザインと同じスレッド。スナップショットは 2025-09-02 (火) 想定で、
 * 相対ラベル（今日 / 昨日 / 月曜）と ISO 日付の曜日が食い違わないようにしてある。
 */
export const mockThreads: ThreadListItem[] = [
  {
    thread_key: 'th-prod-error',
    account_id: 1,
    project_tag: '自社',
    from_name: '佐藤 誠',
    subject: '【至急】本番環境でエラー',
    ai_snippet: '一覧画面で500エラー。ログ確認と一次対応の連絡が必要',
    snippet: '本番の一覧画面で 500 エラーが出ているとの連絡がありました。ログを確認のうえ…',
    time_label: '10:24',
    date: '2025-09-02T01:24:00Z',
    is_unread: true,
    urgency: 'urgent',
    has_attachments: false,
    task_count: 1,
    is_muted: false,
  },
  {
    thread_key: 'th-estimate',
    account_id: 2,
    project_tag: '案件A',
    from_name: '山田 太郎',
    subject: 'Re: 見積の件',
    ai_snippet: '追加要件3点の見積を今週金曜（9/5）までに提出希望',
    snippet: '社内稟議の都合上、今週金曜（9/5）中にお見積をいただけますと大変助かります…',
    time_label: '9:41',
    date: '2025-09-02T00:41:00Z',
    is_unread: true,
    urgency: 'action',
    has_attachments: true,
    task_count: 2,
    is_muted: false,
  },
  {
    thread_key: 'th-monthly-meeting',
    account_id: 4,
    project_tag: '案件B',
    from_name: '総務部 情報システム課',
    subject: '9月定例のご案内',
    ai_snippet: '9/12（金）14:00〜 定例会。議題は障害報告と改修計画',
    snippet: '9月の定例会について下記のとおりご案内いたします。日時: 9/12（金）14:00〜…',
    time_label: '昨日',
    date: '2025-09-01T07:10:00Z',
    is_unread: false,
    urgency: null,
    has_attachments: false,
    task_count: 1,
    is_muted: false,
  },
  {
    thread_key: 'th-product-images',
    account_id: 2,
    project_tag: '案件A',
    from_name: '鈴木 花子',
    subject: 'Re: 商品画像の差し替えについて',
    ai_snippet: '差し替え画像20点を共有フォルダに格納済み。確認依頼',
    snippet: '差し替え画像 20 点を共有フォルダに格納しました。ご確認をお願いいたします。',
    time_label: '昨日',
    date: '2025-09-01T05:30:00Z',
    is_unread: false,
    urgency: null,
    has_attachments: false,
    task_count: 0,
    is_muted: false,
  },
  {
    thread_key: 'th-gitlab-digest',
    account_id: 1,
    project_tag: '自社',
    from_name: 'GitLab',
    subject: 'Weekly digest',
    ai_snippet: null,
    snippet: 'Here is your weekly digest. 3 merge requests were merged, 2 issues were closed…',
    time_label: '月曜',
    date: '2025-09-01T00:00:00Z',
    is_unread: false,
    urgency: null,
    has_attachments: false,
    task_count: 0,
    is_muted: true,
  },
  {
    thread_key: 'th-acceptance-doc',
    account_id: 1,
    project_tag: '自社',
    from_name: '経理部 高橋',
    subject: '8月分検収書のご送付',
    ai_snippet: '検収書を添付。押印のうえ返送を依頼',
    snippet: '8月分の検収書をお送りします。ご押印のうえ、ご返送をお願いいたします。',
    time_label: '8/29',
    date: '2025-08-29T02:00:00Z',
    is_unread: false,
    urgency: null,
    has_attachments: true,
    task_count: 0,
    is_muted: false,
  },
];

const estimateTasks: Task[] = [
  {
    id: 101,
    account_id: 2,
    source_message_id: 1002,
    title: '見積書を送る',
    due: '2025-09-05T09:00:00Z',
    status: 'open',
    // 確度が高いので「確定」扱い。しきい値は ExtractedTasks 側の CONFIDENT_AT
    confidence: 0.94,
    created_by: 'ai',
    created_at: '2025-09-02T01:02:00Z',
  },
  {
    id: 102,
    account_id: 2,
    source_message_id: 1002,
    title: '追加要件のヒアリング日程調整',
    due: null,
    status: 'open',
    confidence: 0.62,
    created_by: 'ai',
    created_at: '2025-09-02T01:02:00Z',
  },
];

export const mockThreadDetails: Record<string, ThreadDetail> = {
  'th-estimate': {
    thread_key: 'th-estimate',
    subject: 'Re: 見積の件',
    project_tag: '案件A',
    counterpart: { name: '山田 太郎', email: 'yamada@client-a.example' },
    message_count: 4,
    summary: {
      target: 'thread:th-estimate',
      generated_label: '10:02 生成',
      generated_at: '2025-09-02T01:02:00Z',
      model: 'claude-opus-5',
      body: [
        { text: 'ECサイトリニューアル案件の追加見積に関するやり取り。山田氏は前回提示分に加え、' },
        { text: '①検索機能の絞り込み拡張 ②会員ランク別価格表示 ③レビュー画像添付', strong: true },
        { text: ' の3点について見積を求めている。' },
        { text: '期限は今週金曜（9/5）', strong: true },
        { text: '。②は仕様が曖昧なため、ヒアリングの場を設けるか確認が必要。' },
      ],
    },
    tasks: estimateTasks,
    messages: [
      {
        id: 1001,
        from: { name: '山田 太郎', email: 'yamada@client-a.example' },
        initial: '山',
        time_label: '9/1 17:20',
        body: [
          {
            text:
              '田中様\nお世話になっております。クライアントA の山田です。\n' +
              '先日のお打ち合わせで挙がった追加要件について、社内で優先度を整理いたしました。' +
              'まずは3点、お見積をお願いできますでしょうか。',
          },
        ],
        quoted_lines: 12,
        quoted_text:
          '> 先日はお時間をいただきありがとうございました。\n' +
          '> 追加要件の候補として以下を挙げております。\n' +
          '> ・検索機能の絞り込み拡張\n' +
          '> ・会員ランク別価格表示\n' +
          '> ・レビュー画像添付\n' +
          '> ・お気に入り一覧の並び替え\n' +
          '> ・カート保存期間の延長\n' +
          '> ・クーポン併用可否の設定\n' +
          '> ・配送日時指定の粒度変更\n' +
          '> ・領収書の一括ダウンロード\n' +
          '> ・管理画面の権限グループ追加\n' +
          '> ・在庫アラートのしきい値設定',
        attachments: [],
        is_latest: false,
        is_read: true,
      },
      {
        id: 1002,
        from: { name: '山田 太郎', email: 'yamada@client-a.example' },
        initial: '山',
        time_label: '今日 9:41',
        body: [
          { text: '田中様\nたびたび失礼いたします。社内稟議の都合上、' },
          { text: '今週金曜（9/5）中', strong: true },
          {
            text:
              'にお見積をいただけますと大変助かります。難しい場合は概算だけでも構いませんので、' +
              'ご一報いただけますでしょうか。\nどうぞよろしくお願いいたします。',
          },
        ],
        quoted_lines: 0,
        quoted_text: '',
        attachments: [{ id: 9001, filename: '追加要件一覧.xlsx', size_label: '18KB' }],
        is_latest: true,
        is_read: false,
      },
    ],
    reply_placeholder: '山田さんへ返信…',
  },

  'th-prod-error': {
    thread_key: 'th-prod-error',
    subject: '【至急】本番環境でエラー',
    project_tag: '自社',
    counterpart: { name: '佐藤 誠', email: 'sato@my-company.example' },
    message_count: 1,
    summary: {
      target: 'thread:th-prod-error',
      generated_label: '10:30 生成',
      generated_at: '2025-09-02T01:30:00Z',
      model: 'claude-opus-5',
      body: [
        { text: '本番環境の一覧画面で 500 エラーが発生している。' },
        { text: 'まずログを確認し、一次対応の連絡を今日中に返す必要がある', strong: true },
        { text: '。再現条件は未特定。' },
      ],
    },
    tasks: [
      {
        id: 201,
        account_id: 1,
        source_message_id: 2001,
        title: '本番エラーの一次対応連絡',
        due: '2025-09-02T09:00:00Z',
        status: 'open',
        confidence: 0.91,
        created_by: 'ai',
        created_at: '2025-09-02T01:30:00Z',
      },
    ],
    messages: [
      {
        id: 2001,
        from: { name: '佐藤 誠', email: 'sato@my-company.example' },
        initial: '佐',
        time_label: '今日 10:24',
        body: [
          {
            text:
              '田中様\nお疲れさまです。佐藤です。\n' +
              '本番の一覧画面で 500 エラーが出ているとの連絡がありました。' +
              'ログを確認のうえ、一次回答を本日中にお願いできますでしょうか。',
          },
        ],
        quoted_lines: 0,
        quoted_text: '',
        attachments: [],
        is_latest: true,
        is_read: false,
      },
    ],
    reply_placeholder: '佐藤さんへ返信…',
  },

  'th-monthly-meeting': {
    thread_key: 'th-monthly-meeting',
    subject: '9月定例のご案内',
    project_tag: '案件B',
    counterpart: { name: '総務部 情報システム課', email: 'info-sys@client-b.example' },
    message_count: 1,
    summary: {
      target: 'thread:th-monthly-meeting',
      generated_label: '昨日 16:20 生成',
      generated_at: '2025-09-01T07:20:00Z',
      model: 'claude-opus-5',
      body: [
        { text: '9月の定例会は ' },
        { text: '9/12（金）14:00〜', strong: true },
        { text: '。議題は障害報告と改修計画。資料は前日までに共有が必要。' },
      ],
    },
    tasks: [
      {
        id: 301,
        account_id: 4,
        source_message_id: 3001,
        title: '9/12 14:00 定例参加',
        due: '2025-09-12T05:00:00Z',
        status: 'open',
        confidence: 0.88,
        created_by: 'ai',
        created_at: '2025-09-01T07:20:00Z',
      },
    ],
    messages: [
      {
        id: 3001,
        from: { name: '総務部 情報システム課', email: 'info-sys@client-b.example' },
        initial: '総',
        time_label: '昨日 16:10',
        body: [
          {
            text:
              '関係者各位\n9月の定例会について下記のとおりご案内いたします。\n' +
              '日時: 9/12（金）14:00〜15:00\n議題: 8月の障害報告、下期の改修計画\n' +
              '資料は前日までに共有フォルダへ格納をお願いいたします。',
          },
        ],
        quoted_lines: 0,
        quoted_text: '',
        attachments: [],
        is_latest: true,
        is_read: true,
      },
    ],
    reply_placeholder: '情報システム課へ返信…',
  },

  'th-product-images': {
    thread_key: 'th-product-images',
    subject: 'Re: 商品画像の差し替えについて',
    project_tag: '案件A',
    counterpart: { name: '鈴木 花子', email: 'suzuki@client-a.example' },
    message_count: 3,
    summary: {
      target: 'thread:th-product-images',
      generated_label: '昨日 14:35 生成',
      generated_at: '2025-09-01T05:35:00Z',
      model: 'claude-opus-5',
      body: [
        { text: '差し替え用の商品画像 20 点が共有フォルダに格納済み。' },
        { text: '内容確認の依頼', strong: true },
        { text: 'が来ている。期限の指定は無い。' },
      ],
    },
    tasks: [],
    messages: [
      {
        id: 4001,
        from: { name: '鈴木 花子', email: 'suzuki@client-a.example' },
        initial: '鈴',
        time_label: '昨日 14:30',
        body: [
          {
            text:
              '田中様\nお世話になっております。鈴木です。\n' +
              '差し替え画像 20 点を共有フォルダに格納しました。ご確認をお願いいたします。',
          },
        ],
        quoted_lines: 6,
        quoted_text:
          '> 商品画像の差し替えについて、対象点数を確認させてください。\n' +
          '> 現状 20 点で認識しています。\n' +
          '> 反映のタイミングは次回リリースで問題ないでしょうか。\n' +
          '> ファイル名の規則は既存のままで統一します。\n' +
          '> サイズは長辺 1600px を想定しています。\n' +
          '> 以上、よろしくお願いいたします。',
        attachments: [],
        is_latest: true,
        is_read: true,
      },
    ],
    reply_placeholder: '鈴木さんへ返信…',
  },

  'th-gitlab-digest': {
    thread_key: 'th-gitlab-digest',
    subject: 'Weekly digest',
    project_tag: '自社',
    counterpart: { name: 'GitLab', email: 'noreply@gitlab.example' },
    message_count: 1,
    summary: null,
    tasks: [],
    messages: [
      {
        id: 5001,
        from: { name: 'GitLab', email: 'noreply@gitlab.example' },
        initial: 'G',
        time_label: '月曜 9:00',
        body: [
          {
            text:
              'Here is your weekly digest.\n' +
              '3 merge requests were merged, 2 issues were closed, 1 pipeline failed.',
          },
        ],
        quoted_lines: 0,
        quoted_text: '',
        attachments: [],
        is_latest: true,
        is_read: true,
      },
    ],
    reply_placeholder: 'GitLab へ返信…',
  },

  'th-acceptance-doc': {
    thread_key: 'th-acceptance-doc',
    subject: '8月分検収書のご送付',
    project_tag: '自社',
    counterpart: { name: '経理部 高橋', email: 'takahashi@my-company.example' },
    message_count: 1,
    summary: {
      target: 'thread:th-acceptance-doc',
      generated_label: '8/29 11:20 生成',
      generated_at: '2025-08-29T02:20:00Z',
      model: 'claude-opus-5',
      body: [
        { text: '8月分の検収書が添付されている。' },
        { text: '押印のうえ返送', strong: true },
        { text: 'が必要。期日の記載は無い。' },
      ],
    },
    tasks: [],
    messages: [
      {
        id: 6001,
        from: { name: '経理部 高橋', email: 'takahashi@my-company.example' },
        initial: '高',
        time_label: '8/29 11:00',
        body: [
          {
            text:
              '田中様\nお疲れさまです。経理部の高橋です。\n' +
              '8月分の検収書をお送りします。ご押印のうえ、ご返送をお願いいたします。',
          },
        ],
        quoted_lines: 0,
        quoted_text: '',
        attachments: [{ id: 9002, filename: '検収書_8月分.pdf', size_label: '112KB' }],
        is_latest: true,
        is_read: true,
      },
    ],
    reply_placeholder: '高橋さんへ返信…',
  },
};
