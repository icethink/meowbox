//! RFC822 raw → 保存用データへの変換。
//!
//! `mail-parser` で RFC822/MIME をパースする。日本語エンコーディング
//! （ISO-2022-JP / Shift_JIS / EUC-JP）は mail-parser が面倒を見る。
//! `body_text` は text/plain を優先し、無ければ HTML からタグを剥がす。
//! そのうえで引用（`>` 行、"On ... wrote:" / "----- Original Message -----" /
//! "----- 元のメッセージ -----" 以降）と署名（`-- ` 以降）を落とした版を要約用に持つ。
//! `thread_key` は References の先頭 → In-Reply-To → 正規化件名の順で決める。

use chrono::{DateTime, Utc};
use mail_parser::{Addr as MpAddr, Address as MpAddress, HeaderValue, MessageParser, MimeHeaders};
use mailcore::Address;

/// 添付ファイルのメタデータ（本体は raw `.eml` の中に残す。P0-b では path は持たない）。
#[derive(Debug, Clone, PartialEq)]
pub struct AttachmentMeta {
    pub filename: String,
    pub mime: String,
    pub size: usize,
}

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
    pub attachments: Vec<AttachmentMeta>,
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

/// mail-parser の `parse_id()` は `<` `>` を剥がして返す。DB / スレッドキーの
/// 表現は Message-ID 形式（`<...>`）で統一するので、ここで付け直す。
fn wrap_id(id: &str) -> String {
    format!("<{id}>")
}

fn header_id_list(hv: &HeaderValue<'_>) -> Vec<String> {
    match hv {
        HeaderValue::Text(id) => vec![wrap_id(id)],
        HeaderValue::TextList(ids) => ids.iter().map(|id| wrap_id(id)).collect(),
        _ => Vec::new(),
    }
}

fn header_first_id(hv: &HeaderValue<'_>) -> Option<String> {
    header_id_list(hv).into_iter().next()
}

fn to_address(a: &MpAddr<'_>) -> Option<Address> {
    let email = a.address.as_ref()?.to_string();
    Some(Address {
        name: a.name.as_ref().map(|n| n.to_string()),
        email,
    })
}

fn addr_list(addr: Option<&MpAddress<'_>>) -> Vec<Address> {
    match addr {
        Some(addr) => addr.iter().filter_map(to_address).collect(),
        None => Vec::new(),
    }
}

/// RFC822 raw バイト列をパースする。パースできなければ `bail!`。
pub fn parse(raw: &[u8]) -> anyhow::Result<Parsed> {
    let msg = MessageParser::default()
        .parse(raw)
        .ok_or_else(|| anyhow::anyhow!("failed to parse RFC822 message"))?;

    let message_id = msg.message_id().map(wrap_id);
    let in_reply_to = header_first_id(msg.in_reply_to());
    let references = header_id_list(msg.references());

    let from = msg.from().and_then(|a| a.first()).and_then(to_address);
    let to = addr_list(msg.to());
    let cc = addr_list(msg.cc());

    let subject = msg.subject().unwrap_or_default().to_string();
    let date = msg
        .date()
        .and_then(|d| DateTime::<Utc>::from_timestamp(d.to_timestamp(), 0));

    let body_html = msg.body_html(0).map(|c| c.into_owned());

    let body_text_raw = if msg.text_body_count() > 0 {
        msg.body_text(0).map(|c| c.into_owned()).unwrap_or_default()
    } else if let Some(html) = &body_html {
        html_to_plain_text(html)
    } else {
        String::new()
    };
    let body_text = strip_quotes_and_signature(&body_text_raw);

    let attachments: Vec<AttachmentMeta> = attachment_parts(&msg)
        .into_iter()
        .map(|part| {
            let filename = part.attachment_name().unwrap_or_default().to_string();
            let mime = part
                .content_type()
                .map(|ct| match &ct.c_subtype {
                    Some(sub) => format!("{}/{}", ct.c_type, sub),
                    None => ct.c_type.to_string(),
                })
                .unwrap_or_else(|| "application/octet-stream".to_string());
            let size = part.contents().len();
            AttachmentMeta {
                filename,
                mime,
                size,
            }
        })
        .collect();
    let has_attachments = !attachments.is_empty();

    Ok(Parsed {
        message_id,
        in_reply_to,
        references,
        from,
        to,
        cc,
        subject,
        date,
        body_text,
        body_html,
        attachments,
        has_attachments,
    })
}

/// `parse()` の `attachments` 一覧と `attachment_bytes` の index がずれないよう、
/// 「添付として数える」条件（`attachment_name()` が `Some`）をここに一本化する。
fn attachment_parts<'a>(
    msg: &'a mail_parser::Message<'a>,
) -> Vec<&'a mail_parser::MessagePart<'a>> {
    msg.attachments()
        .filter(|part| part.attachment_name().is_some())
        .collect()
}

/// raw .eml から `index` 番目の添付の中身を取り出す。
/// `index` は `parse()` が返す `Parsed.attachments` の並びと同じ。
/// 範囲外なら `Err`。
pub fn attachment_bytes(raw: &[u8], index: usize) -> anyhow::Result<Vec<u8>> {
    let msg = MessageParser::default()
        .parse(raw)
        .ok_or_else(|| anyhow::anyhow!("failed to parse RFC822 message"))?;

    attachment_parts(&msg)
        .get(index)
        .map(|part| part.contents().to_vec())
        .ok_or_else(|| anyhow::anyhow!("attachment index {index} out of range"))
}

/// 簡易 HTML → テキスト変換。`text/plain` パートが無いときだけ使う。
/// `<script>` / `<style>` の中身は捨て、タグを除去し、`&amp; &lt; &gt; &quot; &nbsp;`
/// だけ実体参照を戻し、3 行以上の連続空行は 1 行に潰す。
fn html_to_plain_text(html: &str) -> String {
    let lower = html.to_ascii_lowercase();
    let mut result = String::with_capacity(html.len());
    let mut i = 0usize;

    while i < html.len() {
        if lower[i..].starts_with("<script") || lower[i..].starts_with("<style") {
            let close_tag = if lower[i..].starts_with("<script") {
                "</script>"
            } else {
                "</style>"
            };
            match lower[i..].find(close_tag) {
                Some(rel) => i += rel + close_tag.len(),
                None => break,
            }
            continue;
        }
        if html.as_bytes()[i] == b'<' {
            match html[i..].find('>') {
                Some(rel) => i += rel + 1,
                None => break,
            }
            continue;
        }
        let Some(ch) = html[i..].chars().next() else {
            break;
        };
        result.push(ch);
        i += ch.len_utf8();
    }

    let decoded = result
        .replace("&nbsp;", " ")
        .replace("&quot;", "\"")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&");

    collapse_blank_lines(&decoded)
}

/// 3 行以上連続する空行を 1 行に潰す。
fn collapse_blank_lines(text: &str) -> String {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut out: Vec<&str> = Vec::with_capacity(lines.len());
    let mut i = 0usize;
    while i < lines.len() {
        if lines[i].trim().is_empty() {
            let start = i;
            while i < lines.len() && lines[i].trim().is_empty() {
                i += 1;
            }
            if i - start >= 3 {
                out.push(lines[start]);
            } else {
                out.extend_from_slice(&lines[start..i]);
            }
        } else {
            out.push(lines[i]);
            i += 1;
        }
    }
    out.join("\n")
}

fn is_quote_line(line: &str) -> bool {
    line.trim_start().starts_with('>')
}

fn is_reply_header_line(line: &str) -> bool {
    line.starts_with("On ") && line.trim_end().ends_with(" wrote:")
}

/// `s` の先頭が 3 個以上のハイフンなら、その後ろを返す。
fn strip_leading_dashes(s: &str) -> Option<&str> {
    let dash_count = s.chars().take_while(|&c| c == '-').count();
    if dash_count >= 3 {
        Some(&s[dash_count..])
    } else {
        None
    }
}

/// `-----Original Message-----` / `----- 元のメッセージ -----` を
/// 前後の空白・ハイフン数を問わず判定する。
fn is_original_message_marker(line: &str) -> bool {
    ["Original Message", "元のメッセージ"]
        .iter()
        .any(|label| matches_dashed_marker(line, label))
}

fn matches_dashed_marker(line: &str, label: &str) -> bool {
    let trimmed = line.trim();
    let Some(rest) = strip_leading_dashes(trimmed) else {
        return false;
    };
    let Some(rest) = rest.trim_start().strip_prefix(label) else {
        return false;
    };
    strip_leading_dashes(rest.trim_start()).is_some_and(str::is_empty)
}

/// 署名区切り行（`--` または `-- `。ハイフン 2 つのみ、末尾空白可）。
fn is_signature_marker(line: &str) -> bool {
    line.trim_end() == "--"
}

/// 本文を「要約向けの本体」と「落とした引用・署名」に分けた結果。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BodySplit {
    /// 引用・署名を落とした要約向けテキスト。
    pub body: String,
    /// 落とした行をそのまま連結したもの。UI の「引用 N 行を表示」で使う。
    pub quoted: String,
}

/// 各行の前後にある空行（空白のみの行）だけを落として `\n` で連結する。
/// 残った行はインデントも含めてそのまま保つ。
fn join_trimming_blank_lines(lines: &[&str]) -> String {
    let start = lines.iter().position(|l| !l.trim().is_empty());
    let end = lines.iter().rposition(|l| !l.trim().is_empty());
    match (start, end) {
        (Some(s), Some(e)) => lines[s..=e].join("\n"),
        _ => String::new(),
    }
}

/// 引用（`>` 行、"On ... wrote:"、"----- Original Message -----"、
/// "----- 元のメッセージ -----" 以降）と署名（`--` 行以降）を本体から切り離す。
/// 判定ルールは `strip_quotes_and_signature`（本関数の `body` を返すだけの
/// 薄いラッパ）と同一。マーカー行に当たったら、その行自身を含めて以降すべてを
/// `quoted` に入れる。
pub fn split_quotes_and_signature(text: &str) -> BodySplit {
    let mut body_lines: Vec<&str> = Vec::new();
    let mut quoted_lines: Vec<&str> = Vec::new();
    let mut in_quoted = false;

    for line in text.lines() {
        if !in_quoted
            && (is_signature_marker(line)
                || is_original_message_marker(line)
                || is_reply_header_line(line))
        {
            in_quoted = true;
        }

        if in_quoted || is_quote_line(line) {
            quoted_lines.push(line);
        } else {
            body_lines.push(line);
        }
    }

    BodySplit {
        body: body_lines.join("\n").trim().to_string(),
        quoted: join_trimming_blank_lines(&quoted_lines),
    }
}

/// 引用（`>` 行、"On ... wrote:"、"----- Original Message -----"、
/// "----- 元のメッセージ -----" 以降）と署名（`--` 行以降）を落とす。
pub fn strip_quotes_and_signature(text: &str) -> String {
    split_quotes_and_signature(text).body
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

    #[test]
    fn strip_quotes_and_signature_keeps_short_dash_words() {
        let text = "試してください。\n--force で強制上書きできます。\n続きの文章。";
        assert_eq!(strip_quotes_and_signature(text), text);
    }

    #[test]
    fn strip_quotes_and_signature_drops_original_message_marker() {
        let text = "承知しました。\n-----Original Message-----\nFrom: someone\n本文";
        assert_eq!(strip_quotes_and_signature(text), "承知しました。");
    }

    #[test]
    fn strip_quotes_and_signature_drops_signature() {
        let text = "本文です。\n-- \n田中 太郎";
        assert_eq!(strip_quotes_and_signature(text), "本文です。");
    }

    #[test]
    fn strip_quotes_and_signature_drops_quote_lines() {
        let text = "返信です。\n> 元のメールの行\n  > インデントされた引用";
        assert_eq!(strip_quotes_and_signature(text), "返信です。");
    }

    #[test]
    fn split_keeps_quoted_lines() {
        let text = "返信です。\n> 元のメールの行\n  > インデントされた引用";
        let split = split_quotes_and_signature(text);
        assert_eq!(split.body, "返信です。");
        assert_eq!(split.quoted, "> 元のメールの行\n  > インデントされた引用");
    }

    #[test]
    fn split_keeps_everything_after_the_original_message_marker() {
        let text = "承知しました。\n-----Original Message-----\nFrom: someone\n本文";
        let split = split_quotes_and_signature(text);
        assert_eq!(split.body, "承知しました。");
        assert_eq!(
            split.quoted,
            "-----Original Message-----\nFrom: someone\n本文"
        );
    }

    #[test]
    fn split_keeps_the_signature() {
        let text = "本文です。\n-- \n田中 太郎";
        let split = split_quotes_and_signature(text);
        assert_eq!(split.body, "本文です。");
        assert_eq!(split.quoted, "-- \n田中 太郎");
    }

    #[test]
    fn split_quoted_line_count_matches() {
        let text = "返信です。\n> 一行目\n> 二行目\n> 三行目";
        let split = split_quotes_and_signature(text);
        assert_eq!(split.quoted.lines().count(), 3);
    }

    #[test]
    fn split_without_quotes_returns_empty_quoted() {
        let text = "普通の本文です。\n二行目です。";
        let split = split_quotes_and_signature(text);
        assert_eq!(split.body, text);
        assert_eq!(split.quoted, "");
    }
}
