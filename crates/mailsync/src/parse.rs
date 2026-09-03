//! RFC822 raw → 保存用データへの変換。
//!
//! TODO(P0): `mail-parser` を使って実装する。
//! - 日本語エンコーディング（ISO-2022-JP / Shift_JIS / EUC-JP）は mail-parser が面倒を見る。
//! - `body_text` は text/plain を優先し、無ければ HTML からタグを剥がす。
//! - 引用（`>` 行、"On ... wrote:" / "----- Original Message -----" 以降）と
//!   署名（`-- ` 以降）を落とした版を要約用に持つ。
//! - `thread_key` は References の先頭 → In-Reply-To → 正規化件名の順で決める。

use chrono::{DateTime, Utc};
use mailcore::Address;

/// パース結果。`mailstore::NewMessage` に詰め替えて保存する。
#[derive(Debug, Clone, Default)]
pub struct Parsed {
    pub message_id: Option<String>,
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,
    pub from: Option<Address>,
    pub to: Vec<Address>,
    pub cc: Vec<Address>,
    pub subject: String,
    pub date: Option<DateTime<Utc>>,
    pub body_text: String,
    pub body_html: Option<String>,
    pub has_attachments: bool,
}

/// スレッドキーを決める。References → In-Reply-To → 正規化件名。
pub fn thread_key(p: &Parsed) -> String {
    if let Some(first) = p.references.first() {
        return first.clone();
    }
    if let Some(irt) = &p.in_reply_to {
        return irt.clone();
    }
    format!("subj:{}", mailcore::normalize_subject(&p.subject))
}

/// 先頭 N 文字を snippet にする（改行・連続空白を潰す）。
pub fn snippet(body_text: &str, max_chars: usize) -> String {
    let collapsed: String = body_text.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed.chars().take(max_chars).collect()
}

pub fn parse(_raw: &[u8]) -> anyhow::Result<Parsed> {
    anyhow::bail!("mailsync::parse::parse is not implemented yet (P0) — use mail-parser")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thread_key_prefers_references() {
        let p = Parsed {
            references: vec!["<root@x>".into(), "<child@x>".into()],
            in_reply_to: Some("<child@x>".into()),
            subject: "Re: hi".into(),
            ..Default::default()
        };
        assert_eq!(thread_key(&p), "<root@x>");
        let p2 = Parsed {
            subject: "Re: hi".into(),
            ..Default::default()
        };
        assert_eq!(thread_key(&p2), "subj:hi");
    }

    #[test]
    fn snippet_collapses_whitespace() {
        assert_eq!(snippet("a\n\n  b   c", 10), "a b c");
        assert_eq!(snippet("あいうえお", 3), "あいう");
    }
}
