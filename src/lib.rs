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

pub use self::vi::*;
mod vi;

#[derive(Clone, Debug)]
pub enum Event {
    /// Automatically indent
    AutoIndent,
    /// Delete text behind cursor
    Backspace,
    /// Copy to clipboard (TODO: multiple clipboards?)
    Copy,
    /// Delete text in front of cursor
    Delete,
    /// Escape key
    Escape,
    /// Insert character at cursor
    Insert(char),
    /// Move cursor
    Motion(Motion),
    /// Create new line
    NewLine,
    /// Paste from clipboard (TODO: multiple clipboards?)
    Paste,
    /// Notify of a mode change requiring redraw
    Redraw,
    /// Clear selection
    SelectClear,
    /// Start selection
    SelectStart,
    /// Select text object
    SelectTextObject(TextObject, bool),
    /// Set search
    SetSearch(String, bool),
    /// Shift text to the left
    ShiftLeft,
    /// Shift text to the right
    ShiftRight,
    /// Swap case
    SwapCase,
    /// Undo last action
    Undo,
}

#[derive(Clone, Copy, Debug)]
pub enum Key {
    //TODO: Home, End, Page Up, Page Down, etc.
    Backspace,
    Char(char),
    Delete,
    Down,
    Enter,
    Escape,
    Left,
    Right,
    Tab,
    Up,
}

impl Key {
    /// Normalize so that Char('\n') is converted to Enter, for example
    pub fn normalize(self) -> Self {
        match self {
            Key::Char(c) => match c {
                '\x08' => Key::Backspace,
                '\x7F' => Key::Delete,
                '\n' | '\r' => Key::Enter,
                '\x1B' => Key::Escape,
                '\t' => Key::Tab,
                _ => Key::Char(c),
            },
            key => key,
        }
    }
}

pub trait Parser {
    fn reset(&mut self);
    fn parse<F: FnMut(Event)>(&mut self, key: Key, selection: bool, f: F);
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Operator {
    AutoIndent,
    Change,
    Delete,
    ShiftLeft,
    ShiftRight,
    SwapCase,
    Yank,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Word {
    Lower,
    Upper,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WordChar {
    Blank,
    Keyword,
    NonBlank,
}

#[derive(Debug)]
pub struct WordIter<'a> {
    line: &'a str,
    word: Word,
    index: usize,
}

impl<'a> WordIter<'a> {
    pub fn new(line: &'a str, word: Word) -> Self {
        Self {
            line,
            word,
            index: 0,
        }
    }
}

impl<'a> Iterator for WordIter<'a> {
    type Item = (usize, &'a str);

    fn next(&mut self) -> Option<Self::Item> {
        let mut last_kind = WordChar::Blank;
        let mut start_opt = None;
        let mut end_opt = None;
        for (sub_index, c) in self.line.get(self.index..)?.char_indices() {
            let index = self.index.checked_add(sub_index)?;

            let kind = match self.word {
                Word::Lower => {
                    // A "word" is either a group of letters, digits, and underscores,
                    // or a sequence of other non-blank characters
                    if c.is_whitespace() {
                        WordChar::Blank
                    } else if c.is_alphanumeric() || c == '_' {
                        WordChar::Keyword
                    } else {
                        WordChar::NonBlank
                    }
                }
                Word::Upper => {
                    if c.is_whitespace() {
                        WordChar::Blank
                    } else {
                        WordChar::NonBlank
                    }
                }
            };

            if kind != last_kind {
                // Word either starts or ends
                match kind {
                    WordChar::Blank => {
                        end_opt = Some(index);
                        break;
                    }
                    _ => {
                        if start_opt.is_some() {
                            end_opt = Some(index);
                            break;
                        } else {
                            start_opt = Some(index);
                        }
                    }
                }
                last_kind = kind;
            }
        }

        match start_opt {
            Some(start) => {
                let end = end_opt.unwrap_or(self.line.len());
                self.index = end;
                let word = self.line.get(start..end)?;
                Some((start, word))
            }
            None => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Motion {
    Around,
    Down,
    End,
    GotoEof,
    GotoLine(usize),
    Home,
    Inside,
    Left,
    Line,
    NextChar(char),
    NextCharTill(char),
    NextSearch,
    NextWordEnd(Word),
    NextWordStart(Word),
    PreviousChar(char),
    PreviousCharTill(char),
    PreviousSearch,
    PreviousWordEnd(Word),
    PreviousWordStart(Word),
    Right,
    ScreenHigh,
    ScreenLow,
    ScreenMiddle,
    Selection,
    SoftHome,
    Up,
}

impl Motion {
    /// Returns true if text object is needed
    pub fn text_object(&self) -> bool {
        match self {
            Self::Around | Self::Inside => true,
            _ => false,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextObject {
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
    Word(Word),
}
