use crate::config::AppConfig;
use crate::error::{AppError, AppResult};
use crate::smb_service::{normalize_rel, FileEntry};
use crate::state::AppState;
use axum::body::{to_bytes, Body};
use axum::extract::{Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Mutex;
use tauri::{AppHandle, Manager};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

pub const AI_PORT: u16 = 17845;
const MAX_UPLOAD_BYTES: usize = 256 * 1024 * 1024;

struct Running {
    handle: JoinHandle<()>,
}

pub struct AiServer {
    running: Mutex<Option<Running>>,
    last_error: Mutex<String>,
    start_lock: tokio::sync::Mutex<()>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiStatus {
    pub enabled: bool,
    pub running: bool,
    pub url: String,
    pub error: String,
}

impl AiServer {
    pub fn new() -> Self {
        Self {
            running: Mutex::new(None),
            last_error: Mutex::new(String::new()),
            start_lock: tokio::sync::Mutex::new(()),
        }
    }

    pub async fn sync(&self, app: &AppHandle) -> AppResult<()> {
        let _guard = self.start_lock.lock().await;
        let cfg = app.state::<AppState>().config();
        if !cfg.ai_enabled {
            self.stop();
            self.last_error.lock().unwrap().clear();
            return Ok(());
        }
        if self.running.lock().unwrap().is_some() {
            return Ok(());
        }
        let listener = match TcpListener::bind(("127.0.0.1", AI_PORT)).await {
            Ok(listener) => listener,
            Err(error) => {
                let message = format!("Не удалось запустить API ИИ на 127.0.0.1:{AI_PORT}: {error}");
                *self.last_error.lock().unwrap() = message.clone();
                return Err(AppError::msg(message));
            }
        };
        let router = router(app.clone());
        let handle = tokio::spawn(async move {
            if let Err(error) = axum::serve(listener, router).await {
                log::warn!("AI API server stopped: {error}");
            }
        });
        *self.running.lock().unwrap() = Some(Running { handle });
        self.last_error.lock().unwrap().clear();
        Ok(())
    }

    pub fn stop(&self) {
        if let Some(running) = self.running.lock().unwrap().take() {
            running.handle.abort();
        }
    }

    pub fn status(&self, app: &AppHandle) -> AiStatus {
        let cfg = app.state::<AppState>().config();
        let running = self.running.lock().unwrap().is_some();
        AiStatus {
            enabled: cfg.ai_enabled,
            running,
            url: if running {
                format!("http://127.0.0.1:{AI_PORT}")
            } else {
                String::new()
            },
            error: self.last_error.lock().unwrap().clone(),
        }
    }
}

fn router(app: AppHandle) -> Router {
    Router::new()
        .route("/", get(help))
        .route("/v1/help", get(help))
        .route("/v1/info", get(info))
        .route("/v1/files", get(list_files))
        .route("/v1/file", get(download_file).put(put_file))
        .route("/v1/folder", post(create_folder))
        .route("/v1/path", axum::routing::delete(delete_path))
        .fallback(|| async {
            (
                StatusCode::NOT_FOUND,
                Json(json!({ "ok": false, "error": "Неизвестный маршрут" })),
            )
        })
        .with_state(app)
}

struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }

    fn forbidden(message: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, message)
    }

    fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, message)
    }
}

impl From<AppError> for ApiError {
    fn from(error: AppError) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
    }
}

impl From<std::io::Error> for ApiError {
    fn from(error: std::io::Error) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(json!({ "ok": false, "error": self.message })),
        )
            .into_response()
    }
}

#[derive(Deserialize)]
struct PathQuery {
    #[serde(default)]
    path: String,
}

fn authorize(app: &AppHandle) -> Result<AppConfig, ApiError> {
    let cfg = app.state::<AppState>().config();
    if !cfg.ai_enabled {
        return Err(ApiError::forbidden("Доступ для ИИ-агентов выключен"));
    }
    Ok(cfg)
}

fn denied(cfg: &AppConfig, path: &str) -> bool {
    let target = normalize_rel(path).to_lowercase();
    if target.is_empty() {
        return false;
    }
    let target = format!("/{target}");
    cfg.ai_deny_paths.iter().any(|denied| {
        let denied = normalize_rel(denied).to_lowercase();
        if denied.is_empty() {
            return false;
        }
        let denied = format!("/{denied}");
        target == denied || target.starts_with(&format!("{denied}/"))
    })
}

fn check_path(cfg: &AppConfig, path: &str, permission: bool, name: &str) -> Result<(), ApiError> {
    if !permission {
        return Err(ApiError::forbidden(format!("Разрешение «{name}» выключено")));
    }
    if denied(cfg, path) {
        return Err(ApiError::forbidden("Доступ к этой папке запрещён"));
    }
    Ok(())
}

fn contains_denied(cfg: &AppConfig, path: &str) -> bool {
    let target = normalize_rel(path).to_lowercase();
    if target.is_empty() {
        return false;
    }
    let prefix = format!("/{target}/");
    cfg.ai_deny_paths.iter().any(|denied| {
        let denied = normalize_rel(denied).to_lowercase();
        !denied.is_empty() && format!("/{denied}").starts_with(&prefix)
    })
}

fn split_parent(path: &str) -> (String, &str) {
    match path.rsplit_once('/') {
        Some((parent, name)) => (parent.to_string(), name),
        None => (String::new(), path),
    }
}

fn require_path(query: &PathQuery) -> Result<String, ApiError> {
    let path = normalize_rel(&query.path);
    if path.is_empty() {
        return Err(ApiError::bad_request("Параметр path обязателен"));
    }
    Ok(path)
}

async fn help() -> Response {
    let body = format!(
        r#"# ZeroSMB AI API

Локальный HTTP API для доступа к файлам SMB-хранилища.
Базовый адрес: http://127.0.0.1:{AI_PORT}

## Доступ
Токен не нужен. API доступен без авторизации только с этого компьютера.
Доступ регулируется разрешениями и списком запрещённых папок в настройках ZeroSMB.

## Правила путей
Параметр path — путь относительно корня SMB share, например:
- Documents
- Photos/2026
Пути нормализуются, переход через .. запрещён. Запрещённые в приложении папки скрыты и недоступны.

## Маршруты
GET /v1/info
  Права доступа, share, basePath и запрещённые пути.

GET /v1/files?path=<папка>
  Список файлов и папок. Требуется разрешение «Скачивание и чтение».

GET /v1/file?path=<файл>
  Скачать файл. Требуется разрешение «Скачивание и чтение».

PUT /v1/file?path=<файл>
  Тело запроса — сырые байты файла.
  Новый файл требует разрешение «Загрузка новых файлов».
  Существующий файл требует разрешение «Изменение файлов».

POST /v1/folder?path=<папка>
  Создать папку. Требуется разрешение «Создание папок».

DELETE /v1/path?path=<файл или папка>
  Удалить файл или папку рекурсивно. Требуется разрешение «Удаление файлов и папок».

## Примеры
curl "http://127.0.0.1:{AI_PORT}/v1/info"
curl "http://127.0.0.1:{AI_PORT}/v1/files?path=Documents"
curl "http://127.0.0.1:{AI_PORT}/v1/file?path=Documents/readme.txt" -o readme.txt
curl -X PUT --data-binary "@photo.jpg" "http://127.0.0.1:{AI_PORT}/v1/file?path=Photos/photo.jpg"
curl -X POST "http://127.0.0.1:{AI_PORT}/v1/folder?path=NewFolder"
curl -X DELETE "http://127.0.0.1:{AI_PORT}/v1/path?path=OldFolder"

Ответы ошибок имеют вид: {{"ok":false,"error":"..."}}.
"#
    );
    Response::builder()
        .header(header::CONTENT_TYPE, "text/markdown; charset=utf-8")
        .body(Body::from(body))
        .unwrap_or_else(|_| Response::new(Body::empty()))
}

async fn info(
    State(app): State<AppHandle>,
) -> Result<Json<Value>, ApiError> {
    let cfg = authorize(&app)?;
    Ok(Json(json!({
        "ok": true,
        "version": env!("CARGO_PKG_VERSION"),
        "share": cfg.smb.share,
        "basePath": cfg.smb.base_path,
        "permissions": {
            "download": cfg.ai_allow_download,
            "modify": cfg.ai_allow_modify,
            "upload": cfg.ai_allow_upload,
            "createFolders": cfg.ai_allow_mkdir,
            "delete": cfg.ai_allow_delete
        },
        "denyPaths": cfg.ai_deny_paths
    })))
}

async fn list_files(
    State(app): State<AppHandle>,
    Query(query): Query<PathQuery>,
) -> Result<Json<Vec<FileEntry>>, ApiError> {
    let cfg = authorize(&app)?;
    let state = app.state::<AppState>();
    let path = normalize_rel(&query.path);
    check_path(
        &cfg,
        &path,
        cfg.ai_allow_download,
        "Скачивание и чтение",
    )?;
    let password = state.password()?;
    let effective =
        crate::zt_proxy::effective_smb_config(&app, &cfg.smb, &cfg.zerotier_network_id).await?;
    let client = state.inner.smb.connect(&effective, &password).await?;
    let entries = state.inner.smb.list_dir(&client, "", &path).await?;
    let entries = entries
        .into_iter()
        .filter(|entry| !denied(&cfg, &entry.path))
        .collect();
    Ok(Json(entries))
}

async fn download_file(
    State(app): State<AppHandle>,
    Query(query): Query<PathQuery>,
) -> Result<Response, ApiError> {
    let cfg = authorize(&app)?;
    let state = app.state::<AppState>();
    let path = require_path(&query)?;
    check_path(
        &cfg,
        &path,
        cfg.ai_allow_download,
        "Скачивание и чтение",
    )?;
    let password = state.password()?;
    let effective =
        crate::zt_proxy::effective_smb_config(&app, &cfg.smb, &cfg.zerotier_network_id).await?;
    let client = state.inner.smb.connect(&effective, &password).await?;
    let temp_dir = state.data_dir()?.join("ai_tmp");
    tokio::fs::create_dir_all(&temp_dir).await?;
    let temp = temp_dir.join(format!("{}-download", uuid::Uuid::new_v4()));
    state
        .inner
        .smb
        .download_file(&client, &path, &temp)
        .await?;
    let bytes = tokio::fs::read(&temp).await?;
    let _ = tokio::fs::remove_file(&temp).await;
    let mime = mime_guess::from_path(&path)
        .first_or_octet_stream()
        .to_string();
    Ok(Response::builder()
        .header(header::CONTENT_TYPE, mime)
        .body(Body::from(bytes))
        .map_err(AppError::from)?)
}

async fn put_file(
    State(app): State<AppHandle>,
    Query(query): Query<PathQuery>,
    body: Body,
) -> Result<Json<Value>, ApiError> {
    let cfg = authorize(&app)?;
    let state = app.state::<AppState>();
    let path = require_path(&query)?;
    if denied(&cfg, &path) {
        return Err(ApiError::forbidden("Доступ к этой папке запрещён"));
    }
    let password = state.password()?;
    let effective =
        crate::zt_proxy::effective_smb_config(&app, &cfg.smb, &cfg.zerotier_network_id).await?;
    let client = state.inner.smb.connect(&effective, &password).await?;
    let (parent, name) = split_parent(&path);
    let exists = match state.inner.smb.list_dir(&client, "", &parent).await {
        Ok(entries) => entries
            .iter()
            .any(|entry| entry.name.eq_ignore_ascii_case(name)),
        Err(_) => {
            if !cfg.ai_allow_mkdir {
                return Err(ApiError::forbidden(
                    "Папка назначения не существует, а создание папок выключено",
                ));
            }
            if parent.is_empty() {
                return Err(ApiError::bad_request("Не удалось открыть корень share"));
            }
            state.inner.smb.create_dir_path(&client, &parent).await?;
            false
        }
    };
    if exists && !cfg.ai_allow_modify {
        return Err(ApiError::forbidden("Разрешение «Изменение файлов» выключено"));
    }
    if !exists && !cfg.ai_allow_upload {
        return Err(ApiError::forbidden("Разрешение «Загрузка файлов» выключено"));
    }
    let bytes = to_bytes(body, MAX_UPLOAD_BYTES)
        .await
        .map_err(|_| ApiError::bad_request("Тело файла слишком большое"))?;
    let temp_dir = state.data_dir()?.join("ai_tmp");
    tokio::fs::create_dir_all(&temp_dir).await?;
    let temp = temp_dir.join(format!("{}-upload", uuid::Uuid::new_v4()));
    tokio::fs::write(&temp, &bytes).await?;
    let result = state.inner.smb.upload_file(&client, &path, &temp).await;
    let _ = tokio::fs::remove_file(&temp).await;
    let written = result?;
    Ok(Json(json!({
        "ok": true,
        "path": path,
        "bytes": written,
        "created": !exists
    })))
}

async fn create_folder(
    State(app): State<AppHandle>,
    Query(query): Query<PathQuery>,
) -> Result<Json<Value>, ApiError> {
    let cfg = authorize(&app)?;
    let state = app.state::<AppState>();
    let path = require_path(&query)?;
    check_path(
        &cfg,
        &path,
        cfg.ai_allow_mkdir,
        "Создание папок",
    )?;
    let password = state.password()?;
    let effective =
        crate::zt_proxy::effective_smb_config(&app, &cfg.smb, &cfg.zerotier_network_id).await?;
    let client = state.inner.smb.connect(&effective, &password).await?;
    state.inner.smb.create_dir_path(&client, &path).await?;
    Ok(Json(json!({ "ok": true, "path": path })))
}

async fn delete_path(
    State(app): State<AppHandle>,
    Query(query): Query<PathQuery>,
) -> Result<Json<Value>, ApiError> {
    let cfg = authorize(&app)?;
    let state = app.state::<AppState>();
    let path = require_path(&query)?;
    check_path(
        &cfg,
        &path,
        cfg.ai_allow_delete,
        "Удаление файлов и папок",
    )?;
    if contains_denied(&cfg, &path) {
        return Err(ApiError::forbidden(
            "Внутри папки есть запрещённые пути — удаление запрещено",
        ));
    }
    let password = state.password()?;
    let effective =
        crate::zt_proxy::effective_smb_config(&app, &cfg.smb, &cfg.zerotier_network_id).await?;
    let client = state.inner.smb.connect(&effective, &password).await?;
    let is_dir = state.inner.smb.list_dir(&client, "", &path).await.is_ok();
    if is_dir {
        state.inner.smb.delete_tree(&client, &path).await?;
    } else {
        state.inner.smb.delete_path(&client, &path).await?;
    }
    Ok(Json(json!({ "ok": true, "path": path, "directory": is_dir })))
}

#[tauri::command]
pub fn ai_status(app: AppHandle) -> AiStatus {
    app.state::<AppState>().inner.ai.status(&app)
}


