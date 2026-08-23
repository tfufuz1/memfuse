use mailparse::MailHeaderMap;
use memfuse_core::Result;
use std::path::Path;

/// Extrahiert Betreff, Absender und Body-Text aus einer .eml-Datei.
pub struct EmailContent {
    pub subject: String,
    pub from: String,
    pub body: String,
}

pub fn extract_email(path: &Path) -> Result<EmailContent> {
    let raw = std::fs::read(path).map_err(|e| {
        memfuse_core::MemFuseError::Internal(format!(
            "E-Mail lesen fehlgeschlagen für {:?}: {e}",
            path
        ))
    })?;

    let parsed = mailparse::parse_mail(&raw).map_err(|e| {
        memfuse_core::MemFuseError::Internal(format!("E-Mail parsen fehlgeschlagen: {e}"))
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
}
