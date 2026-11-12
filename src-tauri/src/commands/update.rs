use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tauri::ipc::Channel;
use tauri_plugin_updater::{Update, UpdaterExt};
use tauri_specta::Event;

use crate::error::{Error, Result};

#[derive(Serialize, specta::Type)]
pub struct UpdateInfo {
    version: String,
    notes: Option<String>,
    pub_date: Option<String>,
}

#[derive(Serialize, Clone, specta::Type)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UpdateProgress {
    Started { total: Option<u64> },
    Progress { downloaded: u64, total: Option<u64> },
    Finished,
}

#[derive(Serialize, Deserialize, Clone, specta::Type, tauri_specta::Event)]
pub struct UpdateAvailable {
    version: String,
    notes: Option<String>,
}

fn update_info(update: &Update) -> UpdateInfo {
    UpdateInfo {
        version: update.version.clone(),
        notes: update.body.clone(),
        pub_date: update.raw_json["pub_date"].as_str().map(str::to_string),
    }
}

#[tauri::command]
#[specta::specta]
pub async fn update_check(app: AppHandle) -> Result<Option<UpdateInfo>> {
    Ok(app.updater()?.check().await?.as_ref().map(update_info))
}

#[tauri::command]
#[specta::specta]
pub async fn update_install(app: AppHandle, on_progress: Channel<UpdateProgress>) -> Result<()> {
    let Some(update) = app.updater()?.check().await? else {
        return Err(Error::Invalid("새 버전이 없어요".into()));
    };
    let mut downloaded = 0u64;
    let bytes = update
        .download(
            |chunk, total| {
                if downloaded == 0 {
                    let _ = on_progress.send(UpdateProgress::Started { total });
                }
                downloaded += chunk as u64;
                let _ = on_progress.send(UpdateProgress::Progress { downloaded, total });
            },
            || {
                let _ = on_progress.send(UpdateProgress::Finished);
            },
        )
        .await?;
    update.install(bytes)?;
    Ok(())
}

pub async fn announce_update(app: AppHandle) {
    let updater = match app.updater() {
        Ok(updater) => updater,
        Err(e) => {
            log::warn!("updater unavailable: {e}");
            return;
        }
    };
    let update = match updater.check().await {
        Ok(Some(update)) => update,
        Ok(None) => return,
        Err(e) => {
            log::warn!("update check failed: {e}");
            return;
        }
    };
    let event = UpdateAvailable {
        version: update.version.clone(),
        notes: update.body.clone(),
    };
    // on the main thread for the reason in `popups::notify`
    let handle = app.clone();
    let sent = app.run_on_main_thread(move || {
        if let Err(e) = event.emit(&handle) {
            log::warn!("update event failed: {e}");
        }
    });
    if let Err(e) = sent {
        log::warn!("update event failed: {e}");
    }
}
