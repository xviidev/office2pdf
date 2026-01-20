use axum::{
    extract::{DefaultBodyLimit, Multipart, Request},
    http::{header, StatusCode},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Router,
};
use std::env;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tracing::{error, info};
use uuid::Uuid;

#[derive(Clone)]
struct AppState {
    api_key: Option<String>,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let api_key = env::var("API_KEY").ok();
    if api_key.is_some() {
        info!("API Key authentication enabled");
    } else {
        info!("No API Key set, authentication disabled");
    }

    let state = Arc::new(AppState { api_key });

    let app = Router::new()
        .route("/", get(index))
        .route("/convert", post(convert))
        .layer(middleware::from_fn_with_state(state.clone(), auth_middleware))
        .route("/health", get(health).head(health))
        .layer(DefaultBodyLimit::max(10 * 1024 * 1024)) // 10MB limit
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    info!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}

async fn health() -> StatusCode {
    StatusCode::OK
}

async fn index() -> Html<&'static str> {
    Html(include_str!("index.html"))
}

async fn auth_middleware(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    req: Request,
    next: Next,
) -> Response {
    if let Some(ref key) = state.api_key {
        if let Some(auth_header) = req.headers().get("X-Api-Key") {
            if let Ok(value) = auth_header.to_str() {
                if value == key {
                    return next.run(req).await;
                }
            }
        }
        return (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
    }
    next.run(req).await
}


fn sanitize_filename(raw: &str) -> String {
    std::path::Path::new(raw)
        .file_name()
        .map(|f| f.to_string_lossy().to_string())
        .unwrap_or_else(|| "document".to_string())
}

async fn convert(mut multipart: Multipart) -> Response {
    // create a unique directory for this request
    let request_id = Uuid::new_v4();
    let work_dir = PathBuf::from(format!("/tmp/convert/{}", request_id));

    if let Err(e) = fs::create_dir_all(&work_dir).await {
        error!("Failed to create work dir: {}", e);
        return (StatusCode::INTERNAL_SERVER_ERROR, "Internal Error").into_response();
    }

    // Process the upload
    let mut file_path = PathBuf::new();

    while let Ok(Some(mut field)) = multipart.next_field().await {
        if field.name() == Some("file") {
            let raw_filename = field.file_name().unwrap_or("document").to_string();
            let filename = sanitize_filename(&raw_filename);

            file_path = work_dir.join(&filename);

            // Stream to file
            let mut file = match fs::File::create(&file_path).await {
                Ok(f) => f,
                Err(e) => {
                    error!("Failed to create file: {}", e);
                    let _ = fs::remove_dir_all(&work_dir).await;
                    return (StatusCode::INTERNAL_SERVER_ERROR, "Internal Error").into_response();
                }
            };

            let mut success = true;
            loop {
                match field.chunk().await {
                    Ok(Some(chunk)) => {
                        if let Err(e) = file.write_all(&chunk).await {
                            error!("Failed to write chunk: {}", e);
                            success = false;
                            break;
                        }
                    }
                    Ok(None) => break, // End of stream
                    Err(e) => {
                        error!("Failed to read chunk: {}", e);
                        success = false;
                        break;
                    }
                }
            }

            if !success {
                let _ = fs::remove_dir_all(&work_dir).await;
                return (StatusCode::BAD_REQUEST, "Stream interrupted").into_response();
            }

            if let Err(e) = file.flush().await {
                 error!("Failed to flush file: {}", e);
                 let _ = fs::remove_dir_all(&work_dir).await;
                 return (StatusCode::INTERNAL_SERVER_ERROR, "Internal Error").into_response();
            }
            break;
        }
    }

    if file_path.as_os_str().is_empty() {
         let _ = fs::remove_dir_all(&work_dir).await;
         return (StatusCode::BAD_REQUEST, "No file uploaded").into_response();
    }

    // Convert
    info!("Converting file: {:?}", file_path);

    // UserInstallation is set to a temp dir to avoid conflicts and permission issues
    let user_installation = format!("-env:UserInstallation=file://{}/user", work_dir.display());

    // Optimized flags for faster startup
    let output = Command::new("libreoffice")
        .arg("--headless")
        .arg("--nodefault")
        .arg("--nofirststartwizard")
        .arg("--nolockcheck")
        .arg("--nologo")
        .arg("--norestore")
        .arg("--convert-to")
        .arg("pdf")
        .arg("--outdir")
        .arg(&work_dir)
        .arg(&user_installation)
        .arg(&file_path)
        .output()
        .await;

    match output {
        Ok(out) => {
            if !out.status.success() {
                error!("LibreOffice failed: stderr: {}", String::from_utf8_lossy(&out.stderr));
                let _ = fs::remove_dir_all(&work_dir).await;
                return (StatusCode::INTERNAL_SERVER_ERROR, "Conversion failed").into_response();
            }
        }
        Err(e) => {
            error!("Failed to run LibreOffice: {}", e);
            let _ = fs::remove_dir_all(&work_dir).await;
            return (StatusCode::INTERNAL_SERVER_ERROR, "Conversion execution failed").into_response();
        }
    }

    // Find the PDF file
    // LibreOffice creates a file with the same base name and .pdf extension
    let mut found_pdf_path: Option<PathBuf> = None;
    let mut pdf_filename_output = String::from("output.pdf");

    if let Ok(mut entries) = fs::read_dir(&work_dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if path.extension().map_or(false, |e| e == "pdf") {
                found_pdf_path = Some(path.clone());
                if let Some(name) = path.file_name() {
                    pdf_filename_output = name.to_string_lossy().to_string();
                }
                break;
            }
        }
    }

    let pdf_content = match found_pdf_path {
        Some(path) => match fs::read(&path).await {
            Ok(c) => c,
            Err(e) => {
                error!("Failed to read generated PDF: {}", e);
                let _ = fs::remove_dir_all(&work_dir).await;
                return (StatusCode::INTERNAL_SERVER_ERROR, "Read PDF failed").into_response();
            }
        },
        None => {
            error!("No PDF file found in output directory");
            let _ = fs::remove_dir_all(&work_dir).await;
            return (StatusCode::INTERNAL_SERVER_ERROR, "PDF generation failed - output not found").into_response();
        }
    };

    // Cleanup
    let _ = fs::remove_dir_all(&work_dir).await;

    // Return
    // Escape double quotes in filename to prevent header injection
    let escaped_filename = pdf_filename_output.replace('"', "\\\"");
    let headers = [
        (header::CONTENT_TYPE, "application/pdf"),
        (header::CONTENT_DISPOSITION, &format!("attachment; filename=\"{}\"", escaped_filename)),
    ];

    (headers, pdf_content).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("test.docx"), "test.docx");
        assert_eq!(sanitize_filename("/tmp/test.docx"), "test.docx");
        // Windows paths are treated as full filenames on Linux, so we skip that check or expect full string
        // assert_eq!(sanitize_filename("C:\\Windows\\test.docx"), "test.docx");
        // Edge cases
        assert_eq!(sanitize_filename(""), "document");
    }
}
