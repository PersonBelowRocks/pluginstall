use env_logger::WriteStyle;
use log::LevelFilter;
use owo_colors::{AnsiColors, OwoColorize};
use std::{array, cmp::max, io::Write};

pub const LOG_LEVEL_COLORS: [AnsiColors; 5] = [
    AnsiColors::BrightRed,
    AnsiColors::Yellow,
    AnsiColors::BrightBlue,
    AnsiColors::Green,
    AnsiColors::Default,
];

#[cfg(debug_assertions)]
pub const LOG_LEVEL: LevelFilter = LevelFilter::Debug;
#[cfg(not(debug_assertions))]
pub const LOG_LEVEL: LevelFilter = LevelFilter::Info;

/// Initialize the logger with the default format
#[cold]
pub(crate) fn setup_logger() {
    env_logger::builder()
        .parse_default_env()
        .filter_level(LOG_LEVEL)
        .write_style(WriteStyle::Auto)
        .format(|formatter, record| {
            let level = record.level();
            // levels start at ordinal 1, so we need to shift them down by 1
            let color = LOG_LEVEL_COLORS[(level as usize) - 1];

            writeln!(
                formatter,
                "[{level}]: {message}",
                level = level.color(color),
                message = record.args()
            )
        })
        .init();
}

/// A table that can be written to the terminal in a text representation.
#[derive(Debug, Clone)]
pub struct CliTable<const COLS: usize> {
    column_names: [String; COLS],
    rows: Vec<[String; COLS]>,
}

impl<const COLS: usize> CliTable<COLS> {
    /// Create a new CLI table with the given column names.
    #[inline]
    pub fn new(column_names: [impl Into<String>; COLS]) -> Self {
        Self {
            column_names: column_names.map(Into::<String>::into),
            rows: Vec::new(),
        }
    }

    /// The number of rows in this table.
    #[inline]
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// Push a row onto this table.
    #[inline]
    pub fn push(&mut self, column: [impl Into<String>; COLS]) {
        self.rows.push(column.map(Into::<String>::into));
    }

    #[inline]
    pub fn write(
        &self,
        w: &mut impl std::io::Write,
        formatting: &CliTableFormatting<COLS>,
    ) -> std::io::Result<()> {
        // we make a copy of ourselves up here so we can do formatting on the copy
        let mut writable_table = self.clone();

        if formatting.equal_field_width {
            // Find the maximum width of each column. Fields will be padded until they are equal to the maximum width.
            let max_column_widths = {
                // start with the width of the headers if we're supposed to write the headers.
                // we don't want to pad to the width of a header unless the header is actually there, otherwise we
                // risk wasting space.
                let mut cols: [usize; COLS] = if formatting.write_headers {
                    array::from_fn(|i| writable_table.column_names[i].len())
                } else {
                    [0; COLS]
                };

                for row in &writable_table.rows {
                    for i in 0..COLS {
                        let field_width = row[i].len();
                        cols[i] = max(cols[i], field_width)
                    }
                }

                cols
            };

            // do the actual padding
            for row in &mut writable_table.rows {
                for i in 0..COLS {
                    let field = &mut row[i];
                    let target_width = max_column_widths[i];

                    // the number of spaces to insert on the right of the field
                    let right_padding = target_width - field.len();

                    field.push_str(&" ".repeat(right_padding));
                }
            }

            // pad the headers as needed
            for (i, header) in writable_table.column_names.iter_mut().enumerate() {
                let target_width = max_column_widths[i];

                // the number of spaces to insert on the right of the field
                let right_padding = target_width - header.len();

                header.push_str(&" ".repeat(right_padding));
            }
        }

        if formatting.write_headers {
            // the total width of the table.
            // we start at the width of all the borders and the border padding.
            let mut total_width = (COLS * 3) + 1;

            // the left-most border
            write!(w, "|")?;
            for (i, column_name) in writable_table.column_names.iter().enumerate() {
                total_width += column_name.len();

                // this space separates the border to the LEFT of this field from the field value itself
                write!(w, " ")?;
                match formatting.column_header_colors {
                    Some(colors) => {
                        let color = colors[i];
                        write!(w, "{}", column_name.color(color))?;
                    }
                    None => {
                        write!(w, "{column_name}")?;
                    }
                }
                // this space separates the border to the RIGHT of this field from the field value itself
                write!(w, " ")?;
                // the field's right border. will also be the right-most border if this is the last field
                write!(w, "|")?;
            }

            writeln!(w)?;
            // a separating line
            writeln!(w, "{}", "-".repeat(total_width))?;
        }

        for row in &writable_table.rows {
            // the left-most border
            write!(w, "|")?;
            for (i, field) in row.iter().enumerate() {
                // this space separates the border to the LEFT of this field from the field value itself
                write!(w, " ")?;
                match formatting.column_colors {
                    Some(colors) => {
                        let color = colors[i];
                        write!(w, "{}", field.color(color))?;
                    }
                    None => {
                        write!(w, "{field}")?;
                    }
                }
                // this space separates the border to the RIGHT of this field from the field value itself
                write!(w, " ")?;
                // the field's right border. will also be the right-most border if this is the last field
                write!(w, "|")?;
            }

            writeln!(w)?;
        }

        Ok(())
    }
}

/// Describes how a [`CliTable`] should be formatted when being converted to text.
#[derive(Clone, Debug, Default)]
pub struct CliTableFormatting<const COLS: usize> {
    /// Whether all fields in a column should be padded to the same width.
    pub equal_field_width: bool,
    /// The colors of the column fields.
    pub column_colors: Option<[AnsiColors; COLS]>,
    /// Whether the column headers should be written.
    pub write_headers: bool,
    /// The color of the column headers. Doesn't do anything unless `write_headers` is true.
    pub column_header_colors: Option<[AnsiColors; COLS]>,
}
