use crate::error::{AppError, AppResult};
use crate::smb_service::normalize_rel;
use crate::state::AppState;
use crate::zt_proxy;
use http::header::{self, HeaderMap, HeaderValue};
use http::{Request, Response, StatusCode};
use percent_encoding::percent_decode_str;
use std::path::PathBuf;
use tauri::{Manager, UriSchemeContext, UriSchemeResponder};
use tokio::io::{AsyncReadExt, AsyncSeekExt};

const MAX_CHUNK: u64 = 32 * 1024 * 1024;

pub fn handle(
    ctx: UriSchemeContext<'_, tauri::Wry>,
    request: Request<Vec<u8>>,
    responder: UriSchemeResponder,
) {
    let app = ctx.app_handle().clone();
    let uri = request.uri().clone();
    let headers = request.headers().clone();
    tauri::async_runtime::spawn(async move {
        let response = match serve(&app, &uri, &headers).await {
            Ok(response) => response,
            Err(error) => text_response(StatusCode::BAD_REQUEST, &error.to_string()),
        };
        responder.respond(response);
    });
}

async fn serve(
    app: &tauri::AppHandle,
    uri: &http::Uri,
    headers: &HeaderMap,
) -> AppResult<Response<Vec<u8>>> {
    let raw = uri.path().trim_start_matches('/');
    let decoded = percent_decode_str(raw).decode_utf8_lossy().to_string();
    let (kind, value) = decoded
        .split_once('/')
        .ok_or_else(|| AppError::msg("Некорректный media URL"))?;

    match kind {
        "smb" => serve_smb(app, value, headers).await,
        "local" => serve_local(app, value, headers).await,
        _ => Err(AppError::msg("Неизвестный источник media")),
    }
}

async fn serve_smb(
    app: &tauri::AppHandle,
    relative: &str,
    headers: &HeaderMap,
) -> AppResult<Response<Vec<u8>>> {
    let state = app.state::<AppState>();
    let cfg = state.smb_config();
    let network_id = state.config().zerotier_network_id;
    let effective = zt_proxy::effective_smb_config(app, &cfg, &network_id).await?;
    let password = state.password()?;
    let rel = normalize_rel(relative);
    if rel.is_empty() {
        return Err(AppError::msg("Пустой путь к файлу"));
    }
    let _guard = state.inner.media_lock.lock().await;
    let client = state.inner.media_smb.connect(&effective, &password).await?;
    let len = state.inner.media_smb.file_len(&client, "", &rel).await?;
    let (start, end, status) = resolve_range(headers, len)?;

    let mut data = if end >= start {
        state
            .inner
            .media_smb
            .read_range(&client, "", &rel, start, (end - start + 1) as usize)
            .await?
    } else {
        Vec::new()
    };
    data.truncate((end.saturating_sub(start) + 1) as usize);

    build_response(
        StatusCode::from_u16(status).unwrap_or(StatusCode::OK),
        &rel,
        len,
        start,
        end,
        data,
        headers,
    )
}

async fn serve_local(
    app: &tauri::AppHandle,
    token: &str,
    headers: &HeaderMap,
) -> AppResult<Response<Vec<u8>>> {
    let state = app.state::<AppState>();
    let path = state
        .local_media(token)
        .ok_or_else(|| AppError::msg("Локальный медиафайл не найден"))?;
    serve_local_path(path, headers).await
}

async fn serve_local_path(
    path: PathBuf,
    headers: &HeaderMap,
) -> AppResult<Response<Vec<u8>>> {
    let metadata = tokio::fs::metadata(&path).await?;
    let len = metadata.len();
    let (start, end, status) = resolve_range(headers, len)?;
    let mut data = Vec::new();
    if end >= start {
        let count = (end - start + 1) as usize;
        let mut file = tokio::fs::File::open(&path).await?;
        file.seek(std::io::SeekFrom::Start(start)).await?;
        data = vec![0u8; count];
        let mut read = 0usize;
        while read < count {
            let n = file.read(&mut data[read..]).await?;
            if n == 0 {
                break;
            }
            read += n;
        }
        data.truncate(read);
    }
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("media");
    build_response(
        StatusCode::from_u16(status).unwrap_or(StatusCode::OK),
        name,
        len,
        start,
        end,
        data,
        headers,
    )
}

fn resolve_range(headers: &HeaderMap, len: u64) -> AppResult<(u64, u64, u16)> {
    if len == 0 {
        return Ok((0, 0, 200));
    }
    let Some(value) = headers.get(header::RANGE) else {
        let end = (len - 1).min(MAX_CHUNK - 1);
        return if end + 1 < len {
            Ok((0, end, 206))
        } else {
            Ok((0, end, 200))
        };
    };
    let value = value
        .to_str()
        .map_err(|_| AppError::msg("Некорректный Range"))?;
    let value = value
        .strip_prefix("bytes=")
        .ok_or_else(|| AppError::msg("Поддерживаются только byte ranges"))?;
    let first = value.split(',').next().unwrap_or(value);
    let (start_text, end_text) = first
        .split_once('-')
        .ok_or_else(|| AppError::msg("Некорректный Range"))?;
    let (mut start, mut end) = if start_text.is_empty() {
        let suffix: u64 = end_text
            .parse()
            .map_err(|_| AppError::msg("Некорректный Range"))?;
        (len.saturating_sub(suffix), len - 1)
    } else {
        let start: u64 = start_text
            .parse()
            .map_err(|_| AppError::msg("Некорректный Range"))?;
        let end = if end_text.is_empty() {
            len - 1
        } else {
            end_text
                .parse::<u64>()
                .map_err(|_| AppError::msg("Некорректный Range"))?
                .min(len - 1)
        };
        (start, end)
    };
    if start >= len {
        return Err(AppError::msg("Range вне файла"));
    }
    if end < start {
        std::mem::swap(&mut start, &mut end);
    }
    let capped_end = (start + MAX_CHUNK - 1).min(end);
    end = capped_end;
    Ok((start, end, 206))
}

fn build_response(
    status: StatusCode,
    name: &str,
    len: u64,
    start: u64,
    end: u64,
    data: Vec<u8>,
    _headers: &HeaderMap,
) -> AppResult<Response<Vec<u8>>> {
    let mime = mime_guess::from_path(name)
        .first_or_octet_stream()
        .to_string();
    let content_length = data.len().to_string();
    let mut builder = Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, HeaderValue::from_str(&mime).unwrap_or(HeaderValue::from_static("application/octet-stream")))
        .header(header::CONTENT_LENGTH, content_length)
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CACHE_CONTROL, "no-store")
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*");
    if status == StatusCode::PARTIAL_CONTENT {
        builder = builder.header(
            header::CONTENT_RANGE,
            format!("bytes {start}-{end}/{len}"),
        );
    }
    Ok(builder.body(data)?)
}

fn text_response(status: StatusCode, text: &str) -> Response<Vec<u8>> {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .body(text.as_bytes().to_vec())
        .unwrap_or_else(|_| Response::new(Vec::new()))
}
