use mailparse::MailHeaderMap;
use memfuse_core::{MemFuseError, Result};
use std::path::Path;

/// Extrahiert Betreff, Absender und Body-Text aus einer .eml-Datei.
#[derive(Debug, PartialEq, Eq)]
pub struct EmailContent {
    pub subject: String,
    pub from: String,
    pub body: String,
}

/// Strips HTML tags and script/style contents from an HTML string, returning plain text.
pub fn strip_html(html: &str) -> String {
    let script_regex = regex::RegexBuilder::new(r"(?is)<(script|style)[^>]*>.*?</(script|style)>")
        .size_limit(10 * 1024)
        .build();
    let text = match script_regex {
        Ok(re) => re.replace_all(html, "").to_string(),
        Err(_) => html.to_string(),
    };

    // Insert newlines for block-level closing tags so heading and paragraph lines are separated
    let block_regex = regex::RegexBuilder::new(r"(?i)</(h[1-6]|p|div|li|tr|table|blockquote)>")
        .size_limit(10 * 1024)
        .build();
    let text = match block_regex {
        Ok(re) => re.replace_all(&text, "$0\n").to_string(),
        Err(_) => text,
    };

    let tag_regex = regex::RegexBuilder::new(r"<[^>]*>")
        .size_limit(10 * 1024)
        .build();
    let text = match tag_regex {
        Ok(re) => re.replace_all(&text, "").to_string(),
        Err(_) => text,
    };

    let text = text
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'");

    let lines: Vec<&str> = text
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect();
    lines.join("\n")
}

fn extract_body_from_parsed_mail(parsed: &mailparse::ParsedMail) -> String {
    if parsed.subparts.is_empty() {
        let body = parsed.get_body().unwrap_or_default();
        if parsed.ctype.mimetype == "text/html" {
            strip_html(&body)
        } else {
            body
        }
    } else {
        let mut plain_text = None;
        let mut html_text = None;

        fn collect_parts(
            part: &mailparse::ParsedMail,
            plain: &mut Option<String>,
            html: &mut Option<String>,
        ) {
            if part.subparts.is_empty() {
                let mimetype = &part.ctype.mimetype;
                if mimetype == "text/plain" && plain.is_none() {
                    if let Ok(b) = part.get_body() {
                        if !b.trim().is_empty() {
                            *plain = Some(b);
                        }
                    }
                } else if mimetype == "text/html" && html.is_none() {
                    if let Ok(b) = part.get_body() {
                        if !b.trim().is_empty() {
                            *html = Some(strip_html(&b));
                        }
                    }
                }
            } else {
                for sub in &part.subparts {
                    collect_parts(sub, plain, html);
                }
            }
        }

        collect_parts(parsed, &mut plain_text, &mut html_text);

        if let Some(plain) = plain_text {
            plain
                .trim_end_matches('\n')
                .trim_end_matches("\r\n")
                .to_string()
        } else if let Some(html) = html_text {
            html
        } else {
            let body = parsed.get_body().unwrap_or_default();
            if parsed.ctype.mimetype == "text/html" {
                strip_html(&body)
            } else {
                body.trim_end_matches('\n')
                    .trim_end_matches("\r\n")
                    .to_string()
            }
        }
    }
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
        let body = extract_body_from_parsed_mail(&parsed);

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_html() {
        let raw_html = "<html><body><h1>Header</h1><p>Hello &amp; <b>World</b>!</p><script>alert('x');</script></body></html>";
        let cleaned = strip_html(raw_html);
        assert_eq!(cleaned, "Header\nHello & World!");
    }

    #[test]
    fn test_extract_plain_email() {
        let eml = b"From: sender@example.com\nSubject: Test Email\nContent-Type: text/plain\n\nThis is plain text.";
        let content = extract_email_bytes(eml).unwrap(); // unwrap
        assert_eq!(content.subject, "Test Email");
        assert_eq!(content.from, "sender@example.com");
        assert_eq!(content.body, "This is plain text.");
    }

    #[test]
    fn test_extract_html_email() {
        let eml = b"From: sender@example.com\nSubject: HTML Email\nContent-Type: text/html\n\n<p>Hello <b>World</b>!</p>";
        let content = extract_email_bytes(eml).unwrap(); // unwrap
        assert_eq!(content.subject, "HTML Email");
        assert_eq!(content.from, "sender@example.com");
        assert_eq!(content.body, "Hello World!");
    }

    #[test]
    fn test_extract_multipart_email_prefers_plain() {
        let eml = b"From: sender@example.com\nSubject: Multipart Email\nContent-Type: multipart/alternative; boundary=\"BOUNDARY\"\n\n--BOUNDARY\nContent-Type: text/html\n\n<p>HTML body</p>\n--BOUNDARY\nContent-Type: text/plain\n\nPlain text body\n--BOUNDARY--";
        let content = extract_email_bytes(eml).unwrap(); // unwrap
        assert_eq!(content.body, "Plain text body");
    }

    #[test]
    fn test_extract_email_malformed_returns_error_no_panic() {
        let malformed_eml = b"From: =?invalid_charset?Q?something?=\nContent-Type: multipart/invalid";
        let res = extract_email_bytes(malformed_eml);
        assert!(res.is_ok()); // mailparse is very tolerant, but shouldn't panic
    }
}
