//! Leitura e escrita de números no padrão brasileiro (vírgula decimal).
//!
//! Por enquanto a localidade é fixa em pt-BR. Quando houver suporte a outras
//! localidades, estas funções passam a receber a configuração regional.

/// Interpreta o que o usuário digitou como número, se possível.
///
/// Aceita `1234`, `1234,5`, `1.234,56`, `R$ 1.234,56` e, por conveniência,
/// também `1234.5` (ponto decimal sem agrupamento de milhar).
pub fn parse_number(input: &str) -> Option<f64> {
    let text = input.trim();
    let text = text.strip_prefix("R$").map_or(text, str::trim_start);
    let (negative, body) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    if !body.starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    if !body.chars().all(|c| c.is_ascii_digit() || c == '.' || c == ',') {
        return None;
    }

    let normalized = if let Some((int, frac)) = body.split_once(',') {
        if frac.is_empty() || frac.contains(['.', ',']) {
            return None;
        }
        if int.contains('.') && !is_grouped(int) {
            return None;
        }
        format!("{}.{}", int.replace('.', ""), frac)
    } else if is_grouped(body) {
        body.replace('.', "")
    } else if body.matches('.').count() <= 1 && !body.ends_with('.') {
        body.to_string()
    } else {
        return None;
    };

    let value: f64 = normalized.parse().ok()?;
    Some(if negative { -value } else { value })
}

/// `true` para inteiros com ponto de milhar bem formado, como `1.234.567`.
fn is_grouped(text: &str) -> bool {
    let mut parts = text.split('.');
    let first = parts.next().unwrap_or("");
    if first.is_empty() || first.len() > 3 {
        return false;
    }
    let mut groups = 0;
    for part in parts {
        if part.len() != 3 {
            return false;
        }
        groups += 1;
    }
    groups > 0
}

/// Formato "Geral": sem separador de milhar, vírgula decimal e até dez casas.
pub fn format_number(value: f64) -> String {
    if !value.is_finite() {
        return value.to_string();
    }
    let mut text = format!("{value:.10}");
    if text.contains('.') {
        text = text.trim_end_matches('0').trim_end_matches('.').to_string();
    }
    if text == "-0" {
        text = "0".to_string();
    }
    text.replace('.', ",")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_brazilian_numbers() {
        assert_eq!(parse_number("18500"), Some(18500.0));
        assert_eq!(parse_number("1234,5"), Some(1234.5));
        assert_eq!(parse_number("18.500,00"), Some(18500.0));
        assert_eq!(parse_number("1.234.567"), Some(1_234_567.0));
        assert_eq!(parse_number("R$ 9.200,00"), Some(9200.0));
        assert_eq!(parse_number("-42"), Some(-42.0));
        assert_eq!(parse_number(" 7 "), Some(7.0));
    }

    #[test]
    fn accepts_plain_decimal_point() {
        assert_eq!(parse_number("1234.5"), Some(1234.5));
        assert_eq!(parse_number("0.25"), Some(0.25));
    }

    #[test]
    fn rejects_text_and_malformed_numbers() {
        for bad in ["", "abc", "Janeiro", "1,2,3", "1.23,4", "12.", "1,", "-", "1e5", "1.2.3"] {
            assert_eq!(parse_number(bad), None, "{bad}");
        }
    }

    #[test]
    fn formats_general_numbers() {
        assert_eq!(format_number(18500.0), "18500");
        assert_eq!(format_number(1234.5), "1234,5");
        assert_eq!(format_number(0.1 + 0.2), "0,3");
        assert_eq!(format_number(-0.0), "0");
        assert_eq!(format_number(-2.75), "-2,75");
    }
}
