//! UI に返すエラー。`{ code, message }` の JSON になる。
//!
//! **message に秘密情報を入れないこと**（パスワード・トークン）。
//! IMAP の認証失敗はサーバの応答をそのまま返さず、固定の日本語メッセージにする。

#[derive(Debug, Clone, serde::Serialize)]
pub struct AppError {
    /// UI が分岐に使う識別子。"not_found" | "invalid_input" | "conflict" | "auth" | "network" | "internal"
    pub code: String,
    /// 人間に見せる日本語メッセージ。
    pub message: String,
}

impl AppError {
    pub fn not_found(message: impl Into<String>) -> Self {
        Self {
            code: "not_found".into(),
            message: message.into(),
        }
    }

    pub fn invalid_input(message: impl Into<String>) -> Self {
        Self {
            code: "invalid_input".into(),
            message: message.into(),
        }
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self {
            code: "conflict".into(),
            message: message.into(),
        }
    }

    pub fn auth(message: impl Into<String>) -> Self {
        Self {
            code: "auth".into(),
            message: message.into(),
        }
    }

    pub fn network(message: impl Into<String>) -> Self {
        Self {
            code: "network".into(),
            message: message.into(),
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            code: "internal".into(),
            message: message.into(),
        }
    }

    /// `mailcore::BackendError` からの変換。
    pub fn from_backend(e: mailcore::BackendError) -> Self {
        match e {
            // サーバの認証失敗応答をそのまま返すと、パスワードや資格情報の断片が
            // メッセージに混ざる可能性がある。固定文言に落として UI に渡す。
            mailcore::BackendError::Auth(_) => {
                AppError::auth("認証に失敗しました。ユーザー名とパスワードを確認してください。")
            }
            mailcore::BackendError::Network(msg) => {
                AppError::network(format!("接続できませんでした: {msg}"))
            }
            other => AppError::internal(other.to_string()),
        }
    }
}

impl From<anyhow::Error> for AppError {
    fn from(e: anyhow::Error) -> Self {
        AppError::internal(e.to_string())
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for AppError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_backend_hides_the_raw_auth_error_message() {
        let leaked = "password=hunter2 rejected";
        let err = AppError::from_backend(mailcore::BackendError::Auth(leaked.to_string()));
        assert_eq!(err.code, "auth");
        assert!(!err.message.contains(leaked));
        assert!(!err.message.contains("hunter2"));
    }

    #[test]
    fn from_backend_maps_network_errors_with_detail() {
        let err = AppError::from_backend(mailcore::BackendError::Network("timed out".into()));
        assert_eq!(err.code, "network");
        assert!(err.message.contains("timed out"));
    }

    #[test]
    fn from_backend_maps_everything_else_to_internal() {
        let err = AppError::from_backend(mailcore::BackendError::Protocol("bad response".into()));
        assert_eq!(err.code, "internal");
    }
}
