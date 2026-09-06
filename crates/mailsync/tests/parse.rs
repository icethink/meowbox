//! `mailsync::parse::parse` の統合テスト。人工の `.eml` フィクスチャで検証する。

use mailsync::parse::{attachment_bytes, parse, strip_quotes_and_signature, thread_key};

const ISO2022JP_PLAIN: &[u8] = include_bytes!("fixtures/iso2022jp-plain.eml");
const SHIFTJIS_PLAIN: &[u8] = include_bytes!("fixtures/shiftjis-plain.eml");
const UTF8_ALTERNATIVE: &[u8] = include_bytes!("fixtures/utf8-alternative.eml");
const ATTACHMENT_MIXED: &[u8] = include_bytes!("fixtures/attachment-mixed.eml");
const REPLY_MULTIPREFIX: &[u8] = include_bytes!("fixtures/reply-multiprefix.eml");
const HTML_ONLY: &[u8] = include_bytes!("fixtures/html-only.eml");

#[test]
fn parses_iso2022jp_plain() {
    let p = parse(ISO2022JP_PLAIN).expect("parse should succeed");
    assert_eq!(p.subject, "見積の件");
    assert!(p.body_text.contains("見積書をお送りいたします。"));
    let from = p.from.expect("from should be present");
    assert_eq!(from.email, "sato@client-a.example");
    assert_eq!(from.name.as_deref(), Some("佐藤 一郎"));
}

#[test]
fn parses_shiftjis_plain() {
    let p = parse(SHIFTJIS_PLAIN).expect("parse should succeed");
    assert_eq!(p.subject, "請求書の送付");
    assert!(p.body_text.contains("請求書を添付いたします。"));
}

#[test]
fn parses_utf8_alternative() {
    let p = parse(UTF8_ALTERNATIVE).expect("parse should succeed");
    assert!(p.body_text.contains("9月の定例会をご案内します。"));
    let html = p.body_html.expect("body_html should be present");
    assert!(html.contains("<p>"));
}

#[test]
fn parses_attachment_mixed() {
    let p = parse(ATTACHMENT_MIXED).expect("parse should succeed");
    assert!(p.has_attachments);
    assert_eq!(p.attachments.len(), 1);
    assert_eq!(p.attachments[0].filename, "notes.txt");
    assert_eq!(p.attachments[0].size, 14);
    assert!(p.body_text.contains("資料を添付します。"));
}

#[test]
fn attachment_bytes_returns_the_attachment_body() {
    let bytes = attachment_bytes(ATTACHMENT_MIXED, 0).expect("attachment should be found");
    assert_eq!(bytes, b"meeting notes\n");
}

#[test]
fn attachment_bytes_index_matches_parse_order() {
    let p = parse(ATTACHMENT_MIXED).expect("parse should succeed");
    for (i, meta) in p.attachments.iter().enumerate() {
        let bytes = attachment_bytes(ATTACHMENT_MIXED, i).expect("attachment should be found");
        assert_eq!(bytes.len(), meta.size);
    }
    assert_eq!(p.attachments[0].filename, "notes.txt");
}

#[test]
fn attachment_bytes_out_of_range_is_error() {
    assert!(attachment_bytes(ATTACHMENT_MIXED, 1).is_err());
}

#[test]
fn parses_reply_multiprefix_strips_quote_and_signature() {
    let p = parse(REPLY_MULTIPREFIX).expect("parse should succeed");
    assert_eq!(p.body_text.trim(), "承知しました。\n明日までに確認します。");
    assert_eq!(p.references.len(), 2);
    assert_eq!(p.references[0], "<root-001@client-a.example>");
    assert_eq!(thread_key(&p), "<root-001@client-a.example>");
}

#[test]
fn normalizes_reply_multiprefix_subject() {
    let p = parse(REPLY_MULTIPREFIX).expect("parse should succeed");
    assert_eq!(mailcore::normalize_subject(&p.subject), "定例会の件");
}

#[test]
fn parses_html_only_strips_tags_and_decodes_entities() {
    let p = parse(HTML_ONLY).expect("parse should succeed");

    let html = p.body_html.expect("body_html should be present");
    assert!(html.contains("<p>"));

    assert!(p.body_text.contains("受付時間は 9:00 & 18:00 です。"));
    assert!(p.body_text.contains("詳細は <担当> までご連絡ください。"));
    assert!(!p.body_text.contains("color: red"));
    assert!(!p.body_text.contains("var x = 1"));
    assert!(!p.body_text.contains("<p>"));
    assert!(!p.body_text.contains("</body>"));
    assert!(!p.body_text.contains("\n\n\n"));
}

#[test]
fn strip_quotes_and_signature_keeps_dash_words_and_drops_original_message() {
    assert_eq!(
        strip_quotes_and_signature("試してみます。\n--force を付けてください。"),
        "試してみます。\n--force を付けてください。"
    );
    assert_eq!(
        strip_quotes_and_signature("本文\n-----Original Message-----\n引用された内容"),
        "本文"
    );
}
