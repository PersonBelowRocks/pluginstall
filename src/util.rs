use env_logger::WriteStyle;
use hyperx::header::{ContentDisposition, DispositionParam};
use log::LevelFilter;
use owo_colors::{AnsiColors, OwoColorize};
use std::{
    array,
    cmp::max,
    io::Write,
    path::{Path, PathBuf},
    str::FromStr,
};

pub const LOG_LEVEL_COLORS: [AnsiColors; 5] = [
    AnsiColors::BrightRed,
    AnsiColors::Yellow,
    AnsiColors::BrightBlue,
    AnsiColors::Green,
    AnsiColors::Cyan,
];

#[cfg(debug_assertions)]
pub const LOG_LEVEL: LevelFilter = LevelFilter::Trace;
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

/// Get the attachment file name from a 'content-disposition' header.
///
/// Will return [`None`] if the file name could not be extracted or if no file name was specified.
///
/// # Warning
/// This function will only extract the file name from the 'content-disposition' header, and will do no
/// validation of the resulting file name. It more or less takes the header at its word, and returns the file name as-is.
///
/// This can be very problematic if the file name is used directly, since someone could specify a file name that's actually a path,
/// and trick you into writing to or reading from that path.
///
/// Make sure you do the proper validation of the file name provided by this function before you use it!
/// (the [`validate_file_name`] function may come in handy here.)
#[inline]
pub fn content_disposition_file_name(content_disposition: &ContentDisposition) -> Option<PathBuf> {
    content_disposition
        .parameters
        .iter()
        .find_map(|param| -> Option<PathBuf> {
            let DispositionParam::Filename(_charset, _language_tag, file_name) = param else {
                return None;
            };

            let utf8_file_name = String::from_utf8_lossy(file_name);
            let path = PathBuf::from_str(utf8_file_name.as_ref()).unwrap();

            Some(path)
        })
}

/// Checks if a path is a valid file name.
/// Will return `true` if the path is a "valid" file name, and `false` if not.
///
/// # Valid File Names
/// A valid file name is a path with no parent directories, and no trailing path separator.
/// Examples include:
/// - `.hello.txt`
/// - `valid`
/// - `somefile.jar`
/// - `.lots.of.dots`
///
/// # Invalid File Names
/// Examples of invalid file names are:
/// - `/long/big/path`
/// - `subdirectory/file.exe`
/// - `directory/`
/// - `/invalid.json`
/// - `/`
/// - `.`
#[inline]
pub fn validate_file_name(file_name_candidate: &Path) -> bool {
    // the file name must not reference any parent directories!
    if file_name_candidate.parent() != Some(Path::new("")) {
        return false;
    }

    // file name can't be a directory
    if file_name_candidate.is_dir() {
        return false;
    }

    // don't allow any other shenanigans with file names (like the file name being just a single ".")
    let Some(file_name) = file_name_candidate.file_name() else {
        return false;
    };

    if file_name_candidate != file_name {
        return false;
    }

    true
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
