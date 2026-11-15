//! Validação de dados. O IronCalc não a conhece: o Abacus guarda as regras,
//! recusa o que não passa nelas ao digitar, oferece a lista suspensa e grava
//! as regras no `.xlsx` como o Excel faz.

use serde::{Deserialize, Serialize};

use crate::charts::{Node, escape, parse_cell, parse_reference, parse_xml};
use crate::engine::{Engine, Range};
use crate::package;

/// Itens que uma lista suspensa mostra, no máximo.
const MAX_LIST_ITEMS: usize = 500;
/// Ponto em que o Excel passa a usar vírgula entre os itens de uma lista escrita no arquivo.
const FILE_SEPARATOR: &str = ",";

/// O que a regra permite.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ValidationKind {
    /// Um dos itens de uma lista escrita (`Sim; Não`) ou das células de um intervalo (`=A1:A5`).
    List,
    Whole,
    Decimal,
    TextLength,
}

/// Como o valor se compara com os limites.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ValidationOp {
    Between,
    NotBetween,
    Equal,
    NotEqual,
    Greater,
    GreaterEqual,
    Less,
    LessEqual,
}

/// Uma regra de validação de dados.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Validation {
    /// Identifica a regra; o Abacus escolhe ao criar.
    #[serde(default)]
    pub id: u32,
    #[serde(default)]
    pub sheet: u32,
    /// Células da regra, como `A1:B5`.
    pub range: String,
    pub kind: ValidationKind,
    #[serde(default = "default_operator")]
    pub operator: ValidationOp,
    /// Limite, ou a lista (`Sim; Não` ou `=A1:A5`).
    #[serde(default)]
    pub value: String,
    /// Segundo limite, nas regras "entre".
    #[serde(default)]
    pub value2: String,
    /// Texto do aviso; vazio usa um aviso padrão.
    #[serde(default)]
    pub message: String,
}

fn default_operator() -> ValidationOp {
    ValidationOp::Between
}

/// `A1:B5` (ou `A1`) como intervalo a partir de zero.
fn parse_range_text(text: &str) -> Option<Range> {
    let text = text.trim();
    let (first, last) = text.split_once(':').unwrap_or((text, text));
    let (c1, r1) = parse_cell(first)?;
    let (c2, r2) = parse_cell(last)?;
    Some(Range {
        col: c1.min(c2),
        row: r1.min(r2),
        cols: c1.max(c2) - c1.min(c2) + 1,
        rows: r1.max(r2) - r1.min(r2) + 1,
    })
}

fn col_name(col: u32) -> String {
    crate::charts::col_letters(col)
}

fn range_text(range: Range) -> String {
    let first = format!("{}{}", col_name(range.col), range.row + 1);
    if range.cols == 1 && range.rows == 1 {
        return first;
    }
    format!("{first}:{}{}", col_name(range.col + range.cols - 1), range.row + range.rows)
}

/// Lê um número como a pessoa o digita no Brasil (`1.234,5`) ou com ponto decimal (`12.5`).
fn parse_number(text: &str) -> Option<f64> {
    let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    let normalized = if compact.contains(',') { compact.replace('.', "").replace(',', ".") } else { compact };
    normalized.parse::<f64>().ok().filter(|number| number.is_finite())
}

fn compare(operator: ValidationOp, value: f64, first: f64, second: f64) -> bool {
    match operator {
        ValidationOp::Between => value >= first.min(second) && value <= first.max(second),
        ValidationOp::NotBetween => value < first.min(second) || value > first.max(second),
        ValidationOp::Equal => value == first,
        ValidationOp::NotEqual => value != first,
        ValidationOp::Greater => value > first,
        ValidationOp::GreaterEqual => value >= first,
        ValidationOp::Less => value < first,
        ValidationOp::LessEqual => value <= first,
    }
}

/// A condição em palavras, para o aviso padrão.
fn condition(operator: ValidationOp, first: &str, second: &str) -> String {
    match operator {
        ValidationOp::Between => format!("entre {first} e {second}"),
        ValidationOp::NotBetween => format!("fora de {first} a {second}"),
        ValidationOp::Equal => format!("igual a {first}"),
        ValidationOp::NotEqual => format!("diferente de {first}"),
        ValidationOp::Greater => format!("maior que {first}"),
        ValidationOp::GreaterEqual => format!("maior ou igual a {first}"),
        ValidationOp::Less => format!("menor que {first}"),
        ValidationOp::LessEqual => format!("menor ou igual a {first}"),
    }
}

fn default_message(rule: &Validation) -> String {
    let (first, second) = (rule.value.trim(), rule.value2.trim());
    match rule.kind {
        ValidationKind::List => "Escolha um dos valores da lista.".to_string(),
        ValidationKind::Whole => format!("Digite um número inteiro {}.", condition(rule.operator, first, second)),
        ValidationKind::Decimal => format!("Digite um número {}.", condition(rule.operator, first, second)),
        ValidationKind::TextLength => {
            format!("Digite um texto com tamanho {}.", condition(rule.operator, first, second))
        }
    }
}

/// Itens de uma lista escrita, separados por `;`.
fn literal_items(source: &str) -> Vec<String> {
    source.split(';').map(str::trim).filter(|item| !item.is_empty()).map(str::to_string).collect()
}

impl Engine {
    /// Os itens que uma regra de lista aceita.
    fn list_items(&self, rule: &Validation) -> Result<Vec<String>, String> {
        let source = rule.value.trim();
        let Some(reference) = source.strip_prefix('=') else {
            return Ok(literal_items(source));
        };
        let names: Vec<String> = self.model.get_worksheets_properties().into_iter().map(|sheet| sheet.name).collect();
        let (sheet, (c1, r1, c2, r2)) = if reference.contains('!') {
            let (name, bounds) = parse_reference(reference).ok_or("a lista aponta para um intervalo inválido")?;
            (
                names.iter().position(|n| *n == name).ok_or("a lista aponta para uma planilha que não existe")? as u32,
                bounds,
            )
        } else {
            let range = parse_range_text(reference).ok_or("a lista aponta para um intervalo inválido")?;
            (rule.sheet, (range.col, range.row, range.col + range.cols - 1, range.row + range.rows - 1))
        };
        let mut items = Vec::new();
        'rows: for row in r1..=r2 {
            for col in c1..=c2 {
                let text = self.model.get_formatted_cell_value(sheet, (row + 1) as i32, (col + 1) as i32)?;
                if !text.is_empty() {
                    items.push(text);
                }
                if items.len() >= MAX_LIST_ITEMS {
                    break 'rows;
                }
            }
        }
        Ok(items)
    }

    /// O texto passa na regra?
    fn accepts(&self, rule: &Validation, text: &str) -> Result<bool, String> {
        let (first, second) = (parse_number(&rule.value), parse_number(&rule.value2));
        Ok(match rule.kind {
            ValidationKind::List => {
                let wanted = text.to_lowercase();
                self.list_items(rule)?.iter().any(|item| item.to_lowercase() == wanted)
            }
            ValidationKind::Whole | ValidationKind::Decimal => {
                let Some(number) = parse_number(text) else { return Ok(false) };
                if rule.kind == ValidationKind::Whole && number.fract() != 0.0 {
                    return Ok(false);
                }
                compare(rule.operator, number, first.unwrap_or(0.0), second.unwrap_or(0.0))
            }
            ValidationKind::TextLength => {
                compare(rule.operator, text.chars().count() as f64, first.unwrap_or(0.0), second.unwrap_or(0.0))
            }
        })
    }

    /// Recusa o que uma regra da célula não permite. Fórmulas e células
    /// esvaziadas passam, como no Excel.
    pub(crate) fn check_input(&self, sheet: u32, col: u32, row: u32, input: &str) -> Result<(), String> {
        let text = input.trim();
        if text.is_empty() || text.starts_with('=') {
            return Ok(());
        }
        for rule in self.validations.iter().filter(|rule| rule.sheet == sheet) {
            let Some(range) = parse_range_text(&rule.range) else { continue };
            if col < range.col || col >= range.col + range.cols || row < range.row || row >= range.row + range.rows {
                continue;
            }
            if !self.accepts(rule, text)? {
                let message = if rule.message.trim().is_empty() {
                    default_message(rule)
                } else {
                    rule.message.trim().to_string()
                };
                return Err(format!("Validação: {message}"));
            }
        }
        Ok(())
    }

    /// Confere uma regra antes de guardá-la.
    fn check_rule(&self, rule: &Validation) -> Result<Validation, String> {
        let range = parse_range_text(&rule.range).ok_or("escreva as células assim: A1:B5")?;
        let mut rule = Validation { range: range_text(range), ..rule.clone() };
        if usize::try_from(rule.sheet).map_or(true, |sheet| sheet >= self.model.get_worksheets_properties().len()) {
            return Err("a planilha da regra não existe".into());
        }
        match rule.kind {
            ValidationKind::List => {
                rule.operator = ValidationOp::Between;
                rule.value2.clear();
                if rule.value.trim().is_empty() {
                    return Err("escreva os itens da lista, separados por ponto e vírgula (Sim; Não)".into());
                }
                if rule.value.trim().starts_with('=') {
                    self.list_items(&rule)?;
                } else if rule.value.contains(FILE_SEPARATOR) {
                    return Err("os itens da lista não podem ter vírgula; separe-os com ponto e vírgula".into());
                }
            }
            _ => {
                let between = matches!(rule.operator, ValidationOp::Between | ValidationOp::NotBetween);
                if parse_number(&rule.value).is_none() {
                    return Err("escreva o limite como número".into());
                }
                if between && parse_number(&rule.value2).is_none() {
                    return Err("escreva os dois limites como número".into());
                }
                if !between {
                    rule.value2.clear();
                }
            }
        }
        Ok(rule)
    }

    /// Regras de validação de uma planilha.
    pub fn validations(&self, sheet: u32) -> Vec<Validation> {
        self.validations.iter().filter(|rule| rule.sheet == sheet).cloned().collect()
    }

    pub fn add_validation(&mut self, sheet: u32, rule: Validation) -> Result<Validation, String> {
        let mut rule = self.check_rule(&Validation { sheet, ..rule })?;
        rule.id = self.validations.iter().map(|one| one.id).max().unwrap_or(0) + 1;
        self.validations.push(rule.clone());
        Ok(rule)
    }

    pub fn update_validation(&mut self, sheet: u32, rule: Validation) -> Result<Validation, String> {
        let rule = self.check_rule(&Validation { sheet, ..rule })?;
        let slot = self.validations.iter_mut().find(|one| one.id == rule.id).ok_or("a regra não existe mais")?;
        *slot = rule.clone();
        Ok(rule)
    }

    pub fn delete_validation(&mut self, id: u32) -> Result<(), String> {
        let before = self.validations.len();
        self.validations.retain(|rule| rule.id != id);
        if self.validations.len() == before {
            return Err("a regra não existe mais".into());
        }
        Ok(())
    }

    /// Itens da lista suspensa da célula, se uma regra de lista a cobre.
    pub fn list_options(&self, sheet: u32, col: u32, row: u32) -> Result<Option<Vec<String>>, String> {
        for rule in self.validations.iter().filter(|rule| rule.sheet == sheet && rule.kind == ValidationKind::List) {
            let Some(range) = parse_range_text(&rule.range) else { continue };
            if col >= range.col && col < range.col + range.cols && row >= range.row && row < range.row + range.rows {
                return self.list_items(rule).map(Some);
            }
        }
        Ok(None)
    }

    /// As regras de uma planilha que foi excluída somem; as das seguintes sobem um índice.
    pub(crate) fn drop_validations_of(&mut self, sheet: u32) {
        self.validations.retain(|rule| rule.sheet != sheet);
        for rule in &mut self.validations {
            if rule.sheet > sheet {
                rule.sheet -= 1;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Gravação
// ---------------------------------------------------------------------------

fn operator_name(operator: ValidationOp) -> &'static str {
    match operator {
        ValidationOp::Between => "between",
        ValidationOp::NotBetween => "notBetween",
        ValidationOp::Equal => "equal",
        ValidationOp::NotEqual => "notEqual",
        ValidationOp::Greater => "greaterThan",
        ValidationOp::GreaterEqual => "greaterThanOrEqual",
        ValidationOp::Less => "lessThan",
        ValidationOp::LessEqual => "lessThanOrEqual",
    }
}

fn operator_from(name: &str) -> ValidationOp {
    match name {
        "notBetween" => ValidationOp::NotBetween,
        "equal" => ValidationOp::Equal,
        "notEqual" => ValidationOp::NotEqual,
        "greaterThan" => ValidationOp::Greater,
        "greaterThanOrEqual" => ValidationOp::GreaterEqual,
        "lessThan" => ValidationOp::Less,
        "lessThanOrEqual" => ValidationOp::LessEqual,
        _ => ValidationOp::Between,
    }
}

/// Número como o arquivo guarda: ponto decimal.
fn file_number(text: &str) -> String {
    parse_number(text).map_or_else(|| text.trim().to_string(), |number| number.to_string())
}

/// Referência de intervalo como o arquivo guarda (`$A$1:$A$5`, com a planilha se for outra).
fn file_reference(rule: &Validation, reference: &str, sheet_names: &[String]) -> String {
    if reference.contains('!') {
        return reference.to_string();
    }
    let Some(range) = parse_range_text(reference) else { return reference.to_string() };
    let cell = |col: u32, row: u32| format!("${}${}", col_name(col), row + 1);
    let here = if range.cols == 1 && range.rows == 1 {
        cell(range.col, range.row)
    } else {
        format!("{}:{}", cell(range.col, range.row), cell(range.col + range.cols - 1, range.row + range.rows - 1))
    };
    let _ = (rule, sheet_names);
    here
}

fn validation_xml(rule: &Validation, sheet_names: &[String]) -> String {
    let kind = match rule.kind {
        ValidationKind::List => "list",
        ValidationKind::Whole => "whole",
        ValidationKind::Decimal => "decimal",
        ValidationKind::TextLength => "textLength",
    };
    let formulas = match rule.kind {
        ValidationKind::List => {
            let source = rule.value.trim();
            let formula = match source.strip_prefix('=') {
                Some(reference) => file_reference(rule, reference, sheet_names),
                None => format!("\"{}\"", literal_items(source).join(FILE_SEPARATOR)),
            };
            format!("<formula1>{}</formula1>", escape(&formula))
        }
        _ => {
            let second = if rule.value2.trim().is_empty() {
                String::new()
            } else {
                format!("<formula2>{}</formula2>", file_number(&rule.value2))
            };
            format!("<formula1>{}</formula1>{second}", file_number(&rule.value))
        }
    };
    let operator = if rule.kind == ValidationKind::List {
        String::new()
    } else {
        format!(" operator=\"{}\"", operator_name(rule.operator))
    };
    let message = if rule.message.trim().is_empty() {
        String::new()
    } else {
        format!(" error=\"{}\"", escape(rule.message.trim()))
    };
    format!(
        "<dataValidation type=\"{kind}\"{operator} allowBlank=\"1\" showErrorMessage=\"1\"{message} sqref=\"{}\">{formulas}</dataValidation>",
        rule.range
    )
}

/// Tags que vêm depois de `dataValidations` numa planilha; ela entra antes da primeira que existir.
const AFTER_VALIDATIONS: [&str; 21] = [
    "<hyperlinks",
    "<printOptions",
    "<pageMargins",
    "<pageSetup",
    "<headerFooter",
    "<rowBreaks",
    "<colBreaks",
    "<customProperties",
    "<cellWatches",
    "<ignoredErrors",
    "<smartTags",
    "<drawing",
    "<legacyDrawing",
    "<legacyDrawingHF",
    "<picture",
    "<oleObjects",
    "<controls",
    "<webPublishItems",
    "<tableParts",
    "<extLst",
    "</worksheet>",
];

/// Acrescenta as regras às planilhas de um `.xlsx` gravado pelo motor.
pub fn write_into(xlsx: Vec<u8>, sheet_names: &[String], rules: &[Validation]) -> Result<Vec<u8>, String> {
    if rules.is_empty() {
        return Ok(xlsx);
    }
    let mut parts = package::read_parts(&xlsx)?;
    for (index, part) in package::sheet_parts(&xlsx, sheet_names) {
        let here: Vec<&Validation> = rules.iter().filter(|rule| rule.sheet as usize == index).collect();
        if here.is_empty() {
            continue;
        }
        let mut xml = package::take_text(&mut parts, &part).ok_or("planilha não encontrada no arquivo")?;
        let body: String = here.iter().map(|rule| validation_xml(rule, sheet_names)).collect();
        let block = format!("<dataValidations count=\"{}\">{body}</dataValidations>", here.len());
        let at = AFTER_VALIDATIONS.iter().filter_map(|tag| xml.find(tag)).min().ok_or("planilha inválida")?;
        xml.insert_str(at, &block);
        parts.push((part, xml.into_bytes()));
    }
    package::write_parts(parts)
}

// ---------------------------------------------------------------------------
// Leitura
// ---------------------------------------------------------------------------

/// Regras de validação que um arquivo traz; ids a partir de 1. Só as de lista,
/// número inteiro, decimal e tamanho do texto; as de data, hora e fórmula ficam de fora.
pub fn read_from(xlsx: &[u8], sheet_names: &[String]) -> Vec<Validation> {
    let Ok(parts) = package::read_parts(xlsx) else { return Vec::new() };
    let mut rules = Vec::new();
    for (index, part) in package::sheet_parts(xlsx, sheet_names) {
        let Some(root) = parts.iter().find(|(name, _)| *name == part).and_then(|(_, bytes)| parse_xml(bytes)) else {
            continue;
        };
        let mut found = Vec::new();
        root.all("dataValidation", &mut found);
        for node in found {
            for rule in read_rule(node, index as u32) {
                rules.push(Validation { id: rules.len() as u32 + 1, ..rule });
            }
        }
    }
    rules
}

fn read_rule(node: &Node, sheet: u32) -> Vec<Validation> {
    let kind = match node.attr("type") {
        Some("list") => ValidationKind::List,
        Some("whole") => ValidationKind::Whole,
        Some("decimal") => ValidationKind::Decimal,
        Some("textLength") => ValidationKind::TextLength,
        _ => return Vec::new(),
    };
    let formula = |name: &str| node.child(name).map(|f| f.text.trim().to_string()).unwrap_or_default();
    let (value, value2) = match kind {
        ValidationKind::List => {
            let source = formula("formula1");
            let value = match source.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
                Some(items) => items.split(FILE_SEPARATOR).map(str::trim).collect::<Vec<_>>().join("; "),
                None => format!("={source}").replace('$', ""),
            };
            (value, String::new())
        }
        _ => (formula("formula1").replace('.', ","), formula("formula2").replace('.', ",")),
    };
    // `sqref` pode listar vários intervalos; cada um vira uma regra.
    node.attr("sqref")
        .unwrap_or_default()
        .split_whitespace()
        .filter_map(parse_range_text)
        .map(|range| Validation {
            id: 0,
            sheet,
            range: range_text(range),
            kind,
            operator: operator_from(node.attr("operator").unwrap_or("between")),
            value: value.clone(),
            value2: value2.clone(),
            message: node.attr("error").unwrap_or_default().to_string(),
        })
        .collect()
}
