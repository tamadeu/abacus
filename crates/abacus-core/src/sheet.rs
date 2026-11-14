use std::collections::HashMap;

use crate::cell_ref::CellRef;

/// Uma planilha: guarda, por célula, exatamente o que o usuário digitou.
/// Os valores calculados ficam por conta do motor de fórmulas.
#[derive(Debug, Clone, Default)]
pub struct Sheet {
    pub name: String,
    cells: HashMap<CellRef, String>,
}

impl Sheet {
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into(), cells: HashMap::new() }
    }

    /// Define o conteúdo de uma célula. Texto vazio apaga a célula.
    pub fn set_input(&mut self, cell: CellRef, input: impl Into<String>) {
        let input = input.into();
        if input.trim().is_empty() {
            self.cells.remove(&cell);
        } else {
            self.cells.insert(cell, input);
        }
    }

    /// O que foi digitado na célula, ou `None` se estiver vazia.
    pub fn input(&self, cell: CellRef) -> Option<&str> {
        self.cells.get(&cell).map(String::as_str)
    }

    /// Todas as células preenchidas, em ordem arbitrária.
    pub fn cells(&self) -> impl Iterator<Item = (CellRef, &str)> {
        self.cells.iter().map(|(cell, input)| (*cell, input.as_str()))
    }
}

/// Uma pasta de trabalho: a lista de planilhas de um arquivo.
#[derive(Debug, Clone)]
pub struct Workbook {
    pub sheets: Vec<Sheet>,
}

impl Workbook {
    /// Pasta nova, com uma planilha vazia.
    pub fn new() -> Self {
        Self { sheets: vec![Sheet::new("Planilha1")] }
    }
}

impl Default for Workbook {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stores_and_clears_input() {
        let mut sheet = Sheet::new("Teste");
        let a1 = CellRef::parse("A1").unwrap();
        assert_eq!(sheet.input(a1), None);

        sheet.set_input(a1, "=B1+1");
        assert_eq!(sheet.input(a1), Some("=B1+1"));
        assert_eq!(sheet.cells().count(), 1);

        sheet.set_input(a1, "   ");
        assert_eq!(sheet.input(a1), None);
        assert_eq!(sheet.cells().count(), 0);
    }

    #[test]
    fn new_workbook_has_one_sheet() {
        assert_eq!(Workbook::new().sheets.len(), 1);
    }
}
