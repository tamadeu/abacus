//! Tabelas dinâmicas: o núcleo que agrupa e resume uma tabela de dados.
//!
//! Esta parte não conhece planilhas: recebe os dados já lidos (`Source`) e a
//! definição da tabela (`Pivot`) e devolve a grade pronta (`Grid`). Quem lê a
//! origem e escreve o resultado nas células é o motor (`pivot_sheet.rs`).

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

const EMPTY_LABEL: &str = "(vazio)";
const GRAND_LABEL: &str = "Total Geral";
/// Separa os itens de uma chave; não aparece em texto digitado.
const SEP: char = '\u{1}';

fn yes() -> bool {
    true
}

fn default_step() -> f64 {
    10.0
}

/// Como os valores de um campo são juntados em grupos.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Group {
    /// Cada valor é um item.
    #[default]
    None,
    Year,
    Quarter,
    Month,
    YearMonth,
    /// Faixas de tamanho fixo (`step`).
    Bucket,
}

/// Em que ordem os itens de um campo aparecem.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SortKind {
    #[default]
    LabelAsc,
    LabelDesc,
    /// Pelo total do valor `sort_value`, do menor para o maior.
    ValueAsc,
    ValueDesc,
}

/// Um campo nas linhas ou nas colunas.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Field {
    pub name: String,
    #[serde(default)]
    pub group: Group,
    /// Tamanho da faixa, quando `group` é `bucket`.
    #[serde(default = "default_step")]
    pub step: f64,
    #[serde(default)]
    pub sort: SortKind,
    /// Posição (a partir de zero) do valor que ordena, se `sort` for por valor.
    #[serde(default)]
    pub sort_value: u32,
    /// Mostra só os primeiros itens, depois de ordenar.
    #[serde(default)]
    pub top: Option<u32>,
}

/// Como um campo de valor é resumido.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Agg {
    #[default]
    Sum,
    /// Células preenchidas.
    Count,
    /// Células com número.
    CountNums,
    Average,
    Min,
    Max,
    Product,
    StdDev,
    Var,
    /// Quantidade de valores diferentes.
    Distinct,
    /// Conta sobre as somas de outros campos (`formula`).
    Calc,
}

/// O que se mostra no lugar do valor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ShowAs {
    #[default]
    Normal,
    PctTotal,
    PctRow,
    PctCol,
}

/// Um campo na área de valores.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValueField {
    /// Campo de origem; vazio num campo calculado.
    #[serde(default)]
    pub field: String,
    #[serde(default)]
    pub agg: Agg,
    #[serde(default)]
    pub show_as: ShowAs,
    /// Título da coluna; vazio usa um padrão (`Soma de Vendas`).
    #[serde(default)]
    pub title: String,
    /// Formato numérico das células; vazio usa o padrão.
    #[serde(default)]
    pub format: String,
    /// Conta de um campo calculado, como `[Preço] * [Quantidade]`.
    #[serde(default)]
    pub formula: String,
}

/// Itens de um campo que ficam de fora.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Filter {
    pub field: String,
    #[serde(default)]
    pub hidden: Vec<String>,
}

/// A definição de uma tabela dinâmica.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pivot {
    /// Identifica a tabela; o Abacus escolhe ao criar.
    #[serde(default)]
    pub id: u32,
    #[serde(default)]
    pub source_sheet: u32,
    /// Células de origem, como `A1:E100`; a primeira linha tem os nomes dos campos.
    pub source: String,
    /// Planilha onde a tabela aparece.
    #[serde(default)]
    pub sheet: u32,
    /// Canto de cima e da esquerda da tabela (a partir de zero).
    #[serde(default)]
    pub col: u32,
    #[serde(default)]
    pub row: u32,
    #[serde(default)]
    pub rows: Vec<Field>,
    #[serde(default)]
    pub cols: Vec<Field>,
    #[serde(default)]
    pub values: Vec<ValueField>,
    #[serde(default)]
    pub filters: Vec<Filter>,
    /// Linha de total geral, embaixo.
    #[serde(default = "yes")]
    pub grand_rows: bool,
    /// Coluna de total geral, à direita.
    #[serde(default = "yes")]
    pub grand_cols: bool,
    /// Subtotais de cada grupo, quando há mais de um campo.
    #[serde(default = "yes")]
    pub subtotals: bool,
    /// Repete o nome do grupo em todas as linhas.
    #[serde(default)]
    pub repeat_labels: bool,
    /// Tamanho da última grade escrita, para limpá-la ao atualizar.
    #[serde(default)]
    pub width: u32,
    #[serde(default)]
    pub height: u32,
}

/// Uma célula lida da origem.
#[derive(Debug, Clone, Default)]
pub struct SrcCell {
    pub text: String,
    pub num: Option<f64>,
}

/// Os dados de origem: nomes dos campos e as linhas.
pub struct Source {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<SrcCell>>,
}

/// O papel de uma linha ou coluna da grade, para o estilo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Header,
    Body,
    Subtotal,
    Grand,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Out {
    Empty,
    Text(String),
    Num(f64),
}

/// O resultado, pronto para ser escrito.
pub struct Grid {
    pub cells: Vec<Vec<Out>>,
    pub row_roles: Vec<Role>,
    pub col_roles: Vec<Role>,
    /// Formato numérico de cada coluna (só do corpo), se houver.
    pub col_formats: Vec<Option<String>>,
    pub header_rows: usize,
    pub label_cols: usize,
    /// Linhas no começo com os filtros e a linha em branco depois deles.
    pub page_rows: usize,
}

// ---------------------------------------------------------------------------
// Datas
// ---------------------------------------------------------------------------

/// Ano, mês e dia de um número de série do Excel.
fn date_of(serial: f64) -> (i64, i64, i64) {
    let days = serial.floor() as i64 - 25569;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    (if month <= 2 { year + 1 } else { year }, month, day)
}

const MONTHS: [&str; 12] = ["jan", "fev", "mar", "abr", "mai", "jun", "jul", "ago", "set", "out", "nov", "dez"];

fn plain_number(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 { format!("{}", n as i64) } else { format!("{n}") }
}

// ---------------------------------------------------------------------------
// Rótulos
// ---------------------------------------------------------------------------

/// O item a que um valor pertence, com o que serve para ordenar.
#[derive(Debug, Clone)]
struct Label {
    text: String,
    num: Option<f64>,
    blank: bool,
}

fn label_of(field: &Field, cell: &SrcCell) -> Label {
    if cell.text.trim().is_empty() && cell.num.is_none() {
        return Label { text: EMPTY_LABEL.into(), num: None, blank: true };
    }
    let plain = || Label { text: cell.text.clone(), num: cell.num, blank: false };
    let Some(x) = cell.num else { return plain() };
    let made = |text: String, num: f64| Label { text, num: Some(num), blank: false };
    match field.group {
        Group::None => plain(),
        Group::Year => made(date_of(x).0.to_string(), date_of(x).0 as f64),
        Group::Quarter => {
            let q = (date_of(x).1 - 1) / 3 + 1;
            made(format!("T{q}"), q as f64)
        }
        Group::Month => {
            let m = date_of(x).1;
            made(MONTHS[(m - 1) as usize].to_string(), m as f64)
        }
        Group::YearMonth => {
            let (y, m, _) = date_of(x);
            made(format!("{y}-{m:02}"), (y * 12 + m) as f64)
        }
        Group::Bucket => {
            let step = if field.step > 0.0 { field.step } else { 10.0 };
            let low = (x / step).floor() * step;
            made(format!("{}–{}", plain_number(low), plain_number(low + step)), low)
        }
    }
}

fn compare_labels(a: &Label, b: &Label) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    match (a.blank, b.blank) {
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        _ => {}
    }
    match (a.num, b.num) {
        (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(Ordering::Equal),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        _ => a.text.to_lowercase().cmp(&b.text.to_lowercase()),
    }
}

// ---------------------------------------------------------------------------
// Conta de campo calculado
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
enum Expr {
    Num(f64),
    Ref(usize),
    Neg(Box<Expr>),
    Bin(char, Box<Expr>, Box<Expr>),
}

struct Parser<'a> {
    chars: Vec<char>,
    at: usize,
    refs: &'a mut Vec<String>,
}

impl Parser<'_> {
    fn skip(&mut self) {
        while self.chars.get(self.at).is_some_and(|c| c.is_whitespace()) {
            self.at += 1;
        }
    }

    fn peek(&mut self) -> Option<char> {
        self.skip();
        self.chars.get(self.at).copied()
    }

    fn sum(&mut self) -> Result<Expr, String> {
        let mut left = self.product()?;
        while let Some(op @ ('+' | '-')) = self.peek() {
            self.at += 1;
            left = Expr::Bin(op, Box::new(left), Box::new(self.product()?));
        }
        Ok(left)
    }

    fn product(&mut self) -> Result<Expr, String> {
        let mut left = self.unary()?;
        while let Some(op @ ('*' | '/')) = self.peek() {
            self.at += 1;
            left = Expr::Bin(op, Box::new(left), Box::new(self.unary()?));
        }
        Ok(left)
    }

    fn unary(&mut self) -> Result<Expr, String> {
        match self.peek() {
            Some('-') => {
                self.at += 1;
                Ok(Expr::Neg(Box::new(self.unary()?)))
            }
            Some('+') => {
                self.at += 1;
                self.unary()
            }
            _ => self.atom(),
        }
    }

    fn atom(&mut self) -> Result<Expr, String> {
        match self.peek() {
            Some('(') => {
                self.at += 1;
                let inner = self.sum()?;
                if self.peek() != Some(')') {
                    return Err("falta fechar o parêntese".into());
                }
                self.at += 1;
                Ok(inner)
            }
            Some('[') => {
                let start = self.at + 1;
                let end = self.chars[start..].iter().position(|&c| c == ']').ok_or("falta fechar o colchete")? + start;
                let name: String = self.chars[start..end].iter().collect();
                self.at = end + 1;
                let name = name.trim().to_string();
                let index = self.refs.iter().position(|n| *n == name).unwrap_or_else(|| {
                    self.refs.push(name);
                    self.refs.len() - 1
                });
                Ok(Expr::Ref(index))
            }
            Some(c) if c.is_ascii_digit() || c == '.' || c == ',' => {
                let start = self.at;
                while self.chars.get(self.at).is_some_and(|c| c.is_ascii_digit() || *c == '.' || *c == ',') {
                    self.at += 1;
                }
                let text: String = self.chars[start..self.at].iter().collect::<String>().replace(',', ".");
                text.parse::<f64>().map(Expr::Num).map_err(|_| format!("número inválido: {text}"))
            }
            Some(c) => Err(format!("não entendi “{c}” na conta; escreva os campos entre colchetes, como [Preço]")),
            None => Err("a conta está incompleta".into()),
        }
    }
}

/// Lê a conta e devolve a expressão e os nomes dos campos que ela cita.
fn parse_formula(text: &str) -> Result<(Expr, Vec<String>), String> {
    if text.trim().is_empty() {
        return Err("escreva a conta do campo calculado, como [Preço] * [Quantidade]".into());
    }
    let mut refs = Vec::new();
    let mut parser = Parser { chars: text.chars().collect(), at: 0, refs: &mut refs };
    let expr = parser.sum()?;
    if parser.peek().is_some() {
        return Err("sobrou texto no fim da conta".into());
    }
    Ok((expr, refs))
}

fn eval(expr: &Expr, sums: &[f64]) -> Option<f64> {
    Some(match expr {
        Expr::Num(n) => *n,
        Expr::Ref(i) => sums[*i],
        Expr::Neg(e) => -eval(e, sums)?,
        Expr::Bin(op, a, b) => {
            let (x, y) = (eval(a, sums)?, eval(b, sums)?);
            match op {
                '+' => x + y,
                '-' => x - y,
                '*' => x * y,
                _ if y == 0.0 => return None,
                _ => x / y,
            }
        }
    })
}

// ---------------------------------------------------------------------------
// Acumuladores
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct Acc {
    count: f64,
    nums: f64,
    sum: f64,
    sumsq: f64,
    min: f64,
    max: f64,
    prod: f64,
    distinct: HashSet<String>,
    calc: Vec<f64>,
}

impl Acc {
    fn new(refs: usize) -> Self {
        Self {
            count: 0.0,
            nums: 0.0,
            sum: 0.0,
            sumsq: 0.0,
            min: f64::INFINITY,
            max: f64::NEG_INFINITY,
            prod: 1.0,
            distinct: HashSet::new(),
            calc: vec![0.0; refs],
        }
    }

    fn add(&mut self, cell: &SrcCell, distinct: bool) {
        if cell.text.is_empty() && cell.num.is_none() {
            return;
        }
        self.count += 1.0;
        if distinct {
            self.distinct.insert(cell.text.clone());
        }
        if let Some(x) = cell.num {
            self.nums += 1.0;
            self.sum += x;
            self.sumsq += x * x;
            self.min = self.min.min(x);
            self.max = self.max.max(x);
            self.prod *= x;
        }
    }

    fn variance(&self) -> Option<f64> {
        (self.nums > 1.0).then(|| ((self.sumsq - self.sum * self.sum / self.nums) / (self.nums - 1.0)).max(0.0))
    }
}

/// Uma coluna de valor já preparada.
struct Prepared {
    index: usize,
    refs: Vec<usize>,
    expr: Option<Expr>,
}

fn aggregate(vf: &ValueField, prep: &Prepared, acc: &Acc) -> Option<f64> {
    match vf.agg {
        Agg::Sum => Some(acc.sum),
        Agg::Count => Some(acc.count),
        Agg::CountNums => Some(acc.nums),
        Agg::Average => (acc.nums > 0.0).then(|| acc.sum / acc.nums),
        Agg::Min => (acc.nums > 0.0).then_some(acc.min),
        Agg::Max => (acc.nums > 0.0).then_some(acc.max),
        Agg::Product => (acc.nums > 0.0).then_some(acc.prod),
        Agg::StdDev => acc.variance().map(f64::sqrt),
        Agg::Var => acc.variance(),
        Agg::Distinct => Some(acc.distinct.len() as f64),
        Agg::Calc => prep.expr.as_ref().and_then(|e| eval(e, &acc.calc)),
    }
}

fn agg_name(agg: Agg) -> &'static str {
    match agg {
        Agg::Sum => "Soma",
        Agg::Count => "Contagem",
        Agg::CountNums => "Contagem de números",
        Agg::Average => "Média",
        Agg::Min => "Mínimo",
        Agg::Max => "Máximo",
        Agg::Product => "Produto",
        Agg::StdDev => "Desvio padrão",
        Agg::Var => "Variância",
        Agg::Distinct => "Valores distintos",
        Agg::Calc => "Calculado",
    }
}

/// Título de uma coluna de valor.
pub fn value_title(vf: &ValueField) -> String {
    if !vf.title.trim().is_empty() {
        return vf.title.trim().to_string();
    }
    if vf.agg == Agg::Calc {
        return "Campo calculado".into();
    }
    format!("{} de {}", agg_name(vf.agg), vf.field)
}

// ---------------------------------------------------------------------------
// Eixos
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Leaf,
    Sub,
    Grand,
}

#[derive(Debug, Clone)]
struct Item {
    key: Vec<String>,
    kind: Kind,
}

type Labels = HashMap<(usize, String), Label>;

struct AxisCtx<'a> {
    fields: &'a [Field],
    labels: &'a Labels,
    /// Total do valor `index` para uma chave de prefixo.
    metric: &'a dyn Fn(&[String], usize) -> Option<f64>,
    subtotals: bool,
}

fn walk(ctx: &AxisCtx, prefix: &[String], keys: &[&Vec<String>], out: &mut Vec<Item>) {
    let level = prefix.len();
    let field = &ctx.fields[level];
    let mut kids: Vec<&String> = keys.iter().map(|k| &k[level]).collect::<HashSet<_>>().into_iter().collect();
    let by_label = |a: &&String, b: &&String| {
        let (la, lb) = (&ctx.labels[&(level, (*a).clone())], &ctx.labels[&(level, (*b).clone())]);
        compare_labels(la, lb)
    };
    let total = |kid: &&String| {
        let mut key = prefix.to_vec();
        key.push((*kid).clone());
        (ctx.metric)(&key, field.sort_value as usize)
    };
    let by_value = |a: &&String, b: &&String| {
        let (x, y) = (total(a), total(b));
        x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal)
    };
    match field.sort {
        SortKind::LabelAsc => kids.sort_by(by_label),
        SortKind::LabelDesc => kids.sort_by(|a, b| by_label(b, a)),
        SortKind::ValueAsc => kids.sort_by(|a, b| by_value(a, b).then_with(|| by_label(a, b))),
        SortKind::ValueDesc => kids.sort_by(|a, b| by_value(b, a).then_with(|| by_label(a, b))),
    }
    if let Some(top) = field.top.filter(|top| *top > 0) {
        kids.truncate(top as usize);
    }
    for kid in kids {
        let mut next = prefix.to_vec();
        next.push(kid.clone());
        if level + 1 == ctx.fields.len() {
            out.push(Item { key: next, kind: Kind::Leaf });
        } else {
            let sub: Vec<&Vec<String>> = keys.iter().copied().filter(|k| k[level] == *kid).collect();
            walk(ctx, &next, &sub, out);
            if ctx.subtotals {
                out.push(Item { key: next, kind: Kind::Sub });
            }
        }
    }
}

fn build_items(ctx: &AxisCtx, keys: &[Vec<String>], grand: bool) -> Vec<Item> {
    if ctx.fields.is_empty() {
        return vec![Item { key: Vec::new(), kind: Kind::Leaf }];
    }
    let mut seen = HashSet::new();
    let unique: Vec<&Vec<String>> = keys.iter().filter(|k| seen.insert((*k).clone())).collect();
    let mut out = Vec::new();
    walk(ctx, &[], &unique, &mut out);
    if grand {
        out.push(Item { key: Vec::new(), kind: Kind::Grand });
    }
    out
}

fn join(key: &[String]) -> String {
    key.join(&SEP.to_string())
}

// ---------------------------------------------------------------------------
// Cálculo
// ---------------------------------------------------------------------------

/// Calcula a tabela dinâmica.
#[allow(clippy::needless_range_loop)]
pub fn compute(pivot: &Pivot, source: &Source) -> Result<Grid, String> {
    if pivot.values.is_empty() && pivot.rows.is_empty() && pivot.cols.is_empty() {
        let hint = Out::Text("Escolha os campos da tabela dinâmica".into());
        return Ok(Grid {
            cells: vec![vec![hint]],
            row_roles: vec![Role::Header],
            col_roles: vec![Role::Body],
            col_formats: vec![None],
            header_rows: 1,
            label_cols: 1,
            page_rows: 0,
        });
    }
    // Sem valores não há o que cruzar: só a lista de itens das linhas.
    let listing;
    let pivot = if pivot.values.is_empty() && !pivot.cols.is_empty() {
        listing = Pivot { cols: Vec::new(), ..pivot.clone() };
        &listing
    } else {
        pivot
    };
    let index_of = |name: &str| -> Result<usize, String> {
        source
            .headers
            .iter()
            .position(|h| h.trim() == name.trim())
            .ok_or_else(|| format!("o campo “{name}” não existe nos dados de origem"))
    };
    let row_idx: Vec<usize> = pivot.rows.iter().map(|f| index_of(&f.name)).collect::<Result<_, _>>()?;
    let col_idx: Vec<usize> = pivot.cols.iter().map(|f| index_of(&f.name)).collect::<Result<_, _>>()?;
    let mut prepared = Vec::new();
    for vf in &pivot.values {
        if vf.agg == Agg::Calc {
            let (expr, names) = parse_formula(&vf.formula)?;
            let refs = names.iter().map(|n| index_of(n)).collect::<Result<Vec<_>, _>>()?;
            prepared.push(Prepared { index: 0, refs, expr: Some(expr) });
        } else {
            prepared.push(Prepared { index: index_of(&vf.field)?, refs: Vec::new(), expr: None });
        }
    }
    let mut filters = Vec::new();
    for filter in &pivot.filters {
        let field = Field {
            name: filter.field.clone(),
            group: pivot
                .rows
                .iter()
                .chain(&pivot.cols)
                .find(|f| f.name == filter.field)
                .map_or(Group::None, |f| f.group),
            step: pivot.rows.iter().chain(&pivot.cols).find(|f| f.name == filter.field).map_or(10.0, |f| f.step),
            sort: SortKind::LabelAsc,
            sort_value: 0,
            top: None,
        };
        filters.push((index_of(&filter.field)?, field, filter.hidden.iter().cloned().collect::<HashSet<_>>()));
    }

    let mut row_labels = Labels::new();
    let mut col_labels = Labels::new();
    let mut row_keys = Vec::new();
    let mut col_keys = Vec::new();
    let mut accs: HashMap<(String, String), Vec<Acc>> = HashMap::new();
    let fresh = || -> Vec<Acc> { prepared.iter().map(|p| Acc::new(p.refs.len())).collect() };

    for record in &source.rows {
        let cell = |i: usize| record.get(i).cloned().unwrap_or_default();
        if filters.iter().any(|(i, field, hidden)| hidden.contains(&label_of(field, &cell(*i)).text)) {
            continue;
        }
        let key_of = |fields: &[Field], idx: &[usize], labels: &mut Labels| -> Vec<String> {
            fields
                .iter()
                .zip(idx)
                .enumerate()
                .map(|(level, (field, &i))| {
                    let label = label_of(field, &cell(i));
                    let text = label.text.clone();
                    labels.entry((level, text.clone())).or_insert(label);
                    text
                })
                .collect()
        };
        let row_key = key_of(&pivot.rows, &row_idx, &mut row_labels);
        let col_key = key_of(&pivot.cols, &col_idx, &mut col_labels);
        for rp in 0..=row_key.len() {
            let rk = join(&row_key[..rp]);
            for cp in 0..=col_key.len() {
                let slot = accs.entry((rk.clone(), join(&col_key[..cp]))).or_insert_with(&fresh);
                for (vi, vf) in pivot.values.iter().enumerate() {
                    let prep = &prepared[vi];
                    if vf.agg == Agg::Calc {
                        for (r, &i) in prep.refs.iter().enumerate() {
                            slot[vi].calc[r] += cell(i).num.unwrap_or(0.0);
                        }
                    } else {
                        slot[vi].add(&cell(prep.index), vf.agg == Agg::Distinct);
                    }
                }
            }
        }
        row_keys.push(row_key);
        col_keys.push(col_key);
    }

    let raw = |rk: &str, ck: &str, vi: usize| -> Option<f64> {
        let acc = accs.get(&(rk.to_string(), ck.to_string()))?;
        aggregate(&pivot.values[vi], &prepared[vi], &acc[vi])
    };
    let shown = |rk: &str, ck: &str, vi: usize| -> Option<f64> {
        let value = raw(rk, ck, vi)?;
        let denom = match pivot.values[vi].show_as {
            ShowAs::Normal => return Some(value),
            ShowAs::PctTotal => raw("", "", vi),
            ShowAs::PctRow => raw(rk, "", vi),
            ShowAs::PctCol => raw("", ck, vi),
        }?;
        (denom != 0.0).then(|| value / denom)
    };

    let last_value = pivot.values.len().saturating_sub(1);
    let row_metric = |key: &[String], vi: usize| {
        if pivot.values.is_empty() { None } else { raw(&join(key), "", vi.min(last_value)) }
    };
    let col_metric = |key: &[String], vi: usize| {
        if pivot.values.is_empty() { None } else { raw("", &join(key), vi.min(last_value)) }
    };
    let row_items = build_items(
        &AxisCtx { fields: &pivot.rows, labels: &row_labels, metric: &row_metric, subtotals: pivot.subtotals },
        &row_keys,
        pivot.grand_rows,
    );
    let col_items = build_items(
        &AxisCtx { fields: &pivot.cols, labels: &col_labels, metric: &col_metric, subtotals: pivot.subtotals },
        &col_keys,
        pivot.grand_cols,
    );
    // Uma tabela sem registros ainda mostra o cabeçalho.
    let value_count = pivot.values.len();
    let label_cols = pivot.rows.len().max(1);
    let multi = value_count > 1;
    let header_rows = pivot.cols.len() + 1;
    let width = label_cols + col_items.len() * value_count;
    let titles: Vec<String> = pivot.values.iter().map(value_title).collect();

    let mut cells = vec![vec![Out::Empty; width]; header_rows];
    let mut col_roles = vec![Role::Body; width];
    let mut col_formats: Vec<Option<String>> = vec![None; width];
    // Cabeçalhos das colunas.
    let mut previous: Option<&Item> = None;
    for (n, item) in col_items.iter().enumerate() {
        for h in 0..pivot.cols.len() {
            let text = match item.kind {
                Kind::Leaf if item.key.len() > h => {
                    let same = previous.is_some_and(|p| p.kind == Kind::Leaf && p.key[..=h] == item.key[..=h]);
                    (!same || pivot.repeat_labels).then(|| item.key[h].clone())
                }
                Kind::Sub if h + 1 == item.key.len() => Some(format!("{} Total", item.key[h])),
                Kind::Grand if h == 0 => Some(GRAND_LABEL.to_string()),
                _ => None,
            };
            if let Some(text) = text {
                cells[h][label_cols + n * value_count] = Out::Text(text);
            }
        }
        previous = Some(item);
        for vi in 0..value_count {
            let c = label_cols + n * value_count + vi;
            col_roles[c] = match item.kind {
                Kind::Leaf => Role::Body,
                Kind::Sub => Role::Subtotal,
                Kind::Grand => Role::Grand,
            };
            let vf = &pivot.values[vi];
            col_formats[c] = if !vf.format.trim().is_empty() {
                Some(vf.format.trim().to_string())
            } else if vf.show_as != ShowAs::Normal {
                Some("0.00%".into())
            } else {
                None
            };
            if multi || pivot.cols.is_empty() {
                cells[header_rows - 1][c] = Out::Text(titles[vi].clone());
            }
        }
    }
    let last = header_rows - 1;
    if !titles.is_empty() && (pivot.cols.is_empty() || !multi) {
        // Um só valor: o título fica no canto quando há colunas, e nos próprios cabeçalhos quando não há.
        if !pivot.cols.is_empty() {
            cells[0][0] = Out::Text(titles[0].clone());
        }
    }
    for (l, field) in pivot.rows.iter().enumerate() {
        cells[last][l] = Out::Text(field.name.clone());
    }
    if pivot.rows.is_empty() {
        cells[last][0] = Out::Empty;
    }

    let mut row_roles = vec![Role::Header; header_rows];
    let mut previous: Option<&Item> = None;
    for item in &row_items {
        let mut line = vec![Out::Empty; width];
        for l in 0..pivot.rows.len() {
            let text = match item.kind {
                Kind::Leaf if item.key.len() > l => {
                    let same = previous.is_some_and(|p| p.kind == Kind::Leaf && p.key[..=l] == item.key[..=l]);
                    let first_after_sub = previous.is_some_and(|p| p.kind != Kind::Leaf);
                    (!same || first_after_sub || l + 1 == pivot.rows.len() || pivot.repeat_labels)
                        .then(|| item.key[l].clone())
                }
                Kind::Sub if l + 1 == item.key.len() => Some(format!("{} Total", item.key[l])),
                Kind::Grand if l == 0 => Some(GRAND_LABEL.to_string()),
                _ => None,
            };
            if let Some(text) = text {
                line[l] = Out::Text(text);
            }
        }
        if pivot.rows.is_empty() {
            line[0] = Out::Text("Total".into());
        }
        previous = Some(item);
        let rk = join(&item.key);
        for (n, col) in col_items.iter().enumerate() {
            let ck = join(&col.key);
            for vi in 0..value_count {
                if let Some(v) = shown(&rk, &ck, vi) {
                    line[label_cols + n * value_count + vi] = Out::Num(v);
                }
            }
        }
        row_roles.push(match item.kind {
            Kind::Leaf => Role::Body,
            Kind::Sub => Role::Subtotal,
            Kind::Grand => Role::Grand,
        });
        cells.push(line);
    }
    // Os filtros aparecem acima da tabela, como no Excel.
    let mut page_rows = 0;
    if !pivot.filters.is_empty() {
        let mut page: Vec<Vec<Out>> = Vec::new();
        for filter in &pivot.filters {
            let field = filter_field(pivot, filter);
            let all = distinct_items(&field, source)?;
            let hidden: HashSet<&String> = filter.hidden.iter().collect();
            let visible: Vec<&String> = all.iter().filter(|item| !hidden.contains(item)).collect();
            let summary = match visible.len() {
                0 => "(Nenhum)".to_string(),
                1 => visible[0].clone(),
                n if n == all.len() => "(Tudo)".to_string(),
                n => format!("{n} itens"),
            };
            let mut line = vec![Out::Empty; width.max(2)];
            line[0] = Out::Text(filter.field.clone());
            line[1] = Out::Text(summary);
            page.push(line);
        }
        page.push(vec![Out::Empty; width.max(2)]);
        page_rows = page.len();
        for line in &mut cells {
            line.resize(width.max(2), Out::Empty);
        }
        let mut roles = vec![Role::Body; page_rows];
        roles.extend(row_roles);
        row_roles = roles;
        page.extend(cells);
        cells = page;
        col_roles.resize(width.max(2), Role::Body);
        col_formats.resize(width.max(2), None);
    }
    Ok(Grid { cells, row_roles, col_roles, col_formats, header_rows, label_cols, page_rows })
}

/// O campo de um filtro, com o agrupamento que ele tem nas linhas ou colunas.
fn filter_field(pivot: &Pivot, filter: &Filter) -> Field {
    let used = pivot.rows.iter().chain(&pivot.cols).find(|f| f.name == filter.field);
    Field {
        name: filter.field.clone(),
        group: used.map_or(Group::None, |f| f.group),
        step: used.map_or(10.0, |f| f.step),
        sort: SortKind::LabelAsc,
        sort_value: 0,
        top: None,
    }
}

/// Os itens distintos de um campo, na ordem natural (para a lista de filtro).
pub fn distinct_items(field: &Field, source: &Source) -> Result<Vec<String>, String> {
    let i = source
        .headers
        .iter()
        .position(|h| h.trim() == field.name.trim())
        .ok_or_else(|| format!("o campo “{}” não existe nos dados de origem", field.name))?;
    let mut seen: HashMap<String, Label> = HashMap::new();
    for record in &source.rows {
        let label = label_of(field, &record.get(i).cloned().unwrap_or_default());
        seen.entry(label.text.clone()).or_insert(label);
    }
    let mut all: Vec<Label> = seen.into_values().collect();
    all.sort_by(compare_labels);
    Ok(all.into_iter().map(|l| l.text).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn num(x: f64) -> SrcCell {
        SrcCell { text: plain_number(x), num: Some(x) }
    }

    fn text(s: &str) -> SrcCell {
        SrcCell { text: s.into(), num: None }
    }

    /// Região, produto, vendas, quantidade.
    fn data() -> Source {
        let rows = [
            ("Sul", "A", 10.0, 1.0),
            ("Sul", "B", 20.0, 2.0),
            ("Norte", "A", 30.0, 3.0),
            ("Norte", "A", 40.0, 4.0),
            ("Norte", "B", 50.0, 5.0),
        ];
        Source {
            headers: ["Região", "Produto", "Vendas", "Qtd"].map(String::from).to_vec(),
            rows: rows.iter().map(|(r, p, v, q)| vec![text(r), text(p), num(*v), num(*q)]).collect(),
        }
    }

    fn field(name: &str) -> Field {
        Field { name: name.into(), group: Group::None, step: 10.0, sort: SortKind::LabelAsc, sort_value: 0, top: None }
    }

    fn value(name: &str, agg: Agg) -> ValueField {
        ValueField {
            field: name.into(),
            agg,
            show_as: ShowAs::Normal,
            title: String::new(),
            format: String::new(),
            formula: String::new(),
        }
    }

    fn pivot() -> Pivot {
        Pivot {
            id: 0,
            source_sheet: 0,
            source: "A1:D6".into(),
            sheet: 0,
            col: 0,
            row: 0,
            rows: vec![field("Região")],
            cols: vec![],
            values: vec![value("Vendas", Agg::Sum)],
            filters: vec![],
            grand_rows: true,
            grand_cols: true,
            subtotals: true,
            repeat_labels: false,
            width: 0,
            height: 0,
        }
    }

    fn t(s: &str) -> Out {
        Out::Text(s.into())
    }

    #[test]
    fn sums_by_one_field_with_grand_total() {
        let grid = compute(&pivot(), &data()).unwrap();
        assert_eq!(grid.cells[0], vec![t("Região"), t("Soma de Vendas")]);
        assert_eq!(grid.cells[1], vec![t("Norte"), Out::Num(120.0)]);
        assert_eq!(grid.cells[2], vec![t("Sul"), Out::Num(30.0)]);
        assert_eq!(grid.cells[3], vec![t("Total Geral"), Out::Num(150.0)]);
        assert_eq!(grid.row_roles, vec![Role::Header, Role::Body, Role::Body, Role::Grand]);
    }

    #[test]
    fn crosses_rows_and_columns() {
        let mut p = pivot();
        p.cols = vec![field("Produto")];
        let grid = compute(&p, &data()).unwrap();
        assert_eq!(grid.cells[0], vec![t("Soma de Vendas"), t("A"), t("B"), t("Total Geral")]);
        assert_eq!(grid.cells[1], vec![t("Região"), Out::Empty, Out::Empty, Out::Empty]);
        assert_eq!(grid.cells[2], vec![t("Norte"), Out::Num(70.0), Out::Num(50.0), Out::Num(120.0)]);
        assert_eq!(grid.cells[3], vec![t("Sul"), Out::Num(10.0), Out::Num(20.0), Out::Num(30.0)]);
        assert_eq!(grid.cells[4], vec![t("Total Geral"), Out::Num(80.0), Out::Num(70.0), Out::Num(150.0)]);
    }

    #[test]
    fn nested_rows_get_subtotals_and_can_drop_them() {
        let mut p = pivot();
        p.rows = vec![field("Região"), field("Produto")];
        let grid = compute(&p, &data()).unwrap();
        let labels: Vec<_> = grid.cells.iter().map(|r| (r[0].clone(), r[1].clone())).collect();
        assert_eq!(labels[1], (t("Norte"), t("A")));
        assert_eq!(labels[2], (Out::Empty, t("B")));
        assert_eq!(labels[3], (t("Norte Total"), Out::Empty));
        assert_eq!(grid.cells[3][2], Out::Num(120.0));
        assert_eq!(grid.row_roles[3], Role::Subtotal);
        assert_eq!(grid.cells.last().unwrap()[0], t("Total Geral"));
        p.subtotals = false;
        let plain = compute(&p, &data()).unwrap();
        assert_eq!(plain.cells.len(), 1 + 4 + 1);
    }

    #[test]
    fn several_values_and_aggregations() {
        let mut p = pivot();
        p.values = vec![
            value("Vendas", Agg::Average),
            value("Vendas", Agg::Count),
            value("Vendas", Agg::Max),
            value("Produto", Agg::Distinct),
        ];
        let grid = compute(&p, &data()).unwrap();
        assert_eq!(
            grid.cells[0][1..],
            [t("Média de Vendas"), t("Contagem de Vendas"), t("Máximo de Vendas"), t("Valores distintos de Produto")]
        );
        assert_eq!(grid.cells[1][1..], [Out::Num(40.0), Out::Num(3.0), Out::Num(50.0), Out::Num(2.0)]);
    }

    #[test]
    fn calculated_fields_use_sums_of_other_fields() {
        let mut p = pivot();
        let mut calc = value("", Agg::Calc);
        calc.formula = "[Vendas] / [Qtd]".into();
        calc.title = "Preço médio".into();
        p.values = vec![calc];
        let grid = compute(&p, &data()).unwrap();
        assert_eq!(grid.cells[1][1], Out::Num(120.0 / 12.0));
        assert!(
            compute(
                &Pivot {
                    values: vec![{
                        let mut v = value("", Agg::Calc);
                        v.formula = "[Nada]".into();
                        v
                    }],
                    ..pivot()
                },
                &data()
            )
            .is_err()
        );
        assert!(parse_formula("1 +").is_err());
    }

    #[test]
    fn shows_values_as_percentages() {
        let mut p = pivot();
        p.cols = vec![field("Produto")];
        p.values[0].show_as = ShowAs::PctRow;
        let grid = compute(&p, &data()).unwrap();
        assert_eq!(grid.cells[3], vec![t("Sul"), Out::Num(10.0 / 30.0), Out::Num(20.0 / 30.0), Out::Num(1.0)]);
        assert_eq!(grid.col_formats[1].as_deref(), Some("0.00%"));
        p.values[0].show_as = ShowAs::PctTotal;
        let total = compute(&p, &data()).unwrap();
        assert_eq!(total.cells.last().unwrap().last(), Some(&Out::Num(1.0)));
    }

    #[test]
    fn filters_sorts_and_limits() {
        let mut p = pivot();
        p.rows = vec![field("Produto")];
        p.filters = vec![Filter { field: "Região".into(), hidden: vec!["Sul".into()] }];
        let grid = compute(&p, &data()).unwrap();
        // O filtro aparece acima da tabela, com uma linha em branco depois.
        assert_eq!(grid.page_rows, 2);
        assert_eq!(grid.cells[0], vec![t("Região"), t("Norte")]);
        assert_eq!(grid.cells[3], vec![t("A"), Out::Num(70.0)]);
        assert_eq!(grid.cells[4], vec![t("B"), Out::Num(50.0)]);
        p.filters.clear();
        p.rows[0].sort = SortKind::ValueDesc;
        let by_value = compute(&p, &data()).unwrap();
        assert_eq!(by_value.cells[1][0], t("A"));
        p.rows[0].sort = SortKind::ValueAsc;
        p.rows[0].top = Some(1);
        let top = compute(&p, &data()).unwrap();
        assert_eq!(top.cells.len(), 1 + 1 + 1);
        assert_eq!(top.cells[1][0], t("B"));
    }

    #[test]
    fn groups_dates_and_numbers() {
        // 45292 é 1/1/2024; 45383 é 1/4/2024; 45658 é 1/1/2025.
        let rows = [(45292.0, 5.0), (45383.0, 15.0), (45658.0, 25.0), (45300.0, 35.0)];
        let source = Source {
            headers: vec!["Data".into(), "Valor".into()],
            rows: rows.iter().map(|(d, v)| vec![num(*d), num(*v)]).collect(),
        };
        let mut p = pivot();
        p.rows = vec![Field { group: Group::Year, ..field("Data") }];
        p.values = vec![value("Valor", Agg::Sum)];
        let years = compute(&p, &source).unwrap();
        assert_eq!(years.cells[1], vec![t("2024"), Out::Num(55.0)]);
        assert_eq!(years.cells[2], vec![t("2025"), Out::Num(25.0)]);
        p.rows = vec![Field { group: Group::Quarter, ..field("Data") }];
        let quarters = compute(&p, &source).unwrap();
        assert_eq!(quarters.cells[1][0], t("T1"));
        assert_eq!(quarters.cells[1][1], Out::Num(65.0));
        p.rows = vec![Field { group: Group::Bucket, step: 20.0, ..field("Valor") }];
        let buckets = compute(&p, &source).unwrap();
        assert_eq!(buckets.cells[1][0], t("0–20"));
        assert_eq!(date_of(45292.0), (2024, 1, 1));
        assert_eq!(date_of(45658.0), (2025, 1, 1));
    }

    #[test]
    fn reports_missing_fields_and_empty_values() {
        let mut p = pivot();
        p.rows = vec![field("Nada")];
        assert!(compute(&p, &data()).is_err());
        p.rows.clear();
        p.values.clear();
        let hint = compute(&p, &data()).unwrap();
        assert_eq!(hint.cells.len(), 1);
        // Só campos, sem valores: a lista de itens.
        p.rows = vec![field("Região")];
        let list = compute(&p, &data()).unwrap();
        assert_eq!(list.cells[1], vec![t("Norte")]);
        assert_eq!(list.cells.last().unwrap(), &vec![t("Total Geral")]);
        p.cols = vec![field("Produto")];
        assert_eq!(compute(&p, &data()).unwrap().cells, list.cells);
        let blanks = Source {
            headers: vec!["A".into(), "B".into()],
            rows: vec![vec![text(""), num(1.0)], vec![text("x"), num(2.0)]],
        };
        let mut q = pivot();
        q.rows = vec![field("A")];
        q.values = vec![value("B", Agg::Sum)];
        let grid = compute(&q, &blanks).unwrap();
        assert_eq!(grid.cells[1][0], t("x"));
        assert_eq!(grid.cells[2][0], t("(vazio)"));
        assert_eq!(distinct_items(&field("A"), &blanks).unwrap(), vec!["x", "(vazio)"]);
    }

    #[test]
    fn no_row_or_column_fields_gives_one_total() {
        let mut p = pivot();
        p.rows.clear();
        let grid = compute(&p, &data()).unwrap();
        assert_eq!(grid.cells.len(), 2);
        assert_eq!(grid.cells[1], vec![t("Total"), Out::Num(150.0)]);
    }
}
