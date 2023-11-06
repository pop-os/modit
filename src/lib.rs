// SPDX-License-Identifier: MIT OR Apache-2.0

// Not interested in these lints
#![allow(clippy::new_without_default)]
//
// Soundness issues
//
// Overflows can produce unpredictable results and are only checked in debug builds
#![deny(clippy::arithmetic_side_effects)]
// Dereferencing unaligned pointers may be undefined behavior
#![deny(clippy::cast_ptr_alignment)]
// Indexing a slice can cause panics and that is something we always want to avoid
#![deny(clippy::indexing_slicing)]
// Avoid panicking in without information about the panic. Use expect
#![deny(clippy::unwrap_used)]
// Ensure all types have a debug impl
#![deny(missing_debug_implementations)]
// This is usually a serious issue - a missing import of a define where it is interpreted
// as a catch-all variable in a match, for example
#![deny(unreachable_patterns)]
// Ensure that all must_use results are used
#![deny(unused_must_use)]
//
// Style issues
//
// Documentation not ideal
#![warn(clippy::doc_markdown)]
// Document possible errors
#![warn(clippy::missing_errors_doc)]
// Document possible panics
#![warn(clippy::missing_panics_doc)]
// Ensure semicolons are present
#![warn(clippy::semicolon_if_nothing_returned)]
// Ensure numbers are readable
#![warn(clippy::unreadable_literal)]
// no_std support
#![no_std]

extern crate alloc;

use alloc::string::String;

pub const BACKSPACE: char = '\x08';
pub const ESCAPE: char = '\x1b';
pub const ENTER: char = '\n';
pub const TAB: char = '\t';

#[derive(Debug)]
pub enum Event {
    /// Move cursor left
    Left,
    /// Move cursor left (if possible)
    LeftIfPossible,
    /// Move cursor right
    Right,
    /// Move cursor up
    Up,
    /// Move cursor down
    Down,
    /// Move cursor to start of line
    Home,
    /// Move cursor to start of line, skipping whitespace
    SoftHome,
    /// Move cursor to end of line
    End,
    // Go to the top of the screen
    ScreenHigh,
    // Go to the middle of the screen
    ScreenMiddle,
    // Go to the bottom of the screen
    ScreenLow,
    /// Escape, clears selection
    Escape,
    /// Insert character at cursor
    Insert(char),
    /// Create new line
    NewLine,
    /// Delete text behind cursor
    Backspace,
    /// Delete text in front of cursor
    Delete,
    // Indent text (typically Tab)
    Indent,
    // Unindent text (typically Shift+Tab)
    Unindent,
    /// Move cursor to previous word boundary
    PreviousWord,
    /// Move cursor to next word boundary
    NextWord,
    /// Go to previous search item
    PreviousSearch,
    /// Go to next search item
    NextSearch,
    /// Go to end of file
    GotoEof,
    /// Got to specified line
    GotoLine(u32),
    /// Copy to clipboard (TODO: multiple clipboards?)
    Copy,
    /// Paste from clipboard (TODO: multiple clipboards?)
    Paste,
    /// Undo last action
    Undo,
}

pub trait Parser {
    fn reset(&mut self);
    fn parse<F: FnMut(Event)>(&mut self, c: char, selection: bool, f: F);
}

#[derive(Clone, Copy, Debug)]
pub enum ViOperator {
    AutoIndent,
    Change,
    Delete,
    ShiftLeft,
    ShiftRight,
    SwapCase,
    Yank,
}

impl TryFrom<char> for ViOperator {
    type Error = char;

    fn try_from(c: char) -> Result<Self, char> {
        match c {
            '=' => Ok(Self::AutoIndent),
            'c' => Ok(Self::Change),
            'd' => Ok(Self::Delete),
            '<' => Ok(Self::ShiftLeft),
            '>' => Ok(Self::ShiftRight),
            'y' => Ok(Self::Yank),
            '~' => Ok(Self::SwapCase),
            _ => Err(c),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum ViTextObject {
    AngleBrackets,
    Block,
    CurlyBrackets,
    DoubleQuotes,
    Paragraph,
    Parentheses,
    Sentence,
    SingleQuotes,
    SquareBrackets,
    Tag,
    Ticks,
    Word,
}

impl TryFrom<char> for ViTextObject {
    type Error = char;

    fn try_from(c: char) -> Result<Self, char> {
        match c {
            '<' | '>' => Ok(Self::AngleBrackets),
            //TODO: should B be different?
            'b' | 'B' => Ok(Self::Block),
            '{' | '}' => Ok(Self::CurlyBrackets),
            '"' => Ok(Self::DoubleQuotes),
            'p' => Ok(Self::Paragraph),
            '(' | ')' => Ok(Self::Parentheses),
            's' => Ok(Self::Sentence),
            '\'' => Ok(Self::SingleQuotes),
            '[' | ']' => Ok(Self::SquareBrackets),
            't' => Ok(Self::Tag),
            '`' => Ok(Self::Ticks),
            'w' => Ok(Self::Word),
            _ => Err(c),
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ViNormal {
    count: Option<u32>,
    op: Option<ViOperator>,
}

impl ViNormal {
    /// Repeat the provided function count times, resetting count after
    pub fn repeat<F: FnMut(u32)>(&mut self, mut f: F) {
        for i in 0..self.count.take().unwrap_or(1) {
            f(i);
        }
    }

    /// Run operation, resetting it to defaults
    pub fn run<F: FnMut(Event)>(&mut self, mut f: F) {
        let count = self.count.take().unwrap_or(1);
        let op = self.op.take();
        match op {
            Some(_) => {
                //TODO: handle ops
            }
            None => {
                // Just a move
            }
        }
    }
}

#[derive(Debug)]
pub enum ViMode {
    /// Normal mode
    Normal(ViNormal),
    /// Insert mode
    Insert,
    /// Command mode
    Command { value: String },
    /// Search mode
    Search { value: String, forwards: bool },
}

impl ViMode {
    // Default normal state
    pub fn normal() -> Self {
        Self::Normal(ViNormal::default())
    }
}

#[derive(Debug)]
pub struct ViParser {
    pub mode: ViMode,
}

impl ViParser {
    pub fn new() -> Self {
        Self {
            mode: ViMode::normal(),
        }
    }
}

impl Parser for ViParser {
    fn reset(&mut self) {
        self.mode = ViMode::normal();
    }

    fn parse<F: FnMut(Event)>(&mut self, c: char, selection: bool, mut f: F) {
        match self.mode {
            ViMode::Normal(ref mut normal) => match c {
                // Enter insert mode after cursor
                'a' => {
                    f(Event::Right);
                    self.mode = ViMode::Insert;
                }
                // Enter insert mode at end of line
                'A' => {
                    f(Event::End);
                    self.mode = ViMode::Insert;
                }
                // Previous word
                'b' => {
                    //TODO: WORD vs word, iterate by vi word rules
                    normal.repeat(|_| f(Event::PreviousWord));
                }
                // Previous WORD
                'B' => {
                    //TODO: WORD vs word, iterate by vi word rules
                    normal.repeat(|_| f(Event::PreviousWord));
                }
                // Change mode
                'c' => {
                    /*TODO
                    if self.editor.select_opt().is_some() {
                        f(Event::Delete);
                        self.mode = ViMode::Insert;
                    } else {
                        //TODO: change to next cursor movement
                    }
                    */
                }
                //TODO: Change to end of line
                'C' => {}
                // Delete mode
                'd' => {
                    /*TODO
                    if self.editor.select_opt().is_some() {
                        f(Event::Delete);
                    } else {
                        //TODO: delete to next cursor movement
                    }
                    */
                }
                //TODO: Delete to end of line
                'D' => {}
                // End of word
                'e' => {
                    //TODO: WORD vs word, iterate by vi word rules
                    normal.repeat(|_| f(Event::NextWord));
                }
                // End of WORD
                'E' => {
                    //TODO: WORD vs word, iterate by vi word rules
                    normal.repeat(|_| f(Event::NextWord));
                }
                //TODO: Find char forwards
                'f' => {}
                //TODO: Find char backwords
                'F' => {}
                //TODO: Extra commands
                'g' => {}
                // Goto line (or end of file)
                'G' => match normal.count.take() {
                    Some(line) => f(Event::GotoLine(line)),
                    None => f(Event::GotoEof),
                },
                // Left
                'h' | BACKSPACE => normal.repeat(|_| f(Event::Left)),
                // Top of screen
                'H' => f(Event::ScreenHigh),
                // Enter insert mode at cursor
                'i' => {
                    self.mode = ViMode::Insert;
                }
                // Enter insert mode at start of line
                'I' => {
                    f(Event::SoftHome);
                    self.mode = ViMode::Insert;
                }
                // Down
                'j' => normal.repeat(|_| f(Event::Down)),
                //TODO: Join lines
                'J' => {}
                // Up
                'k' => normal.repeat(|_| f(Event::Up)),
                //TODO: Look up keyword (vim looks up word under cursor in man pages)
                'K' => {}
                // Right
                'l' | ' ' => normal.repeat(|_| f(Event::Right)),
                // Bottom of screen
                'L' => f(Event::ScreenLow),
                //TODO: Set mark
                'm' => {}
                // Middle of screen
                'M' => f(Event::ScreenMiddle),
                // Next search item
                'n' => normal.repeat(|_| f(Event::NextSearch)),
                // Previous search item
                'N' => normal.repeat(|_| f(Event::PreviousSearch)),
                // Create line after and enter insert mode
                'o' => {
                    f(Event::End);
                    f(Event::NewLine);
                    self.mode = ViMode::Insert;
                }
                // Create line before and enter insert mode
                'O' => {
                    f(Event::Home);
                    f(Event::NewLine);
                    f(Event::Up);
                    self.mode = ViMode::Insert;
                }
                // Paste after
                'p' => {
                    f(Event::Right);
                    f(Event::Paste);
                }
                // Paste before
                'P' => {
                    f(Event::Paste);
                }
                //TODO: q, Q
                //TODO: Replace char
                'r' => {}
                //TODO: Replace mode
                'R' => {}
                // Substitute char
                's' => {
                    normal.repeat(|_| f(Event::Delete));
                    self.mode = ViMode::Insert;
                }
                //TODO: Substitute line
                'S' => {}
                //TODO: Until character forwards
                't' => {}
                //TODO: Until character backwards
                'T' => {}
                // Undo
                'u' => {
                    f(Event::Undo);
                }
                //TODO: U
                // Enter visual mode
                'v' => {
                    /*TODO
                    if self.editor.select_opt().is_some() {
                        self.editor.set_select_opt(None);
                    } else {
                        self.editor.set_select_opt(Some(self.editor.cursor()));
                    }
                    */
                }
                // Enter line visual mode
                'V' => {
                    /*TODO
                    if self.editor.select_opt().is_some() {
                        self.editor.set_select_opt(None);
                    } else {
                        f(Event::Home);
                        self.editor.set_select_opt(Some(self.editor.cursor()));
                        //TODO: set cursor_x_opt to max
                        f(Event::End);
                    }
                    */
                }
                // Next word
                'w' => {
                    //TODO: WORD vs word, iterate by vi word rules
                    normal.repeat(|_| f(Event::NextWord));
                }
                // Next WORD
                'W' => {
                    //TODO: WORD vs word, iterate by vi word rules
                    normal.repeat(|_| f(Event::NextWord));
                }
                // Remove character at cursor
                'x' => normal.repeat(|_| f(Event::Delete)),
                // Remove character before cursor
                'X' => normal.repeat(|_| f(Event::Backspace)),
                // Yank
                'y' => f(Event::Copy),
                //TODO: Yank line
                'Y' => {}
                //TODO: z, Z
                // Go to start of line
                '0' => f(Event::Home),
                // Count of next action
                '1'..='9' => {
                    let number = (c as u32).saturating_sub('0' as u32);
                    normal.count = Some(match normal.count.take() {
                        Some(mut count) => count.saturating_mul(10).saturating_add(number),
                        None => number,
                    });
                }
                // Go to end of line
                '$' => f(Event::End),
                // Go to start of line after whitespace
                '^' => f(Event::SoftHome),
                // Enter command mode
                ':' => {
                    self.mode = ViMode::Command {
                        value: String::new(),
                    };
                }
                // Indent
                '>' => {
                    //TODO: selection
                    normal.repeat(|_| f(Event::Indent));
                }
                // Unindent
                '<' => {
                    //TODO: selection
                    normal.repeat(|_| f(Event::Unindent));
                }
                // Enter search mode
                '/' => {
                    self.mode = ViMode::Search {
                        value: String::new(),
                        forwards: true,
                    };
                }
                // Enter search backwards mode
                '?' => {
                    self.mode = ViMode::Search {
                        value: String::new(),
                        forwards: false,
                    };
                }
                ENTER => {
                    normal.repeat(|_| f(Event::Down));
                    f(Event::SoftHome);
                }
                ESCAPE => f(Event::Escape),
                _ => (),
            },
            ViMode::Insert => match c {
                ESCAPE => {
                    f(Event::LeftIfPossible);
                    self.mode = ViMode::normal();
                }
                _ => f(Event::Insert(c)),
            },
            ViMode::Command { ref mut value } => match c {
                ESCAPE => {
                    self.mode = ViMode::normal();
                }
                ENTER => {
                    //TODO: run command
                    self.mode = ViMode::normal();
                }
                BACKSPACE => {
                    if value.pop().is_none() {
                        self.mode = ViMode::normal();
                    }
                }
                _ => {
                    value.push(c);
                }
            },
            ViMode::Search {
                ref mut value,
                forwards,
            } => match c {
                ESCAPE => {
                    self.mode = ViMode::normal();
                }
                ENTER => {
                    //TODO: run search
                    self.mode = ViMode::normal();
                }
                BACKSPACE => {
                    if value.pop().is_none() {
                        self.mode = ViMode::normal();
                    }
                }
                _ => {
                    value.push(c);
                }
            },
        }
    }
}
