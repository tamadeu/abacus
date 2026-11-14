//! Casca desktop do Ábaco: guarda a pasta de trabalho aberta e expõe o
//! motor de planilhas para a interface por meio de comandos do Tauri.

mod engine;

use std::sync::Mutex;

use engine::{CellView, Engine};
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

/// Grava o conteúdo de uma célula. Texto vazio apaga a célula.
#[tauri::command]
fn set_cell(state: State<'_, AppState>, col: u32, row: u32, input: String) -> Result<(), String> {
    state.with(|engine| engine.set_cell(col, row, &input))
}

/// Devolve as células preenchidas dentro da janela visível, já calculadas.
#[tauri::command]
fn get_view(
    state: State<'_, AppState>,
    first_col: u32,
    first_row: u32,
    cols: u32,
    rows: u32,
) -> Result<Vec<CellView>, String> {
    state.with(|engine| engine.view(first_col, first_row, cols, rows))
}

#[tauri::command]
fn undo(state: State<'_, AppState>) -> Result<(), String> {
    state.with(Engine::undo)
}

#[tauri::command]
fn redo(state: State<'_, AppState>) -> Result<(), String> {
    state.with(Engine::redo)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let engine = Engine::new().expect("erro ao criar a pasta de trabalho");
    tauri::Builder::default()
        .manage(AppState { engine: Mutex::new(engine) })
        .invoke_handler(tauri::generate_handler![set_cell, get_view, undo, redo])
        .run(tauri::generate_context!())
        .expect("erro ao iniciar o Ábaco");
}
