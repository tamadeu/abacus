use std::fmt;

use crate::number::{format_number, parse_number};

/// Valor calculado de uma célula.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Empty,
    Number(f64),
    Text(String),
    Error(CellError),
}

impl Value {
    /// Valor de uma célula sem fórmula, a partir do texto digitado.
    pub fn from_input(input: &str) -> Self {
        if input.trim().is_empty() {
            Value::Empty
        } else if let Some(number) = parse_number(input) {
            Value::Number(number)
        } else {
            Value::Text(input.to_string())
        }
    }

    /// Texto mostrado na grade.
    pub fn display(&self) -> String {
        match self {
            Value::Empty => String::new(),
            Value::Number(n) => format_number(*n),
            Value::Text(t) => t.clone(),
            Value::Error(e) => e.to_string(),
        }
    }
}

/// Erros que uma célula pode exibir, com os nomes do Excel em português.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellError {
    /// Divisão por zero.
    Div0,
    /// Operação aritmética com texto.
    Value,
    /// Função desconhecida.
    Name,
    /// Fórmula que depende de si mesma.
    Cycle,
    /// Fórmula mal formada.
    Syntax,
}

impl fmt::Display for CellError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            CellError::Div0 => "#DIV/0!",
            CellError::Value => "#VALOR!",
            CellError::Name => "#NOME?",
            CellError::Cycle => "#CICLO!",
            CellError::Syntax => "#ERRO!",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_input() {
        assert_eq!(Value::from_input("  "), Value::Empty);
        assert_eq!(Value::from_input("18.500,00"), Value::Number(18500.0));
        assert_eq!(Value::from_input("Janeiro"), Value::Text("Janeiro".into()));
    }

    #[test]
    fn displays_values() {
        assert_eq!(Value::Number(9300.0).display(), "9300");
        assert_eq!(Value::Error(CellError::Div0).display(), "#DIV/0!");
        assert_eq!(Value::Empty.display(), "");
    }
}
