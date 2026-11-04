use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use banner_core::check::export_check as core_export_check;
use banner_core::check::ExportWarning;
use banner_core::export::{export_png, export_svg};
use banner_core::image as cimg;
use banner_core::model::*;
use banner_core::render::render_layers as core_render_layers;
use banner_core::store::ProjectHandle;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::DialogExt;

use crate::agent::{self, img, prompt::TurnKind, SendContext};
use crate::settings::{SettingsPatch, SettingsView};
use crate::state::AppState;

type CmdResult<T> = Result<T, String>;

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

#[derive(Serialize)]
pub struct ProjectSummaryView {
    id: String,
    name: String,
    width_mm: f64,
    height_mm: f64,
    updated_at: String,
    thumbnail_png: Option<String>,
}

#[derive(Serialize)]
pub struct VersionView {
    seq: u64,
    ts: String,
    actor: Actor,
    summary: String,
    turn: Option<String>,
    thumbnail_png: Option<String>,
}

#[derive(Serialize)]
pub struct OpenedProject {
    project: Project,
    document: Document,
    memory: String,
    chat: Vec<Value>,
    versions: Vec<VersionView>,
    can_undo: bool,
    can_redo: bool,
}

#[derive(Serialize)]
pub struct ApplyResult {
    document: Document,
    entries: Vec<OpEntry>,
    can_undo: bool,
    can_redo: bool,
}

#[derive(Deserialize)]
pub struct CreateArgs {
    name: String,
    width_mm: f64,
    height_mm: f64,
    #[serde(default)]
    original_path: Option<String>,
}

#[derive(Serialize)]
pub struct LayerRasterView {
    id: String,
    png_base64: String,
    x_px: i32,
    y_px: i32,
    w_px: u32,
    h_px: u32,
}

#[derive(Serialize)]
pub struct RenderResultView {
    scale_px_per_mm: f64,
    width_px: u32,
    height_px: u32,
    layers: Vec<LayerRasterView>,
}

#[derive(Deserialize)]
pub struct ExportOptionsIn {
    format: String,
    scale: u32,
    dpi: f64,
    #[serde(default)]
    outline_text: bool,
}

#[derive(Serialize)]
pub struct ExportResultView {
    path: String,
    width_px: Option<u32>,
    height_px: Option<u32>,
    bytes: u64,
    warnings: Vec<ExportWarning>,
}

#[derive(Serialize)]
pub struct ImageInfoView {
    width_px: u32,
    height_px: u32,
    preview_png: String,
}

fn file_data_url(path: &Path) -> Option<String> {
    std::fs::read(path).ok().map(|b| img::data_url(&b))
}

fn versions_view(h: &ProjectHandle) -> Vec<VersionView> {
    h.versions()
        .unwrap_or_default()
        .into_iter()
        .map(|v| VersionView { thumbnail_png: file_data_url(&h.version_thumbnail_path(v.seq)), seq: v.seq, ts: v.ts, actor: v.actor, summary: v.summary, turn: v.turn })
        .collect()
}

fn apply_result(h: &ProjectHandle, entries: Vec<OpEntry>) -> ApplyResult {
    ApplyResult { document: h.document.clone(), entries, can_undo: h.can_undo(), can_redo: h.can_redo() }
}

fn refresh_thumbnail(state: &AppState, h: &mut ProjectHandle) {
    if let Ok(png) = img::thumbnail_png(h, &state.fonts) {
        let _ = h.write_thumbnail(&png);
    }
}

#[tauri::command]
pub fn list_projects(state: State<'_, AppState>) -> CmdResult<Vec<ProjectSummaryView>> {
    let store = state.store();
    std::fs::create_dir_all(store.root()).map_err(err)?;
    Ok(store
        .list()
        .map_err(err)?
        .into_iter()
        .map(|p| ProjectSummaryView { thumbnail_png: p.thumbnail.as_deref().and_then(file_data_url), id: p.id, name: p.name, width_mm: p.width_mm, height_mm: p.height_mm, updated_at: p.updated_at })
        .collect())
}

#[tauri::command]
pub fn create_project(state: State<'_, AppState>, args: CreateArgs) -> CmdResult<Project> {
    if args.name.trim().is_empty() || args.width_mm <= 0.0 || args.height_mm <= 0.0 {
        return Err("이름과 크기를 확인해 주세요".into());
    }
    let store = state.store();
    std::fs::create_dir_all(store.root()).map_err(err)?;
    let mut h = store.create(args.name.trim(), args.width_mm, args.height_mm).map_err(err)?;
    if let Some(p) = args.original_path.as_deref().filter(|p| !p.is_empty()) {
        h.add_original(Path::new(p)).map_err(err)?;
        if let Ok(orig) = cimg::load(&h.original_path(1)) {
            if let Ok(png) = cimg::to_png_bytes(&cimg::resize_max(&orig, 640)) {
                let _ = h.write_thumbnail(&png);
            }
        }
    } else {
        refresh_thumbnail(&state, &mut h);
    }
    let project = h.project.clone();
    state.open.lock().map_err(err)?.insert(project.id.clone(), std::sync::Arc::new(std::sync::Mutex::new(h)));
    Ok(project)
}

#[tauri::command]
pub fn delete_project(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    state.forget(&id);
    state.store().delete(&id).map_err(err)
}

#[tauri::command]
pub fn rename_project(state: State<'_, AppState>, id: String, name: String) -> CmdResult<Project> {
    let h = state.handle(&id).map_err(err)?;
    let mut h = img::lock(&h);
    h.rename(name.trim()).map_err(err)?;
    Ok(h.project.clone())
}

#[tauri::command]
pub fn duplicate_project(state: State<'_, AppState>, id: String) -> CmdResult<Project> {
    let h = state.store().duplicate(&id).map_err(err)?;
    Ok(h.project.clone())
}

#[tauri::command]
pub fn open_project(state: State<'_, AppState>, id: String) -> CmdResult<OpenedProject> {
    let h = state.handle(&id).map_err(err)?;
    let h = img::lock(&h);
    Ok(OpenedProject { project: h.project.clone(), document: h.document.clone(), memory: h.memory().unwrap_or_default(), chat: h.chat().unwrap_or_default(), versions: versions_view(&h), can_undo: h.can_undo(), can_redo: h.can_redo() })
}

#[tauri::command]
pub fn show_in_folder(app: AppHandle, state: State<'_, AppState>, id: String) -> CmdResult<()> {
    let dir = state.store().root().join(&id);
    tauri_plugin_opener::reveal_item_in_dir(&dir).map_err(err)?;
    let _ = app;
    Ok(())
}

#[tauri::command]
pub fn apply_ops(state: State<'_, AppState>, id: String, ops: Vec<Op>, actor: Actor) -> CmdResult<ApplyResult> {
    let h = state.handle(&id).map_err(err)?;
    let mut h = img::lock(&h);
    let entries = h.apply_ops(ops, actor, None).map_err(err)?;
    refresh_thumbnail(&state, &mut h);
    let recent_user = h.versions().unwrap_or_default().into_iter().next().map(|v| v.actor == Actor::User && chrono::DateTime::parse_from_rfc3339(&v.ts).map(|t| (chrono::Utc::now() - t.with_timezone(&chrono::Utc)).num_seconds() < 60).unwrap_or(false)).unwrap_or(false);
    if !recent_user {
        let thumb = img::thumbnail_png(&h, &state.fonts).ok();
        let summary = format!("직접 편집 · {}", entries.first().map(|e| banner_core::ops::describe(&e.inverse, &h.document)).unwrap_or_default());
        let _ = h.snapshot(Actor::User, &summary, None, thumb.as_deref());
    }
    Ok(apply_result(&h, entries))
}

#[tauri::command]
pub fn undo(state: State<'_, AppState>, id: String) -> CmdResult<ApplyResult> {
    let h = state.handle(&id).map_err(err)?;
    let mut h = img::lock(&h);
    let entries = h.undo().map_err(err)?.unwrap_or_default();
    refresh_thumbnail(&state, &mut h);
    Ok(apply_result(&h, entries))
}

#[tauri::command]
pub fn redo(state: State<'_, AppState>, id: String) -> CmdResult<ApplyResult> {
    let h = state.handle(&id).map_err(err)?;
    let mut h = img::lock(&h);
    let entries = h.redo().map_err(err)?.unwrap_or_default();
    refresh_thumbnail(&state, &mut h);
    Ok(apply_result(&h, entries))
}

#[tauri::command]
pub fn restore_version(state: State<'_, AppState>, id: String, seq: u64) -> CmdResult<ApplyResult> {
    let h = state.handle(&id).map_err(err)?;
    let mut h = img::lock(&h);
    let entries = h.restore_version(seq).map_err(err)?;
    refresh_thumbnail(&state, &mut h);
    let thumb = img::thumbnail_png(&h, &state.fonts).ok();
    let _ = h.snapshot(Actor::User, &format!("{seq}번으로 되돌림 · 나"), Some(format!("restore:{seq}")), thumb.as_deref());
    Ok(apply_result(&h, entries))
}

#[tauri::command]
pub fn render_layers(state: State<'_, AppState>, id: String, scale_px_per_mm: f64) -> CmdResult<RenderResultView> {
    let h = state.handle(&id).map_err(err)?;
    let h = img::lock(&h);
    let scale = scale_px_per_mm.clamp(0.01, 40.0);
    let rasters = core_render_layers(&h.document, &h.assets_dir(), &state.fonts, scale).map_err(err)?;
    let mut layers = Vec::new();
    let mut bg_doc = h.document.clone();
    bg_doc.layers.clear();
    let bg = banner_core::render::render_document(&bg_doc, &h.assets_dir(), &state.fonts, &banner_core::render::RenderOptions { scale_px_per_mm: scale, background: true }).map_err(err)?;
    layers.push(LayerRasterView { id: "__background".into(), png_base64: cimg::to_base64_png(&bg).map_err(err)?, x_px: 0, y_px: 0, w_px: bg.width(), h_px: bg.height() });
    for r in rasters {
        let png = match &r.image {
            Some(im) => cimg::to_base64_png(im).map_err(err)?,
            None => continue,
        };
        layers.push(LayerRasterView { id: r.id, png_base64: png, x_px: r.x_px, y_px: r.y_px, w_px: r.w_px, h_px: r.h_px });
    }
    Ok(RenderResultView { scale_px_per_mm: scale, width_px: (h.document.artboard.width_mm * scale).round() as u32, height_px: (h.document.artboard.height_mm * scale).round() as u32, layers })
}

#[tauri::command]
pub fn render_original(state: State<'_, AppState>, id: String, version: u32, max_px: u32) -> CmdResult<String> {
    let h = state.handle(&id).map_err(err)?;
    let h = img::lock(&h);
    let orig = img::original_image(&h, version).map_err(err)?;
    let small = cimg::resize_max(&orig, max_px.clamp(64, 4096));
    Ok(img::data_url(&cimg::to_png_bytes(&small).map_err(err)?))
}

#[tauri::command]
pub fn render_composite(state: State<'_, AppState>, id: String, max_px: u32) -> CmdResult<String> {
    let h = state.handle(&id).map_err(err)?;
    let h = img::lock(&h);
    let im = img::work_image(&h, &state.fonts, max_px.clamp(64, 4096)).map_err(err)?;
    Ok(img::data_url(&cimg::to_png_bytes(&im).map_err(err)?))
}

fn default_export_path(app: &AppHandle, h: &ProjectHandle, scale: u32, ext: &str) -> PathBuf {
    let dir = app.path().desktop_dir().unwrap_or_else(|_| h.dir.clone());
    let safe: String = h.project.name.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
    dir.join(format!("{}_1to{}.{}", safe, scale, ext))
}

#[tauri::command]
pub fn export_project(app: AppHandle, state: State<'_, AppState>, id: String, options: ExportOptionsIn, dest_path: Option<String>) -> CmdResult<ExportResultView> {
    let h = state.handle(&id).map_err(err)?;
    let h = img::lock(&h);
    let scale = if options.scale == 10 { 10 } else { 1 };
    let ext = if options.format == "svg" { "svg" } else { "png" };
    let out = dest_path.filter(|p| !p.is_empty()).map(PathBuf::from).unwrap_or_else(|| default_export_path(&app, &h, scale, ext));
    let warnings = core_export_check(&h.document, options.dpi);
    let info = if ext == "svg" { export_svg(&h.document, &h.assets_dir(), &state.fonts, scale, options.outline_text, &out) } else { export_png(&h.document, &h.assets_dir(), &state.fonts, options.dpi, scale, &out) }.map_err(err)?;
    Ok(ExportResultView { path: info.path, width_px: info.width_px, height_px: info.height_px, bytes: info.bytes, warnings })
}

#[tauri::command]
pub fn export_check(state: State<'_, AppState>, id: String, dpi: f64) -> CmdResult<Vec<ExportWarning>> {
    let h = state.handle(&id).map_err(err)?;
    let h = img::lock(&h);
    Ok(core_export_check(&h.document, dpi))
}

#[tauri::command]
pub fn replace_original(app: AppHandle, state: State<'_, AppState>, id: String, path: String) -> CmdResult<Project> {
    let h = state.handle(&id).map_err(err)?;
    let project = {
        let mut g = img::lock(&h);
        g.add_original(Path::new(&path)).map_err(err)?;
        g.project.clone()
    };
    agent::spawn_turn(app, id, "원본을 새 이미지로 바꿨어요.".into(), SendContext::default(), TurnKind::ReplaceOriginal);
    Ok(project)
}

#[tauri::command]
pub fn agent_send(app: AppHandle, state: State<'_, AppState>, id: String, text: String, context: SendContext) -> CmdResult<String> {
    let h = state.handle(&id).map_err(err)?;
    let kind = {
        let g = img::lock(&h);
        if agent::is_first_turn(&g) {
            if g.project.originals.is_empty() { TurnKind::Scratch } else { TurnKind::Reconstruct }
        } else {
            TurnKind::Normal
        }
    };
    if let Some(run) = state.agents.lock().map_err(err)?.get(&id) {
        if run.running.load(Ordering::SeqCst) {
            return Err("도우미가 아직 작업 중이에요".into());
        }
    }
    agent::spawn_turn(app, id.clone(), text, context, kind);
    Ok(id)
}

#[tauri::command]
pub fn agent_stop(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    if let Some(run) = state.agents.lock().map_err(err)?.get(&id) {
        run.request_stop();
    }
    Ok(())
}

#[tauri::command]
pub fn agent_answer(state: State<'_, AppState>, id: String, question_id: String, answer: String) -> CmdResult<()> {
    let delivered = state.agents.lock().map_err(err)?.get(&id).map(|r| r.answer(&question_id, &answer)).unwrap_or(false);
    if !delivered {
        return Err("답할 질문이 없어요".into());
    }
    if let Ok(h) = state.handle(&id) {
        let g = img::lock(&h);
        if let Ok(chat) = g.chat() {
            if let Some(mut item) = chat.into_iter().find(|c| c["question"]["id"] == json!(question_id)) {
                item["answer"] = json!(answer);
                let cid = item["id"].as_str().unwrap_or("").to_string();
                let _ = g.update_chat(&cid, &item);
            }
        }
    }
    Ok(())
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> CmdResult<SettingsView> {
    Ok(state.settings_snapshot().view())
}

#[tauri::command]
pub fn set_settings(state: State<'_, AppState>, patch: SettingsPatch) -> CmdResult<SettingsView> {
    {
        let mut s = state.settings.lock().map_err(err)?;
        if let Some(k) = patch.openai_key {
            s.set_openai_key(&k);
        }
        if let Some(d) = patch.projects_dir.filter(|d| !d.trim().is_empty()) {
            s.projects_dir = PathBuf::from(d.trim());
        }
        if let Some(m) = patch.agent_model.filter(|m| !m.trim().is_empty()) {
            s.agent_model = m;
        }
        if let Some(m) = patch.heavy_model.filter(|m| !m.trim().is_empty()) {
            s.heavy_model = m;
        }
        if let Some(m) = patch.cheap_model.filter(|m| !m.trim().is_empty()) {
            s.cheap_model = m;
        }
        if let Some(m) = patch.image_model.filter(|m| !m.trim().is_empty()) {
            s.image_model = m;
        }
    }
    state.save_settings().map_err(err)?;
    state.open.lock().map_err(err)?.clear();
    Ok(state.settings_snapshot().view())
}

#[tauri::command]
pub async fn pick_image_file(app: AppHandle) -> CmdResult<Option<String>> {
    let picked = app.dialog().file().add_filter("이미지", &["png", "jpg", "jpeg", "webp", "bmp"]).set_title("현수막 이미지 고르기").blocking_pick_file();
    Ok(picked.and_then(|p| p.into_path().ok()).map(|p| p.to_string_lossy().into_owned()))
}

#[tauri::command]
pub async fn pick_folder(app: AppHandle) -> CmdResult<Option<String>> {
    let picked = app.dialog().file().set_title("프로젝트 저장 폴더").blocking_pick_folder();
    Ok(picked.and_then(|p| p.into_path().ok()).map(|p| p.to_string_lossy().into_owned()))
}

#[tauri::command]
pub async fn pick_save_path(app: AppHandle, default_name: String) -> CmdResult<Option<String>> {
    let ext = default_name.rsplit('.').next().unwrap_or("png").to_string();
    let mut b = app.dialog().file().set_file_name(&default_name).set_title("내보내기");
    if let Ok(d) = app.path().desktop_dir() {
        b = b.set_directory(d);
    }
    let picked = b.add_filter(ext.to_uppercase(), &[ext.as_str()]).blocking_save_file();
    Ok(picked.and_then(|p| p.into_path().ok()).map(|p| p.to_string_lossy().into_owned()))
}

#[tauri::command]
pub fn read_image_info(path: String) -> CmdResult<ImageInfoView> {
    let im = cimg::load(Path::new(&path)).map_err(err)?;
    let preview = cimg::to_png_bytes(&cimg::resize_max(&im, 640)).map_err(err)?;
    Ok(ImageInfoView { width_px: im.width(), height_px: im.height(), preview_png: img::data_url(&preview) })
}
