use serde_json::Value;

pub struct Cell {
    pub index: usize,
    pub source: String,
}

pub struct Notebook {
    pub path: String,
    pub code_cells: Vec<Cell>,
    pub markdown_cells: usize,
    pub markdown_chars: usize,
}

fn join_source(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Array(lines) => lines
            .iter()
            .filter_map(|line| line.as_str())
            .collect::<Vec<_>>()
            .concat(),
        _ => String::new(),
    }
}

fn strip_magics(source: &str) -> String {
    source
        .lines()
        .map(|line| {
            let trimmed = line.trim_start();
            if trimmed.starts_with('!') || trimmed.starts_with('%') || trimmed.starts_with("?") {
                ""
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn parse(path: &str, content: &str) -> Option<Notebook> {
    let parsed: Value = serde_json::from_str(content).ok()?;
    let cells = parsed.get("cells")?.as_array()?;

    let mut code_cells = Vec::new();
    let mut markdown_cells = 0usize;
    let mut markdown_chars = 0usize;

    for (index, cell) in cells.iter().enumerate() {
        let kind = cell.get("cell_type").and_then(|v| v.as_str()).unwrap_or("");
        let source = cell.get("source").map(join_source).unwrap_or_default();

        match kind {
            "code" => {
                let cleaned = strip_magics(&source);
                if !cleaned.trim().is_empty() {
                    code_cells.push(Cell {
                        index: index + 1,
                        source: cleaned,
                    });
                }
            }
            "markdown" => {
                let trimmed = source.trim();
                if !trimmed.is_empty() {
                    markdown_cells += 1;
                    markdown_chars += trimmed.chars().count();
                }
            }
            _ => {}
        }
    }

    Some(Notebook {
        path: path.to_string(),
        code_cells,
        markdown_cells,
        markdown_chars,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r##"{
      "cells": [
        {"cell_type":"markdown","source":["# Analisis Penjualan\n","Bagian ini menjawab pertanyaan bisnis."]},
        {"cell_type":"code","source":["!pip install pandas\n","import pandas as pd\n","df = pd.read_csv('data.csv')\n"],"outputs":[]},
        {"cell_type":"code","source":["%matplotlib inline\n","df.describe()\n"],
         "outputs":[{"output_type":"stream","text":["accuracy: 0.91\n"]}]},
        {"cell_type":"markdown","source":["   "]}
      ]
    }"##;

    #[test]
    fn extracts_code_cells_and_counts_markdown() {
        let nb = parse("eda.ipynb", SAMPLE).unwrap();
        assert_eq!(nb.code_cells.len(), 2);
        assert_eq!(nb.markdown_cells, 1, "markdown kosong tidak dihitung");
        assert!(nb.markdown_chars > 0);
    }

    #[test]
    fn strips_shell_and_magic_lines() {
        let nb = parse("eda.ipynb", SAMPLE).unwrap();
        let first = &nb.code_cells[0].source;
        assert!(!first.contains("pip install"), "baris ! harus dibuang");
        assert!(first.contains("import pandas"));

        let second = &nb.code_cells[1].source;
        assert!(
            !second.contains("matplotlib inline"),
            "baris % harus dibuang"
        );
        assert!(second.contains("df.describe()"));
    }

    #[test]
    fn keeps_cell_numbers_for_evidence() {
        let nb = parse("eda.ipynb", SAMPLE).unwrap();
        assert_eq!(nb.code_cells[0].index, 2);
        assert_eq!(nb.code_cells[1].index, 3);
    }

    #[test]
    fn rejects_non_notebook_json() {
        assert!(parse("x.ipynb", r#"{"foo":1}"#).is_none());
        assert!(parse("x.ipynb", "bukan json").is_none());
    }
}
