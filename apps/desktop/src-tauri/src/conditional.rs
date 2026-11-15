//! Formatação condicional. O IronCalc avalia e grava as regras; aqui elas são
//! traduzidas para um formato simples, que a interface edita, e de volta.

use ironcalc::base::cf_types::{CfRule, CfRuleInput, Cfvo, ColorScaleThreshold, TextOperator, ValueOperator};
use ironcalc::base::types::{Color, Dxf, DxfFont, Fill};
use serde::{Deserialize, Serialize};

use crate::engine::Engine;

/// A cor de uma barra de dados com valor negativo.
const NEGATIVE_BAR: &str = "#FF6B6B";
/// Maior posição aceita em "10 maiores".
const MAX_RANK: u32 = 1000;

/// O que a regra testa.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CondKind {
    Greater,
    GreaterEqual,
    Less,
    LessEqual,
    Equal,
    NotEqual,
    Between,
    NotBetween,
    Contains,
    NotContains,
    BeginsWith,
    EndsWith,
    Duplicates,
    Unique,
    Blanks,
    NotBlanks,
    Errors,
    AboveAverage,
    BelowAverage,
    Top,
    Bottom,
    ColorScale,
    DataBar,
    /// Regra que o Abacus não edita (fórmula, período, conjunto de ícones…);
    /// só dá para apagar.
    Other,
}

/// Uma regra como a interface a vê.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CondRule {
    /// Posição da regra na planilha; vazia numa regra nova.
    #[serde(default)]
    pub index: Option<u32>,
    /// Células da regra, como `A1:B5`.
    pub range: String,
    pub kind: CondKind,
    /// Valor, texto ou posição (10 maiores), conforme o tipo.
    #[serde(default)]
    pub value: String,
    /// Segundo valor, nas regras "entre".
    #[serde(default)]
    pub value2: String,
    /// Cor de fundo; nas barras de dados, a cor da barra; nas escalas, a do valor mais alto.
    #[serde(default)]
    pub fill: Option<String>,
    /// Cor do texto; nas escalas de cor, a do valor mais baixo.
    #[serde(default)]
    pub color: Option<String>,
    /// Só nas escalas de três cores: a do meio.
    #[serde(default)]
    pub mid: Option<String>,
    #[serde(default)]
    pub bold: bool,
}

fn hex(color: &Option<String>) -> Result<Option<String>, String> {
    let Some(text) = color.as_deref().filter(|text| !text.is_empty()) else { return Ok(None) };
    let digits = text.strip_prefix('#').unwrap_or(text);
    if digits.len() != 6 || !digits.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("cor inválida: {text}"));
    }
    Ok(Some(format!("#{}", digits.to_ascii_uppercase())))
}

fn rgb(text: String) -> Color {
    Color::Rgb(text)
}

/// O formato aplicado pelas regras que pintam a célula.
fn format_of(rule: &CondRule) -> Result<Dxf, String> {
    let fill = hex(&rule.fill)?;
    let color = hex(&rule.color)?;
    if fill.is_none() && color.is_none() && !rule.bold {
        return Err("escolha pelo menos uma cor ou o negrito".into());
    }
    Ok(Dxf {
        font: (rule.bold || color.is_some()).then(|| DxfFont {
            b: rule.bold.then_some(true),
            color: color.map(rgb).unwrap_or_default(),
            ..DxfFont::default()
        }),
        fill: fill.map(|color| Fill { color: rgb(color) }),
        ..Dxf::default()
    })
}

/// A regra no formato do motor.
fn to_input(rule: &CondRule) -> Result<CfRuleInput, String> {
    let value = rule.value.trim().to_string();
    let need_value = || if value.is_empty() { Err("escreva o valor da regra".to_string()) } else { Ok(value.clone()) };
    let cell_is = |operator: ValueOperator| -> Result<CfRuleInput, String> {
        let between = matches!(operator, ValueOperator::Between | ValueOperator::NotBetween);
        let second = rule.value2.trim().to_string();
        if between && second.is_empty() {
            return Err("escreva os dois valores".into());
        }
        Ok(CfRuleInput::CellIs {
            operator,
            formula: need_value()?,
            formula2: between.then_some(second),
            format: format_of(rule)?,
            stop_if_true: false,
        })
    };
    let text = |operator: TextOperator| -> Result<CfRuleInput, String> {
        Ok(CfRuleInput::Text { operator, value: need_value()?, format: format_of(rule)?, stop_if_true: false })
    };
    let rank = || -> Result<u32, String> {
        value
            .parse::<u32>()
            .ok()
            .filter(|rank| (1..=MAX_RANK).contains(rank))
            .ok_or_else(|| format!("escreva uma posição de 1 a {MAX_RANK}"))
    };
    Ok(match rule.kind {
        CondKind::Greater => cell_is(ValueOperator::GreaterThan)?,
        CondKind::GreaterEqual => cell_is(ValueOperator::GreaterThanOrEqual)?,
        CondKind::Less => cell_is(ValueOperator::LessThan)?,
        CondKind::LessEqual => cell_is(ValueOperator::LessThanOrEqual)?,
        CondKind::Equal => cell_is(ValueOperator::Equal)?,
        CondKind::NotEqual => cell_is(ValueOperator::NotEqual)?,
        CondKind::Between => cell_is(ValueOperator::Between)?,
        CondKind::NotBetween => cell_is(ValueOperator::NotBetween)?,
        CondKind::Contains => text(TextOperator::Contains)?,
        CondKind::NotContains => text(TextOperator::DoesNotContain)?,
        CondKind::BeginsWith => text(TextOperator::BeginsWith)?,
        CondKind::EndsWith => text(TextOperator::EndsWith)?,
        CondKind::Duplicates => CfRuleInput::DuplicateValues { format: format_of(rule)?, stop_if_true: false },
        CondKind::Unique => CfRuleInput::UniqueValues { format: format_of(rule)?, stop_if_true: false },
        CondKind::Blanks => CfRuleInput::Blanks { format: format_of(rule)?, stop_if_true: false },
        CondKind::NotBlanks => CfRuleInput::NotBlanks { format: format_of(rule)?, stop_if_true: false },
        CondKind::Errors => CfRuleInput::Errors { format: format_of(rule)?, stop_if_true: false },
        CondKind::AboveAverage => CfRuleInput::AboveAverage { format: format_of(rule)?, stop_if_true: false },
        CondKind::BelowAverage => CfRuleInput::BelowAverage { format: format_of(rule)?, stop_if_true: false },
        CondKind::Top => {
            CfRuleInput::Top10 { rank: rank()?, percent: false, format: format_of(rule)?, stop_if_true: false }
        }
        CondKind::Bottom => {
            CfRuleInput::Bottom10 { rank: rank()?, percent: false, format: format_of(rule)?, stop_if_true: false }
        }
        CondKind::ColorScale => {
            let low = hex(&rule.color)?.ok_or("escolha a cor do menor valor")?;
            let high = hex(&rule.fill)?.ok_or("escolha a cor do maior valor")?;
            let mut thresholds = vec![ColorScaleThreshold { cfvo: Cfvo::Min, color: rgb(low) }];
            if let Some(mid) = hex(&rule.mid)? {
                thresholds.push(ColorScaleThreshold { cfvo: Cfvo::Percentile(50.0), color: rgb(mid) });
            }
            thresholds.push(ColorScaleThreshold { cfvo: Cfvo::Max, color: rgb(high) });
            CfRuleInput::ColorScale { thresholds }
        }
        CondKind::DataBar => CfRuleInput::DataBar {
            min: None,
            max: None,
            positive_color: rgb(hex(&rule.fill)?.ok_or("escolha a cor da barra")?),
            negative_color: rgb(NEGATIVE_BAR.to_string()),
            is_gradient: true,
            show_value: true,
        },
        CondKind::Other => return Err("o Abacus não edita esse tipo de regra; apague-a e crie outra".into()),
    })
}

fn operator_kind(operator: &ValueOperator) -> CondKind {
    match operator {
        ValueOperator::Equal => CondKind::Equal,
        ValueOperator::GreaterThan => CondKind::Greater,
        ValueOperator::GreaterThanOrEqual => CondKind::GreaterEqual,
        ValueOperator::LessThan => CondKind::Less,
        ValueOperator::LessThanOrEqual => CondKind::LessEqual,
        ValueOperator::NotEqual => CondKind::NotEqual,
        ValueOperator::Between => CondKind::Between,
        ValueOperator::NotBetween => CondKind::NotBetween,
    }
}

impl Engine {
    fn color_text(&self, color: &Color) -> Option<String> {
        Some(self.model.resolve_color(color)).filter(|hex| !hex.is_empty())
    }

    /// As regras de uma planilha, da de maior prioridade para a de menor.
    pub fn cond_rules(&self, sheet: u32) -> Result<Vec<CondRule>, String> {
        let mut rules = Vec::new();
        for view in self.model.get_conditional_formatting_list(sheet)? {
            let dxf = self.model.get_dxf_for_conditional_formatting(sheet, view.index as u32)?;
            let fill = dxf.as_ref().and_then(|dxf| dxf.fill.as_ref()).and_then(|fill| self.color_text(&fill.color));
            let font = dxf.as_ref().and_then(|dxf| dxf.font.as_ref());
            let color = font.and_then(|font| self.color_text(&font.color));
            let bold = font.and_then(|font| font.b).unwrap_or(false);
            let mut rule = CondRule {
                index: Some(view.index as u32),
                range: view.range.clone(),
                kind: CondKind::Other,
                value: String::new(),
                value2: String::new(),
                fill,
                color,
                mid: None,
                bold,
            };
            match &view.cf_rule {
                CfRule::CellIs { operator, formula, formula2, .. } => {
                    rule.kind = operator_kind(operator);
                    rule.value = formula.clone();
                    rule.value2 = formula2.clone().unwrap_or_default();
                }
                CfRule::Text { operator, value, .. } => {
                    rule.kind = match operator {
                        TextOperator::Contains => CondKind::Contains,
                        TextOperator::DoesNotContain => CondKind::NotContains,
                        TextOperator::BeginsWith => CondKind::BeginsWith,
                        TextOperator::EndsWith => CondKind::EndsWith,
                        // "É igual a" um texto é a regra de valor "Igual a".
                        TextOperator::Equals => CondKind::Other,
                    };
                    rule.value = value.clone();
                }
                CfRule::DuplicateValues { .. } => rule.kind = CondKind::Duplicates,
                CfRule::UniqueValues { .. } => rule.kind = CondKind::Unique,
                CfRule::Blanks { .. } => rule.kind = CondKind::Blanks,
                CfRule::NotBlanks { .. } => rule.kind = CondKind::NotBlanks,
                CfRule::Errors { .. } => rule.kind = CondKind::Errors,
                CfRule::AboveAverage { .. } => rule.kind = CondKind::AboveAverage,
                CfRule::BelowAverage { .. } => rule.kind = CondKind::BelowAverage,
                CfRule::Top10 { rank, percent: false, .. } => {
                    rule.kind = CondKind::Top;
                    rule.value = rank.to_string();
                }
                CfRule::Bottom10 { rank, percent: false, .. } => {
                    rule.kind = CondKind::Bottom;
                    rule.value = rank.to_string();
                }
                CfRule::ColorScale { thresholds } if (2..=3).contains(&thresholds.len()) => {
                    rule.kind = CondKind::ColorScale;
                    rule.color = self.color_text(&thresholds[0].color);
                    rule.fill = self.color_text(&thresholds[thresholds.len() - 1].color);
                    rule.mid = (thresholds.len() == 3).then(|| self.color_text(&thresholds[1].color)).flatten();
                }
                CfRule::DataBar { positive_color, .. } => {
                    rule.kind = CondKind::DataBar;
                    rule.fill = self.color_text(positive_color);
                }
                _ => {}
            }
            rules.push(rule);
        }
        Ok(rules)
    }

    /// Cria uma regra no fim da lista (a de maior prioridade).
    pub fn add_cond_rule(&mut self, sheet: u32, rule: CondRule) -> Result<(), String> {
        let input = to_input(&rule)?;
        self.model.add_conditional_formatting(sheet, rule.range.trim(), input)
    }

    /// Troca o intervalo e a regra de uma já existente.
    pub fn update_cond_rule(&mut self, sheet: u32, rule: CondRule) -> Result<(), String> {
        let index = rule.index.ok_or("a regra não existe mais")?;
        let input = to_input(&rule)?;
        self.model.update_conditional_formatting(sheet, index, rule.range.trim(), input)
    }

    pub fn delete_cond_rule(&mut self, sheet: u32, index: u32) -> Result<(), String> {
        self.model.delete_conditional_formatting(sheet, index)
    }
}
