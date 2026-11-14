use std::fmt;

use abacus_core::CellRef;

/// Árvore sintática de uma fórmula.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Number(f64),
    Ref(CellRef),
    Range(CellRef, CellRef),
    Negate(Box<Expr>),
    Binary(Box<Expr>, BinaryOp, Box<Expr>),
    /// Chamada de função. O nome fica em maiúsculas.
    Call(String, Vec<Expr>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
}

/// Fórmula mal formada.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError(String);

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ParseError {}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Number(f64),
    Ident(String),
    Plus,
    Minus,
    Star,
    Slash,
    Caret,
    LParen,
    RParen,
    Colon,
    Semicolon,
}

fn tokenize(source: &str) -> Result<Vec<Token>, ParseError> {
    let chars: Vec<char> = source.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
        } else if c.is_ascii_digit() {
            let start = i;
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
            }
            // Parte decimal: vírgula (pt-BR) ou ponto, seguido de dígito.
            if i + 1 < chars.len() && matches!(chars[i], ',' | '.') && chars[i + 1].is_ascii_digit() {
                i += 1;
                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                }
            }
            let text: String = chars[start..i].iter().map(|&d| if d == ',' { '.' } else { d }).collect();
            let number = text.parse().map_err(|_| ParseError(format!("número inválido: {text}")))?;
            tokens.push(Token::Number(number));
        } else if c.is_alphabetic() {
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || matches!(chars[i], '.' | '_')) {
                i += 1;
            }
            let ident: String = chars[start..i].iter().collect();
            tokens.push(Token::Ident(ident.to_uppercase()));
        } else {
            tokens.push(match c {
                '+' => Token::Plus,
                '-' => Token::Minus,
                '*' => Token::Star,
                '/' => Token::Slash,
                '^' => Token::Caret,
                '(' => Token::LParen,
                ')' => Token::RParen,
                ':' => Token::Colon,
                ';' => Token::Semicolon,
                other => return Err(ParseError(format!("caractere inesperado: {other}"))),
            });
            i += 1;
        }
    }
    Ok(tokens)
}

/// Analisa o texto de uma fórmula, sem o `=` inicial.
pub fn parse(source: &str) -> Result<Expr, ParseError> {
    let mut parser = Parser { tokens: tokenize(source)?, pos: 0 };
    let expr = parser.expr()?;
    match parser.peek() {
        None => Ok(expr),
        Some(token) => Err(ParseError(format!("sobrou {token:?} no fim da fórmula"))),
    }
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn next(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.pos).cloned();
        self.pos += 1;
        token
    }

    fn eat(&mut self, token: &Token) -> bool {
        let matched = self.peek() == Some(token);
        if matched {
            self.pos += 1;
        }
        matched
    }

    fn expect(&mut self, token: Token) -> Result<(), ParseError> {
        if self.eat(&token) { Ok(()) } else { Err(ParseError(format!("esperava {token:?}"))) }
    }

    // expr := term (('+' | '-') term)*
    fn expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.term()?;
        loop {
            let op = match self.peek() {
                Some(Token::Plus) => BinaryOp::Add,
                Some(Token::Minus) => BinaryOp::Sub,
                _ => return Ok(left),
            };
            self.pos += 1;
            left = Expr::Binary(Box::new(left), op, Box::new(self.term()?));
        }
    }

    // term := power (('*' | '/') power)*
    fn term(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.power()?;
        loop {
            let op = match self.peek() {
                Some(Token::Star) => BinaryOp::Mul,
                Some(Token::Slash) => BinaryOp::Div,
                _ => return Ok(left),
            };
            self.pos += 1;
            left = Expr::Binary(Box::new(left), op, Box::new(self.power()?));
        }
    }

    // power := unary ('^' unary)*
    // Como no Excel: associa à esquerda (2^3^2 = 64) e o sinal vem antes
    // da potência (-2^2 = 4).
    fn power(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.unary()?;
        while self.eat(&Token::Caret) {
            left = Expr::Binary(Box::new(left), BinaryOp::Pow, Box::new(self.unary()?));
        }
        Ok(left)
    }

    // unary := ('-' | '+') unary | primary
    fn unary(&mut self) -> Result<Expr, ParseError> {
        if self.eat(&Token::Minus) {
            Ok(Expr::Negate(Box::new(self.unary()?)))
        } else if self.eat(&Token::Plus) {
            self.unary()
        } else {
            self.primary()
        }
    }

    fn primary(&mut self) -> Result<Expr, ParseError> {
        match self.next() {
            Some(Token::Number(n)) => Ok(Expr::Number(n)),
            Some(Token::LParen) => {
                let inner = self.expr()?;
                self.expect(Token::RParen)?;
                Ok(inner)
            }
            Some(Token::Ident(name)) => {
                if self.eat(&Token::LParen) {
                    return Ok(Expr::Call(name, self.arguments()?));
                }
                let start = CellRef::parse(&name).ok_or_else(|| ParseError(format!("nome desconhecido: {name}")))?;
                if !self.eat(&Token::Colon) {
                    return Ok(Expr::Ref(start));
                }
                match self.next() {
                    Some(Token::Ident(end)) => CellRef::parse(&end)
                        .map(|end| Expr::Range(start, end))
                        .ok_or_else(|| ParseError(format!("fim de intervalo inválido: {end}"))),
                    _ => Err(ParseError("esperava uma célula depois de ':'".into())),
                }
            }
            Some(token) => Err(ParseError(format!("não esperava {token:?}"))),
            None => Err(ParseError("fórmula incompleta".into())),
        }
    }

    // Argumentos depois do '(' já consumido, separados por ';'.
    fn arguments(&mut self) -> Result<Vec<Expr>, ParseError> {
        let mut args = Vec::new();
        if self.eat(&Token::RParen) {
            return Ok(args);
        }
        loop {
            args.push(self.expr()?);
            if self.eat(&Token::RParen) {
                return Ok(args);
            }
            self.expect(Token::Semicolon)?;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell(text: &str) -> CellRef {
        CellRef::parse(text).unwrap()
    }

    #[test]
    fn parses_subtraction_of_refs() {
        assert_eq!(
            parse("B3-C3").unwrap(),
            Expr::Binary(Box::new(Expr::Ref(cell("B3"))), BinaryOp::Sub, Box::new(Expr::Ref(cell("C3"))))
        );
    }

    #[test]
    fn parses_function_with_range() {
        assert_eq!(
            parse("soma(B3:B6; 1,5)").unwrap(),
            Expr::Call("SOMA".into(), vec![Expr::Range(cell("B3"), cell("B6")), Expr::Number(1.5)])
        );
    }

    #[test]
    fn multiplication_binds_tighter_than_addition() {
        assert_eq!(
            parse("1+2*3").unwrap(),
            Expr::Binary(
                Box::new(Expr::Number(1.0)),
                BinaryOp::Add,
                Box::new(Expr::Binary(Box::new(Expr::Number(2.0)), BinaryOp::Mul, Box::new(Expr::Number(3.0))))
            )
        );
    }

    #[test]
    fn uppercases_accented_function_names() {
        assert_eq!(parse("média()").unwrap(), Expr::Call("MÉDIA".into(), vec![]));
    }

    #[test]
    fn rejects_malformed_formulas() {
        for bad in ["", "1+", "(1", "1 2", "SOMA(1;", "A1:", "A1:2", "1 # 2", "FOO", "SOMA(1,5,2)"] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }
}
