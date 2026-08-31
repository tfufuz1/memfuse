use memfuse_tauri_lib::ingestion::{
    docx::extract_docx_bytes, email::extract_email_bytes, pdf::extract_pdf_bytes,
    pipeline::extract_text_from_bytes,
};
use std::time::Instant;

fn main() {
    println!("=== MemFuse Brain - Parser & Ingestion Micro-Benchmarks ===");

    // 1. PDF Parser Benchmark
    let pdf_bytes = create_test_pdf(100); // 100 pages/blocks
    let start = Instant::now();
    let iterations = 100;
    for _ in 0..iterations {
        let _ = extract_pdf_bytes(&pdf_bytes);
    }
    let elapsed = start.elapsed();
    let avg_ms = elapsed.as_secs_f64() * 1000.0 / iterations as f64;
    println!(
        "PDF Parser Throughput: {:.3} ms/doc ({:.2} MB/s)",
        avg_ms,
        (pdf_bytes.len() as f64 / (1024.0 * 1024.0)) / (avg_ms / 1000.0)
    );

    // 2. DOCX Parser Benchmark
    let docx_bytes = create_test_docx("Sample text for benchmarking ");
    let start = Instant::now();
    for _ in 0..iterations {
        let _ = extract_docx_bytes(&docx_bytes);
    }
    let elapsed = start.elapsed();
    let avg_ms = elapsed.as_secs_f64() * 1000.0 / iterations as f64;
    println!(
        "DOCX Parser Throughput: {:.3} ms/doc ({:.2} MB/s)",
        avg_ms,
        (docx_bytes.len() as f64 / (1024.0 * 1024.0)) / (avg_ms / 1000.0)
    );

    // 3. EML Parser Benchmark
    let eml_bytes = create_test_eml(500); // 500 lines email
    let start = Instant::now();
    for _ in 0..iterations {
        let _ = extract_email_bytes(&eml_bytes);
    }
    let elapsed = start.elapsed();
    let avg_ms = elapsed.as_secs_f64() * 1000.0 / iterations as f64;
    println!(
        "EML Parser Throughput: {:.3} ms/doc ({:.2} MB/s)",
        avg_ms,
        (eml_bytes.len() as f64 / (1024.0 * 1024.0)) / (avg_ms / 1000.0)
    );

    // 4. Text Extraction Routing Benchmark
    let start = Instant::now();
    for _ in 0..iterations {
        let _ = extract_text_from_bytes(&eml_bytes, "eml");
    }
    let elapsed = start.elapsed();
    println!(
        "Ingestion Text Extraction Pipeline Latency: {:.3} ms/doc",
        elapsed.as_secs_f64() * 1000.0 / iterations as f64
    );
}

fn create_test_pdf(_blocks: usize) -> Vec<u8> {
    let header = "%PDF-1.4\n";
    let obj1 = "1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n";
    let obj2 = "2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n";
    let obj3 = "3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>\nendobj\n";
    let stream_data = "BT /F1 12 Tf 100 700 Td (Benchmark PDF Document Content Line) Tj ET";
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

fn create_test_docx(repeat_text: &str) -> Vec<u8> {
    use docx_rs::*;
    let mut buf = Vec::new();
    let mut p = Paragraph::new();
    for _ in 0..50 {
        p = p.add_run(Run::new().add_text(repeat_text));
    }
    let docx = Docx::new().add_paragraph(p);
    docx.pack(std::io::Cursor::new(&mut buf)).unwrap();
    buf
}

fn create_test_eml(line_count: usize) -> Vec<u8> {
    let mut eml = String::from("From: bench@example.com\nSubject: Benchmark EML\n\n");
    for i in 0..line_count {
        eml.push_str(&format!("Line {i}: This is benchmark email body text.\n"));
    }
    eml.into_bytes()
}
