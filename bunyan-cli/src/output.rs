use serde::Serialize;

#[derive(Clone, Copy)]
pub enum OutputMode {
    Table,
    Json,
    Quiet,
}

pub fn print_value<T: Serialize>(mode: OutputMode, value: &T) {
    match mode {
        OutputMode::Json => {
            println!("{}", serde_json::to_string_pretty(value).unwrap());
        }
        _ => {
            // Table/Quiet modes handled per-command
            println!("{}", serde_json::to_string_pretty(value).unwrap());
        }
    }
}

pub fn print_table(headers: &[&str], rows: &[Vec<String>]) {
    if rows.is_empty() {
        return;
    }

    let mut widths: Vec<usize> = headers.iter().map(|h| h.len()).collect();
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            if i < widths.len() {
                widths[i] = widths[i].max(cell.len());
            }
        }
    }

    // Header
    let header_line: Vec<String> = headers
        .iter()
        .enumerate()
        .map(|(i, h)| format!("{:<width$}", h, width = widths[i]))
        .collect();
    println!("{}", header_line.join("  "));

    // Separator
    let sep: Vec<String> = widths.iter().map(|w| "-".repeat(*w)).collect();
    println!("{}", sep.join("  "));

    // Rows
    for row in rows {
        let line: Vec<String> = row
            .iter()
            .enumerate()
            .map(|(i, cell)| {
                let w = widths.get(i).copied().unwrap_or(0);
                format!("{:<width$}", cell, width = w)
            })
            .collect();
        println!("{}", line.join("  "));
    }
}
