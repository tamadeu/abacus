//! Casca desktop do Ábaco: guarda a pasta de trabalho aberta e expõe o
//! motor de planilhas para a interface por meio de comandos do Tauri.

mod engine;

use std::path::PathBuf;
use std::sync::Mutex;

use engine::{CellView, Engine, SheetInfo};
use tauri::State;

/// Pasta de trabalho aberta na janela.
struct AppState {
    engine: Mutex<Engine>,
}

impl AppState {
    fn with<T>(&self, action: impl FnOnce(&mut Engine) -> Result<T, String>) -> Result<T, String> {
        let mut engine = self.engine.lock().map_err(|e| e.to_string())?;
        action(&mut engine)
    }
}

/// Confere se o caminho termina em `.xlsx`, o único formato aceito.
fn xlsx_path(path: String) -> Result<PathBuf, String> {
    let path = PathBuf::from(path);
    let is_xlsx = path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("xlsx"));
    if is_xlsx { Ok(path) } else { Err("o Ábaco só abre e salva arquivos .xlsx".into()) }
}

/// Grava o conteúdo de uma célula. Texto vazio apaga a célula.
#[tauri::command]
fn set_cell(state: State<'_, AppState>, sheet: u32, col: u32, row: u32, input: String) -> Result<(), String> {
    state.with(|engine| engine.set_cell(sheet, col, row, &input))
}

/// Devolve as células com conteúdo ou estilo dentro da janela visível.
#[tauri::command]
fn get_view(
    state: State<'_, AppState>,
    sheet: u32,
    first_col: u32,
    first_row: u32,
    cols: u32,
    rows: u32,
) -> Result<Vec<CellView>, String> {
    state.with(|engine| engine.view(sheet, first_col, first_row, cols, rows))
}

#[tauri::command]
fn get_sheets(state: State<'_, AppState>) -> Result<Vec<SheetInfo>, String> {
    state.with(|engine| Ok(engine.sheets()))
}

/// Cria uma planilha vazia e devolve o índice dela.
#[tauri::command]
fn add_sheet(state: State<'_, AppState>) -> Result<u32, String> {
    state.with(Engine::add_sheet)
}

#[tauri::command]
fn undo(state: State<'_, AppState>) -> Result<(), String> {
    state.with(Engine::undo)
}

#[tauri::command]
fn redo(state: State<'_, AppState>) -> Result<(), String> {
    state.with(Engine::redo)
}

/// Troca a pasta de trabalho aberta pelo conteúdo de um arquivo `.xlsx`.
/// Se o arquivo não puder ser lido, a pasta atual continua aberta.
#[tauri::command]
fn open_file(state: State<'_, AppState>, path: String) -> Result<(), String> {
    let opened = Engine::open(&xlsx_path(path)?)?;
    state.with(|engine| {
        *engine = opened;
        Ok(())
    })
}

/// Salva a pasta de trabalho num arquivo `.xlsx`.
#[tauri::command]
fn save_file(state: State<'_, AppState>, path: String) -> Result<(), String> {
    let path = xlsx_path(path)?;
    state.with(|engine| engine.save(&path))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let engine = Engine::new().expect("erro ao criar a pasta de trabalho");
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState { engine: Mutex::new(engine) })
        .invoke_handler(tauri::generate_handler![
            set_cell, get_view, get_sheets, add_sheet, undo, redo, open_file, save_file
        ])
        .run(tauri::generate_context!())
        .expect("erro ao iniciar o Ábaco");
}
