//! Casca desktop do Abacus: guarda a pasta de trabalho aberta e expõe o
//! motor de planilhas para a interface por meio de comandos do Tauri.

mod charts;
mod conditional;
mod engine;
mod package;
mod pivot;
mod pivot_sheet;
mod pivot_xlsx;
mod validation;

use std::path::PathBuf;
use std::sync::Mutex;

use charts::{Chart, ChartKind};
use conditional::CondRule;
use engine::{
    Axis, BorderKind, CellPos, ChartView, Direction, Engine, FillAxis, FilterOptions, Range, SheetInfo, Stats,
    Structure, StyleProperty, View,
};
use pivot::Pivot;
use tauri::{AppHandle, State};
use tauri_plugin_clipboard_manager::ClipboardExt;
use validation::Validation;

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

/// Confere se o caminho termina na extensão pedida.
fn path_with_extension(path: String, extension: &str, message: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(path);
    if path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case(extension)) { Ok(path) } else { Err(message.into()) }
}

/// Confere se o caminho termina em `.xlsx`, o único formato em que se salva a pasta.
fn xlsx_path(path: String) -> Result<PathBuf, String> {
    path_with_extension(path, "xlsx", "o Abacus só salva a pasta de trabalho em arquivos .xlsx")
}

/// Grava o conteúdo de uma célula. Texto vazio apaga a célula.
#[tauri::command]
fn set_cell(state: State<'_, AppState>, sheet: u32, col: u32, row: u32, input: String) -> Result<(), String> {
    state.with(|engine| engine.set_cell(sheet, col, row, &input))
}

/// Devolve o que cabe numa tela de `width` por `height` pixels a partir da
/// célula dada.
#[tauri::command]
fn get_view(
    state: State<'_, AppState>,
    sheet: u32,
    first_col: u32,
    first_row: u32,
    width: f64,
    height: f64,
) -> Result<View, String> {
    state.with(|engine| engine.view(sheet, first_col, first_row, width, height))
}

/// Altera uma propriedade de estilo nas células do intervalo.
#[tauri::command]
fn format_cells(
    state: State<'_, AppState>,
    sheet: u32,
    range: Range,
    property: StyleProperty,
    value: String,
) -> Result<(), String> {
    state.with(|engine| engine.format(sheet, range, property, &value))
}

/// Apaga o conteúdo das células do intervalo, mantendo o estilo.
#[tauri::command]
fn clear_cells(state: State<'_, AppState>, sheet: u32, range: Range) -> Result<(), String> {
    state.with(|engine| engine.clear(sheet, range))
}

/// Mescla o intervalo numa célula só, centralizada.
#[tauri::command]
fn merge_cells(state: State<'_, AppState>, sheet: u32, range: Range) -> Result<(), String> {
    state.with(|engine| engine.merge(sheet, range))
}

/// Desfaz as áreas mescladas que tocam o intervalo.
#[tauri::command]
fn unmerge_cells(state: State<'_, AppState>, sheet: u32, range: Range) -> Result<(), String> {
    state.with(|engine| engine.unmerge(sheet, range))
}

/// Desenha (ou remove) bordas no intervalo.
#[tauri::command]
fn set_border(state: State<'_, AppState>, sheet: u32, range: Range, kind: BorderKind) -> Result<(), String> {
    state.with(|engine| engine.set_border(sheet, range, kind))
}

/// Insere ou exclui `count` linhas ou colunas a partir de `index`.
#[tauri::command]
fn change_structure(
    state: State<'_, AppState>,
    sheet: u32,
    change: Structure,
    index: u32,
    count: u32,
) -> Result<(), String> {
    state.with(|engine| engine.change_structure(sheet, change, index, count))
}

/// Congela as primeiras colunas e linhas da planilha; zero descongela.
#[tauri::command]
fn set_frozen(state: State<'_, AppState>, sheet: u32, cols: u32, rows: u32) -> Result<(), String> {
    state.with(|engine| engine.set_frozen(sheet, cols, rows))
}

/// Oculta ou reexibe linhas ou colunas.
#[tauri::command]
fn set_hidden(
    state: State<'_, AppState>,
    sheet: u32,
    axis: Axis,
    first: u32,
    count: u32,
    hidden: bool,
) -> Result<(), String> {
    state.with(|engine| engine.set_hidden(sheet, axis, first, count, hidden))
}

/// Células da planilha que contêm o texto.
#[tauri::command]
fn find_cells(state: State<'_, AppState>, sheet: u32, query: String) -> Result<Vec<CellPos>, String> {
    state.with(|engine| engine.find(sheet, &query))
}

/// Troca o texto procurado numa célula. Devolve se havia o que trocar.
#[tauri::command]
fn replace_cell(
    state: State<'_, AppState>,
    sheet: u32,
    col: u32,
    row: u32,
    query: String,
    replacement: String,
) -> Result<bool, String> {
    state.with(|engine| engine.replace(sheet, col, row, &query, &replacement))
}

/// Troca o texto na planilha inteira. Devolve quantas células mudaram.
#[tauri::command]
fn replace_all(state: State<'_, AppState>, sheet: u32, query: String, replacement: String) -> Result<u32, String> {
    state.with(|engine| engine.replace_all(sheet, &query, &replacement))
}

/// Classifica as linhas da seleção (ou da tabela em volta de uma célula) por uma coluna.
#[tauri::command]
fn sort_cells(
    state: State<'_, AppState>,
    sheet: u32,
    range: Range,
    key_col: u32,
    descending: bool,
) -> Result<Range, String> {
    state.with(|engine| engine.sort(sheet, range, key_col, descending))
}

/// Valores que dá para filtrar na coluna da célula.
#[tauri::command]
fn filter_options(state: State<'_, AppState>, sheet: u32, col: u32, row: u32) -> Result<FilterOptions, String> {
    state.with(|engine| engine.filter_options(sheet, col, row))
}

/// Mostra só as linhas com os valores escolhidos. Devolve quantas ficaram ocultas.
#[tauri::command]
fn apply_filter(
    state: State<'_, AppState>,
    sheet: u32,
    range: Range,
    col: u32,
    allowed: Vec<String>,
) -> Result<u32, String> {
    state.with(|engine| engine.apply_filter(sheet, range, col, &allowed))
}

/// Preenche a partir do intervalo até a linha ou coluna `to`, como ao
/// arrastar a alça da seleção.
#[tauri::command]
fn fill_cells(state: State<'_, AppState>, sheet: u32, range: Range, axis: FillAxis, to: u32) -> Result<(), String> {
    state.with(|engine| engine.fill(sheet, range, axis, to))
}

/// Copia (ou recorta) o intervalo para a área de transferência do sistema.
#[tauri::command]
fn copy_cells(app: AppHandle, state: State<'_, AppState>, sheet: u32, range: Range, cut: bool) -> Result<(), String> {
    let text = state.with(|engine| engine.copy(sheet, range, cut))?;
    app.clipboard().write_text(text).map_err(|e| format!("não foi possível gravar na área de transferência: {e}"))
}

/// Cola a área de transferência do sistema a partir da célula dada e devolve
/// o intervalo preenchido, ou `None` se não havia o que colar.
#[tauri::command]
fn paste_cells(
    app: AppHandle,
    state: State<'_, AppState>,
    sheet: u32,
    col: u32,
    row: u32,
) -> Result<Option<Range>, String> {
    // A leitura falha quando a área de transferência está vazia ou guarda
    // algo que não é texto, como uma imagem; nos dois casos não há o que colar.
    let text = app.clipboard().read_text().unwrap_or_default();
    state.with(|engine| engine.paste(sheet, col, row, &text))
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
fn rename_sheet(state: State<'_, AppState>, sheet: u32, name: String) -> Result<(), String> {
    state.with(|engine| engine.rename_sheet(sheet, &name))
}

#[tauri::command]
fn delete_sheet(state: State<'_, AppState>, sheet: u32) -> Result<(), String> {
    state.with(|engine| engine.delete_sheet(sheet))
}

/// Define a largura de uma coluna, em pixels.
#[tauri::command]
fn set_col_width(state: State<'_, AppState>, sheet: u32, col: u32, width: f64) -> Result<(), String> {
    state.with(|engine| engine.set_col_width(sheet, col, width))
}

/// Define a altura de uma linha, em pixels.
#[tauri::command]
fn set_row_height(state: State<'_, AppState>, sheet: u32, row: u32, height: f64) -> Result<(), String> {
    state.with(|engine| engine.set_row_height(sheet, row, height))
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
    let path = PathBuf::from(path);
    let opened = match path.extension().and_then(|ext| ext.to_str()).map(str::to_ascii_lowercase).as_deref() {
        Some("xlsx") => Engine::open(&path)?,
        Some("csv") => Engine::from_csv(&path)?,
        _ => return Err("o Abacus só abre arquivos .xlsx e .csv".into()),
    };
    state.with(|engine| {
        *engine = opened;
        Ok(())
    })
}

/// Grava uma planilha como CSV, sem alterar a pasta aberta.
#[tauri::command]
fn export_csv(state: State<'_, AppState>, sheet: u32, path: String) -> Result<(), String> {
    let path = path_with_extension(path, "csv", "o arquivo precisa terminar em .csv")?;
    state.with(|engine| engine.save_csv(sheet, &path))
}

/// Regras de formatação condicional de uma planilha.
#[tauri::command]
fn list_cond_rules(state: State<'_, AppState>, sheet: u32) -> Result<Vec<CondRule>, String> {
    state.with(|engine| engine.cond_rules(sheet))
}

/// Cria uma regra de formatação condicional.
#[tauri::command]
fn add_cond_rule(state: State<'_, AppState>, sheet: u32, rule: CondRule) -> Result<(), String> {
    state.with(|engine| engine.add_cond_rule(sheet, rule))
}

/// Troca uma regra de formatação condicional.
#[tauri::command]
fn update_cond_rule(state: State<'_, AppState>, sheet: u32, rule: CondRule) -> Result<(), String> {
    state.with(|engine| engine.update_cond_rule(sheet, rule))
}

/// Apaga uma regra de formatação condicional.
#[tauri::command]
fn delete_cond_rule(state: State<'_, AppState>, sheet: u32, index: u32) -> Result<(), String> {
    state.with(|engine| engine.delete_cond_rule(sheet, index))
}

/// Regras de validação de dados de uma planilha.
#[tauri::command]
fn list_validations(state: State<'_, AppState>, sheet: u32) -> Result<Vec<Validation>, String> {
    state.with(|engine| Ok(engine.validations(sheet)))
}

/// Cria uma regra de validação de dados.
#[tauri::command]
fn add_validation(state: State<'_, AppState>, sheet: u32, rule: Validation) -> Result<Validation, String> {
    state.with(|engine| engine.add_validation(sheet, rule))
}

/// Troca uma regra de validação de dados.
#[tauri::command]
fn update_validation(state: State<'_, AppState>, sheet: u32, rule: Validation) -> Result<Validation, String> {
    state.with(|engine| engine.update_validation(sheet, rule))
}

/// Apaga uma regra de validação de dados.
#[tauri::command]
fn delete_validation(state: State<'_, AppState>, id: u32) -> Result<(), String> {
    state.with(|engine| engine.delete_validation(id))
}

/// Itens da lista suspensa de uma célula, se ela tiver uma.
#[tauri::command]
fn list_options(state: State<'_, AppState>, sheet: u32, col: u32, row: u32) -> Result<Option<Vec<String>>, String> {
    state.with(|engine| engine.list_options(sheet, col, row))
}

/// Tabelas dinâmicas que aparecem numa planilha.
#[tauri::command]
fn list_pivots(state: State<'_, AppState>, sheet: u32) -> Result<Vec<Pivot>, String> {
    state.with(|engine| Ok(engine.pivots(sheet)))
}

/// Nomes dos campos de uma origem de dados.
#[tauri::command]
fn pivot_fields(state: State<'_, AppState>, sheet: u32, source: String) -> Result<Vec<String>, String> {
    state.with(|engine| engine.pivot_fields(sheet, &source))
}

/// Itens de um campo, para escolher o que filtrar.
#[tauri::command]
fn pivot_items(
    state: State<'_, AppState>,
    sheet: u32,
    source: String,
    field: pivot::Field,
) -> Result<Vec<String>, String> {
    state.with(|engine| engine.pivot_items(sheet, &source, field))
}

/// Cria uma tabela dinâmica.
#[tauri::command]
fn add_pivot(state: State<'_, AppState>, pivot: Pivot) -> Result<Pivot, String> {
    state.with(|engine| engine.add_pivot(pivot))
}

/// Troca a definição de uma tabela dinâmica e a redesenha.
#[tauri::command]
fn update_pivot(state: State<'_, AppState>, pivot: Pivot) -> Result<Pivot, String> {
    state.with(|engine| engine.update_pivot(pivot))
}

/// Lê a origem de novo e redesenha a tabela dinâmica.
#[tauri::command]
fn refresh_pivot(state: State<'_, AppState>, id: u32) -> Result<Pivot, String> {
    state.with(|engine| engine.refresh_pivot(id))
}

/// Atualiza todas as tabelas dinâmicas.
#[tauri::command]
fn refresh_all_pivots(state: State<'_, AppState>) -> Result<u32, String> {
    state.with(|engine| engine.refresh_all_pivots())
}

/// Apaga a definição de uma tabela dinâmica (e a grade, se `clear`).
#[tauri::command]
fn delete_pivot(state: State<'_, AppState>, id: u32, clear: bool) -> Result<(), String> {
    state.with(|engine| engine.delete_pivot(id, clear))
}

/// O bloco de dados contínuo em volta de uma célula.
#[tauri::command]
fn data_region(state: State<'_, AppState>, sheet: u32, col: u32, row: u32) -> Result<Range, String> {
    state.with(|engine| engine.data_region(sheet, col, row))
}

/// Gráficos de uma planilha, com os dados que mostram.
#[tauri::command]
fn list_charts(state: State<'_, AppState>, sheet: u32) -> Result<Vec<ChartView>, String> {
    state.with(|engine| Ok(engine.charts(sheet)))
}

/// Cria um gráfico com os dados do intervalo.
#[tauri::command]
fn add_chart(state: State<'_, AppState>, sheet: u32, kind: ChartKind, range: Range) -> Result<ChartView, String> {
    state.with(|engine| engine.add_chart(sheet, kind, range))
}

/// Grava o novo estado de um gráfico.
#[tauri::command]
fn update_chart(state: State<'_, AppState>, chart: Chart) -> Result<ChartView, String> {
    state.with(|engine| engine.update_chart(chart))
}

/// Apaga um gráfico.
#[tauri::command]
fn delete_chart(state: State<'_, AppState>, id: u32) -> Result<(), String> {
    state.with(|engine| engine.delete_chart(id))
}

/// Contagem e soma das células da seleção.
#[tauri::command]
fn selection_stats(state: State<'_, AppState>, sheet: u32, range: Range) -> Result<Stats, String> {
    state.with(|engine| engine.stats(sheet, range))
}

/// Para onde vai a célula ativa com Ctrl+seta.
#[tauri::command]
fn jump_cell(
    state: State<'_, AppState>,
    sheet: u32,
    col: u32,
    row: u32,
    direction: Direction,
) -> Result<CellPos, String> {
    state.with(|engine| engine.jump(sheet, col, row, direction))
}

/// A última célula preenchida da planilha.
#[tauri::command]
fn last_cell(state: State<'_, AppState>, sheet: u32) -> Result<CellPos, String> {
    state.with(|engine| engine.last_cell(sheet))
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
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState { engine: Mutex::new(engine) })
        .invoke_handler(tauri::generate_handler![
            set_cell,
            get_view,
            format_cells,
            clear_cells,
            merge_cells,
            unmerge_cells,
            set_border,
            change_structure,
            fill_cells,
            list_cond_rules,
            add_cond_rule,
            update_cond_rule,
            delete_cond_rule,
            list_validations,
            add_validation,
            update_validation,
            delete_validation,
            list_options,
            list_pivots,
            pivot_fields,
            pivot_items,
            add_pivot,
            update_pivot,
            refresh_pivot,
            refresh_all_pivots,
            delete_pivot,
            data_region,
            list_charts,
            add_chart,
            update_chart,
            delete_chart,
            set_frozen,
            set_hidden,
            find_cells,
            replace_cell,
            replace_all,
            sort_cells,
            filter_options,
            apply_filter,
            copy_cells,
            paste_cells,
            get_sheets,
            add_sheet,
            rename_sheet,
            delete_sheet,
            set_col_width,
            set_row_height,
            undo,
            redo,
            open_file,
            save_file,
            export_csv,
            selection_stats,
            jump_cell,
            last_cell
        ])
        .run(tauri::generate_context!())
        .expect("erro ao iniciar o Abacus");
}
