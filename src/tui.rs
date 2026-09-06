//! Utilities for the terminal UI of toipe.

use std::{
    fmt::Display,
    io::{stdout, Stdout, Write},
};

use termion::{
    clear,
    color::{self, Color},
    cursor,
    raw::{IntoRawMode, RawTerminal},
    style, terminal_size,
};

use crate::ToipeError;
use anyhow::Result;

const MIN_LINE_WIDTH: usize = 50;
const MAX_WORDS_PER_LINE: usize = 10;

pub fn wrap_words(words: &[String], max_width: u16, max_words_per_line: usize) -> Vec<Vec<String>> {
    let mut current_len = 0;
    let mut line: Vec<String> = Vec::new();
    let mut lines: Vec<Vec<String>> = Vec::new();
    for word in words {
        let new_len = current_len + word.len() as u16 + 1;
        if line.len() < max_words_per_line && new_len <= max_width {
            line.push(word.clone());
            current_len = new_len;
        } else {
            lines.push(std::mem::take(&mut line));
            line.push(word.clone());
            current_len = word.len() as u16 + 1;
        }
    }
    lines.push(line);
    lines
}

pub fn take_words_for_lines(
    words: &[String],
    max_width: u16,
    max_words_per_line: usize,
    max_lines: usize,
) -> usize {
    let mut lines = 0;
    let mut line_words = 0;
    let mut current_len = 0u16;
    for (i, word) in words.iter().enumerate() {
        let new_len = current_len + word.len() as u16 + 1;
        if line_words < max_words_per_line && new_len <= max_width {
            line_words += 1;
            current_len = new_len;
        } else {
            lines += 1;
            if lines >= max_lines {
                return i;
            }
            line_words = 1;
            current_len = word.len() as u16 + 1;
        }
    }
    words.len()
}

/// Describes something that has a printable length.
///
/// For example, a string containing color characters has a different
/// length when printed than the number of bytes or chars in it.
pub trait HasLength {
    /// number of char widths taken when printed on the terminal
    fn length(&self) -> usize;
}

/// Holds some text that is to be printed on the terminal.
///
/// This provides an abstraction for
/// - retrieving the number of actual characters when printed on the
///   terminal through the [`HasLength`] trait.
/// - for formatting the text through the various `with_*` methods.
///
/// Usually, this is used in the slice form as `&[Text]` since a
/// single [`Text`] only holds one string with all of it formatted in
/// the same way. For example, you cannot format one part of a [`Text`]
/// with green color while the rest is in red. You should instead use a
/// slice of [`Text`]s with each formatted in a different way.
#[derive(Debug, Clone)]
pub struct Text {
    /// the raw text
    raw_text: String,
    /// text without formatting
    text: String,
    /// actual number of char width taken when printed on the terminal
    length: usize,
}

impl Text {
    /// Constructs a new Text from a raw string
    ///
    /// NOTE: ensure that this string does not itself have formatting
    /// characters, zero-width characters or multi-width characters.
    pub fn new(text: String) -> Self {
        let length = text.chars().count();
        Self {
            raw_text: text.clone(),
            text,
            length,
        }
    }

    /// the actual printed text without formatting
    pub fn text(&self) -> &String {
        &self.text
    }

    /// adds faint style to the text
    pub fn with_faint(mut self) -> Self {
        self.raw_text = format!("{}{}{}", style::Faint, self.raw_text, style::NoFaint);
        self
    }

    /// adds underline to the text
    pub fn with_underline(mut self) -> Self {
        self.raw_text = format!("{}{}{}", style::Underline, self.raw_text, style::Reset);
        self
    }

    /// adds given color to the text
    pub fn with_color<C>(mut self, color: C) -> Self
    where
        C: Color,
    {
        self.raw_text = format!(
            "{}{}{}",
            color::Fg(color),
            self.raw_text,
            color::Fg(color::Reset)
        );
        self
    }
}

impl HasLength for Text {
    fn length(&self) -> usize {
        self.length
    }
}

/// NOTE: note to be confused with `.len()` which provides the number
/// of elements in the slice.
impl HasLength for [Text] {
    fn length(&self) -> usize {
        self.iter().map(|t| t.length()).sum()
    }
}

impl From<String> for Text {
    /// Constructs a new Text from a raw string
    ///
    /// NOTE: ensure that this string does not itself have formatting
    /// characters, zero-width characters or multi-width characters.
    fn from(text: String) -> Self {
        Self::new(text)
    }
}

impl From<&str> for Text {
    /// Constructs a new Text from a raw string
    ///
    /// NOTE: ensure that this string does not itself have formatting
    /// characters, zero-width characters or multi-width characters.
    fn from(text: &str) -> Self {
        Self::new(text.to_string())
    }
}

impl From<char> for Text {
    /// Constructs a new Text from a character
    ///
    /// NOTE: ensure that this character is itself not a formatting
    /// character, zero-width character or a multi-width character.
    fn from(c: char) -> Self {
        Self::new(c.to_string())
    }
}

/// Displays the raw string as-is. No surprises here.
impl Display for Text {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.raw_text)
    }
}

/// the position of a line of words
#[derive(Clone, Copy)]
struct LinePos {
    /// y-position of line in the terminal window
    pub y: u16,
    /// x-position of the first char in the line
    pub x: u16,
    /// length (number of chars) in this line
    pub length: u16,
}

struct CursorPos {
    pub lines: Vec<LinePos>,
    pub cur_line: usize,
    pub cur_char_in_line: u16,
}

impl CursorPos {
    pub fn new() -> Self {
        Self {
            lines: Vec::new(),
            cur_line: 0,
            cur_char_in_line: 0,
        }
    }

    pub fn next(&mut self) -> (u16, u16) {
        let line = self.lines[self.cur_line];
        let max_chars_index = line.length - 1;

        if self.cur_char_in_line < max_chars_index {
            // more chars in line
            self.cur_char_in_line += 1;
        } else {
            // reached the end of line
            if self.cur_line + 1 < self.lines.len() {
                // more lines available
                self.cur_line += 1;
                self.cur_char_in_line = 0;
            }
        }

        self.cur_pos()
    }

    pub fn prev(&mut self) -> (u16, u16) {
        if self.cur_char_in_line > 0 {
            // more chars behind in line
            self.cur_char_in_line -= 1;
        } else {
            // reached the start of line
            if self.cur_line > 0 {
                // more lines available
                self.cur_line -= 1;
                self.cur_char_in_line = self.lines[self.cur_line].length - 1;
            }
        }

        self.cur_pos()
    }

    pub fn cur_pos(&self) -> (u16, u16) {
        let line = self.lines[self.cur_line];
        (line.x + self.cur_char_in_line, line.y)
    }
}

/// terminal UI of toipe
pub struct ToipeTui {
    stdout: RawTerminal<Stdout>,
    cursor_pos: CursorPos,
    bottom_lines_len: usize,
}

type MaybeError<T = ()> = Result<T>;

impl ToipeTui {
    /// Initializes stdout in raw mode for the TUI.
    ///
    /// NOTE: does not clear the screen when initialized.
    pub fn new() -> Self {
        Self {
            stdout: stdout().into_raw_mode().unwrap(),
            cursor_pos: CursorPos::new(),
            bottom_lines_len: 0,
        }
    }

    pub fn reset(&mut self) {
        self.cursor_pos = CursorPos::new();
    }

    /// Flushes stdout
    pub(crate) fn flush(&mut self) -> MaybeError {
        self.stdout.flush()?;
        Ok(())
    }

    /// Resets the TUI.
    ///
    /// Clears screen, moves cursor to the center and changes cursor to
    /// a blinking bar.
    pub fn reset_screen(&mut self) -> MaybeError {
        let (sizex, sizey) = terminal_size()?;

        write!(
            self.stdout,
            "{}{}{}",
            clear::All,
            cursor::Goto(sizex / 2, sizey / 2),
            cursor::BlinkingBar
        )?;
        self.flush()?;

        Ok(())
    }

    /// Displays a single line of text.
    ///
    /// - A line of text is described by a slice of [`Text`]s, they are
    ///   concatenated and displayed on the same line.
    ///
    /// - The line is displayed on whatever Y-position the cursor is
    ///   currently on.
    ///
    /// - The line is centered horizontally.
    pub fn display_a_line(&mut self, text: &[Text]) -> MaybeError {
        self.display_a_line_raw(text)?;
        self.flush()?;

        Ok(())
    }

    /// Same as [`display_a_line`] but without the flush.
    fn display_a_line_raw<T, U>(&mut self, text: U) -> MaybeError
    where
        U: AsRef<[T]>,
        [T]: HasLength,
        T: Display,
    {
        let len = text.as_ref().length() as u16;
        write!(self.stdout, "{}", cursor::Left(len / 2),)?;

        for t in text.as_ref() {
            self.display_raw_text(t)?;
        }
        write!(self.stdout, "{}", cursor::Left(len),)?;

        Ok(())
    }

    /// Displays multiple lines of text.
    ///
    /// - A line of text is described by a slice of [`Text`]s, they are
    ///   concatenated and displayed on the same line.
    ///
    /// - The lines are centered vertically and each line itself is
    ///   centered horizontally.
    // Ref for this generic thingy: https://stackoverflow.com/a/50056925/11199009
    pub fn display_lines<T, U>(&mut self, lines: &[T], track: bool) -> MaybeError
    where
        T: AsRef<[U]>,
        [U]: HasLength,
        U: Display,
    {
        let (sizex, sizey) = terminal_size()?;

        let line_offset = lines.len() as u16 / 2;

        for (line_no, line) in lines.iter().enumerate() {
            let y = sizey / 2 + (line_no as u16) - line_offset;
            write!(self.stdout, "{}", cursor::Goto(sizex / 2, y))?;
            if track {
                let len = line.as_ref().length() as u16;
                self.cursor_pos.lines.push(LinePos {
                    x: sizex / 2 - len / 2,
                    y,
                    length: len,
                });
            }
            self.display_a_line_raw(line.as_ref())?;
        }
        self.flush()?;

        Ok(())
    }

    /// Displays multiple lines of text at the bottom of the screen.
    ///
    /// See [`display_lines`] for more information.
    pub fn display_lines_bottom<T, U>(&mut self, lines: &[T]) -> MaybeError
    where
        T: AsRef<[U]>,
        [U]: HasLength,
        U: Display,
    {
        let (sizex, sizey) = terminal_size()?;

        let line_offset = lines.len() as u16;
        self.bottom_lines_len = lines.len();

        for (line_no, line) in lines.iter().enumerate() {
            write!(
                self.stdout,
                "{}",
                cursor::Goto(sizex / 2, sizey - 1 + (line_no as u16) - line_offset)
            )?;
            self.display_a_line_raw(line.as_ref())?;
        }
        self.flush()?;

        Ok(())
    }

    pub fn display_words(&mut self, words: &[String]) -> MaybeError<Vec<Text>> {
        self.reset();
        let (terminal_width, terminal_height) = terminal_size()?;
        let max_width = terminal_width * 2 / 5;

        let wrapped = wrap_words(words, max_width, MAX_WORDS_PER_LINE);
        let max_word_len = std::cmp::max(
            words.iter().map(|w| w.len() + 1).max().unwrap_or(0) + 1,
            MIN_LINE_WIDTH,
        );

        let lines: Vec<Text> = wrapped
            .iter()
            .enumerate()
            .map(|(i, line)| {
                let joined = line.join(" ");
                if i + 1 == wrapped.len() {
                    Text::from(joined).with_faint()
                } else {
                    Text::from(joined + " ").with_faint()
                }
            })
            .collect();

        if lines.len() + self.bottom_lines_len + 2 > terminal_height as usize {
            return Err(ToipeError::from(format!(
                "Terminal height is too short! Toipe requires at least {} lines, got {} lines",
                lines.len() + self.bottom_lines_len + 2,
                terminal_height,
            ))
            .into());
        } else if max_word_len > terminal_width as usize {
            return Err(ToipeError::from(format!(
                "Terminal width is too low! Toipe requires at least {} columns, got {} columns",
                max_word_len, terminal_width,
            ))
            .into());
        }

        self.display_lines(
            lines
                .iter()
                .cloned()
                .map(|line| [line])
                .collect::<Vec<[Text; 1]>>()
                .as_slice(),
            true,
        )?;

        self.move_to_cur_pos()?;
        self.flush()?;

        Ok(lines)
    }

    pub fn write_row(&mut self, y: u16, text: &str) -> MaybeError {
        write!(
            self.stdout,
            "{}{}{}",
            cursor::Goto(1, y),
            clear::CurrentLine,
            text
        )?;
        self.flush()?;
        Ok(())
    }

    pub fn jump_to_char(&mut self, index: usize) -> MaybeError {
        let mut remaining = index;
        let last = self.cursor_pos.lines.len().saturating_sub(1);
        for (i, line) in self.cursor_pos.lines.iter().enumerate() {
            let len = line.length as usize;
            if i == last || remaining < len {
                self.cursor_pos.cur_line = i;
                self.cursor_pos.cur_char_in_line = remaining.min(len.saturating_sub(1)) as u16;
                break;
            }
            remaining -= len;
        }
        let (x, y) = self.cursor_pos.cur_pos();
        write!(self.stdout, "{}", cursor::Goto(x, y))?;
        Ok(())
    }

    /// Displays a [`Text`].
    pub fn display_raw_text<T>(&mut self, text: &T) -> MaybeError
    where
        T: Display,
    {
        write!(self.stdout, "{}", text)?;
        Ok(())
    }

    /// Hides the cursor.
    pub fn hide_cursor(&mut self) -> MaybeError {
        write!(self.stdout, "{}", cursor::Hide)?;
        self.flush()?;
        Ok(())
    }

    /// Shows the cursor.
    pub fn show_cursor(&mut self) -> MaybeError {
        write!(self.stdout, "{}", cursor::Show)?;
        self.flush()?;
        Ok(())
    }

    pub fn replace_text(&mut self, text: Text) -> MaybeError {
        debug_assert_eq!(
            text.text().chars().count(),
            1,
            "replace_text expects a single character, got '{}'",
            text.text()
        );

        self.move_to_prev_char()?;
        self.display_raw_text(&text)?;
        self.move_to_cur_pos()?;

        Ok(())
    }

    /// Moves the cursor to the next char
    pub fn move_to_next_char(&mut self) -> MaybeError {
        let (x, y) = self.cursor_pos.next();
        write!(self.stdout, "{}", cursor::Goto(x, y))?;

        Ok(())
    }

    /// Moves the cursor to the previous char
    pub fn move_to_prev_char(&mut self) -> MaybeError {
        let (x, y) = self.cursor_pos.prev();
        write!(self.stdout, "{}", cursor::Goto(x, y))?;

        Ok(())
    }

    /// Moves the cursor to just before the character to be typed next
    pub fn move_to_cur_pos(&mut self) -> MaybeError {
        let (x, y) = self.cursor_pos.cur_pos();
        write!(self.stdout, "{}", cursor::Goto(x, y))?;

        Ok(())
    }

    /// Returns the current line the cursor is on
    pub fn current_line(&self) -> usize {
        self.cursor_pos.cur_line
    }
}

impl Default for ToipeTui {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for ToipeTui {
    /// Resets terminal.
    ///
    /// Clears screen and sets the cursor to a non-blinking block.
    ///
    /// TODO: print error message when terminal height/width is too small.
    /// Take a look at https://github.com/Samyak2/toipe/pull/28#discussion_r851784291 for more info.
    fn drop(&mut self) {
        write!(
            self.stdout,
            "{}{}{}",
            clear::All,
            cursor::SteadyBlock,
            cursor::Goto(1, 1)
        )
        .expect("Could not reset terminal while exiting");
        self.flush().expect("Could not flush stdout while exiting");
    }
}

#[cfg(test)]
mod tests {
    use super::{take_words_for_lines, wrap_words};

    fn words(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn wrap_obeys_width() {
        let lines = wrap_words(&words(&["a", "b", "c", "d"]), 4, 10);
        assert_eq!(
            lines,
            vec![
                vec!["a".to_string(), "b".to_string()],
                vec!["c".to_string(), "d".to_string()]
            ]
        );
    }

    #[test]
    fn wrap_respects_word_cap() {
        let lines = wrap_words(&words(&["a", "b", "c", "d"]), 1000, 2);
        assert_eq!(
            lines,
            vec![
                vec!["a".to_string(), "b".to_string()],
                vec!["c".to_string(), "d".to_string()]
            ]
        );
    }

    #[test]
    fn take_words_counts_lines() {
        let ws = words(&["a", "b", "c", "d", "e"]);
        assert_eq!(take_words_for_lines(&ws, 4, 10, 2), 4);
        assert_eq!(take_words_for_lines(&ws, 4, 10, 1), 2);
        assert_eq!(take_words_for_lines(&ws, 1000, 10, 3), 5);
    }
}
