use serde::Serialize;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferItem {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub total: u64,
    pub transferred: u64,
    pub status: String,
    pub error: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferSnapshot {
    pub items: Vec<TransferItem>,
}

pub struct TransferManager {
    items: Mutex<Vec<TransferItem>>,
}

impl TransferManager {
    pub fn new() -> Self {
        Self {
            items: Mutex::new(Vec::new()),
        }
    }

    pub fn begin_many(
        &self,
        app: &AppHandle,
        kind: &str,
        files: Vec<(String, u64)>,
    ) -> Vec<String> {
        let mut items = self.items.lock().unwrap();
        if items.len() > 200 {
            items.retain(|item| item.status == "queued" || item.status == "active");
        }
        let mut ids = Vec::with_capacity(files.len());
        for (name, total) in files {
            let id = uuid::Uuid::new_v4().to_string();
            items.push(TransferItem {
                id: id.clone(),
                kind: kind.to_string(),
                name,
                total,
                transferred: 0,
                status: "queued".to_string(),
                error: String::new(),
            });
            ids.push(id);
        }
        drop(items);
        emit(app, &self.snapshot());
        ids
    }

    pub fn start(&self, app: &AppHandle, id: &str) {
        self.update(id, |item| {
            item.status = "active".to_string();
        });
        emit(app, &self.snapshot());
    }

    pub fn progress(&self, app: &AppHandle, id: &str, transferred: u64, total: u64) {
        self.update(id, |item| {
            item.transferred = transferred;
            item.total = total;
            item.status = "active".to_string();
        });
        emit(app, &self.snapshot());
    }

    pub fn finish(&self, app: &AppHandle, id: &str, error: Option<String>) {
        self.update(id, |item| {
            if let Some(error) = error {
                item.status = "error".to_string();
                item.error = error;
            } else {
                item.status = "done".to_string();
                item.error.clear();
                if item.total > 0 {
                    item.transferred = item.total;
                }
            }
        });
        emit(app, &self.snapshot());
    }

    pub fn clear_completed(&self, app: &AppHandle) {
        let mut items = self.items.lock().unwrap();
        items.retain(|item| item.status == "queued" || item.status == "active");
        drop(items);
        emit(app, &self.snapshot());
    }

    pub fn snapshot(&self) -> TransferSnapshot {
        TransferSnapshot {
            items: self.items.lock().unwrap().clone(),
        }
    }

    fn update(&self, id: &str, f: impl FnOnce(&mut TransferItem)) {
        let mut items = self.items.lock().unwrap();
        if let Some(item) = items.iter_mut().find(|item| item.id == id) {
            f(item);
        }
    }
}

fn emit(app: &AppHandle, snapshot: &TransferSnapshot) {
    let _ = app.emit("transfer-update", snapshot);
}

#[tauri::command]
pub fn transfer_snapshot(state: tauri::State<'_, crate::state::AppState>) -> TransferSnapshot {
    state.inner.transfers.snapshot()
}

#[tauri::command]
pub fn transfer_clear_completed(
    app: AppHandle,
    state: tauri::State<'_, crate::state::AppState>,
) -> TransferSnapshot {
    state.inner.transfers.clear_completed(&app);
    state.inner.transfers.snapshot()
}
