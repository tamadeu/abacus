use std::fmt;

/// Número de colunas de uma planilha, igual ao do Excel (A até XFD).
pub const MAX_COLS: u32 = 16_384;
/// Número de linhas de uma planilha, igual ao do Excel.
pub const MAX_ROWS: u32 = 1_048_576;

/// Posição de uma célula. `col` e `row` começam em zero: `A1` é `(0, 0)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CellRef {
    pub col: u32,
    pub row: u32,
}

impl CellRef {
    /// Cria uma referência, ou `None` se estiver fora dos limites da planilha.
    pub fn new(col: u32, row: u32) -> Option<Self> {
        (col < MAX_COLS && row < MAX_ROWS).then_some(Self { col, row })
    }

    /// Lê uma referência no estilo A1 (`B3`, `aa10`). Referências absolutas
    /// (`$A$1`) ainda não são aceitas.
    pub fn parse(text: &str) -> Option<Self> {
        let split = text.find(|c: char| !c.is_ascii_alphabetic())?;
        let (letters, digits) = text.split_at(split);
        if letters.is_empty() || letters.len() > 3 || digits.is_empty() {
            return None;
        }
        if !digits.bytes().all(|b| b.is_ascii_digit()) || digits.len() > 7 {
            return None;
        }
        let col = letters
            .bytes()
            .fold(0u32, |acc, b| acc * 26 + u32::from(b.to_ascii_uppercase() - b'A') + 1);
        let row: u32 = digits.parse().ok()?;
        if row == 0 {
            return None;
        }
        Self::new(col - 1, row - 1)
    }

    /// Letras da coluna: `0` vira `A`, `26` vira `AA`.
    pub fn col_name(col: u32) -> String {
        let mut n = col + 1;
        let mut letters = Vec::new();
        while n > 0 {
            let rem = (n - 1) % 26;
            letters.push(b'A' + rem as u8);
            n = (n - 1) / 26;
        }
        letters.reverse();
        String::from_utf8(letters).expect("somente letras ASCII")
    }
}

impl fmt::Display for CellRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", Self::col_name(self.col), self.row + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a1_style() {
        assert_eq!(CellRef::parse("A1"), Some(CellRef { col: 0, row: 0 }));
        assert_eq!(CellRef::parse("b3"), Some(CellRef { col: 1, row: 2 }));
        assert_eq!(CellRef::parse("AA10"), Some(CellRef { col: 26, row: 9 }));
    }

    #[test]
    fn respects_sheet_limits() {
        let last = CellRef::parse("XFD1048576").unwrap();
        assert_eq!((last.col, last.row), (MAX_COLS - 1, MAX_ROWS - 1));
        assert_eq!(CellRef::parse("XFE1"), None);
        assert_eq!(CellRef::parse("A1048577"), None);
        assert_eq!(CellRef::parse("A0"), None);
    }

    #[test]
    fn rejects_malformed_text() {
        for bad in ["", "A", "1", "1A", "A1B", "$A$1", "SOMA"] {
            assert_eq!(CellRef::parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn round_trips_through_display() {
        for text in ["A1", "Z99", "AA10", "XFD1048576"] {
            assert_eq!(CellRef::parse(text).unwrap().to_string(), text);
        }
    }
}
