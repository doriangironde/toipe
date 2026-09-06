//! Toipe is a terminal-based typing test application.
//!
//! Please see the [README](https://github.com/Samyak2/toipe/) for
//! installation and usage instructions.
//!
//! Toipe provides an API to invoke it from another application or
//! library. This documentation describes the API and algorithms used
//! internally.
//!
//! See [`RawWordSelector`] if you're looking for the word selection
//! algorithm.

pub mod config;
pub mod results;
pub mod stats;
pub mod textgen;
pub mod tui;
pub mod wordlists;

use std::io::stdin;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{Duration, Instant};

use config::ToipeConfig;
use results::ToipeResults;
use termion::terminal_size;
use termion::{color, event::Key, input::TermRead};
use textgen::{PunctuatedWordSelector, RawWordSelector, WordSelector};
use tui::{Text, ToipeTui};
use wordlists::{BuiltInWordlist, OS_WORDLIST_PATH};

use anyhow::{Context, Result};

/// Typing test terminal UI and logic.
pub struct Toipe {
    tui: ToipeTui,
    words: Vec<String>,
    word_selector: Box<dyn WordSelector>,
    config: ToipeConfig,
    rx: Rc<Receiver<Key>>,
    window_start: usize,
    window_size: usize,
}

/// Represents any error caught in Toipe.
#[derive(Debug)]
pub struct ToipeError {
    /// Error message. Should not start with "error" or similar.
    pub msg: String,
}

impl ToipeError {
    /// Prefixes the message with a context
    pub fn with_context(mut self, context: &str) -> Self {
        self.msg = context.to_owned() + &self.msg;
        self
    }
}

impl From<String> for ToipeError {
    fn from(error: String) -> Self {
        ToipeError { msg: error }
    }
}

impl std::fmt::Display for ToipeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(format!("ToipeError: {}", self.msg).as_str())
    }
}

impl std::error::Error for ToipeError {}

fn word_offsets(words: &[String]) -> Vec<usize> {
    let mut offs = Vec::with_capacity(words.len() + 1);
    let mut acc = 0;
    offs.push(0);
    for w in words {
        acc += w.len() + 1;
        offs.push(acc);
    }
    offs
}

enum TestStatus {
    NotDone,
    Done,
    Quit,
    Restart,
}

impl TestStatus {
    fn to_process_more_keys(&self) -> bool {
        matches!(self, TestStatus::NotDone)
    }

    fn to_display_results(&self) -> bool {
        matches!(self, TestStatus::Done)
    }

    fn to_restart(&self) -> bool {
        matches!(self, TestStatus::Restart)
    }
}

struct TestState {
    input: Vec<char>,
    num_errors: usize,
    num_chars_typed: usize,
}

impl Toipe {
    /// Initializes a new typing test on the standard output.
    ///
    /// See [`ToipeConfig`] for configuration options.
    ///
    /// Initializes the word selector.
    /// Also invokes [`Toipe::restart()`].
    pub fn new(config: ToipeConfig) -> Result<Self> {
        let mut word_selector: Box<dyn WordSelector> = if let Some(wordlist_path) =
            config.wordlist_file.clone()
        {
            Box::new(
                RawWordSelector::from_path(PathBuf::from(wordlist_path.clone())).with_context(
                    || format!("reading the word list from given path '{}'", wordlist_path),
                )?,
            )
        } else if let Some(word_list) = config.wordlist.contents() {
            Box::new(
                RawWordSelector::from_string(word_list.to_string()).with_context(|| {
                    format!("reading the built-in word list {:?}", config.wordlist)
                })?,
            )
        } else if let BuiltInWordlist::OS = config.wordlist {
            Box::new(
                RawWordSelector::from_path(PathBuf::from(OS_WORDLIST_PATH)).with_context(|| {
                    format!(
                        "reading from the OS wordlist at path '{}'. See https://en.wikipedia.org/wiki/Words_(Unix) for more info on this file and how it can be installed.",
                        OS_WORDLIST_PATH
                    )
                })?,
            )
        } else {
            // this should never happen!
            // TODO: somehow enforce this at compile time?
            return Err(ToipeError::from("Undefined word list or path.".to_owned()))?;
        };

        if config.punctuation {
            word_selector = Box::new(PunctuatedWordSelector::from_word_selector(
                word_selector,
                0.15,
            ))
        }

        let tui = ToipeTui::new();
        let (tx, rx) = mpsc::channel();
        let rx = Rc::new(rx);
        std::thread::spawn(move || {
            for key in stdin().lock().keys().flatten() {
                if tx.send(key).is_err() {
                    break;
                }
            }
        });

        let mut toipe = Toipe {
            tui,
            words: Vec::new(),
            word_selector,
            config,
            rx,
            window_start: 0,
            window_size: 0,
        };

        toipe.restart()?;

        Ok(toipe)
    }

    /// Make the terminal ready for the next typing test.
    ///
    /// Clears the screen, generates new words and displays them on the
    /// UI.
    pub fn restart(&mut self) -> Result<()> {
        self.tui.reset_screen()?;

        let word_count = match self.config.time {
            Some(t) => (t * 3).max(30) as usize,
            None => self.config.num_words,
        };
        self.words = self.word_selector.new_words(word_count)?;

        self.tui.display_lines_bottom(&[&[
            Text::from("ctrl-r").with_color(color::Blue),
            Text::from(" to restart, ").with_faint(),
            Text::from("ctrl-c").with_color(color::Blue),
            Text::from(" to quit ").with_faint(),
        ]])?;

        self.window_start = 0;
        self.window_size = match self.config.time {
            Some(_) => {
                let (width, height) = terminal_size()?;
                let max_lines = height.saturating_sub(3).max(2) as usize;
                let m = tui::take_words_for_lines(&self.words, width * 2 / 5, 10, max_lines);
                m.max(1).min(self.words.len())
            }
            None => self.words.len(),
        };

        self.render_words(&[])?;

        Ok(())
    }

    fn render_words(&mut self, input: &[char]) -> Result<()> {
        let end = (self.window_start + self.window_size).min(self.words.len());
        let chunk = &self.words[self.window_start..end];
        let off = word_offsets(&self.words)[self.window_start];
        self.tui.display_words(chunk)?;
        let flat: Vec<char> = chunk.join(" ").chars().collect();
        let n = input.len().saturating_sub(off).min(flat.len());
        for k in 0..n {
            let c = flat[k];
            let text = if input[off + k] == c {
                Text::from(c).with_color(color::LightGreen)
            } else {
                Text::from(c).with_underline().with_color(color::Red)
            };
            self.tui.display_raw_text(&text)?;
            self.tui.move_to_next_char()?;
        }
        self.tui.jump_to_char(n)?;
        self.tui.flush()?;
        Ok(())
    }

    fn ensure_window(&mut self, input: &[char]) -> Result<()> {
        if self.config.time.is_none() || self.words.is_empty() {
            return Ok(());
        }
        let offs = word_offsets(&self.words);
        let chunk_end_idx = (self.window_start + self.window_size).min(self.words.len());
        let i = input.len();
        if i >= offs[chunk_end_idx] && chunk_end_idx < self.words.len() {
            self.window_start = chunk_end_idx;
            self.render_words(input)?;
        } else if i < offs[self.window_start] && self.window_start > 0 {
            self.window_start = self.window_start.saturating_sub(self.window_size);
            self.render_words(input)?;
        }
        Ok(())
    }

    fn show_live_stats(
        &mut self,
        input: &[char],
        flat: &[char],
        started_at: Instant,
    ) -> Result<()> {
        let typed = input.len();
        let correct = input
            .iter()
            .zip(flat.iter())
            .filter(|(a, b)| a == b)
            .count();
        let elapsed = started_at.elapsed().as_secs_f64().max(0.001);
        let wpm = correct as f64 / 5.0 / (elapsed / 60.0);
        let acc = if typed > 0 {
            correct as f64 / typed as f64
        } else {
            1.0
        };
        let mut s = format!(
            "{:.0} wpm  {:.0}%  {}/{}",
            wpm,
            acc * 100.0,
            typed,
            flat.len()
        );
        if let Some(limit) = self.config.time {
            let left = limit.saturating_sub(started_at.elapsed().as_secs());
            s.push_str(&format!("  {}s left", left));
        }
        let (_, sizey) = terminal_size()?;
        self.tui.write_row(sizey - 1, &s)?;
        Ok(())
    }

    fn process_key(
        &mut self,
        key: Key,
        state: &mut TestState,
        flat: &[char],
        total_chars: usize,
        started_at: Instant,
    ) -> Result<TestStatus> {
        let input = &mut state.input;
        match key {
            Key::Ctrl('c') => {
                return Ok(TestStatus::Quit);
            }
            Key::Ctrl('r') => {
                return Ok(TestStatus::Restart);
            }
            Key::Ctrl('w') => {
                // delete last word
                while !matches!(input.last(), Some(' ') | None) {
                    if input.pop().is_some() {
                        self.tui
                            .replace_text(Text::from(flat[input.len()]).with_faint())?;
                    }
                }
            }
            Key::Char(c) => {
                state.num_chars_typed += 1;
                input.push(c);

                if input.len() >= total_chars {
                    return Ok(TestStatus::Done);
                }

                if flat[input.len() - 1] == c {
                    self.tui
                        .display_raw_text(&Text::from(c).with_color(color::LightGreen))?;
                    self.tui.move_to_next_char()?;
                } else {
                    self.tui.display_raw_text(
                        &Text::from(flat[input.len() - 1])
                            .with_underline()
                            .with_color(color::Red),
                    )?;
                    self.tui.move_to_next_char()?;
                    state.num_errors += 1;
                }
            }
            Key::Backspace | Key::Ctrl('h') if input.pop().is_some() => {
                self.tui
                    .replace_text(Text::from(flat[input.len()]).with_faint())?;
            }
            _ => {}
        }

        self.ensure_window(&state.input)?;
        self.show_live_stats(&state.input, flat, started_at)?;
        self.tui.flush()?;

        Ok(TestStatus::NotDone)
    }

    /// Start typing test by monitoring input keys.
    ///
    /// Must only be invoked after [`Toipe::restart()`].
    ///
    /// If the test completes successfully, returns a boolean indicating
    /// whether the user wants to do another test and the
    /// [`ToipeResults`] for this test.
    pub fn test(&mut self) -> Result<(bool, ToipeResults)> {
        let rx = Rc::clone(&self.rx);
        let flat: Vec<char> = self.words.join(" ").chars().collect();
        let total_chars = flat.len();
        let mut state = TestState {
            input: Vec::new(),
            num_errors: 0,
            num_chars_typed: 0,
        };

        let first_key = rx.recv()?;
        let started_at = Instant::now();

        let mut status = self.process_key(first_key, &mut state, &flat, total_chars, started_at)?;

        if status.to_process_more_keys() {
            match self.config.time {
                Some(t) => {
                    let duration = Duration::from_secs(t);
                    loop {
                        match rx.try_recv() {
                            Ok(key) => {
                                status = self.process_key(
                                    key,
                                    &mut state,
                                    &flat,
                                    total_chars,
                                    started_at,
                                )?;
                                if !status.to_process_more_keys() {
                                    break;
                                }
                            }
                            Err(TryRecvError::Empty) => {
                                if started_at.elapsed() >= duration {
                                    status = TestStatus::Done;
                                    break;
                                }
                                std::thread::sleep(Duration::from_millis(2));
                            }
                            Err(TryRecvError::Disconnected) => {
                                status = TestStatus::Quit;
                                break;
                            }
                        }
                    }
                }
                None => {
                    for key in rx.iter() {
                        status =
                            self.process_key(key, &mut state, &flat, total_chars, started_at)?;
                        if !status.to_process_more_keys() {
                            break;
                        }
                    }
                }
            }
        }

        let ended_at = Instant::now();

        let (final_chars_typed_correctly, final_uncorrected_errors) =
            state.input.iter().zip(flat.iter()).fold(
                (0, 0),
                |(total_chars_typed_correctly, total_uncorrected_errors),
                 (typed_char, orig_char)| {
                    if typed_char == orig_char {
                        (total_chars_typed_correctly + 1, total_uncorrected_errors)
                    } else {
                        (total_chars_typed_correctly, total_uncorrected_errors + 1)
                    }
                },
            );

        let results = ToipeResults {
            total_words: self.words.len(),
            total_chars_typed: state.num_chars_typed,
            total_chars_in_text: state.input.len(),
            total_char_errors: state.num_errors,
            final_chars_typed_correctly,
            final_uncorrected_errors,
            started_at,
            ended_at,
        };

        let to_restart = if status.to_display_results() {
            self.display_results(results.clone(), &state.input, &rx)?
        } else {
            status.to_restart()
        };

        Ok((to_restart, results))
    }

    fn display_results(
        &mut self,
        results: ToipeResults,
        input: &[char],
        rx: &Receiver<Key>,
    ) -> Result<bool> {
        self.tui.reset_screen()?;

        let records = stats::load();
        let best = stats::best_wpm(&records);
        let is_pb = best.is_none_or(|b| results.wpm() > b + 1e-9);

        let headline = match self.config.time {
            Some(t) => format!(
                "Took {}s of a {}s test ({})",
                results.duration().as_secs(),
                t,
                self.config.text_name()
            ),
            None => format!(
                "Took {}s for {} words of {}",
                results.duration().as_secs(),
                results.total_words,
                self.config.text_name()
            ),
        };

        let mut lines: Vec<Vec<Text>> = vec![
            vec![Text::from(headline)],
            vec![
                Text::from(format!("Accuracy: {:.1}%", results.accuracy() * 100.0))
                    .with_color(color::Blue),
            ],
            vec![Text::from(format!(
                "Mistakes: {} out of {} characters",
                results.total_char_errors, results.total_chars_in_text
            ))],
            vec![
                Text::from("Speed: "),
                Text::from(format!("{:.1} wpm", results.wpm())).with_color(color::Green),
                Text::from(" (words per minute)"),
            ],
            vec![Text::from(format!("Raw: {:.1} wpm", results.raw_wpm()))],
        ];

        match (best, is_pb) {
            (_, true) => lines.push(vec![Text::from(format!(
                "New personal best: {:.1} wpm",
                results.wpm()
            ))
            .with_color(color::Green)]),
            (Some(b), false) => lines.push(vec![Text::from(format!("Best: {:.1} wpm", b))]),
            (None, false) => {}
        }

        let record = stats::TestRecord {
            ts: stats::now_ts(),
            wpm: results.wpm(),
            accuracy: results.accuracy(),
            duration_ms: results.duration().as_millis() as u64,
            words: results.total_words,
            chars: results.total_chars_in_text,
            errors: results.total_char_errors,
            punct: self.config.punctuation,
        };
        let _ = stats::append_record(&record);

        let offs = word_offsets(&self.words);
        let end = (self.window_start + self.window_size).min(self.words.len());
        let chunk = &self.words[self.window_start..end];
        let off = offs[self.window_start];

        let (width, height) = terminal_size()?;
        let max_width = width * 2 / 5;
        let heatmap_words = &chunk[..tui::take_words_for_lines(
            chunk,
            max_width,
            10,
            height.saturating_sub(8).max(2) as usize,
        )];
        let flat: Vec<char> = heatmap_words.join(" ").chars().collect();

        let mut idx = 0;
        let mut heatmap: Vec<Vec<Text>> = Vec::new();
        for line in tui::wrap_words(heatmap_words, max_width, 10) {
            let mut texts = Vec::new();
            for w in line {
                for c in w.chars() {
                    let g = off + idx;
                    let t = if g < input.len() {
                        if input[g] == c {
                            Text::from(c).with_color(color::LightGreen)
                        } else {
                            Text::from(c).with_underline().with_color(color::Red)
                        }
                    } else {
                        Text::from(c).with_faint()
                    };
                    texts.push(t);
                    idx += 1;
                }
                if idx < flat.len() {
                    let g = off + idx;
                    let t = if g < input.len() {
                        if input[g] == ' ' {
                            Text::from(' ').with_color(color::LightGreen)
                        } else {
                            Text::from(' ').with_underline().with_color(color::Red)
                        }
                    } else {
                        Text::from(' ').with_faint()
                    };
                    texts.push(t);
                    idx += 1;
                }
            }
            heatmap.push(texts);
        }

        let slice_refs: Vec<&[Text]> = lines
            .iter()
            .chain(heatmap.iter())
            .map(|l| l.as_slice())
            .collect();
        self.tui.display_lines::<&[Text], _>(&slice_refs, false)?;

        self.tui.display_lines_bottom(&[&[
            Text::from("ctrl-r").with_color(color::Blue),
            Text::from(" to restart, ").with_faint(),
            Text::from("ctrl-c").with_color(color::Blue),
            Text::from(" to quit ").with_faint(),
        ]])?;
        // no cursor on results page
        self.tui.hide_cursor()?;

        let mut to_restart: Option<bool> = None;
        while to_restart.is_none() {
            match rx.recv() {
                // press ctrl + 'r' to restart
                Ok(Key::Ctrl('r')) => to_restart = Some(true),
                // press ctrl + 'c' to quit
                Ok(Key::Ctrl('c')) => to_restart = Some(false),
                Ok(_) => {}
                Err(_) => to_restart = Some(false),
            }
        }

        self.tui.show_cursor()?;

        Ok(to_restart.unwrap_or(false))
    }
}

#[cfg(test)]
mod tests {
    use super::word_offsets;

    #[test]
    fn offsets() {
        let words = vec!["ab".to_string(), "c".to_string(), "def".to_string()];
        let offs = word_offsets(&words);
        assert_eq!(offs, vec![0, 3, 5, 9]);
        let flat: Vec<char> = words.join(" ").chars().collect();
        assert_eq!(flat.len(), 8);
        assert_eq!(flat[offs[1]], 'c');
        assert_eq!(flat[offs[2]], 'd');
    }
}
