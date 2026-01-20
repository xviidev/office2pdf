# Office to PDF Converter

A robust, container-ready API service written in Rust that converts Office documents (Word, Excel, PowerPoint, etc.) to PDF using LibreOffice in headless mode.

## Features

- **Document Conversion**: Convert `.docx`, `.xlsx`, `.pptx`, and other supported formats to PDF.
- **REST API**: Simple HTTP interface for integration.
- **High Performance**: Built with [Axum](https://github.com/tokio-rs/axum) and [Tokio](https://tokio.rs/) for efficient async processing.
- **Containerized**: Docker support with multi-stage build for small image size and ease of deployment.
- **Security**: Optional API Key authentication.
- **Isolation**: Each conversion runs with a unique temporary user profile to ensure thread safety and prevent lock files issues.

## Prerequisites

- **Rust**: 1.75+ (if running locally)
- **LibreOffice**: Installed and available in PATH (if running locally)
- **Docker**: For containerized deployment

## Getting Started

### Running Locally

1.  **Install LibreOffice**:
    Ensure `libreoffice` is installed and the binary is in your system PATH.

2.  **Run the application**:
    ```bash
    cargo run
    ```
    The server will start on `http://0.0.0.0:3000`.

### Running with Docker

You can pull the pre-built image from GitHub Container Registry:

```bash
docker pull ghcr.io/xviidev/office2pdf:latest
docker run -p 3000:3000 ghcr.io/xviidev/office2pdf:latest
```

Or build it locally:

1.  **Build the image**:
    ```bash
    docker build -t office-pdf-converter .
    ```

2.  **Run the container**:
    ```bash
    docker run -p 3000:3000 office-pdf-converter
    ```

## Configuration

The application can be configured via environment variables:

| Variable | Description | Default |
| :--- | :--- | :--- |
| `API_KEY` | If set, the server requires `X-Api-Key` header for the `/convert` endpoint. | (Disabled) |
| `RUST_LOG` | Logging level (e.g., `info`, `debug`, `error`). | `info` (via tracing) |

## API Documentation

The OpenApi 3.0.3 specification is available in [`openapi.yaml`](./openapi.yaml).

### Health Check

Check if the service is running.

- **URL**: `/health`
- **Method**: `GET` or `HEAD`
- **Response**: `200 OK`

### Convert Document

Upload a file to convert it to PDF.

- **URL**: `/convert`
- **Method**: `POST`
- **Content-Type**: `multipart/form-data`
- **Headers**:
    - `X-Api-Key`: `<Your API Key>` (Only if `API_KEY` env var is set)
- **Body**:
    - `file`: The document file to convert (binary).

#### Example using cURL

**Without Authentication:**
```bash
curl -X POST http://localhost:3000/convert \
  -F "file=@/path/to/your/document.docx" \
  --output document.pdf
```

**With Authentication:**
```bash
curl -X POST http://localhost:3000/convert \
  -H "X-Api-Key: your_secret_key" \
  -F "file=@/path/to/your/document.docx" \
  --output document.pdf
```

## Development

### Running Tests

```bash
cargo test
```

### File Structure

- `src/main.rs`: Application entry point and logic.
- `Dockerfile`: Multi-stage Docker build definition.
- `openapi.yaml`: API specification.
