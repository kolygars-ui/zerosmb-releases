use crate::error::{AppError, AppResult};
use crate::smb_service::normalize_rel;
use crate::state::AppState;
use crate::zt_proxy;
use serde::Serialize;
use std::collections::hash_map::DefaultHasher;
use std::hash::Hasher;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager, State};
use unrar_rs::RarArchive;
use zip::ZipArchive;

const MAX_ARCHIVE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_TEXT_BYTES: usize = 4 * 1024 * 1024;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentPreview {
    pub name: String,
    pub text: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveEntry {
    pub index: usize,
    pub name: String,
    pub size: u64,
    pub is_dir: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalFile {
    pub local_path: String,
    pub signature: String,
}

fn file_signature(path: &Path) -> AppResult<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = DefaultHasher::new();
    let mut buffer = [0u8; 64 * 1024];
    let mut total = 0u64;
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        total += read as u64;
        hasher.write(&buffer[..read]);
    }
    Ok(format!("{total}:{:016x}", hasher.finish()))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ArchiveKind {
    Zip,
    Rar,
}

fn archive_kind(path: &str) -> Option<ArchiveKind> {
    let extension = path.rsplit('.').next()?.to_ascii_lowercase();
    match extension.as_str() {
        "zip" | "docx" | "xlsx" | "pptx" | "odt" | "ods" | "odp" | "epub" => {
            Some(ArchiveKind::Zip)
        }
        "rar" => Some(ArchiveKind::Rar),
        _ => None,
    }
}

async fn read_remote_bytes(
    app: &AppHandle,
    state: &AppState,
    path: &str,
) -> AppResult<Vec<u8>> {
    let cfg = state.config();
    let password = state.password()?;
    let effective =
        zt_proxy::effective_smb_config(app, &cfg.smb, &cfg.zerotier_network_id).await?;
    let client = state.inner.smb.connect(&effective, &password).await?;
    let len = state.inner.smb.file_len(&client, "", path).await?;
    if len > MAX_ARCHIVE_BYTES {
        return Err(AppError::msg("Файл больше 512 МБ — предпросмотр недоступен"));
    }
    state
        .inner
        .smb
        .read_range(&client, "", path, 0, len as usize)
        .await
}

fn xml_to_text(xml: &str) -> String {
    let mut output = String::with_capacity(xml.len() / 4);
    let mut in_tag = false;
    let mut tag = String::new();
    for ch in xml.chars() {
        if in_tag {
            if ch == '>' {
                let tag_name = tag
                    .trim()
                    .trim_start_matches('/')
                    .split_whitespace()
                    .next()
                    .unwrap_or("");
                if matches!(
                    tag_name,
                    "p" | "w:p" | "a:p" | "text:p" | "br" | "w:br" | "a:br" | "tr" | "w:tr"
                ) {
                    output.push('\n');
                }
                in_tag = false;
                tag.clear();
            } else {
                tag.push(ch);
            }
            continue;
        }
        if ch == '<' {
            in_tag = true;
            continue;
        }
        output.push(match ch {
            '&' => ' ',
            '\r' => ' ',
            _ => ch,
        });
    }
    let output = output
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'");
    let mut result = String::new();
    let mut last_space = false;
    for line in output.lines() {
        let line = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if line.is_empty() {
            continue;
        }
        if !result.is_empty() {
            result.push('\n');
        }
        result.push_str(&line);
        last_space = true;
    }
    if !last_space && result.is_empty() {
        String::new()
    } else {
        result
    }
}

fn office_paths(name: &str, names: &[String]) -> Vec<String> {
    let extension = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match extension.as_str() {
        "docx" => names
            .iter()
            .filter(|name| name.as_str() == "word/document.xml")
            .cloned()
            .collect(),
        "pptx" => names
            .iter()
            .filter(|name| name.starts_with("ppt/slides/slide") && name.ends_with(".xml"))
            .cloned()
            .collect(),
        "xlsx" => names
            .iter()
            .filter(|name| {
                name.as_str() == "xl/sharedStrings.xml"
                    || (name.starts_with("xl/worksheets/") && name.ends_with(".xml"))
            })
            .cloned()
            .collect(),
        "odt" | "ods" | "odp" => names
            .iter()
            .filter(|name| name.as_str() == "content.xml")
            .cloned()
            .collect(),
        "epub" => names
            .iter()
            .filter(|name| {
                name.ends_with(".xhtml") || name.ends_with(".html") || name.ends_with(".htm")
            })
            .cloned()
            .collect(),
        _ => Vec::new(),
    }
}

fn preview_office_with_zip(name: &str, bytes: Vec<u8>) -> AppResult<DocumentPreview> {
    let mut archive =
        ZipArchive::new(Cursor::new(bytes)).map_err(|error| AppError::msg(error.to_string()))?;
    let names: Vec<String> = (0..archive.len())
        .filter_map(|index| archive.by_index(index).ok().map(|file| file.name().to_string()))
        .collect();
    let paths = office_paths(name, &names);
    if paths.is_empty() {
        return Err(AppError::msg("Не удалось найти текст в документе"));
    }
    let mut text = String::new();
    for path in paths {
        let Ok(mut file) = archive.by_name(&path) else {
            continue;
        };
        if file.size() > MAX_TEXT_BYTES as u64 {
            continue;
        }
        let mut data = Vec::new();
        file.read_to_end(&mut data)?;
        let xml = String::from_utf8_lossy(&data);
        let part = xml_to_text(&xml);
        if !part.is_empty() {
            if !text.is_empty() {
                text.push_str("\n\n");
            }
            text.push_str(&part);
        }
        if text.len() > MAX_TEXT_BYTES {
            text.truncate(MAX_TEXT_BYTES);
            text.push_str("\n\n…");
            break;
        }
    }
    Ok(DocumentPreview {
        name: name.to_string(),
        text,
    })
}

fn open_zip_entry(bytes: Vec<u8>, index: usize) -> AppResult<Vec<u8>> {
    let mut archive =
        ZipArchive::new(Cursor::new(bytes)).map_err(|error| AppError::msg(error.to_string()))?;
    let mut file = archive
        .by_index(index)
        .map_err(|error| AppError::msg(error.to_string()))?;
    let mut data = Vec::new();
    file.read_to_end(&mut data)?;
    Ok(data)
}

fn list_zip(bytes: Vec<u8>) -> AppResult<Vec<ArchiveEntry>> {
    let mut archive =
        ZipArchive::new(Cursor::new(bytes)).map_err(|error| AppError::msg(error.to_string()))?;
    let mut entries = Vec::new();
    for index in 0..archive.len() {
        let file = archive
            .by_index(index)
            .map_err(|error| AppError::msg(error.to_string()))?;
        entries.push(ArchiveEntry {
            index,
            name: file.name().to_string(),
            size: file.size(),
            is_dir: file.is_dir(),
        });
    }
    Ok(entries)
}

fn open_rar_entry(bytes: Vec<u8>, index: usize) -> AppResult<Vec<u8>> {
    let mut archive =
        RarArchive::open(Cursor::new(bytes)).map_err(|error| AppError::msg(error.to_string()))?;
    let mut data = Vec::new();
    archive
        .by_index(index)
        .and_then(|entry| entry.copy_to(&mut data))
        .map_err(|error| AppError::msg(error.to_string()))?;
    Ok(data)
}

fn list_rar(bytes: Vec<u8>) -> AppResult<Vec<ArchiveEntry>> {
    let archive =
        RarArchive::open(Cursor::new(bytes)).map_err(|error| AppError::msg(error.to_string()))?;
    Ok(archive
        .entries()
        .enumerate()
        .map(|(index, entry)| ArchiveEntry {
            index,
            name: entry.name,
            size: entry.unpacked_size.unwrap_or(0),
            is_dir: entry.is_directory,
        })
        .collect())
}

fn archive_entry_name(kind: ArchiveKind, bytes: &[u8], index: usize) -> AppResult<String> {
    let entries = match kind {
        ArchiveKind::Zip => list_zip(bytes.to_vec())?,
        ArchiveKind::Rar => list_rar(bytes.to_vec())?,
    };
    entries
        .into_iter()
        .find(|entry| entry.index == index)
        .map(|entry| entry.name)
        .ok_or_else(|| AppError::msg("Элемент архива не найден"))
}

fn entry_file_name(name: &str) -> String {
    name.rsplit(['/', '\\'])
        .next()
        .unwrap_or("file")
        .chars()
        .map(|ch| if ch.is_control() { '_' } else { ch })
        .collect()
}

fn mime_for(name: &str) -> String {
    mime_guess::from_path(name)
        .first_or_octet_stream()
        .to_string()
}

fn preview_root(app: &AppHandle, state: &AppState) -> AppResult<PathBuf> {
    #[cfg(target_os = "android")]
    {
        let _ = state;
        Ok(app
            .path()
            .app_cache_dir()
            .map_err(|error| AppError::msg(error.to_string()))?
            .join("previews"))
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        Ok(state.data_dir()?.join("previews"))
    }
}

async fn open_bytes_in_system(
    app: &AppHandle,
    state: &AppState,
    name: &str,
    bytes: &[u8],
) -> AppResult<String> {
    let dir = preview_root(app, state)?;
    tokio::fs::create_dir_all(&dir).await?;
    let target = dir.join(format!("{}-{}", uuid::Uuid::new_v4(), entry_file_name(name)));
    tokio::fs::write(&target, bytes).await?;
    #[cfg(target_os = "android")]
    {
        let _: serde_json::Value = crate::android_bridge::call(
            app,
            "openFile",
            serde_json::json!({
                "path": target.to_string_lossy(),
                "name": name,
                "mime": mime_for(name)
            }),
        )
        .await?;
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = state;
        open::that(&target).map_err(|error| AppError::msg(error.to_string()))?;
    }
    Ok(target.to_string_lossy().to_string())
}

async fn save_bytes_to_downloads(
    app: &AppHandle,
    state: &AppState,
    name: &str,
    bytes: &[u8],
) -> AppResult<String> {
    let dir = state.data_dir()?.join("downloads");
    tokio::fs::create_dir_all(&dir).await?;
    let source = dir.join(format!("{}-{}", uuid::Uuid::new_v4(), entry_file_name(name)));
    tokio::fs::write(&source, bytes).await?;
    #[cfg(target_os = "android")]
    {
        let value: serde_json::Value = crate::android_bridge::call(
            app,
            "saveToDownloads",
            serde_json::json!({
                "path": source.to_string_lossy(),
                "name": entry_file_name(name),
                "mime": mime_for(name),
                "relativePath": serde_json::Value::Null,
                "conflictPolicy": "rename"
            }),
        )
        .await?;
        let _ = tokio::fs::remove_file(&source).await;
        return Ok(value
            .get("path")
            .and_then(|value| value.as_str())
            .unwrap_or("Download/ZeroSMB")
            .to_string());
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        let cfg = state.config();
        let target_dir = if cfg.download_dir.trim().is_empty() {
            app.path()
                .download_dir()
                .unwrap_or_else(|_| dir.clone())
        } else {
            PathBuf::from(cfg.download_dir.trim())
        };
        tokio::fs::create_dir_all(&target_dir).await?;
        let target = target_dir.join(entry_file_name(name));
        let target = if target.exists() {
            let stem = target
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("file");
            let extension = target.extension().and_then(|value| value.to_str());
            let name = match extension {
                Some(extension) => format!("{stem}-{}.{extension}", uuid::Uuid::new_v4()),
                None => format!("{stem}-{}", uuid::Uuid::new_v4()),
            };
            target_dir.join(name)
        } else {
            target
        };
        tokio::fs::copy(&source, &target).await?;
        let _ = tokio::fs::remove_file(&source).await;
        Ok(target.to_string_lossy().to_string())
    }
}

#[tauri::command]
pub async fn preview_document(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> AppResult<DocumentPreview> {
    let bytes = read_remote_bytes(&app, &state, &path).await?;
    let name = path.rsplit('/').next().unwrap_or(&path).to_string();
    preview_office_with_zip(&name, bytes)
}

#[tauri::command]
pub async fn archive_list(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> AppResult<Vec<ArchiveEntry>> {
    let kind = archive_kind(&path)
        .ok_or_else(|| AppError::msg("Поддерживаются архивы ZIP и RAR"))?;
    let bytes = read_remote_bytes(&app, &state, &path).await?;
    match kind {
        ArchiveKind::Zip => list_zip(bytes),
        ArchiveKind::Rar => list_rar(bytes),
    }
}

#[tauri::command]
pub async fn archive_read(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
    index: usize,
) -> AppResult<tauri::ipc::Response> {
    let kind = archive_kind(&path)
        .ok_or_else(|| AppError::msg("Поддерживаются архивы ZIP и RAR"))?;
    let bytes = read_remote_bytes(&app, &state, &path).await?;
    let data = match kind {
        ArchiveKind::Zip => open_zip_entry(bytes, index)?,
        ArchiveKind::Rar => open_rar_entry(bytes, index)?,
    };
    Ok(tauri::ipc::Response::new(data))
}

#[tauri::command]
pub async fn archive_preview_entry(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
    index: usize,
) -> AppResult<DocumentPreview> {
    let kind = archive_kind(&path)
        .ok_or_else(|| AppError::msg("Поддерживаются архивы ZIP и RAR"))?;
    let bytes = read_remote_bytes(&app, &state, &path).await?;
    let name = archive_entry_name(kind, &bytes, index)?;
    let data = match kind {
        ArchiveKind::Zip => open_zip_entry(bytes, index)?,
        ArchiveKind::Rar => open_rar_entry(bytes, index)?,
    };
    preview_office_with_zip(&name, data)
}

#[tauri::command]
pub async fn archive_save_entry(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
    index: usize,
) -> AppResult<String> {
    let kind = archive_kind(&path)
        .ok_or_else(|| AppError::msg("Поддерживаются архивы ZIP и RAR"))?;
    let bytes = read_remote_bytes(&app, &state, &path).await?;
    let name = archive_entry_name(kind, &bytes, index)?;
    let data = match kind {
        ArchiveKind::Zip => open_zip_entry(bytes, index)?,
        ArchiveKind::Rar => open_rar_entry(bytes, index)?,
    };
    save_bytes_to_downloads(&app, &state, &name, &data).await
}

#[tauri::command]
pub async fn archive_open_entry(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
    index: usize,
) -> AppResult<String> {
    let kind = archive_kind(&path)
        .ok_or_else(|| AppError::msg("Поддерживаются архивы ZIP и RAR"))?;
    let bytes = read_remote_bytes(&app, &state, &path).await?;
    let name = archive_entry_name(kind, &bytes, index)?;
    let data = match kind {
        ArchiveKind::Zip => open_zip_entry(bytes, index)?,
        ArchiveKind::Rar => open_rar_entry(bytes, index)?,
    };
    open_bytes_in_system(&app, &state, &name, &data).await
}

#[tauri::command]
pub async fn save_external_file(
    app: AppHandle,
    state: State<'_, AppState>,
    remote_path: String,
    local_path: String,
) -> AppResult<String> {
    let preview_root = preview_root(&app, &state)?;
    let local = PathBuf::from(&local_path);
    if !local.starts_with(&preview_root) {
        return Err(AppError::msg("Недоступный временный файл"));
    }
    if !local.is_file() {
        return Err(AppError::msg("Временный файл не найден"));
    }
    let cfg = state.config();
    let password = state.password()?;
    let effective =
        zt_proxy::effective_smb_config(&app, &cfg.smb, &cfg.zerotier_network_id).await?;
    let client = state.inner.smb.connect(&effective, &password).await?;
    let written = state
        .inner
        .smb
        .upload_file(&client, &normalize_rel(&remote_path), &local)
        .await?;
    state.inner.smb.reset().await;
    Ok(format!(
        "Сохранено на NAS: {} ({written} байт)",
        remote_path
    ))
}

#[tauri::command]
pub async fn external_file_signature(
    app: AppHandle,
    state: State<'_, AppState>,
    local_path: String,
) -> AppResult<String> {
    let preview_root = preview_root(&app, &state)?;
    let local = PathBuf::from(&local_path);
    if !local.starts_with(&preview_root) {
        return Err(AppError::msg("Недоступный временный файл"));
    }
    tokio::task::spawn_blocking(move || file_signature(&local))
        .await
        .map_err(|error| AppError::msg(error.to_string()))?
}

#[tauri::command]
pub async fn open_remote_file(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> AppResult<ExternalFile> {
    let bytes = read_remote_bytes(&app, &state, &path).await?;
    let name = path.rsplit('/').next().unwrap_or(&path).to_string();
    let local_path = open_bytes_in_system(&app, &state, &name, &bytes).await?;
    let signature = file_signature(Path::new(&local_path))?;
    Ok(ExternalFile {
        local_path,
        signature,
    })
}
