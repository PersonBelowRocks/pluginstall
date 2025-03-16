use env_logger::WriteStyle;
use hyperx::header::{ContentDisposition, DispositionParam};
use log::LevelFilter;
use owo_colors::{AnsiColors, OwoColorize};
use std::{
    cmp::max,
    fmt,
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

/// A row in a [`CliTable`], holding a list of the cells in the row.
///
/// May be indexed to access the contained cells.
#[derive(Debug, Clone, dm::Index, dm::IndexMut)]
pub struct CliTableRow {
    #[index]
    #[index_mut]
    cells: Vec<CliTableCell>,
    /// The background that this row should have when being printed.
    /// Set to [`None`] for no background color (i.e., the default background color).
    pub bg_color: AnsiColors,
}

impl CliTableRow {
    /// Create a new empty row with a given number of columns.
    #[inline]
    #[must_use]
    pub fn empty(columns: usize) -> Self {
        Self {
            cells: vec![CliTableCell::default(); columns],
            bg_color: AnsiColors::Default,
        }
    }

    /// Create a new row with the provided text in the columns.
    /// Cell colors will be the default colors.
    #[inline]
    #[must_use]
    pub fn new(columns: &[String]) -> Self {
        Self {
            cells: columns
                .iter()
                .cloned()
                .map(CliTableCell::new)
                .collect::<Vec<_>>(),
            bg_color: AnsiColors::Default,
        }
    }

    /// Check if this row is empty.
    /// A row is empty if:
    /// - All contained cells have no text
    /// - The row has the default background color
    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        matches!(self.bg_color, AnsiColors::Default)
            && self.cells.iter().all(|cell| cell.text.is_empty())
    }

    /// The number of columns in this row.
    #[inline]
    #[must_use]
    pub fn columns(&self) -> usize {
        self.cells.len()
    }

    /// Apply the given color to the text in all contained cells.
    #[inline]
    pub fn color_all(&mut self, color: AnsiColors) {
        for cell in self.cells.iter_mut() {
            cell.color = color
        }
    }

    /// Write this table row to the formatter.
    /// Columns will be padded until they reach their width as described in the `widths` slice.
    ///
    /// Will not write a newline at the end.
    ///
    /// # Panics
    /// Will panic if the length of `width` is not the same as the number of columns in this row,
    /// or if a cell in this row is wider than the width of its column in `widths`
    #[inline]
    pub fn write(&self, f: &mut fmt::Formatter, widths: &[usize]) -> fmt::Result {
        assert_eq!(widths.len(), self.columns(), "Number of columns must match");

        for i in 0..self.columns() {
            let cell = &self[i];
            let target_width = widths[i];

            // the number of spaces to insert on the right of the field
            let right_padding = target_width - cell.width();

            // leftward cell border, also the rightward cell border of the leftward cell
            write!(f, "{}", '|'.on_color(self.bg_color).dimmed())?;

            // padding against the leftward cell border
            write!(f, "{}", ' '.on_color(self.bg_color))?;

            // writing the text
            write!(
                f,
                "{}",
                &cell.text.on_color(self.bg_color).color(cell.color)
            )?;

            // padding to fit the column width
            write!(f, "{}", &" ".repeat(right_padding).on_color(self.bg_color))?;

            // padding against the rightward cell border
            write!(f, "{}", ' '.on_color(self.bg_color))?;
        }

        // rightmost cell border
        write!(f, "{}", '|'.on_color(self.bg_color).dimmed())?;

        Ok(())
    }
}

/// A cell in a [`CliTable`].
///
/// Holds the text contents of the cell and the ANSI color that the text should be printed in.
#[derive(Debug, Clone)]
pub struct CliTableCell {
    pub text: String,
    pub color: AnsiColors,
}

impl Default for CliTableCell {
    fn default() -> Self {
        Self {
            text: String::new(),
            color: AnsiColors::Default,
        }
    }
}

impl CliTableCell {
    /// Create a new cell with the given text and default colors.
    #[inline]
    #[must_use]
    pub fn new(text: String) -> Self {
        Self {
            text,
            ..Default::default()
        }
    }

    /// The width of the text in this cell.
    #[inline]
    #[must_use]
    pub fn width(&self) -> usize {
        self.text.len()
    }
}

/// A table that can be written to the terminal in a text representation.
#[derive(Debug, Clone)]
pub struct CliTable {
    /// The names of the columns in the row. Will be printed as a header or footer.
    column_names: CliTableRow,
    rows: Vec<CliTableRow>,
}

impl CliTable {
    /// Create a new empty CLI table, using the given row as the column names.
    /// The number of columns in the given row will be the number of columns in the table.
    #[inline]
    pub fn new(columns: CliTableRow) -> Self {
        Self {
            column_names: columns,
            rows: Vec::new(),
        }
    }

    /// The number of rows in this table.
    #[inline]
    pub fn rows(&self) -> usize {
        self.rows.len()
    }

    /// The number of columns in this table.
    #[inline]
    pub fn columns(&self) -> usize {
        self.column_names.columns()
    }

    /// Push a row onto this table. Returns the index of the added row.
    ///
    /// # Panics
    /// Will panic if the row has a different number of columns than the table.
    #[inline]
    pub fn add(&mut self, row: CliTableRow) -> usize {
        assert_eq!(
            row.columns(),
            self.columns(),
            "Row must have the same number of columns as the table"
        );

        self.rows.push(row);

        self.rows() - 1
    }

    /// Remove a row with the given index from this table, returning it.
    /// This shifts the remaining rows "upwards" so that the order is preserved.
    ///
    /// Returns [`None`] if no row with the index existed.
    #[inline]
    pub fn remove(&mut self, row_index: usize) -> Option<CliTableRow> {
        if self.rows() <= row_index {
            None
        } else {
            Some(self.rows.remove(row_index))
        }
    }

    /// Find the maximum width of each column in this table.
    /// A column's max width can be used to calculate how much padding is needed for a cell's text.
    ///
    /// This operation is somewhat costly, so the result should be cached and invalidated whenever the table updates.
    #[inline]
    #[must_use]
    pub fn calculate_max_widths(&self) -> Vec<usize> {
        let mut cols = vec![0usize; self.columns()];

        for i in 0..self.columns() {
            cols[i] = self.column_names[i].width()
        }

        for row in self.iter() {
            for i in 0..self.columns() {
                cols[i] = max(cols[i], row[i].width())
            }
        }

        cols
    }

    /// Iterate over the rows in this table, in order of insertion.
    #[inline]
    pub fn iter(&self) -> impl Iterator<Item = &CliTableRow> + use<'_> {
        self.rows.iter()
    }
}

/// Calculate the width of a table's borders and their padding.
#[inline]
fn calculate_border_widths(columns: usize) -> usize {
    (columns * 3) + 1
}

impl fmt::Display for CliTable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // we should handle the case where the table is completely empty
        if self.columns() == 0 {
            todo!()
        }

        // Find the maximum width of each column. Fields will be padded until they are equal to the maximum width.
        let max_column_widths = self.calculate_max_widths();

        // the total width the table takes up
        let total_table_width =
            max_column_widths.iter().sum::<usize>() + calculate_border_widths(self.columns());

        // write the column headers if they're not empty
        if !self.column_names.is_empty() {
            self.column_names.write(f, &max_column_widths)?;
            // newline after the headers
            writeln!(f)?;
            // a horizontal separator underneath the headers
            write!(f, "{}", "-".repeat(total_table_width).dimmed())?;
        }

        // write the table contents with appropriate padding
        for row in self.iter() {
            // new line for a new row
            writeln!(f)?;

            row.write(f, &max_column_widths)?;
        }

        Ok(())
    }
}

/// Return early with an `Ok(None)` if the result of the given expression is [`None`].
/// Otherwise return the value contained in [`Some`].
#[macro_export]
macro_rules! ok_none {
    ($e:expr) => {
        match $e {
            std::option::Option::Some(out) => out,
            std::option::Option::None => return Ok(None),
        }
    };
}
