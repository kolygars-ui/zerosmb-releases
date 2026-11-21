use tauri::AppHandle;

pub async fn update(
    app: &AppHandle,
    title: &str,
    text: &str,
    current: usize,
    total: usize,
    done: bool,
) {
    let _ = (app, title, text, current, total, done);
    #[cfg(target_os = "android")]
    {
        let _: Result<serde_json::Value, crate::error::AppError> = crate::android_bridge::call(
            app,
            "transferProgress",
            serde_json::json!({
                "title": title,
                "text": text,
                "current": current,
                "total": total,
                "done": done,
            }),
        )
        .await;
    }
}
