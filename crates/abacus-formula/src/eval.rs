use std::collections::HashMap;

use abacus_core::{CellError, CellRef, Sheet, Value};

use crate::parser::{BinaryOp, Expr, parse};

/// De onde o avaliador lê o conteúdo digitado nas células.
pub trait CellSource {
    fn input(&self, cell: CellRef) -> Option<&str>;
}

impl CellSource for Sheet {
    fn input(&self, cell: CellRef) -> Option<&str> {
        Sheet::input(self, cell)
    }
}

/// Calcula o valor de uma única célula.
pub fn evaluate<S: CellSource + ?Sized>(source: &S, cell: CellRef) -> Value {
    Evaluator::new(source).value(cell)
}

/// Calcula valores de células, guardando os resultados já obtidos.
///
/// Use o mesmo avaliador para todas as células de uma atualização de tela:
/// assim cada célula é calculada uma vez só. Depois de qualquer edição, crie
/// um avaliador novo. Ainda não há grafo de dependências nem recálculo
/// incremental.
pub struct Evaluator<'a, S: CellSource + ?Sized> {
    source: &'a S,
    cache: HashMap<CellRef, Value>,
    /// Células em cálculo no momento, para detectar ciclos.
    in_progress: Vec<CellRef>,
}

impl<'a, S: CellSource + ?Sized> Evaluator<'a, S> {
    pub fn new(source: &'a S) -> Self {
        Self { source, cache: HashMap::new(), in_progress: Vec::new() }
    }

    /// Valor calculado da célula.
    pub fn value(&mut self, cell: CellRef) -> Value {
        if let Some(cached) = self.cache.get(&cell) {
            return cached.clone();
        }
        if self.in_progress.contains(&cell) {
            return Value::Error(CellError::Cycle);
        }
        self.in_progress.push(cell);
        let value = self.compute(cell);
        self.in_progress.pop();
        self.cache.insert(cell, value.clone());
        value
    }

    fn compute(&mut self, cell: CellRef) -> Value {
        let source = self.source;
        let Some(input) = source.input(cell) else {
            return Value::Empty;
        };
        match input.strip_prefix('=') {
            Some(formula) => match parse(formula) {
                Ok(expr) => self.eval(&expr),
                Err(_) => Value::Error(CellError::Syntax),
            },
            None => Value::from_input(input),
        }
    }

    fn eval(&mut self, expr: &Expr) -> Value {
        match expr {
            Expr::Number(n) => Value::Number(*n),
            Expr::Ref(cell) => self.value(*cell),
            // Um intervalo só faz sentido como argumento de função.
            Expr::Range(..) => Value::Error(CellError::Value),
            Expr::Negate(inner) => match self.number(inner) {
                Ok(n) => Value::Number(-n),
                Err(e) => Value::Error(e),
            },
            Expr::Binary(left, op, right) => match self.binary(left, *op, right) {
                Ok(n) => Value::Number(n),
                Err(e) => Value::Error(e),
            },
            Expr::Call(name, args) => match self.call(name, args) {
                Ok(n) => Value::Number(n),
                Err(e) => Value::Error(e),
            },
        }
    }

    /// Avalia a expressão como operando aritmético: vazio vale zero e texto
    /// é erro.
    fn number(&mut self, expr: &Expr) -> Result<f64, CellError> {
        match self.eval(expr) {
            Value::Number(n) => Ok(n),
            Value::Empty => Ok(0.0),
            Value::Text(_) => Err(CellError::Value),
            Value::Error(e) => Err(e),
        }
    }

    fn binary(&mut self, left: &Expr, op: BinaryOp, right: &Expr) -> Result<f64, CellError> {
        let a = self.number(left)?;
        let b = self.number(right)?;
        let result = match op {
            BinaryOp::Add => a + b,
            BinaryOp::Sub => a - b,
            BinaryOp::Mul => a * b,
            BinaryOp::Div if b == 0.0 => return Err(CellError::Div0),
            BinaryOp::Div => a / b,
            BinaryOp::Pow => a.powf(b),
        };
        if result.is_finite() { Ok(result) } else { Err(CellError::Value) }
    }

    fn call(&mut self, name: &str, args: &[Expr]) -> Result<f64, CellError> {
        let function: fn(&[f64]) -> Result<f64, CellError> = match name {
            "SOMA" | "SUM" => |xs| Ok(xs.iter().sum()),
            "MÉDIA" | "MEDIA" | "AVERAGE" => |xs| {
                if xs.is_empty() { Err(CellError::Div0) } else { Ok(xs.iter().sum::<f64>() / xs.len() as f64) }
            },
            "MÁXIMO" | "MAXIMO" | "MAX" => |xs| Ok(xs.iter().copied().reduce(f64::max).unwrap_or(0.0)),
            "MÍNIMO" | "MINIMO" | "MIN" => |xs| Ok(xs.iter().copied().reduce(f64::min).unwrap_or(0.0)),
            "CONT.NÚM" | "CONT.NUM" | "COUNT" => |xs| Ok(xs.len() as f64),
            _ => return Err(CellError::Name),
        };
        let numbers = self.collect_numbers(args)?;
        function(&numbers)
    }

    /// Junta os números dos argumentos de uma função de agregação. Em
    /// intervalos, células vazias e de texto são ignoradas, como no Excel.
    fn collect_numbers(&mut self, args: &[Expr]) -> Result<Vec<f64>, CellError> {
        let mut numbers = Vec::new();
        for arg in args {
            if let Expr::Range(a, b) = arg {
                for col in a.col.min(b.col)..=a.col.max(b.col) {
                    for row in a.row.min(b.row)..=a.row.max(b.row) {
                        match self.value(CellRef { col, row }) {
                            Value::Number(n) => numbers.push(n),
                            Value::Error(e) => return Err(e),
                            Value::Empty | Value::Text(_) => {}
                        }
                    }
                }
            } else {
                match self.eval(arg) {
                    Value::Number(n) => numbers.push(n),
                    Value::Empty => {}
                    Value::Text(_) => return Err(CellError::Value),
                    Value::Error(e) => return Err(e),
                }
            }
        }
        Ok(numbers)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sheet(cells: &[(&str, &str)]) -> Sheet {
        let mut sheet = Sheet::new("Teste");
        for (cell, input) in cells {
            sheet.set_input(CellRef::parse(cell).unwrap(), *input);
        }
        sheet
    }

    fn value(sheet: &Sheet, cell: &str) -> Value {
        evaluate(sheet, CellRef::parse(cell).unwrap())
    }

    fn formula(text: &str) -> Value {
        value(&sheet(&[("A1", text)]), "A1")
    }

    #[test]
    fn financial_report() {
        let report = sheet(&[
            ("A2", "Categoria"),
            ("A3", "Janeiro"),
            ("B3", "18.500,00"),
            ("C3", "9.200,00"),
            ("D3", "=B3-C3"),
            ("B4", "22400"),
            ("C4", "11800"),
            ("D4", "=B4-C4"),
            ("B5", "26800"),
            ("C5", "13400"),
            ("D5", "=B5-C5"),
            ("B6", "24100"),
            ("C6", "12100"),
            ("D6", "=B6-C6"),
            ("B8", "=SOMA(B3:B6)"),
            ("C8", "=SOMA(C3:C6)"),
            ("D8", "=B8-C8"),
        ]);
        assert_eq!(value(&report, "D3"), Value::Number(9300.0));
        assert_eq!(value(&report, "B8"), Value::Number(91800.0));
        assert_eq!(value(&report, "C8"), Value::Number(46500.0));
        assert_eq!(value(&report, "D8"), Value::Number(45300.0));
        // O cabeçalho de texto dentro do intervalo é ignorado.
        assert_eq!(value(&sheet(&[("B2", "Receita"), ("B3", "10"), ("B9", "=SOMA(B2:B3)")]), "B9"), Value::Number(10.0));
    }

    #[test]
    fn arithmetic_and_precedence() {
        assert_eq!(formula("=1+2*3"), Value::Number(7.0));
        assert_eq!(formula("=(1+2)*3"), Value::Number(9.0));
        assert_eq!(formula("=10/4"), Value::Number(2.5));
        assert_eq!(formula("=1,5*2"), Value::Number(3.0));
        assert_eq!(formula("=-2^2"), Value::Number(4.0));
        assert_eq!(formula("=2^3^2"), Value::Number(64.0));
        assert_eq!(formula("=--3"), Value::Number(3.0));
    }

    #[test]
    fn aggregate_functions() {
        let data = sheet(&[("A1", "2"), ("A2", "4"), ("A3", "texto"), ("A4", "9")]);
        let with = |f: &str| {
            let mut s = data.clone();
            s.set_input(CellRef::parse("C1").unwrap(), f);
            value(&s, "C1")
        };
        assert_eq!(with("=SOMA(A1:A4)"), Value::Number(15.0));
        assert_eq!(with("=MÉDIA(A1:A4)"), Value::Number(5.0));
        assert_eq!(with("=máximo(A1:A4; 20)"), Value::Number(20.0));
        assert_eq!(with("=MÍNIMO(A1:A4)"), Value::Number(2.0));
        assert_eq!(with("=CONT.NÚM(A1:A4)"), Value::Number(3.0));
        assert_eq!(with("=SUM(A4:A1)"), Value::Number(15.0));
        assert_eq!(with("=MÉDIA(B1:B4)"), Value::Error(CellError::Div0));
    }

    #[test]
    fn references_pass_values_through() {
        let s = sheet(&[("A1", "Janeiro"), ("B1", "=A1"), ("C1", "=Z9"), ("D1", "=Z9+1")]);
        assert_eq!(value(&s, "B1"), Value::Text("Janeiro".into()));
        assert_eq!(value(&s, "C1"), Value::Empty);
        assert_eq!(value(&s, "D1"), Value::Number(1.0));
    }

    #[test]
    fn errors() {
        assert_eq!(formula("=1/0"), Value::Error(CellError::Div0));
        assert_eq!(formula("=DESCONHECIDA(1)"), Value::Error(CellError::Name));
        assert_eq!(formula("=1+"), Value::Error(CellError::Syntax));
        assert_eq!(formula("=B1:B3"), Value::Error(CellError::Value));
        let s = sheet(&[("A1", "texto"), ("B1", "=A1+1"), ("C1", "=B1*2"), ("D1", "=SOMA(B1:B1)")]);
        assert_eq!(value(&s, "B1"), Value::Error(CellError::Value));
        // Erros se propagam para quem depende da célula.
        assert_eq!(value(&s, "C1"), Value::Error(CellError::Value));
        assert_eq!(value(&s, "D1"), Value::Error(CellError::Value));
    }

    #[test]
    fn detects_cycles() {
        assert_eq!(formula("=A1"), Value::Error(CellError::Cycle));
        let s = sheet(&[("A1", "=B1"), ("B1", "=C1+1"), ("C1", "=A1"), ("D1", "=SOMA(A1:C1)")]);
        for cell in ["A1", "B1", "C1", "D1"] {
            assert_eq!(value(&s, cell), Value::Error(CellError::Cycle), "{cell}");
        }
    }

    #[test]
    fn shared_dependency_is_not_a_cycle() {
        let s = sheet(&[("A1", "5"), ("B1", "=A1*2"), ("C1", "=A1+B1"), ("D1", "=B1+C1")]);
        assert_eq!(value(&s, "D1"), Value::Number(25.0));
    }

    #[test]
    fn evaluator_reuses_results() {
        let s = sheet(&[("A1", "3"), ("B1", "=A1*A1")]);
        let mut evaluator = Evaluator::new(&s);
        assert_eq!(evaluator.value(CellRef::parse("B1").unwrap()), Value::Number(9.0));
        assert_eq!(evaluator.value(CellRef::parse("B1").unwrap()), Value::Number(9.0));
    }
}
