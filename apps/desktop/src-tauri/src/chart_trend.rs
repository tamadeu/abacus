//! Linhas de tendência dos gráficos: ajuste por mínimos quadrados e média móvel.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrendKind {
    Linear,
    /// Polinomial; o grau está em `Trend::order`.
    Poly,
    Exp,
    Log,
    /// Média móvel; o número de pontos está em `Trend::period`.
    Moving,
}

/// Uma linha de tendência de uma série.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Trend {
    pub kind: TrendKind,
    /// Grau do polinômio (2 a 6).
    #[serde(default = "default_order")]
    pub order: u32,
    /// Pontos da média móvel (2 ou mais).
    #[serde(default = "default_period")]
    pub period: u32,
    /// Mostrar a equação no gráfico.
    #[serde(default)]
    pub equation: bool,
    /// Mostrar o R² no gráfico.
    #[serde(default)]
    pub r2: bool,
}

fn default_order() -> u32 {
    2
}

fn default_period() -> u32 {
    2
}

impl Trend {
    /// Ajusta a tendência a valores válidos.
    pub fn sanitized(&self) -> Trend {
        let moving = self.kind == TrendKind::Moving;
        Trend {
            kind: self.kind,
            order: if self.kind == TrendKind::Poly { self.order.clamp(2, 6) } else { default_order() },
            period: if moving { self.period.clamp(2, 100) } else { default_period() },
            // A média móvel não tem equação nem R².
            equation: self.equation && !moving,
            r2: self.r2 && !moving,
        }
    }
}

/// A curva calculada: pontos para desenhar e o texto da equação.
#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct TrendLine {
    pub points: Vec<[f64; 2]>,
    /// Equação e/ou R², como o Excel mostra; vazio se nada foi pedido.
    pub label: String,
}

/// Resolve o sistema `a · x = b` por eliminação de Gauss com pivô parcial.
fn solve(mut a: Vec<Vec<f64>>, mut b: Vec<f64>) -> Option<Vec<f64>> {
    let n = b.len();
    for col in 0..n {
        let pivot = (col..n).max_by(|&i, &j| a[i][col].abs().total_cmp(&a[j][col].abs()))?;
        if a[pivot][col].abs() < 1e-12 {
            return None;
        }
        a.swap(col, pivot);
        b.swap(col, pivot);
        for row in col + 1..n {
            let factor = a[row][col] / a[col][col];
            let pivot_row = a[col].clone();
            for (cell, p) in a[row][col..].iter_mut().zip(&pivot_row[col..]) {
                *cell -= factor * p;
            }
            b[row] -= factor * b[col];
        }
    }
    let mut x = vec![0.0; n];
    for row in (0..n).rev() {
        let rest: f64 = (row + 1..n).map(|k| a[row][k] * x[k]).sum();
        x[row] = (b[row] - rest) / a[row][row];
    }
    Some(x)
}

/// Coeficientes (do termo constante para cima) do polinômio de `degree` que melhor se ajusta.
fn polyfit(xs: &[f64], ys: &[f64], degree: usize) -> Option<Vec<f64>> {
    if xs.len() <= degree {
        return None;
    }
    let size = degree + 1;
    let mut a = vec![vec![0.0; size]; size];
    let mut b = vec![0.0; size];
    for (x, y) in xs.iter().zip(ys) {
        let powers: Vec<f64> = (0..=2 * degree)
            .scan(1.0, |p, _| {
                let current = *p;
                *p *= x;
                Some(current)
            })
            .collect();
        for i in 0..size {
            for j in 0..size {
                a[i][j] += powers[i + j];
            }
            b[i] += powers[i] * y;
        }
    }
    solve(a, b)
}

fn evaluate(coefficients: &[f64], x: f64) -> f64 {
    coefficients.iter().rev().fold(0.0, |acc, c| acc * x + c)
}

/// Número com até 4 algarismos significativos, vírgula decimal.
fn number(n: f64) -> String {
    if n == 0.0 {
        return "0".into();
    }
    let digits = (3 - n.abs().log10().floor() as i32).clamp(0, 10) as usize;
    let text = format!("{n:.digits$}");
    let text = if text.contains('.') { text.trim_end_matches('0').trim_end_matches('.').to_string() } else { text };
    text.replace('.', ",")
}

fn term(coefficient: f64, power: usize, first: bool) -> String {
    let sign = if coefficient < 0.0 {
        "- "
    } else if first {
        ""
    } else {
        "+ "
    };
    let magnitude = number(coefficient.abs());
    let variable = match power {
        0 => String::new(),
        1 => "x".into(),
        n => format!("x{}", ["", "", "²", "³", "⁴", "⁵", "⁶"][n.min(6)]),
    };
    let body = if power > 0 && magnitude == "1" { variable } else { format!("{magnitude}{variable}") };
    format!("{sign}{body}")
}

fn r_squared(ys: &[f64], predicted: &[f64]) -> f64 {
    let mean = ys.iter().sum::<f64>() / ys.len() as f64;
    let total: f64 = ys.iter().map(|y| (y - mean).powi(2)).sum();
    let residual: f64 = ys.iter().zip(predicted).map(|(y, p)| (y - p).powi(2)).sum();
    if total == 0.0 { 1.0 } else { 1.0 - residual / total }
}

/// Coeficientes, função de previsão e texto da equação de um ajuste.
type Fit = (Vec<f64>, Box<dyn Fn(f64) -> f64>, String);

/// A tendência de uma série. `points` são pares (x, y) já sem valores vazios.
pub fn compute(trend: &Trend, points: &[(f64, f64)]) -> TrendLine {
    let trend = trend.sanitized();
    if points.len() < 2 {
        return TrendLine::default();
    }
    if trend.kind == TrendKind::Moving {
        let period = trend.period as usize;
        let line: Vec<[f64; 2]> = (period.saturating_sub(1)..points.len())
            .filter(|_| points.len() >= period)
            .map(|end| {
                let window = &points[end + 1 - period..=end];
                [points[end].0, window.iter().map(|p| p.1).sum::<f64>() / period as f64]
            })
            .collect();
        return TrendLine { points: line, label: String::new() };
    }

    let xs: Vec<f64> = points.iter().map(|p| p.0).collect();
    let ys: Vec<f64> = points.iter().map(|p| p.1).collect();
    let (min, max) =
        (xs.iter().cloned().fold(f64::INFINITY, f64::min), xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max));

    // Cada tipo vira um polinômio em dados transformados (ln y ou ln x).
    let (coefficients, predict, equation): Fit = match trend.kind {
        TrendKind::Linear | TrendKind::Poly => {
            let degree = if trend.kind == TrendKind::Linear { 1 } else { trend.order as usize };
            let Some(c) = polyfit(&xs, &ys, degree) else { return TrendLine::default() };
            let text: Vec<String> = (0..=degree).rev().enumerate().map(|(k, p)| term(c[p], p, k == 0)).collect();
            let c2 = c.clone();
            (c, Box::new(move |x| evaluate(&c2, x)), format!("y = {}", text.join(" ")))
        }
        TrendKind::Exp => {
            if ys.iter().any(|y| *y <= 0.0) {
                return TrendLine::default();
            }
            let logs: Vec<f64> = ys.iter().map(|y| y.ln()).collect();
            let Some(c) = polyfit(&xs, &logs, 1) else { return TrendLine::default() };
            let (a, b) = (c[0].exp(), c[1]);
            (c, Box::new(move |x| a * (b * x).exp()), format!("y = {}e^{}x", number(a), number(b)))
        }
        TrendKind::Log => {
            if xs.iter().any(|x| *x <= 0.0) {
                return TrendLine::default();
            }
            let logs: Vec<f64> = xs.iter().map(|x| x.ln()).collect();
            let Some(c) = polyfit(&logs, &ys, 1) else { return TrendLine::default() };
            let (a, b) = (c[1], c[0]);
            let sign = if b < 0.0 { "-" } else { "+" };
            (c, Box::new(move |x| a * x.ln() + b), format!("y = {}ln(x) {sign} {}", number(a), number(b.abs())))
        }
        TrendKind::Moving => unreachable!(),
    };
    let _ = coefficients;

    // R² como o Excel: nos dados transformados para exponencial e logarítmica.
    let r2 = match trend.kind {
        TrendKind::Exp => {
            let logs: Vec<f64> = ys.iter().map(|y| y.ln()).collect();
            let predicted: Vec<f64> = xs.iter().map(|x| predict(*x).ln()).collect();
            r_squared(&logs, &predicted)
        }
        _ => r_squared(&ys, &xs.iter().map(|x| predict(*x)).collect::<Vec<_>>()),
    };

    let samples = if trend.kind == TrendKind::Linear { 2 } else { 48 };
    let line = (0..samples)
        .map(|k| {
            let x = min + (max - min) * k as f64 / (samples - 1) as f64;
            [x, predict(x)]
        })
        .collect();
    let mut parts = Vec::new();
    if trend.equation {
        parts.push(equation);
    }
    if trend.r2 {
        parts.push(format!("R² = {}", number(r2)));
    }
    TrendLine { points: line, label: parts.join("\n") }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trend(kind: TrendKind) -> Trend {
        Trend { kind, order: 2, period: 2, equation: true, r2: true }
    }

    #[test]
    fn linear_fit_matches_a_known_line() {
        let points: Vec<(f64, f64)> = (1..=5).map(|x| (x as f64, 2.0 * x as f64 + 1.0)).collect();
        let line = compute(&trend(TrendKind::Linear), &points);
        assert_eq!(line.label, "y = 2x + 1\nR² = 1");
        assert_eq!(line.points.len(), 2);
        assert!((line.points[0][1] - 3.0).abs() < 1e-9 && (line.points[1][1] - 11.0).abs() < 1e-9);
    }

    #[test]
    fn polynomial_exponential_and_log_fits() {
        let parabola: Vec<(f64, f64)> = (1..=6).map(|x| (x as f64, (x * x) as f64 - 3.0 * x as f64 + 2.0)).collect();
        let line = compute(&trend(TrendKind::Poly), &parabola);
        assert_eq!(line.label, "y = x² - 3x + 2\nR² = 1");

        let growth: Vec<(f64, f64)> = (1..=5).map(|x| (x as f64, 3.0 * (0.5 * x as f64).exp())).collect();
        assert_eq!(compute(&trend(TrendKind::Exp), &growth).label, "y = 3e^0,5x\nR² = 1");

        let logs: Vec<(f64, f64)> = (1..=5).map(|x| (x as f64, 2.0 * (x as f64).ln() + 1.0)).collect();
        assert_eq!(compute(&trend(TrendKind::Log), &logs).label, "y = 2ln(x) + 1\nR² = 1");
    }

    #[test]
    fn moving_average_and_degenerate_inputs() {
        let points = vec![(1.0, 2.0), (2.0, 4.0), (3.0, 6.0), (4.0, 8.0)];
        let line = compute(&Trend { period: 2, ..trend(TrendKind::Moving) }, &points);
        assert_eq!(line.points, vec![[2.0, 3.0], [3.0, 5.0], [4.0, 7.0]]);
        // Poucos pontos, ou valores que o modelo não aceita, não dão curva.
        assert!(compute(&trend(TrendKind::Linear), &points[..1]).points.is_empty());
        assert!(compute(&trend(TrendKind::Exp), &[(1.0, -1.0), (2.0, 3.0)]).points.is_empty());
        assert!(compute(&trend(TrendKind::Log), &[(0.0, 1.0), (2.0, 3.0)]).points.is_empty());
        assert!(compute(&trend(TrendKind::Linear), &[(1.0, 1.0), (1.0, 2.0)]).points.is_empty());
    }
}
