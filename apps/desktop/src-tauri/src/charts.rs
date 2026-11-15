//! Gráficos. O IronCalc não conhece gráficos, então o Abacus guarda os seus
//! e os grava no `.xlsx` como gráficos nativos do Excel, acrescentados ao
//! arquivo depois que o motor o salva. Na abertura, os gráficos de colunas,
//! barras, linhas e pizza do arquivo voltam a ser lidos.

use std::io::{Cursor, Read, Write};

use quick_xml::Reader;
use quick_xml::events::Event;
use serde::{Deserialize, Serialize};
use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

use crate::chart_trend::{self, Trend, TrendKind, TrendLine};
use crate::engine::Range;

/// Colunas e linhas que um gráfico pode usar de dados, para não gastar a
/// memória à toa com um intervalo enorme.
pub const MAX_CHART_COLS: u32 = 50;
pub const MAX_CHART_ROWS: u32 = 5000;

/// Cores das séries, na ordem.
const PALETTE: [&str; 8] = ["4F46E5", "F59E0B", "10B981", "EF4444", "3B82F6", "8B5CF6", "EC4899", "14B8A6"];

const NS_CHART: &str = "http://schemas.openxmlformats.org/drawingml/2006/chart";
const NS_DRAWING: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const NS_SHEET_DRAWING: &str = "http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing";
const NS_REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const REL_DRAWING: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/drawing";
const REL_CHART: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart";
const XML_DECLARATION: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#;
/// Identificador da relação planilha → desenho; não colide com as do motor (`rId1`, …).
const DRAWING_REL_ID: &str = "rIdAbacusDrawing";

/// O tipo do gráfico.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChartKind {
    Column,
    Bar,
    Line,
    Pie,
    Area,
    /// Dispersão (XY): a primeira coluna são os valores de X.
    Scatter,
    Donut,
}

/// Como as séries se empilham (colunas, barras, linhas e áreas).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Stacking {
    #[default]
    None,
    Stacked,
    /// Empilhado até 100%.
    Percent,
}

/// Onde fica a legenda.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LegendPos {
    /// Aparece embaixo quando há mais de uma série (ou na pizza).
    #[default]
    Auto,
    None,
    Bottom,
    Right,
    Top,
    Left,
}

/// Aparência e extras de uma série, na ordem em que as séries saem do intervalo.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SeriesStyle {
    /// `#RRGGBB`; sem cor, vale a da paleta.
    pub color: Option<String>,
    /// Em colunas, a série vira linha (gráfico combinado).
    pub line: bool,
    /// Usa o eixo de valores da direita.
    pub secondary: bool,
    pub trend: Option<Trend>,
}

/// As opções de aparência de um gráfico.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ChartOptions {
    pub stacking: Stacking,
    pub legend: LegendPos,
    /// Mostra o valor (ou o percentual, na pizza) sobre os dados.
    pub labels: bool,
    /// Título do eixo das categorias (ou do X, na dispersão).
    pub x_title: String,
    /// Título do eixo de valores.
    pub y_title: String,
    pub y_min: Option<f64>,
    pub y_max: Option<f64>,
    pub grid: bool,
    /// Linhas suavizadas.
    pub smooth: bool,
    pub styles: Vec<SeriesStyle>,
    /// Tabela dinâmica que alimenta o gráfico: o intervalo acompanha a tabela.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pivot: Option<u32>,
}

impl Default for ChartOptions {
    fn default() -> Self {
        ChartOptions {
            stacking: Stacking::None,
            legend: LegendPos::Auto,
            labels: false,
            x_title: String::new(),
            y_title: String::new(),
            y_min: None,
            y_max: None,
            grid: true,
            smooth: false,
            styles: Vec::new(),
            pivot: None,
        }
    }
}

/// Um gráfico: os dados vêm de um intervalo da própria planilha e ele fica
/// sobre a grade, do canto da célula `from` ao fim da célula `to` (inclusive).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Chart {
    pub id: u32,
    pub sheet: u32,
    pub kind: ChartKind,
    pub title: String,
    pub range: Range,
    pub from_col: u32,
    pub from_row: u32,
    pub to_col: u32,
    pub to_row: u32,
    #[serde(flatten)]
    pub opts: ChartOptions,
}

/// Para onde vai o início (ou o fim) de uma faixa quando linhas ou colunas
/// entram ou saem. `None` se, ao excluir, o ponto some e `end` pede a anterior.
fn shift_point(x: u32, index: u32, count: u32, insert: bool, end: bool) -> u32 {
    if insert {
        return if x >= index { x + count } else { x };
    }
    if x >= index + count {
        x - count
    } else if x >= index {
        // Dentro do trecho excluído: o início vai para o primeiro que fica, o fim para o último.
        if end { index.saturating_sub(1) } else { index }
    } else {
        x
    }
}

impl Chart {
    /// Acompanha linhas ou colunas inseridas ou excluídas na planilha do gráfico.
    /// Devolve `false` se os dados do gráfico foram todos excluídos.
    pub fn follow(&mut self, rows: bool, index: u32, count: u32, insert: bool) -> bool {
        let shift = |first: u32, last: u32| {
            let a = shift_point(first, index, count, insert, false);
            let b = shift_point(last, index, count, insert, true);
            // Uma faixa inteira excluída some.
            let gone = !insert && first >= index && last < index + count;
            (a, b.max(a), gone)
        };
        let (first, last, gone) = if rows {
            shift(self.range.row, self.range.row + self.range.rows - 1)
        } else {
            shift(self.range.col, self.range.col + self.range.cols - 1)
        };
        if gone {
            return false;
        }
        if rows {
            self.range.row = first;
            self.range.rows = last - first + 1;
            let (a, b, _) = shift(self.from_row, self.to_row);
            self.from_row = a;
            self.to_row = b;
        } else {
            self.range.col = first;
            self.range.cols = last - first + 1;
            let (a, b, _) = shift(self.from_col, self.to_col);
            self.from_col = a;
            self.to_col = b;
        }
        true
    }

    /// O estilo da série `index` (o padrão, se não houver).
    pub fn style(&self, index: usize) -> SeriesStyle {
        self.opts.styles.get(index).cloned().unwrap_or_default()
    }

    /// Cor da série, sem o `#`.
    pub fn color(&self, index: usize) -> String {
        self.opts
            .styles
            .get(index)
            .and_then(|s| s.color.as_deref())
            .and_then(valid_color)
            .unwrap_or_else(|| PALETTE[index % PALETTE.len()].to_string())
    }

    /// Mantém só o que o tipo do gráfico aceita.
    pub fn normalize(&mut self) {
        let cartesian = !matches!(self.kind, ChartKind::Pie | ChartKind::Donut | ChartKind::Scatter);
        let combinable = matches!(self.kind, ChartKind::Column | ChartKind::Line | ChartKind::Area);
        if !cartesian {
            self.opts.stacking = Stacking::None;
        }
        let pie = matches!(self.kind, ChartKind::Pie | ChartKind::Donut);
        for style in &mut self.opts.styles {
            style.line &= self.kind == ChartKind::Column;
            style.secondary &= combinable;
            style.color = style.color.as_deref().and_then(valid_color).map(|c| format!("#{c}"));
            if pie {
                style.trend = None;
            }
            style.trend = style.trend.as_ref().map(Trend::sanitized);
        }
        if pie {
            self.opts.y_min = None;
            self.opts.y_max = None;
        }
        // Um mínimo acima do máximo não desenha nada.
        if let (Some(min), Some(max)) = (self.opts.y_min, self.opts.y_max)
            && min >= max
        {
            self.opts.y_max = None;
        }
        self.opts.styles.truncate(MAX_CHART_COLS as usize);
    }
}

/// `#RRGGBB` ou `RRGGBB` como `RRGGBB` em maiúsculas.
fn valid_color(text: &str) -> Option<String> {
    let hex = text.strip_prefix('#').unwrap_or(text);
    (hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit())).then(|| hex.to_ascii_uppercase())
}

/// Uma série de valores; células vazias ou de texto ficam como `None`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Series {
    pub name: String,
    pub values: Vec<Option<f64>>,
    /// A linha de tendência pedida para a série.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trend: Option<TrendLine>,
}

/// O que o gráfico mostra, tirado das células.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ChartData {
    pub categories: Vec<String>,
    /// Valores de X de cada categoria: o número da primeira coluna na dispersão, 1, 2, 3… nos demais.
    pub x: Vec<Option<f64>>,
    pub series: Vec<Series>,
    /// A primeira coluna do intervalo é de categorias.
    #[serde(skip)]
    pub has_categories: bool,
    /// A primeira linha do intervalo é de títulos das séries.
    #[serde(skip)]
    pub has_header: bool,
}

/// O que se precisa saber de uma célula para montar o gráfico.
pub struct CellInfo {
    pub text: String,
    pub number: Option<f64>,
}

/// Como o intervalo se divide: a primeira coluna pode ser de categorias e a
/// primeira linha, de títulos das séries.
struct Layout {
    categories: bool,
    header: bool,
}

fn layout(range: Range, scatter: bool, cell: &impl Fn(u32, u32) -> CellInfo) -> Layout {
    let first_series_col = |categories: bool| u32::from(categories);
    // A primeira coluna vira categorias quando tem texto (fora o título); na dispersão, é sempre o X.
    let categories = range.cols >= 2
        && (scatter
            || (0..range.rows).any(|offset| {
                let info = cell(range.row + offset, range.col);
                offset > 0 && info.number.is_none() && !info.text.is_empty()
            }));
    // A primeira linha é de títulos quando as séries começam por texto.
    let header = range.rows >= 2
        && (first_series_col(categories)..range.cols).all(|offset| {
            let info = cell(range.row, range.col + offset);
            info.number.is_none() && !info.text.is_empty()
        });
    Layout { categories, header }
}

/// As categorias e séries de um intervalo.
pub fn build_data(range: Range, scatter: bool, cell: impl Fn(u32, u32) -> CellInfo) -> ChartData {
    let layout = layout(range, scatter, &cell);
    let first_row = range.row + u32::from(layout.header);
    let last_row = range.row + range.rows;
    let first_col = range.col + u32::from(layout.categories);
    let categories = (first_row..last_row)
        .map(|row| if layout.categories { cell(row, range.col).text } else { (row - first_row + 1).to_string() })
        .collect();
    // Na dispersão o X é a primeira coluna; se ela não tem número nenhum (são textos), vale 1, 2, 3…
    let numeric_x =
        scatter && layout.categories && (first_row..last_row).any(|row| cell(row, range.col).number.is_some());
    let x = (first_row..last_row)
        .map(|row| if numeric_x { cell(row, range.col).number } else { Some(f64::from(row - first_row + 1)) })
        .collect();
    let series = (first_col..range.col + range.cols)
        .enumerate()
        .map(|(index, col)| Series {
            name: if layout.header { cell(range.row, col).text } else { format!("Série {}", index + 1) },
            values: (first_row..last_row).map(|row| cell(row, col).number).collect(),
            trend: None,
        })
        .collect();
    ChartData { categories, x, series, has_categories: layout.categories, has_header: layout.header }
}

/// Os dados de um gráfico, já com as linhas de tendência pedidas.
pub fn data_for(chart: &Chart, cell: impl Fn(u32, u32) -> CellInfo) -> ChartData {
    let mut data = build_data(chart.range, chart.kind == ChartKind::Scatter, cell);
    for (index, series) in data.series.iter_mut().enumerate() {
        let Some(trend) = chart.style(index).trend else { continue };
        let points: Vec<(f64, f64)> =
            data.x.iter().zip(&series.values).filter_map(|(x, y)| Some(((*x)?, (*y)?))).collect();
        let line = chart_trend::compute(&trend, &points);
        if !line.points.is_empty() {
            series.trend = Some(line);
        }
    }
    data
}

// ---------------------------------------------------------------------------
// Gravação
// ---------------------------------------------------------------------------

/// Letras da coluna (`0` → `A`).
pub(crate) fn col_letters(col: u32) -> String {
    let mut n = col + 1;
    let mut letters = Vec::new();
    while n > 0 {
        n -= 1;
        letters.push(b'A' + (n % 26) as u8);
        n /= 26;
    }
    letters.reverse();
    String::from_utf8(letters).unwrap_or_default()
}

pub(crate) fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// `'Planilha 1'!$A$1:$B$3`
fn reference(sheet_name: &str, col: u32, row: u32, cols: u32, rows: u32) -> String {
    let name = sheet_name.replace('\'', "''");
    let start = format!("${}${}", col_letters(col), row + 1);
    if cols == 1 && rows == 1 {
        return format!("'{name}'!{start}");
    }
    format!("'{name}'!{start}:${}${}", col_letters(col + cols - 1), row + rows)
}

fn str_cache(values: &[String]) -> String {
    let points: String =
        values.iter().enumerate().map(|(i, v)| format!("<c:pt idx=\"{i}\"><c:v>{}</c:v></c:pt>", escape(v))).collect();
    format!("<c:strCache><c:ptCount val=\"{}\"/>{points}</c:strCache>", values.len())
}

fn num_cache(values: &[Option<f64>]) -> String {
    let points: String = values
        .iter()
        .enumerate()
        .filter_map(|(i, v)| v.map(|v| format!("<c:pt idx=\"{i}\"><c:v>{v}</c:v></c:pt>")))
        .collect();
    format!(
        "<c:numCache><c:formatCode>General</c:formatCode><c:ptCount val=\"{}\"/>{points}</c:numCache>",
        values.len()
    )
}

fn fill(color: &str) -> String {
    format!("<a:solidFill><a:srgbClr val=\"{color}\"/></a:solidFill>")
}

/// Famílias de gráficos que o Excel escreve cada uma num elemento.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Group {
    Bar,
    Line,
    Area,
    Scatter,
    Pie,
    Donut,
}

const TEXT_STYLE: &str = "<c:txPr><a:bodyPr/><a:lstStyle/><a:p><a:pPr><a:defRPr sz=\"900\"/></a:pPr><a:endParaRPr lang=\"pt-BR\"/></a:p></c:txPr>";

fn text_title(text: &str, size: u32, bold: bool) -> String {
    let b = u8::from(bold);
    format!(
        "<c:title><c:tx><c:rich><a:bodyPr/><a:lstStyle/><a:p><a:pPr><a:defRPr sz=\"{size}\" b=\"{b}\"/></a:pPr><a:r><a:rPr lang=\"pt-BR\" sz=\"{size}\" b=\"{b}\"/><a:t>{}</a:t></a:r></a:p></c:rich></c:tx><c:overlay val=\"0\"/></c:title>",
        escape(text)
    )
}

fn grouping(stacking: Stacking) -> &'static str {
    match stacking {
        Stacking::None => "clustered",
        Stacking::Stacked => "stacked",
        Stacking::Percent => "percentStacked",
    }
}

/// Uma série, no elemento certo para a família.
fn series_xml(chart: &Chart, data: &ChartData, sheet_name: &str, index: usize, group: Group) -> String {
    let layout_info = Layout { categories: data.has_categories, header: data.has_header };
    let first_row = chart.range.row + u32::from(layout_info.header);
    let first_col = chart.range.col + u32::from(layout_info.categories);
    let n = data.categories.len() as u32;
    let series = &data.series[index];
    let col = first_col + index as u32;
    let color = chart.color(index);

    let name = if layout_info.header {
        format!(
            "<c:tx><c:strRef><c:f>{}</c:f>{}</c:strRef></c:tx>",
            escape(&reference(sheet_name, col, chart.range.row, 1, 1)),
            str_cache(std::slice::from_ref(&series.name))
        )
    } else {
        format!("<c:tx><c:v>{}</c:v></c:tx>", escape(&series.name))
    };
    let marker = |size: u32| {
        format!(
            "<c:marker><c:symbol val=\"circle\"/><c:size val=\"{size}\"/><c:spPr>{}</c:spPr></c:marker>",
            fill(&color)
        )
    };
    let (shape, marker_xml, invert) = match group {
        Group::Bar => (format!("<c:spPr>{}</c:spPr>", fill(&color)), String::new(), "<c:invertIfNegative val=\"0\"/>"),
        Group::Area => (format!("<c:spPr>{}</c:spPr>", fill(&color)), String::new(), ""),
        Group::Line => {
            (format!("<c:spPr><a:ln w=\"28575\" cap=\"rnd\">{}<a:round/></a:ln></c:spPr>", fill(&color)), marker(5), "")
        }
        Group::Scatter => ("<c:spPr><a:ln w=\"19050\"><a:noFill/></a:ln></c:spPr>".to_string(), marker(7), ""),
        Group::Pie | Group::Donut => (String::new(), String::new(), ""),
    };
    let slices = if matches!(group, Group::Pie | Group::Donut) {
        (0..data.categories.len())
            .map(|i| {
                format!(
                    "<c:dPt><c:idx val=\"{i}\"/><c:bubble3D val=\"0\"/><c:spPr>{}</c:spPr></c:dPt>",
                    fill(PALETTE[i % PALETTE.len()])
                )
            })
            .collect()
    } else {
        String::new()
    };
    let labels = if chart.opts.labels {
        let percent = matches!(group, Group::Pie | Group::Donut);
        format!(
            "<c:dLbls>{TEXT_STYLE}<c:showLegendKey val=\"0\"/><c:showVal val=\"{}\"/><c:showCatName val=\"0\"/><c:showSerName val=\"0\"/><c:showPercent val=\"{}\"/><c:showBubbleSize val=\"0\"/></c:dLbls>",
            u8::from(!percent),
            u8::from(percent)
        )
    } else {
        String::new()
    };
    let trend = match chart.style(index).trend {
        Some(t) if !matches!(group, Group::Pie | Group::Donut) => {
            let t = t.sanitized();
            let (kind, extra) = match t.kind {
                TrendKind::Linear => ("linear", String::new()),
                TrendKind::Poly => ("poly", format!("<c:order val=\"{}\"/>", t.order)),
                TrendKind::Exp => ("exp", String::new()),
                TrendKind::Log => ("log", String::new()),
                TrendKind::Moving => ("movingAvg", format!("<c:period val=\"{}\"/>", t.period)),
            };
            let flags = format!("<c:dispRSqr val=\"{}\"/><c:dispEq val=\"{}\"/>", u8::from(t.r2), u8::from(t.equation));
            format!(
                "<c:trendline><c:spPr><a:ln w=\"19050\" cap=\"rnd\">{}<a:prstDash val=\"sysDash\"/></a:ln></c:spPr><c:trendlineType val=\"{kind}\"/>{extra}{flags}</c:trendline>",
                fill(&color)
            )
        }
        _ => String::new(),
    };
    let values_ref = escape(&reference(sheet_name, col, first_row, 1, n));
    let axis_values = if group == Group::Scatter {
        let x_ref = escape(&reference(sheet_name, chart.range.col, first_row, 1, n));
        format!(
            "<c:xVal><c:numRef><c:f>{x_ref}</c:f>{}</c:numRef></c:xVal><c:yVal><c:numRef><c:f>{values_ref}</c:f>{}</c:numRef></c:yVal>",
            num_cache(&data.x),
            num_cache(&series.values)
        )
    } else {
        let categories = if layout_info.categories {
            format!(
                "<c:cat><c:strRef><c:f>{}</c:f>{}</c:strRef></c:cat>",
                escape(&reference(sheet_name, chart.range.col, first_row, 1, n)),
                str_cache(&data.categories)
            )
        } else {
            String::new()
        };
        format!("{categories}<c:val><c:numRef><c:f>{values_ref}</c:f>{}</c:numRef></c:val>", num_cache(&series.values))
    };
    let smooth = match group {
        Group::Line | Group::Scatter => format!("<c:smooth val=\"{}\"/>", u8::from(chart.opts.smooth)),
        _ => String::new(),
    };
    format!(
        "<c:ser><c:idx val=\"{index}\"/><c:order val=\"{index}\"/>{name}{shape}{invert}{marker_xml}{slices}{labels}{trend}{axis_values}{smooth}</c:ser>"
    )
}

/// O XML de um gráfico (`xl/charts/chartN.xml`).
fn chart_xml(chart: &Chart, data: &ChartData, sheet_name: &str) -> String {
    let mut chart = chart.clone();
    chart.normalize();
    let chart = &chart;
    let opts = &chart.opts;
    let count = data.series.len();
    let kind = chart.kind;
    let secondary_of = |i: usize| chart.style(i).secondary;
    let line_of = |i: usize| chart.style(i).line;

    let base_group = match kind {
        ChartKind::Column | ChartKind::Bar => Group::Bar,
        ChartKind::Line => Group::Line,
        ChartKind::Area => Group::Area,
        ChartKind::Scatter => Group::Scatter,
        ChartKind::Pie => Group::Pie,
        ChartKind::Donut => Group::Donut,
    };
    let stacking_value = match (opts.stacking, base_group) {
        (Stacking::None, Group::Line) => "standard",
        (Stacking::None, Group::Area) => "standard",
        (other, _) => grouping(other),
    };
    let mut plot = String::new();
    let mut axes = String::new();
    let mut has_secondary = false;

    if matches!(base_group, Group::Pie | Group::Donut) {
        let sers = (0..count).map(|i| series_xml(chart, data, sheet_name, i, base_group)).collect::<String>();
        plot = if base_group == Group::Pie {
            format!("<c:pieChart><c:varyColors val=\"1\"/>{sers}<c:firstSliceAng val=\"0\"/></c:pieChart>")
        } else {
            format!(
                "<c:doughnutChart><c:varyColors val=\"1\"/>{sers}<c:firstSliceAng val=\"0\"/><c:holeSize val=\"55\"/></c:doughnutChart>"
            )
        };
    } else {
        // Primeiro o eixo principal, depois o secundário; em cada um, a família da base e as linhas do combinado.
        for secondary in [false, true] {
            for overlay_lines in [false, true] {
                let members: Vec<usize> = (0..count)
                    .filter(|&i| {
                        secondary_of(i) == secondary && (kind == ChartKind::Column && line_of(i)) == overlay_lines
                    })
                    .collect();
                if members.is_empty() {
                    continue;
                }
                has_secondary |= secondary;
                let group = if overlay_lines { Group::Line } else { base_group };
                let sers: String = members.iter().map(|&i| series_xml(chart, data, sheet_name, i, group)).collect();
                let (cat_id, val_id) = if secondary { (333, 444) } else { (111, 222) };
                let ids = format!("<c:axId val=\"{cat_id}\"/><c:axId val=\"{val_id}\"/>");
                plot.push_str(&match group {
                    Group::Bar => {
                        let overlap = if opts.stacking == Stacking::None { "" } else { "<c:overlap val=\"100\"/>" };
                        format!(
                            "<c:barChart><c:barDir val=\"{}\"/><c:grouping val=\"{stacking_value}\"/><c:varyColors val=\"0\"/>{sers}<c:gapWidth val=\"80\"/>{overlap}{ids}</c:barChart>",
                            if kind == ChartKind::Bar { "bar" } else { "col" }
                        )
                    }
                    Group::Line => {
                        let grouping = if overlay_lines { "standard" } else { stacking_value };
                        format!(
                            "<c:lineChart><c:grouping val=\"{grouping}\"/><c:varyColors val=\"0\"/>{sers}<c:marker val=\"1\"/>{ids}</c:lineChart>"
                        )
                    }
                    Group::Area => format!(
                        "<c:areaChart><c:grouping val=\"{stacking_value}\"/><c:varyColors val=\"0\"/>{sers}{ids}</c:areaChart>"
                    ),
                    _ => format!(
                        "<c:scatterChart><c:scatterStyle val=\"lineMarker\"/><c:varyColors val=\"0\"/>{sers}{ids}</c:scatterChart>"
                    ),
                });
            }
        }

        let scaling = {
            let max = opts.y_max.map(|v| format!("<c:max val=\"{v}\"/>")).unwrap_or_default();
            let min = opts.y_min.map(|v| format!("<c:min val=\"{v}\"/>")).unwrap_or_default();
            move |orientation: &str| format!("<c:scaling><c:orientation val=\"{orientation}\"/>{max}{min}</c:scaling>")
        };
        let horizontal = kind == ChartKind::Bar;
        let (cat_pos, val_pos) = if horizontal { ("l", "b") } else { ("b", "l") };
        let grid = if opts.grid {
            "<c:majorGridlines><c:spPr><a:ln w=\"6350\"><a:solidFill><a:srgbClr val=\"E5E7EB\"/></a:solidFill></a:ln></c:spPr></c:majorGridlines>"
        } else {
            ""
        };
        let number_format = if opts.stacking == Stacking::Percent { "0%" } else { "General" };
        let title_of = |text: &str| if text.is_empty() { String::new() } else { text_title(text, 900, false) };
        let x_title = title_of(&opts.x_title);
        let y_title = title_of(&opts.y_title);
        let flat = |id: u32| format!("<c:axId val=\"{id}\"/>");
        let _ = flat;
        if kind == ChartKind::Scatter {
            axes.push_str(&format!(
                "<c:valAx><c:axId val=\"111\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"b\"/>{x_title}<c:numFmt formatCode=\"General\" sourceLinked=\"0\"/><c:majorTickMark val=\"none\"/><c:minorTickMark val=\"none\"/><c:tickLblPos val=\"nextTo\"/>{TEXT_STYLE}<c:crossAx val=\"222\"/><c:crosses val=\"autoZero\"/><c:crossBetween val=\"midCat\"/></c:valAx>\
<c:valAx><c:axId val=\"222\"/>{}<c:delete val=\"0\"/><c:axPos val=\"l\"/>{grid}{y_title}<c:numFmt formatCode=\"General\" sourceLinked=\"0\"/><c:majorTickMark val=\"none\"/><c:minorTickMark val=\"none\"/><c:tickLblPos val=\"nextTo\"/>{TEXT_STYLE}<c:crossAx val=\"111\"/><c:crosses val=\"autoZero\"/><c:crossBetween val=\"midCat\"/></c:valAx>",
                scaling("minMax")
            ));
        } else {
            let (cat_orientation, val_crosses) = if horizontal { ("maxMin", "max") } else { ("minMax", "autoZero") };
            let between = if kind == ChartKind::Area { "midCat" } else { "between" };
            axes.push_str(&format!(
                "<c:catAx><c:axId val=\"111\"/><c:scaling><c:orientation val=\"{cat_orientation}\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"{cat_pos}\"/>{x_title}<c:numFmt formatCode=\"General\" sourceLinked=\"0\"/><c:majorTickMark val=\"none\"/><c:minorTickMark val=\"none\"/><c:tickLblPos val=\"nextTo\"/>{TEXT_STYLE}<c:crossAx val=\"222\"/><c:crosses val=\"autoZero\"/><c:auto val=\"1\"/><c:lblAlgn val=\"ctr\"/><c:lblOffset val=\"100\"/><c:noMultiLvlLbl val=\"0\"/></c:catAx>\
<c:valAx><c:axId val=\"222\"/>{}<c:delete val=\"0\"/><c:axPos val=\"{val_pos}\"/>{grid}{y_title}<c:numFmt formatCode=\"{number_format}\" sourceLinked=\"0\"/><c:majorTickMark val=\"none\"/><c:minorTickMark val=\"none\"/><c:tickLblPos val=\"nextTo\"/>{TEXT_STYLE}<c:crossAx val=\"111\"/><c:crosses val=\"{val_crosses}\"/><c:crossBetween val=\"{between}\"/></c:valAx>",
                scaling("minMax")
            ));
            if has_secondary {
                axes.push_str(&format!(
                    "<c:catAx><c:axId val=\"333\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"1\"/><c:axPos val=\"b\"/><c:majorTickMark val=\"none\"/><c:minorTickMark val=\"none\"/><c:tickLblPos val=\"nextTo\"/><c:crossAx val=\"444\"/><c:crosses val=\"autoZero\"/><c:auto val=\"1\"/><c:lblAlgn val=\"ctr\"/><c:lblOffset val=\"100\"/><c:noMultiLvlLbl val=\"0\"/></c:catAx>\
<c:valAx><c:axId val=\"444\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/><c:axPos val=\"r\"/><c:numFmt formatCode=\"General\" sourceLinked=\"0\"/><c:majorTickMark val=\"none\"/><c:minorTickMark val=\"none\"/><c:tickLblPos val=\"nextTo\"/>{TEXT_STYLE}<c:crossAx val=\"333\"/><c:crosses val=\"max\"/><c:crossBetween val=\"{between}\"/></c:valAx>"
                ));
            }
        }
    }

    let title = if chart.title.is_empty() {
        "<c:autoTitleDeleted val=\"1\"/>".to_string()
    } else {
        format!("{}<c:autoTitleDeleted val=\"0\"/>", text_title(&chart.title, 1200, true))
    };
    let legend_position = match opts.legend {
        LegendPos::None => None,
        LegendPos::Auto => (count > 1 || matches!(kind, ChartKind::Pie | ChartKind::Donut)).then_some("b"),
        LegendPos::Bottom => Some("b"),
        LegendPos::Right => Some("r"),
        LegendPos::Top => Some("t"),
        LegendPos::Left => Some("l"),
    };
    let legend = legend_position
        .map(|pos| format!("<c:legend><c:legendPos val=\"{pos}\"/><c:overlay val=\"0\"/>{TEXT_STYLE}</c:legend>"))
        .unwrap_or_default();
    format!(
        "{XML_DECLARATION}\n<c:chartSpace xmlns:c=\"{NS_CHART}\" xmlns:a=\"{NS_DRAWING}\" xmlns:r=\"{NS_REL}\"><c:roundedCorners val=\"0\"/><c:chart>{title}<c:plotArea><c:layout/>{plot}{axes}</c:plotArea>{legend}<c:plotVisOnly val=\"1\"/><c:dispBlanksAs val=\"gap\"/></c:chart></c:chartSpace>"
    )
}

/// O XML do desenho de uma planilha, com um gráfico por âncora.
fn drawing_xml(charts: &[&Chart]) -> String {
    let anchors: String = charts
        .iter()
        .enumerate()
        .map(|(k, chart)| {
            format!(
                "<xdr:twoCellAnchor><xdr:from><xdr:col>{}</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>{}</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from><xdr:to><xdr:col>{}</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>{}</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:to><xdr:graphicFrame macro=\"\"><xdr:nvGraphicFramePr><xdr:cNvPr id=\"{}\" name=\"Gráfico {}\"/><xdr:cNvGraphicFramePr/></xdr:nvGraphicFramePr><xdr:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/></xdr:xfrm><a:graphic><a:graphicData uri=\"{NS_CHART}\"><c:chart xmlns:c=\"{NS_CHART}\" xmlns:r=\"{NS_REL}\" r:id=\"rId{}\"/></a:graphicData></a:graphic></xdr:graphicFrame><xdr:clientData/></xdr:twoCellAnchor>",
                chart.from_col,
                chart.from_row,
                chart.to_col + 1,
                chart.to_row + 1,
                k + 2,
                k + 1,
                k + 1
            )
        })
        .collect();
    format!(
        "{XML_DECLARATION}\n<xdr:wsDr xmlns:xdr=\"{NS_SHEET_DRAWING}\" xmlns:a=\"{NS_DRAWING}\">{anchors}</xdr:wsDr>"
    )
}

fn relationships(items: &[(String, &str, String)]) -> String {
    let body: String = items
        .iter()
        .map(|(id, kind, target)| format!("<Relationship Id=\"{id}\" Type=\"{kind}\" Target=\"{target}\"/>"))
        .collect();
    format!(
        "{XML_DECLARATION}\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">{body}</Relationships>"
    )
}

/// Acrescenta os gráficos a um `.xlsx` gravado pelo motor. `sheet_names` está
/// na ordem das planilhas, e `charts` traz cada gráfico com os seus dados.
pub fn write_into(xlsx: Vec<u8>, sheet_names: &[String], charts: &[(Chart, ChartData)]) -> Result<Vec<u8>, String> {
    if charts.is_empty() {
        return Ok(xlsx);
    }
    let fail = |e: &dyn std::fmt::Display| format!("não foi possível gravar os gráficos: {e}");
    let mut archive = ZipArchive::new(Cursor::new(xlsx)).map_err(|e| fail(&e))?;
    let mut parts: Vec<(String, Vec<u8>)> = Vec::new();
    for index in 0..archive.len() {
        let mut file = archive.by_index(index).map_err(|e| fail(&e))?;
        if file.is_dir() {
            continue;
        }
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).map_err(|e| fail(&e))?;
        let name = file.name().map_err(|e| fail(&e))?.into_owned();
        parts.push((name, bytes));
    }
    let take = |parts: &mut Vec<(String, Vec<u8>)>, name: &str| -> Option<String> {
        parts.iter().position(|(n, _)| n == name).map(|i| String::from_utf8_lossy(&parts.remove(i).1).into_owned())
    };

    let mut content_types = take(&mut parts, "[Content_Types].xml").ok_or("o arquivo não tem [Content_Types].xml")?;
    let mut overrides = String::new();
    let mut chart_number = 0;
    let mut sheets: Vec<u32> = charts.iter().map(|(c, _)| c.sheet).collect();
    sheets.sort_unstable();
    sheets.dedup();
    for sheet in sheets {
        let sheet_name = sheet_names.get(sheet as usize).ok_or("planilha do gráfico não encontrada")?;
        let number = sheet + 1;
        let here: Vec<&(Chart, ChartData)> = charts.iter().filter(|(c, _)| c.sheet == sheet).collect();
        let mut drawing_rels = Vec::new();
        for (k, (chart, data)) in here.iter().enumerate() {
            chart_number += 1;
            parts.push((format!("xl/charts/chart{chart_number}.xml"), chart_xml(chart, data, sheet_name).into_bytes()));
            overrides.push_str(&format!("<Override PartName=\"/xl/charts/chart{chart_number}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.drawingml.chart+xml\"/>"));
            drawing_rels.push((format!("rId{}", k + 1), REL_CHART, format!("../charts/chart{chart_number}.xml")));
        }
        let only_charts: Vec<&Chart> = here.iter().map(|(c, _)| c).collect();
        parts.push((format!("xl/drawings/drawing{number}.xml"), drawing_xml(&only_charts).into_bytes()));
        parts.push((format!("xl/drawings/_rels/drawing{number}.xml.rels"), relationships(&drawing_rels).into_bytes()));
        overrides.push_str(&format!("<Override PartName=\"/xl/drawings/drawing{number}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.drawing+xml\"/>"));

        // A planilha aponta para o desenho.
        let sheet_part = format!("xl/worksheets/sheet{number}.xml");
        let mut sheet_xml = take(&mut parts, &sheet_part).ok_or("planilha do gráfico não encontrada no arquivo")?;
        let tag = format!("<drawing r:id=\"{DRAWING_REL_ID}\"/>");
        let at = sheet_xml.rfind("<extLst").or_else(|| sheet_xml.rfind("</worksheet>")).ok_or("planilha inválida")?;
        sheet_xml.insert_str(at, &tag);
        parts.push((sheet_part, sheet_xml.into_bytes()));

        let rels_part = format!("xl/worksheets/_rels/sheet{number}.xml.rels");
        let link = format!(
            "<Relationship Id=\"{DRAWING_REL_ID}\" Type=\"{REL_DRAWING}\" Target=\"../drawings/drawing{number}.xml\"/>"
        );
        let rels = match take(&mut parts, &rels_part) {
            Some(mut existing) => {
                let at = existing.rfind("</Relationships>").ok_or("relações inválidas")?;
                existing.insert_str(at, &link);
                existing
            }
            None => format!(
                "{XML_DECLARATION}\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">{link}</Relationships>"
            ),
        };
        parts.push((rels_part, rels.into_bytes()));
    }
    let at = content_types.rfind("</Types>").ok_or("[Content_Types].xml inválido")?;
    content_types.insert_str(at, &overrides);
    parts.insert(0, ("[Content_Types].xml".to_string(), content_types.into_bytes()));

    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (name, bytes) in parts {
        writer.start_file(name, options).map_err(|e| fail(&e))?;
        writer.write_all(&bytes).map_err(|e| fail(&e))?;
    }
    Ok(writer.finish().map_err(|e| fail(&e))?.into_inner())
}

// ---------------------------------------------------------------------------
// Leitura
// ---------------------------------------------------------------------------

/// Um elemento XML, só com o que a leitura precisa.
#[derive(Default)]
pub(crate) struct Node {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Node>,
    pub text: String,
}

impl Node {
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs.iter().find(|(key, _)| key == name).map(|(_, value)| value.as_str())
    }

    pub fn child(&self, name: &str) -> Option<&Node> {
        self.children.iter().find(|child| child.name == name)
    }

    /// Todos os descendentes com o nome dado, em ordem.
    pub fn all<'a>(&'a self, name: &str, found: &mut Vec<&'a Node>) {
        for child in &self.children {
            if child.name == name {
                found.push(child);
            }
            child.all(name, found);
        }
    }

    pub fn first(&self, name: &str) -> Option<&Node> {
        let mut found = Vec::new();
        self.all(name, &mut found);
        found.into_iter().next()
    }
}

pub(crate) fn parse_xml(bytes: &[u8]) -> Option<Node> {
    let mut reader = Reader::from_reader(bytes);
    let mut stack = vec![Node::default()];
    let make = |element: &quick_xml::events::BytesStart| {
        let mut node = Node { name: element.local_name().as_ref().to_string(), ..Node::default() };
        for attr in element.attributes().flatten() {
            let key = attr.key.local_name().as_ref().to_string();
            let value =
                attr.normalized_value(quick_xml::XmlVersion::default()).map(|v| v.into_owned()).unwrap_or_default();
            node.attrs.push((key, value));
        }
        node
    };
    loop {
        match reader.read_event().ok()? {
            Event::Start(element) => stack.push(make(&element)),
            Event::Empty(element) => stack.last_mut()?.children.push(make(&element)),
            Event::End(_) => {
                let node = stack.pop()?;
                stack.last_mut()?.children.push(node);
            }
            Event::Text(text) => stack.last_mut()?.text.push_str(&text.xml10_content()),
            Event::GeneralRef(entity) => {
                let name = entity.as_ref();
                let decoded = match name {
                    "amp" => Some('&'),
                    "lt" => Some('<'),
                    "gt" => Some('>'),
                    "quot" => Some('"'),
                    "apos" => Some('\''),
                    _ => entity.resolve_char_ref().ok().flatten(),
                };
                if let Some(c) = decoded {
                    stack.last_mut()?.text.push(c);
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    let mut root = stack.pop()?;
    if !stack.is_empty() || root.children.len() != 1 {
        return None;
    }
    root.children.pop()
}

/// Resolve `target` em relação à pasta de `base` (`xl/worksheets/sheet1.xml`
/// + `../drawings/d.xml` → `xl/drawings/d.xml`).
pub(crate) fn resolve(base: &str, target: &str) -> String {
    let mut parts: Vec<&str> = if let Some(absolute) = target.strip_prefix('/') {
        return absolute.to_string();
    } else {
        base.split('/').collect()
    };
    parts.pop();
    for piece in target.split('/') {
        match piece {
            ".." => {
                parts.pop();
            }
            "." | "" => {}
            other => parts.push(other),
        }
    }
    parts.join("/")
}

/// Pasta de relações de um arquivo: `xl/a/b.xml` → `xl/a/_rels/b.xml.rels`.
pub(crate) fn rels_path(part: &str) -> String {
    match part.rsplit_once('/') {
        Some((dir, file)) => format!("{dir}/_rels/{file}.rels"),
        None => format!("_rels/{part}.rels"),
    }
}

/// Lê `A1` ou `$B$3` como (coluna, linha) a partir de zero.
pub(crate) fn parse_cell(text: &str) -> Option<(u32, u32)> {
    let text = text.replace('$', "");
    let split = text.find(|c: char| c.is_ascii_digit())?;
    let (letters, digits) = text.split_at(split);
    if letters.is_empty() || !letters.chars().all(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    let col = letters
        .to_ascii_uppercase()
        .bytes()
        .fold(0u32, |acc, b| acc.saturating_mul(26).saturating_add(u32::from(b - b'A') + 1))
        - 1;
    Some((col, digits.parse::<u32>().ok()?.checked_sub(1)?))
}

/// Lê `'Nome'!$A$1:$B$3` como a planilha e o intervalo (colunas e linhas, inclusive).
pub(crate) fn parse_reference(text: &str) -> Option<(String, (u32, u32, u32, u32))> {
    let (sheet, cells) = text.rsplit_once('!')?;
    let sheet = sheet
        .strip_prefix('\'')
        .and_then(|s| s.strip_suffix('\''))
        .map(|s| s.replace("''", "'"))
        .unwrap_or_else(|| sheet.to_string());
    let (first, last) = cells.split_once(':').unwrap_or((cells, cells));
    let (col1, row1) = parse_cell(first)?;
    let (col2, row2) = parse_cell(last)?;
    Some((sheet, (col1.min(col2), row1.min(row2), col1.max(col2), row1.max(row2))))
}

/// Gráficos que um arquivo traz. Os ids começam em 1; gráficos de outros
/// tipos, ou com dados que não estão numa planilha, ficam de fora.
pub fn read_from(xlsx: &[u8], sheet_names: &[String]) -> Vec<Chart> {
    read_inner(xlsx, sheet_names).unwrap_or_default()
}

fn read_inner(xlsx: &[u8], sheet_names: &[String]) -> Option<Vec<Chart>> {
    let mut archive = ZipArchive::new(Cursor::new(xlsx)).ok()?;
    let mut read = |name: &str| -> Option<Vec<u8>> {
        let mut file = archive.by_name(name).ok()?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).ok()?;
        Some(bytes)
    };
    let rels_of = |bytes: Option<Vec<u8>>| -> Vec<(String, String, String)> {
        let Some(root) = bytes.as_deref().and_then(parse_xml) else { return Vec::new() };
        root.children
            .iter()
            .map(|rel| {
                (
                    rel.attr("Id").unwrap_or_default().to_string(),
                    rel.attr("Type").unwrap_or_default().to_string(),
                    rel.attr("Target").unwrap_or_default().to_string(),
                )
            })
            .collect()
    };

    let workbook = parse_xml(&read("xl/workbook.xml")?)?;
    let workbook_rels = rels_of(read("xl/_rels/workbook.xml.rels"));
    let mut charts = Vec::new();
    for sheet_node in &workbook.first("sheets")?.children {
        let name = sheet_node.attr("name")?;
        let Some(sheet) = sheet_names.iter().position(|n| n == name) else { continue };
        let Some(target) =
            workbook_rels.iter().find(|(id, _, _)| Some(id.as_str()) == sheet_node.attr("id")).map(|r| r.2.clone())
        else {
            continue;
        };
        let part = resolve("xl/workbook.xml", &target);
        let sheet_rels = rels_of(read(&rels_path(&part)));
        let Some(drawing_target) = sheet_rels.iter().find(|r| r.1 == REL_DRAWING).map(|r| r.2.clone()) else {
            continue;
        };
        let drawing_part = resolve(&part, &drawing_target);
        let Some(drawing) = read(&drawing_part).as_deref().and_then(parse_xml) else { continue };
        let drawing_rels = rels_of(read(&rels_path(&drawing_part)));

        for anchor in &drawing.children {
            let Some(frame) = anchor.first("chart") else { continue };
            let Some(rel_id) = frame.attr("id") else { continue };
            let Some(chart_target) =
                drawing_rels.iter().find(|r| r.0 == rel_id && r.1 == REL_CHART).map(|r| r.2.clone())
            else {
                continue;
            };
            let Some(chart_xml) = read(&resolve(&drawing_part, &chart_target)).as_deref().and_then(parse_xml) else {
                continue;
            };
            let Some(chart) = read_chart(&chart_xml, anchor, name) else { continue };
            charts.push(Chart { id: charts.len() as u32 + 1, sheet: sheet as u32, ..chart });
        }
    }
    Some(charts)
}

fn title_text(node: Option<&Node>) -> String {
    let Some(title) = node.and_then(|n| n.child("title")) else { return String::new() };
    let mut texts = Vec::new();
    title.all("t", &mut texts);
    texts.iter().map(|t| t.text.as_str()).collect()
}

fn flag(node: &Node, name: &str) -> bool {
    node.child(name).is_some_and(|n| n.attr("val").is_none_or(|v| v == "1" || v == "true"))
}

fn number_child(node: &Node, name: &str) -> Option<f64> {
    node.child(name)?.attr("val")?.parse().ok()
}

/// A cor `RRGGBB` de uma série: a do preenchimento, da linha ou, na dispersão, do marcador.
fn series_color(ser: &Node) -> Option<String> {
    let from =
        |node: Option<&Node>| node.and_then(|n| n.first("srgbClr")).and_then(|c| c.attr("val")).map(str::to_string);
    from(ser.child("spPr")).or_else(|| from(ser.child("marker").and_then(|m| m.child("spPr"))))
}

fn read_trend(ser: &Node) -> Option<Trend> {
    let line = ser.child("trendline")?;
    let kind = match line.child("trendlineType")?.attr("val")? {
        "linear" => TrendKind::Linear,
        "poly" => TrendKind::Poly,
        "exp" => TrendKind::Exp,
        "log" => TrendKind::Log,
        "movingAvg" => TrendKind::Moving,
        _ => return None,
    };
    Some(
        Trend {
            kind,
            order: number_child(line, "order").map_or(2, |v| v as u32),
            period: number_child(line, "period").map_or(2, |v| v as u32),
            equation: flag(line, "dispEq"),
            r2: flag(line, "dispRSqr"),
        }
        .sanitized(),
    )
}

/// Um gráfico a partir do XML do gráfico e da âncora no desenho.
fn read_chart(chart_xml: &Node, anchor: &Node, sheet_name: &str) -> Option<Chart> {
    let plot = chart_xml.first("plotArea")?;
    let groups: Vec<&Node> = plot.children.iter().filter(|n| n.name.ends_with("Chart")).collect();
    let has = |name: &str| groups.iter().copied().find(|g| g.name == name);
    // O grupo que dá o tipo: pizza, rosca, dispersão, barras, área ou linha, nessa ordem.
    let base = has("pieChart")
        .or_else(|| has("doughnutChart"))
        .or_else(|| has("scatterChart"))
        .or_else(|| has("barChart"))
        .or_else(|| has("areaChart"))
        .or_else(|| has("lineChart"))?;
    let kind = match base.name.as_str() {
        "pieChart" => ChartKind::Pie,
        "doughnutChart" => ChartKind::Donut,
        "scatterChart" => ChartKind::Scatter,
        "areaChart" => ChartKind::Area,
        "lineChart" => ChartKind::Line,
        _ => {
            if base.child("barDir").and_then(|d| d.attr("val")) == Some("bar") {
                ChartKind::Bar
            } else {
                ChartKind::Column
            }
        }
    };
    let title = title_text(chart_xml.child("chart"));

    // Os dados: a menor área que cobre todas as referências à planilha do gráfico.
    let mut formulas = Vec::new();
    chart_xml.all("f", &mut formulas);
    let mut bounds: Option<(u32, u32, u32, u32)> = None;
    for formula in formulas {
        let Some((sheet, (c1, r1, c2, r2))) = parse_reference(&formula.text) else { continue };
        if sheet != sheet_name {
            continue;
        }
        bounds = Some(match bounds {
            None => (c1, r1, c2, r2),
            Some((a, b, c, d)) => (a.min(c1), b.min(r1), c.max(c2), d.max(r2)),
        });
    }
    let (c1, r1, c2, r2) = bounds?;
    let range = Range { col: c1, row: r1, cols: c2 - c1 + 1, rows: r2 - r1 + 1 };
    if range.cols > MAX_CHART_COLS || range.rows > MAX_CHART_ROWS {
        return None;
    }

    // A âncora: duas células, ou uma célula e um tamanho em EMU (9525 por pixel).
    let corner = |name: &str| -> Option<(u32, u32, u32, u32)> {
        let node = anchor.child(name)?;
        let number = |field: &str| node.child(field)?.text.trim().parse::<u32>().ok();
        Some((number("col")?, number("row")?, number("colOff")?, number("rowOff")?))
    };
    let (from_col, from_row, _, _) = corner("from")?;
    let (to_col, to_row) = match corner("to") {
        Some((col, row, col_off, row_off)) => (
            if col_off == 0 && col > from_col { col - 1 } else { col },
            if row_off == 0 && row > from_row { row - 1 } else { row },
        ),
        None => {
            let ext = anchor.child("ext")?;
            let size = |field: &str| ext.attr(field)?.parse::<u32>().ok();
            (from_col + (size("cx")? / 9525 / 90).max(1) - 1, from_row + (size("cy")? / 9525 / 20).max(1) - 1)
        }
    };

    // Opções: empilhamento, séries (cor, linha, eixo secundário, tendência), rótulos e eixos.
    let mut opts = ChartOptions {
        stacking: match base.child("grouping").and_then(|g| g.attr("val")) {
            Some("stacked") => Stacking::Stacked,
            Some("percentStacked") => Stacking::Percent,
            _ => Stacking::None,
        },
        ..ChartOptions::default()
    };
    let axis_ids = |group: &Node| -> Vec<String> {
        group.children.iter().filter(|n| n.name == "axId").filter_map(|n| n.attr("val")).map(str::to_string).collect()
    };
    let primary_axes = axis_ids(base);
    let combinable = matches!(kind, ChartKind::Column | ChartKind::Line | ChartKind::Area);
    for group in &groups {
        let mut sers = Vec::new();
        group.all("ser", &mut sers);
        let line_overlay = group.name == "lineChart" && kind == ChartKind::Column;
        let secondary = combinable && axis_ids(group) != primary_axes;
        for (position, ser) in sers.iter().enumerate() {
            let index =
                number_child(ser, "order").or_else(|| number_child(ser, "idx")).map_or(position, |v| v as usize);
            if index >= MAX_CHART_COLS as usize {
                continue;
            }
            if opts.styles.len() <= index {
                opts.styles.resize(index + 1, SeriesStyle::default());
            }
            let style = &mut opts.styles[index];
            style.color = series_color(ser)
                .filter(|c| !c.eq_ignore_ascii_case(PALETTE[index % PALETTE.len()]))
                .map(|c| format!("#{}", c.to_ascii_uppercase()));
            style.line = line_overlay;
            style.secondary = secondary;
            style.trend = read_trend(ser);
            if let Some(dlbls) = ser.child("dLbls") {
                opts.labels |= flag(dlbls, "showVal") || flag(dlbls, "showPercent");
            }
            if matches!(group.name.as_str(), "lineChart" | "scatterChart") {
                opts.smooth |= flag(ser, "smooth");
            }
        }
    }
    // Estilos que não mudam nada não precisam ficar.
    while opts.styles.last().is_some_and(|s| *s == SeriesStyle::default()) {
        opts.styles.pop();
    }

    let series_count = {
        let mut all = Vec::new();
        plot.all("ser", &mut all);
        all.len()
    };
    let pie = matches!(kind, ChartKind::Pie | ChartKind::Donut);
    opts.legend = match chart_xml.first("legend") {
        None => {
            if series_count > 1 || pie {
                LegendPos::None
            } else {
                LegendPos::Auto
            }
        }
        Some(legend) => match legend.child("legendPos").and_then(|p| p.attr("val")) {
            Some("r") => LegendPos::Right,
            Some("t") => LegendPos::Top,
            Some("l") => LegendPos::Left,
            _ if series_count > 1 || pie => LegendPos::Auto,
            _ => LegendPos::Bottom,
        },
    };

    // Eixos: o de valores é o que a família principal aponta em segundo lugar.
    if !pie {
        let value_axes: Vec<&Node> = plot.children.iter().filter(|n| n.name == "valAx").collect();
        let category_axes: Vec<&Node> = plot.children.iter().filter(|n| n.name == "catAx").collect();
        let id_of = |n: &Node| n.child("axId").and_then(|a| a.attr("val")).unwrap_or_default().to_string();
        let value_axis = value_axes.iter().copied().find(|n| Some(&id_of(n)) == primary_axes.get(1));
        let x_axis = if kind == ChartKind::Scatter {
            value_axes.iter().copied().find(|n| Some(&id_of(n)) == primary_axes.first())
        } else {
            category_axes.iter().copied().find(|n| Some(&id_of(n)) == primary_axes.first())
        };
        opts.x_title = title_text(x_axis);
        opts.y_title = title_text(value_axis);
        if let Some(axis) = value_axis {
            opts.grid = axis.child("majorGridlines").is_some();
            if let Some(scaling) = axis.child("scaling") {
                opts.y_min = number_child(scaling, "min");
                opts.y_max = number_child(scaling, "max");
            }
        }
    }
    Some(Chart { id: 0, sheet: 0, kind, title, range, from_col, from_row, to_col, to_row, opts })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_references_and_paths() {
        assert_eq!(parse_reference("Planilha1!$B$2:$C$5"), Some(("Planilha1".into(), (1, 1, 2, 4))));
        assert_eq!(parse_reference("'Dados d''água'!A1"), Some(("Dados d'água".into(), (0, 0, 0, 0))));
        assert_eq!(parse_reference("sem planilha"), None);
        assert_eq!(parse_cell("AA10"), Some((26, 9)));
        assert_eq!(col_letters(26), "AA");
        assert_eq!(resolve("xl/worksheets/sheet1.xml", "../drawings/drawing1.xml"), "xl/drawings/drawing1.xml");
        assert_eq!(resolve("xl/workbook.xml", "/xl/worksheets/sheet1.xml"), "xl/worksheets/sheet1.xml");
        assert_eq!(rels_path("xl/worksheets/sheet1.xml"), "xl/worksheets/_rels/sheet1.xml.rels");
    }

    #[test]
    fn unreadable_files_have_no_charts() {
        assert!(read_from(b"isto nao e um zip", &["A".to_string()]).is_empty());
    }
}
