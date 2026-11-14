//! Casca desktop do Ábaco: guarda a pasta de trabalho aberta e expõe o
//! núcleo em Rust para a interface por meio de comandos do Tauri.

use std::sync::Mutex;

use abacus_core::{CellRef, Value, Workbook};
use abacus_formula::Evaluator;
use serde::Serialize;
use tauri::State;

/// Pasta de trabalho aberta na janela.
struct AppState {
    workbook: Mutex<Workbook>,
}

/// O que a interface precisa saber para desenhar uma célula.
#[derive(Serialize)]
struct CellView {
    col: u32,
    row: u32,
    /// Texto digitado (o que aparece na barra de fórmulas).
    input: String,
    /// Texto mostrado na grade.
    display: String,
    kind: CellKind,
}

#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
enum CellKind {
    Number,
    Text,
    Error,
}

fn cell_ref(col: u32, row: u32) -> Result<CellRef, String> {
    CellRef::new(col, row).ok_or_else(|| format!("célula fora da planilha: coluna {col}, linha {row}"))
}

/// Grava o conteúdo de uma célula da primeira planilha. Texto vazio apaga.
#[tauri::command]
fn set_cell(state: State<'_, AppState>, col: u32, row: u32, input: String) -> Result<(), String> {
    let cell = cell_ref(col, row)?;
    let mut workbook = state.workbook.lock().map_err(|e| e.to_string())?;
    workbook.sheets[0].set_input(cell, input);
    Ok(())
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
    let workbook = state.workbook.lock().map_err(|e| e.to_string())?;
    let sheet = &workbook.sheets[0];
    let mut evaluator = Evaluator::new(sheet);
    let last_col = first_col.saturating_add(cols);
    let last_row = first_row.saturating_add(rows);

    let view = sheet
        .cells()
        .filter(|(cell, _)| {
            (first_col..last_col).contains(&cell.col) && (first_row..last_row).contains(&cell.row)
        })
        .map(|(cell, input)| {
            let value = evaluator.value(cell);
            let kind = match value {
                Value::Number(_) => CellKind::Number,
                Value::Error(_) => CellKind::Error,
                Value::Text(_) | Value::Empty => CellKind::Text,
            };
            CellView { col: cell.col, row: cell.row, input: input.to_string(), display: value.display(), kind }
        })
        .collect();
    Ok(view)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState { workbook: Mutex::new(Workbook::new()) })
        .invoke_handler(tauri::generate_handler![set_cell, get_view])
        .run(tauri::generate_context!())
        .expect("erro ao iniciar o Ábaco");
}
