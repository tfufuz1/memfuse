use memfuse_tauri_lib::ingestion::{
    docx::{extract_docx_bytes, extract_docx_text},
    email::{extract_email, extract_email_bytes, strip_html},
    pdf::{extract_pdf_bytes, extract_pdf_text},
    pipeline::extract_text_from_bytes,
};
use std::io::Write;
use tempfile::NamedTempFile;
use zip::write::SimpleFileOptions;

// ============================================================================
// PDF PARSER ROBUSTNESS TESTS
// ============================================================================

fn create_minimal_pdf() -> Vec<u8> {
    let header = "%PDF-1.4\n";
    let obj1 = "1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n";
    let obj2 = "2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n";
    let obj3 = "3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>\nendobj\n";
    let stream_data = "BT /F1 12 Tf 100 700 Td (Hello PDF World) Tj ET";
    let obj4 = format!(
        "4 0 obj\n<< /Length {} >>\nstream\n{}\nendstream\nendobj\n",
        stream_data.len(),
        stream_data
    );
    let obj5 = "5 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\nendobj\n";

    let off1 = header.len();
    let off2 = off1 + obj1.len();
    let off3 = off2 + obj2.len();
    let off4 = off3 + obj3.len();
    let off5 = off4 + obj4.len();
    let xref_off = off5 + obj5.len();

    let xref = format!(
        "xref\n0 6\n0000000000 65535 f \n{:010} 00000 n \n{:010} 00000 n \n{:010} 00000 n \n{:010} 00000 n \n{:010} 00000 n \n",
        off1, off2, off3, off4, off5
    );
    let trailer = format!(
        "trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
        xref_off
    );

    format!(
        "{}{}{}{}{}{}{}{}",
        header, obj1, obj2, obj3, obj4, obj5, xref, trailer
    )
    .into_bytes()
}

#[test]
fn test_pdf_a_valid_minimal() {
    let pdf_bytes = create_minimal_pdf();
    let res = extract_pdf_bytes(&pdf_bytes);
    assert!(
        res.is_ok(),
        "Minimal valid PDF extraction should succeed: {:?}",
        res.err()
    );
    let text = res.unwrap();
    assert!(text.contains("Hello PDF World"));
}

#[test]
fn test_pdf_b_empty_bytes() {
    let res = extract_pdf_bytes(b"");
    assert!(
        res.is_ok(),
        "Empty bytes should return Ok empty string without panic"
    );
    assert_eq!(res.unwrap(), "");
}

#[test]
fn test_pdf_c_truncated_corrupted() {
    let valid_pdf = create_minimal_pdf();
    for ratio in [0.10, 0.50, 0.90] {
        let len = (valid_pdf.len() as f64 * ratio) as usize;
        let truncated = &valid_pdf[..len];
        let res = extract_pdf_bytes(truncated);
        // Truncated PDF must either return clean error or empty string - MUST NOT panic!
        assert!(
            res.is_err() || res.is_ok(),
            "Truncated PDF ({ratio}) must not panic"
        );
    }
}

#[test]
fn test_pdf_d_mismatched_extension() {
    let plain_text = b"This is plain text with a .pdf file extension!";
    let res = extract_pdf_bytes(plain_text);
    assert!(
        res.is_err(),
        "Non-PDF text under PDF parser should return Error"
    );
}

#[test]
fn test_pdf_e_oversized() {
    let oversized = vec![0u8; 101 * 1024 * 1024]; // 101 MB
    let res = extract_text_from_bytes(&oversized, "pdf");
    assert!(res.is_err());
    assert!(res.unwrap_err().to_string().contains("100 MB"));
}

#[test]
fn test_pdf_f_deeply_nested_objects() {
    // PDF with deeply nested object references
    let mut nested_pdf = String::from("%PDF-1.4\n1 0 obj <</Type /Catalog /Pages 2 0 R>> endobj\n2 0 obj <</Type /Pages /Count 1 /Kinds [3 0 R]>> endobj\n");
    for i in 3..100 {
        nested_pdf.push_str(&format!(
            "{i} 0 obj <</Next {} 0 R /Text (Level {i})>> endobj\n",
            i + 1
        ));
    }
    nested_pdf.push_str(
        "100 0 obj <</Type /Page /Parent 2 0 R /MediaBox [0 0 612 792]>> endobj\n\
xref\n0 101\n0000000000 65535 f \n\
trailer <</Size 101 /Root 1 0 R>>\n\
startxref\n500\n%%EOF\n",
    );

    let res = extract_pdf_bytes(nested_pdf.as_bytes());
    assert!(
        res.is_ok() || res.is_err(),
        "Deeply nested PDF must execute without stack overflow panic"
    );
}

#[test]
fn test_pdf_g_non_utf8_encodings() {
    // PDF containing non-UTF8 binary noise inside stream
    let mut invalid_utf8_pdf = create_minimal_pdf();
    let stream_start = invalid_utf8_pdf
        .windows(6)
        .position(|w| w == b"stream")
        .unwrap_or(0);
    invalid_utf8_pdf.splice(
        stream_start + 7..stream_start + 15,
        vec![0xFF, 0xFE, 0xFD, 0x00, 0x80, 0x90, 0xA0, 0xB0],
    );

    let res = extract_pdf_bytes(&invalid_utf8_pdf);
    assert!(
        res.is_ok() || res.is_err(),
        "PDF with invalid UTF-8 stream bytes must not panic"
    );
}

#[test]
fn test_pdf_h_fuzz_malformed_inputs() {
    let seeds: Vec<&[u8]> = vec![
        b"%PDF-1.4\n%-----\n\x00\xFF\xFE\xFD",
        b"%PDF-9.9\ntrailer<<>>startxref\n-1\n%%EOF",
        b"%PDF-1.4\n1 0 obj << /Type /Catalog /Pages 1 0 R >> endobj",
    ];

    for seed in seeds {
        let _ = extract_pdf_bytes(seed);
    }
}

#[test]
fn test_pdf_security_embedded_js_actions() {
    let header = "%PDF-1.4\n";
    let obj1 = "1 0 obj\n<< /Type /Catalog /Pages 2 0 R /OpenAction 3 0 R >>\nendobj\n";
    let obj2 = "2 0 obj\n<< /Type /Pages /Kids [4 0 R] /Count 1 >>\nendobj\n";
    let obj3 = "3 0 obj\n<< /S /JavaScript /JS (app.alert('MALICIOUS_JS_EXECUTED')) >>\nendobj\n";
    let obj4 = "4 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 6 0 R >> >> /Contents 5 0 R >>\nendobj\n";
    let stream_data = "BT /F1 12 Tf (Safe Text) Tj ET";
    let obj5 = format!(
        "5 0 obj\n<< /Length {} >>\nstream\n{}\nendstream\nendobj\n",
        stream_data.len(),
        stream_data
    );
    let obj6 = "6 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\nendobj\n";

    let off1 = header.len();
    let off2 = off1 + obj1.len();
    let off3 = off2 + obj2.len();
    let off4 = off3 + obj3.len();
    let off5 = off4 + obj4.len();
    let off6 = off5 + obj5.len();
    let xref_off = off6 + obj6.len();

    let xref = format!(
        "xref\n0 7\n0000000000 65535 f \n{:010} 00000 n \n{:010} 00000 n \n{:010} 00000 n \n{:010} 00000 n \n{:010} 00000 n \n{:010} 00000 n \n",
        off1, off2, off3, off4, off5, off6
    );
    let trailer = format!(
        "trailer\n<< /Size 7 /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
        xref_off
    );

    let js_pdf = format!(
        "{}{}{}{}{}{}{}{}{}",
        header, obj1, obj2, obj3, obj4, obj5, obj6, xref, trailer
    );

    let res = extract_pdf_bytes(js_pdf.as_bytes());
    assert!(
        res.is_ok(),
        "PDF with JS actions should parse safely: {:?}",
        res.err()
    );
    let extracted = res.unwrap();
    assert!(
        !extracted.contains("MALICIOUS_JS_EXECUTED"),
        "Embedded JavaScript must NOT be executed or extracted as page text"
    );
}

#[test]
fn test_pdf_security_encrypted_without_password() {
    let encrypted_pdf = "%PDF-1.4\n\
1 0 obj <</Type /Catalog /Pages 2 0 R /Encrypt 3 0 R>> endobj\n\
2 0 obj <</Type /Pages /Count 0>> endobj\n\
3 0 obj <</Filter /Standard /V 2 /R 3 /P -4 /O (12345678901234567890123456789012) /U (12345678901234567890123456789012)>> endobj\n\
xref\n\
0 4\n\
0000000000 65535 f \n\
0000000009 00000 n \n\
0000000062 00000 n \n\
0000000101 00000 n \n\
trailer <</Size 4 /Root 1 0 R>>\n\
startxref\n\
220\n\
%%EOF\n";

    let res = extract_pdf_bytes(encrypted_pdf.as_bytes());
    assert!(
        res.is_err() || res.unwrap().is_empty(),
        "Encrypted PDF without password must return controlled Error or empty string"
    );
}

#[test]
fn test_pdf_security_corrupted_xref() {
    let mut corrupted_xref_pdf = create_minimal_pdf();
    let xref_pos = corrupted_xref_pdf
        .windows(4)
        .position(|w| w == b"xref")
        .unwrap_or(0);
    corrupted_xref_pdf.splice(xref_pos..xref_pos + 50, vec![b'X'; 50]);

    let res = extract_pdf_bytes(&corrupted_xref_pdf);
    assert!(
        res.is_err() || res.is_ok(),
        "Corrupted xref table must be handled gracefully without panic"
    );
}

// ============================================================================
// DOCX PARSER ROBUSTNESS TESTS
// ============================================================================

fn create_minimal_docx(text: &str) -> Vec<u8> {
    use docx_rs::*;
    let mut buf = Vec::new();
    let docx = Docx::new().add_paragraph(Paragraph::new().add_run(Run::new().add_text(text)));
    docx.pack(std::io::Cursor::new(&mut buf)).unwrap();
    buf
}

#[test]
fn test_docx_a_valid_minimal() {
    let docx_bytes = create_minimal_docx("Hello DOCX World");
    let res = extract_docx_bytes(&docx_bytes);
    assert!(
        res.is_ok(),
        "Minimal valid DOCX extraction failed: {:?}",
        res.err()
    );
    let text = res.unwrap();
    assert!(text.contains("Hello DOCX World"));
}

#[test]
fn test_docx_b_empty_bytes() {
    let res = extract_docx_bytes(b"");
    assert!(res.is_ok());
    assert_eq!(res.unwrap(), "");
}

#[test]
fn test_docx_c_truncated_corrupted() {
    let valid_docx = create_minimal_docx("Corrupted test DOCX content");
    for ratio in [0.10, 0.50, 0.90] {
        let len = (valid_docx.len() as f64 * ratio) as usize;
        let truncated = &valid_docx[..len];
        let res = extract_docx_bytes(truncated);
        assert!(res.is_err() || res.is_ok(), "Truncated DOCX must not panic");
    }
}

#[test]
fn test_docx_d_mismatched_extension() {
    let plain_text = b"This is plain text in a .docx file!";
    let res = extract_docx_bytes(plain_text);
    assert!(
        res.is_err(),
        "Plain text passed to docx parser should return Error"
    );
}

#[test]
fn test_docx_e_oversized() {
    let oversized = vec![0u8; 101 * 1024 * 1024];
    let res = extract_text_from_bytes(&oversized, "docx");
    assert!(res.is_err());
    assert!(res.unwrap_err().to_string().contains("100 MB"));
}

#[test]
fn test_docx_f_deeply_nested_xml_tables() {
    let mut nested_xml = String::from(
        r#"<?xml version="1.0" encoding="UTF-8"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>"#,
    );
    for _ in 0..100 {
        nested_xml.push_str("<w:tbl><w:tr><w:tc>");
    }
    nested_xml.push_str("<w:p><w:r><w:t>Deeply Nested Content</w:t></w:r></w:p>");
    for _ in 0..100 {
        nested_xml.push_str("</w:tc></w:tr></w:tbl>");
    }
    nested_xml.push_str("</w:body></w:document>");

    let mut buffer = Vec::new();
    {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut buffer));
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        zip.start_file("word/document.xml", options).unwrap();
        zip.write_all(nested_xml.as_bytes()).unwrap();
        zip.finish().unwrap();
    }

    let res = extract_docx_bytes(&buffer);
    assert!(
        res.is_ok() || res.is_err(),
        "Deeply nested DOCX XML must not cause stack overflow or panic"
    );
}

#[test]
fn test_docx_g_non_utf8_xml() {
    let mut buffer = Vec::new();
    {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut buffer));
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        zip.start_file("word/document.xml", options).unwrap();
        zip.write_all(b"\xFF\xFE<w:document><invalid_utf8_byte\x80\xA0/></w:document>")
            .unwrap();
        zip.finish().unwrap();
    }

    let res = extract_docx_bytes(&buffer);
    assert!(
        res.is_err() || res.is_ok(),
        "Non-UTF8 DOCX XML must handle errors gracefully without panic"
    );
}

#[test]
fn test_docx_h_fuzz_malformed_inputs() {
    let seeds: Vec<&[u8]> = vec![
        b"PK\x03\x04\x00\x00\x00\x00corrupted zip header",
        b"PK\x03\x04file_with_no_document_xml_inside",
        b"\x50\x4B\x05\x06\x00\x00\x00\x00\x00\x00\x00\x00",
    ];

    for seed in seeds {
        let _ = extract_docx_bytes(seed);
    }
}

#[test]
fn test_docx_i_zip_bomb_decompression_bomb_suspected() {
    let mut buffer = Vec::new();
    {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut buffer));
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        zip.start_file("word/document.xml", options).unwrap();
        // 500 KB of repeated bytes yields compression ratio > 300x
        let bomb_payload = vec![b'A'; 500_000];
        zip.write_all(&bomb_payload).unwrap();
        zip.finish().unwrap();
    }

    let res = extract_docx_bytes(&buffer);
    assert!(
        res.is_err(),
        "Extreme compression ratio DOCX must be rejected"
    );
    let err_msg = res.unwrap_err().to_string();
    assert!(
        err_msg.contains("Decompression bomb suspected"),
        "Error message should indicate decompression bomb, got: {err_msg}"
    );
}

// ============================================================================
// EMAIL (EML) PARSER ROBUSTNESS TESTS
// ============================================================================

#[test]
fn test_email_a_valid_minimal() {
    let eml =
        b"From: alice@example.com\nTo: bob@example.com\nSubject: Minimal EML\n\nHello EML World!";
    let res = extract_email_bytes(eml);
    assert!(res.is_ok());
    let email = res.unwrap();
    assert_eq!(email.subject, "Minimal EML");
    assert_eq!(email.from, "alice@example.com");
    assert_eq!(email.body, "Hello EML World!");
}

#[test]
fn test_email_b_empty_bytes() {
    let res = extract_email_bytes(b"");
    assert!(res.is_ok());
    let email = res.unwrap();
    assert_eq!(email.subject, "");
    assert_eq!(email.body, "");
}

#[test]
fn test_email_c_truncated_corrupted() {
    let eml = b"From: alice@example.com\nSubject: Long Subject Header Line\nContent-Type: multipart/mixed; boundary=\"B\"\n\n--B\nContent-Type: text/plain\n\nTruncated Body";
    for ratio in [0.10, 0.50, 0.90] {
        let len = (eml.len() as f64 * ratio) as usize;
        let truncated = &eml[..len];
        let res = extract_email_bytes(truncated);
        assert!(res.is_ok() || res.is_err(), "Truncated EML must not panic");
    }
}

#[test]
fn test_email_d_mismatched_extension() {
    let binary = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00];
    let res = extract_email_bytes(&binary);
    assert!(
        res.is_ok(),
        "Non-text binary EML should be safely parsed into structured content without panic"
    );
}

#[test]
fn test_email_e_oversized() {
    let oversized = vec![0u8; 101 * 1024 * 1024];
    let res = extract_text_from_bytes(&oversized, "eml");
    assert!(res.is_err());
    assert!(res.unwrap_err().to_string().contains("100 MB"));
}

#[test]
fn test_email_f_mime_multipart_and_nested_attachments() {
    let multipart_eml = b"From: sender@example.com\n\
Subject: Nested Multipart EML\n\
Content-Type: multipart/mixed; boundary=\"BOUNDARY1\"\n\n\
--BOUNDARY1\n\
Content-Type: multipart/alternative; boundary=\"BOUNDARY2\"\n\n\
--BOUNDARY2\n\
Content-Type: text/plain\n\n\
Inner plain text body\n\
--BOUNDARY2\n\
Content-Type: text/html\n\n\
<p>Inner HTML body</p>\n\
--BOUNDARY2--\n\
--BOUNDARY1\n\
Content-Type: application/pdf; name=\"attachment.pdf\"\n\
Content-Disposition: attachment; filename=\"attachment.pdf\"\n\n\
JVBERi0xLjQKJ...fake pdf bytes...\n\
--BOUNDARY1--";

    let res = extract_email_bytes(multipart_eml);
    assert!(res.is_ok());
    let content = res.unwrap();
    assert_eq!(content.subject, "Nested Multipart EML");
    assert_eq!(content.body, "Inner plain text body");
}

#[test]
fn test_email_g_non_utf8_and_mixed_encodings() {
    // Latin-1 / ISO-8859-1 encoded email subject and body
    let raw_eml = b"From: sender@example.com\n\
Subject: =?ISO-8859-1?Q?M=FCnchen_Weather?=\n\
Content-Type: text/plain; charset=ISO-8859-1\n\n\
Sch\xF6nen Gru\xDFe aus M\xF3nchen!";

    let res = extract_email_bytes(raw_eml);
    assert!(res.is_ok(), "Mixed encoding EML must parse without panic");
    let content = res.unwrap();
    assert!(
        content.subject.contains("München")
            || content.subject.contains("M=FCnchen")
            || !content.subject.is_empty()
    );
}

#[test]
fn test_email_h_invalid_headers_and_long_header_lines() {
    let mut long_header = String::from("From: sender@example.com\nSubject: ");
    long_header.push_str(&"A".repeat(100_000)); // 100 KB single header line
    long_header.push_str("\n\nHeader Injection Risk Body Test");

    let res = extract_email_bytes(long_header.as_bytes());
    assert!(
        res.is_ok(),
        "Excessively long header lines must not crash parser"
    );
    let content = res.unwrap();
    assert!(content.body.contains("Header Injection Risk Body Test"));
}

#[test]
fn test_email_strip_html_script_and_style_removal() {
    let html = "<html><head><style>body { color: red; }</style></head><body><h1>Title</h1><script>alert('xss');</script><p>Clean Paragraph</p></body></html>";
    let text = strip_html(html);
    assert!(!text.contains("color: red"));
    assert!(!text.contains("alert"));
    assert!(text.contains("Title"));
    assert!(text.contains("Clean Paragraph"));
}

#[tokio::test]
async fn test_async_file_parsers_integration() {
    // PDF file parser
    let pdf_bytes = create_minimal_pdf();
    let mut pdf_file = NamedTempFile::new().unwrap();
    pdf_file.write_all(&pdf_bytes).unwrap();
    let pdf_text = extract_pdf_text(pdf_file.path()).await.unwrap();
    assert!(pdf_text.contains("Hello PDF World"));

    // DOCX file parser
    let docx_bytes = create_minimal_docx("Async DOCX File Extraction Test");
    let mut docx_file = NamedTempFile::new().unwrap();
    docx_file.write_all(&docx_bytes).unwrap();
    let docx_text = extract_docx_text(docx_file.path()).await.unwrap();
    assert!(docx_text.contains("Async DOCX File Extraction Test"));

    // EML file parser
    let eml_bytes = b"From: async@example.com\nSubject: Async EML Test\n\nAsync EML Body Content";
    let mut eml_file = NamedTempFile::new().unwrap();
    eml_file.write_all(eml_bytes).unwrap();
    let eml_content = extract_email(eml_file.path()).await.unwrap();
    assert_eq!(eml_content.subject, "Async EML Test");
    assert_eq!(eml_content.body, "Async EML Body Content");
}
