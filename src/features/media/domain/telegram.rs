use crate::common::http::error::AppError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelegramStickerSet {
    pub name: String,
    pub title: String,
    pub stickers: Vec<TelegramSticker>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelegramSticker {
    pub file_id: String,
    pub file_unique_id: String,
    pub emoji: Option<String>,
    pub is_animated: bool,
    pub is_video: bool,
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadedSticker {
    pub bytes: Vec<u8>,
    pub filename: String,
    pub content_type: String,
}

/// Parses a Telegram sticker set name from raw name or various link formats:
/// - "animals"
/// - "https://t.me/addstickers/animals"
/// - "http://t.me/addstickers/animals"
/// - "t.me/addstickers/animals"
/// - "tg://addstickers?set=animals"
pub fn parse_telegram_pack_name(input: &str) -> Result<String, AppError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(AppError::ValidationError(
            "Telegram sticker link or name cannot be empty".to_string(),
        ));
    }

    if trimmed.len() > 1024 {
        return Err(AppError::ValidationError(
            "Telegram sticker link or name cannot exceed 1024 characters".to_string(),
        ));
    }

    // Check if input looks like a URL or scheme
    let is_url_or_scheme = trimmed.contains("://")
        || trimmed.starts_with("http://")
        || trimmed.starts_with("https://")
        || trimmed.starts_with("tg:")
        || trimmed.starts_with("t.me/")
        || trimmed.starts_with("telegram.me/")
        || trimmed.starts_with("telegram.dog/");

    let raw_name = if is_url_or_scheme {
        let lower = trimmed.to_ascii_lowercase();

        // 1. Strict scheme & domain whitelist to prevent SSRF / spoofed URLs
        let is_valid_tg_scheme =
            lower.starts_with("tg://addstickers") || lower.starts_with("tg:addstickers");

        let is_valid_tg_domain = lower.starts_with("https://t.me/")
            || lower.starts_with("http://t.me/")
            || lower.starts_with("t.me/")
            || lower.starts_with("https://telegram.me/")
            || lower.starts_with("http://telegram.me/")
            || lower.starts_with("telegram.me/")
            || lower.starts_with("https://telegram.dog/")
            || lower.starts_with("http://telegram.dog/")
            || lower.starts_with("telegram.dog/")
            || lower.starts_with("https://www.t.me/")
            || lower.starts_with("http://www.t.me/")
            || lower.starts_with("www.t.me/");

        if !is_valid_tg_scheme && !is_valid_tg_domain {
            return Err(AppError::ValidationError(
                "Invalid Telegram sticker link: URL host is not an authorized Telegram domain (expected t.me, telegram.me, or tg://addstickers)".to_string(),
            ));
        }

        if let Some(idx) = trimmed.find("set=") {
            &trimmed[idx + "set=".len()..]
        } else if let Some(idx) = trimmed.find("addstickers/") {
            &trimmed[idx + "addstickers/".len()..]
        } else {
            return Err(AppError::ValidationError(
                "Invalid Telegram sticker link: URL must contain 'addstickers/<pack_name>' or 'tg://addstickers?set=<pack_name>'".to_string(),
            ));
        }
    } else {
        trimmed
    };

    // Strip any URL query parameters, parameters (&), or hash anchors (#)
    let name = raw_name
        .split('?')
        .next()
        .unwrap_or(raw_name)
        .split('&')
        .next()
        .unwrap_or(raw_name)
        .split('#')
        .next()
        .unwrap_or(raw_name)
        .trim_matches('/');

    if name.is_empty() {
        return Err(AppError::ValidationError(
            "Invalid Telegram sticker pack link: missing pack name".to_string(),
        ));
    }

    if name.len() > 64 {
        return Err(AppError::ValidationError(
            "Telegram sticker pack name cannot exceed 64 characters".to_string(),
        ));
    }

    // Telegram sticker set names must consist of alphanumeric characters and underscores
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(AppError::ValidationError(
            "Telegram sticker pack name can only contain letters, numbers, and underscores"
                .to_string(),
        ));
    }

    Ok(name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_telegram_pack_name_variants() {
        assert_eq!(parse_telegram_pack_name("animals").unwrap(), "animals");
        assert_eq!(
            parse_telegram_pack_name("https://t.me/addstickers/animals").unwrap(),
            "animals"
        );
        assert_eq!(
            parse_telegram_pack_name("http://t.me/addstickers/animals").unwrap(),
            "animals"
        );
        assert_eq!(
            parse_telegram_pack_name("t.me/addstickers/animals/").unwrap(),
            "animals"
        );
        assert_eq!(
            parse_telegram_pack_name("https://t.me/addstickers/animals?start=123").unwrap(),
            "animals"
        );
        assert_eq!(
            parse_telegram_pack_name("tg://addstickers?set=animals_pack_2").unwrap(),
            "animals_pack_2"
        );
        assert_eq!(
            parse_telegram_pack_name("tg:addstickers?set=animals_pack_3&ref=mobile").unwrap(),
            "animals_pack_3"
        );
        assert_eq!(
            parse_telegram_pack_name("https://telegram.me/addstickers/animals").unwrap(),
            "animals"
        );
        assert_eq!(
            parse_telegram_pack_name("https://telegram.dog/addstickers/animals").unwrap(),
            "animals"
        );
    }

    #[test]
    fn test_parse_telegram_pack_name_invalid() {
        assert!(parse_telegram_pack_name("").is_err());
        assert!(parse_telegram_pack_name("   ").is_err());
        assert!(parse_telegram_pack_name("https://t.me/addstickers/").is_err());
        assert!(parse_telegram_pack_name("animals!@#").is_err());
        assert!(parse_telegram_pack_name("русские_буквы").is_err());
        assert!(parse_telegram_pack_name(&"a".repeat(65)).is_err());
        assert!(parse_telegram_pack_name(&"a".repeat(1025)).is_err());
    }

    #[test]
    fn test_parse_telegram_pack_name_prevents_injections() {
        // 1. SSRF & Arbitrary Host injection
        assert!(parse_telegram_pack_name("http://169.254.169.254/addstickers/animals").is_err());
        assert!(parse_telegram_pack_name("http://127.0.0.1:8080/addstickers/animals").is_err());
        assert!(parse_telegram_pack_name("http://localhost:5432/addstickers/animals").is_err());
        assert!(parse_telegram_pack_name("https://evil.com/addstickers/animals").is_err());
        assert!(parse_telegram_pack_name("https://evil.com/t.me/addstickers/animals").is_err());
        assert!(parse_telegram_pack_name("https://attacker.org/?set=animals").is_err());

        // 2. Dangerous protocols
        assert!(parse_telegram_pack_name("file:///etc/passwd").is_err());
        assert!(parse_telegram_pack_name("javascript:alert(1)").is_err());
        assert!(parse_telegram_pack_name("gopher://127.0.0.1:6379").is_err());

        // 3. SQL Injection attempts
        assert!(parse_telegram_pack_name("animals'; DROP TABLE media_assets; --").is_err());
        assert!(parse_telegram_pack_name("' OR '1'='1").is_err());

        // 4. Path traversal attempts
        assert!(parse_telegram_pack_name("../../etc/passwd").is_err());
        assert!(parse_telegram_pack_name("animals/../secrets").is_err());

        // 5. CRLF / HTTP Header injection attempts
        assert!(parse_telegram_pack_name("animals\r\nSet-Cookie: evil=1").is_err());
        assert!(parse_telegram_pack_name("animals\0nullbyte").is_err());

        // 6. Non-addstickers Telegram links (channels, users)
        assert!(parse_telegram_pack_name("https://t.me/animals").is_err());
        assert!(parse_telegram_pack_name("https://t.me/joinchat/something").is_err());
    }
}
