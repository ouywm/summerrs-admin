use summer_common::error::ApiErrors;
use summer_common::file_util::read_multipart_files;
use summer_web::axum::body::Body;
use summer_web::axum::extract::{FromRequest, Multipart};
use summer_web::axum::http::{Request, header};

fn build_request(boundary: &str, body: Vec<u8>) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/upload")
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(Body::from(body))
        .expect("build request failed")
}

fn multipart_ok(boundary: &str, filename: &str, content_type: &str, content: &[u8]) -> Vec<u8> {
    let mut body = Vec::new();

    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        format!(
            "Content-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(format!("Content-Type: {content_type}\r\n").as_bytes());
    body.extend_from_slice(b"\r\n");
    body.extend_from_slice(content);
    body.extend_from_slice(b"\r\n");
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());

    body
}

fn multipart_incomplete_stream(boundary: &str, filename: &str, content_type: &str) -> Vec<u8> {
    // Simulate a truncated client request: starts a field but never ends the stream with a closing boundary.
    let mut body = Vec::new();
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        format!(
            "Content-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(format!("Content-Type: {content_type}\r\n").as_bytes());
    body.extend_from_slice(b"\r\n");
    body.extend_from_slice(b"PDFDATA");
    body
}

#[tokio::test]
async fn read_multipart_files_ok_ascii_filename() {
    let boundary = "----WebKitFormBoundaryKB0segspPGr3CjJK";
    let body = multipart_ok(boundary, "test.pdf", "application/pdf", b"PDFDATA");
    let req = build_request(boundary, body);

    let mut multipart = Multipart::from_request(req, &())
        .await
        .expect("extract Multipart failed");

    let files = read_multipart_files(&mut multipart)
        .await
        .expect("read_multipart_files failed");

    assert_eq!(files.len(), 1);
    assert_eq!(files[0].file_name, "test.pdf");
    assert_eq!(files[0].content_type.as_deref(), Some("application/pdf"));
    assert_eq!(files[0].data.as_ref(), b"PDFDATA");
}

#[tokio::test]
async fn read_multipart_files_ok_utf8_filename() {
    let boundary = "----WebKitFormBoundaryKB0segspPGr3CjJK";
    let body = multipart_ok(
        boundary,
        "大模型应用!算法学习路线+八股+面试实战_2.pdf",
        "application/pdf",
        b"PDFDATA",
    );
    let req = build_request(boundary, body);

    let mut multipart = Multipart::from_request(req, &())
        .await
        .expect("extract Multipart failed");

    let files = read_multipart_files(&mut multipart)
        .await
        .expect("read_multipart_files failed");

    assert_eq!(files.len(), 1);
    assert_eq!(files[0].file_name, "大模型应用!算法学习路线+八股+面试实战_2.pdf");
}

#[tokio::test]
async fn read_multipart_files_incomplete_stream_returns_detail_message() {
    let boundary = "----WebKitFormBoundaryKB0segspPGr3CjJK";
    let body = multipart_incomplete_stream(boundary, "test.pdf", "application/pdf");
    let req = build_request(boundary, body);

    let mut multipart = Multipart::from_request(req, &())
        .await
        .expect("extract Multipart failed");

    let err = match read_multipart_files(&mut multipart).await {
        Ok(_) => panic!("expected multipart parsing error, got Ok"),
        Err(err) => err,
    };

    let ApiErrors::BadRequest(message) = err else {
        panic!("expected BadRequest, got: {err:?}");
    };

    // We should surface Multer's body_text (e.g. "incomplete multipart stream") instead of the generic
    // "Error parsing `multipart/form-data` request".
    assert!(message.contains("读取文件内容失败"), "{message}");
    assert!(!message.contains("Error parsing"), "{message}");
    let lower = message.to_lowercase();
    assert!(
        lower.contains("incomplete") || lower.contains("boundary") || lower.contains("multipart"),
        "{message}"
    );
}
