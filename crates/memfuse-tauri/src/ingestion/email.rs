use mailparse::MailHeaderMap;
use memfuse_core::{MemFuseError, Result};
use std::path::Path;

/// Extrahiert Betreff, Absender und Body-Text aus einer .eml-Datei.
pub struct EmailContent {
    pub subject: String,
    pub from: String,
    pub body: String,
}

/// Extrahiert Betreff, Absender und Body-Text aus .eml-Bytes (panikgeschützt).
pub fn extract_email_bytes(bytes: &[u8]) -> Result<EmailContent> {
    if bytes.is_empty() {
        return Ok(EmailContent {
            subject: String::new(),
            from: String::new(),
            body: String::new(),
        });
    }
    std::panic::catch_unwind(|| {
        let parsed = mailparse::parse_mail(bytes)
            .map_err(|e| MemFuseError::Internal(format!("Failed to parse email: {e}")))?;

        let subject = parsed
            .headers
            .get_first_value("Subject")
            .unwrap_or_default();
        let from = parsed.headers.get_first_value("From").unwrap_or_default();
        let body = parsed.get_body().unwrap_or_default();

        Ok(EmailContent {
            subject,
            from,
            body,
        })
    })
    .map_err(|_| MemFuseError::Internal("EML extraction panicked on malformed file".into()))?
}

/// Extrahiert Betreff, Absender und Body-Text aus einer .eml-Datei (panikgeschützt via spawn_blocking + catch_unwind).
pub async fn extract_email(path: &Path) -> Result<EmailContent> {
    let path_buf = path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let bytes = std::fs::read(&path_buf).map_err(|e| {
            MemFuseError::Internal(format!("Failed to read email file {:?}: {e}", path_buf))
        })?;
        extract_email_bytes(&bytes)
    })
    .await
    .map_err(|e| MemFuseError::Internal(format!("EML extraction task panicked: {e:?}")))?
}
