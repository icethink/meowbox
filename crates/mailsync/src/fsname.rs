//! ファイルシステムのパス 1 セグメントとして安全な名前に直す。
//! IMAP のフォルダ名も添付のファイル名も信頼境界の外から来るので、
//! ディスクに書く前に必ずここを通す。

/// フォルダ名・ファイル名をファイルパスの 1 セグメントとして安全に使える形にする。
/// `/ \ : * ? " < > |` と制御文字を `_` に置換する。パス区切りや予約文字を
/// 含まない名前（日本語含む）はそのまま通す。
///
/// 名前は IMAP サーバや `.eml` 由来で信頼境界の外にある。置換後の結果が
/// `"."` / `".."`（カレント/親ディレクトリ）や空文字列になる場合、そのまま
/// パスセグメントとして使うと親ディレクトリの外に書き込めてしまうため、
/// 安全な別名に潰す。
pub fn sanitize_path_segment(name: &str) -> String {
    let replaced: String = name
        .chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();

    match replaced.as_str() {
        "" => "_".to_string(),
        "." => "_".to_string(),
        ".." => "__".to_string(),
        _ => replaced,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_path_segment_keeps_safe_names() {
        assert_eq!(sanitize_path_segment("INBOX.送信済み"), "INBOX.送信済み");
    }

    #[test]
    fn sanitize_path_segment_replaces_reserved_characters() {
        assert_eq!(sanitize_path_segment("INBOX/Sent"), "INBOX_Sent");
        assert_eq!(
            sanitize_path_segment("a\\b:c*d?e\"f<g>h|i"),
            "a_b_c_d_e_f_g_h_i"
        );
    }

    #[test]
    fn sanitize_path_segment_rejects_dot_only_names() {
        assert_ne!(sanitize_path_segment(".."), "..");
        assert_ne!(sanitize_path_segment("."), ".");
        assert_ne!(sanitize_path_segment(""), "");
    }

    #[test]
    fn sanitize_path_segment_replaces_empty_name() {
        assert_eq!(sanitize_path_segment(""), "_");
    }

    #[test]
    fn sanitize_path_segment_replaces_single_dot() {
        assert_eq!(sanitize_path_segment("."), "_");
    }

    #[test]
    fn sanitize_path_segment_replaces_double_dot() {
        assert_eq!(sanitize_path_segment(".."), "__");
    }

    #[test]
    fn sanitize_path_segment_replaces_path_traversal_attempt() {
        assert_eq!(sanitize_path_segment("../.."), ".._..");
    }

    #[test]
    fn sanitize_path_segment_keeps_japanese_folder_name() {
        assert_eq!(sanitize_path_segment("案件A"), "案件A");
    }

    #[test]
    fn sanitize_path_segment_replaces_slash_in_attachment_name() {
        assert_eq!(sanitize_path_segment("a/b.txt"), "a_b.txt");
    }
}
