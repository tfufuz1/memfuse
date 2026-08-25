use mailparse::MailHeaderMap;
use memfuse_core::{MemFuseError, Result};
use std::path::Path;

/// Extrahiert Betreff, Absender und Body-Text aus einer .eml-Datei.
pub struct EmailContent {
    pub subject: String,
    pub from: String,
    pub body: String,
}

/// Extrahiert Betreff, Absender und Body-Text aus einer .eml-Datei (panikgeschützt via spawn_blocking + catch_unwind).
pub async fn extract_email(path: &Path) -> Result<EmailContent> {
    let path_buf = path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        std::panic::catch_unwind(|| {
            let raw = std::fs::read(&path_buf).map_err(|e| {
                MemFuseError::Internal(format!(
                    "E-Mail lesen fehlgeschlagen für {:?}: {e}",
                    path_buf
                ))
            })?;

            let parsed = mailparse::parse_mail(&raw).map_err(|e| {
                MemFuseError::Internal(format!("E-Mail parsen fehlgeschlagen: {e}"))
            })?;

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
    })
    .await
    .map_err(|e| MemFuseError::Internal(format!("EML extraction task panicked: {e:?}")))?
    .map_err(|_| MemFuseError::Internal("EML extraction panicked on malformed file".into()))?
}
