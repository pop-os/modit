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
pub const DELETE: char = '\x7F';
pub const ESCAPE: char = '\x1B';
pub const ENTER: char = '\n';
pub const TAB: char = '\t';

#[derive(Clone, Copy, Debug)]
pub enum Event {
    /// Move cursor left
    Left,
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
    Escape,
    /// Insert character at cursor
    Insert(char),
    /// Replace character at cursor, moving cursor to the next
    Replace(char),
    /// Create new line
    NewLine,
    /// Delete text behind cursor
    Backspace,
    /// Delete text in front of cursor
    Delete,
    /// Paste from clipboard (TODO: multiple clipboards?)
    Paste,
    /// Undo last action
    Undo,
    //TODO: special hack, clean up!
    Operator(usize, Operator, Motion, Option<TextObject>),
}

pub trait Parser {
    fn reset(&mut self);
    fn parse<F: FnMut(Event)>(&mut self, c: char, selection: bool, f: F);
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Operator {
    AutoIndent,
    Change,
    Delete,
    Move,
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
    NextSearch,
    NextWordEnd(Word),
    NextWordStart(Word),
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

#[derive(Clone, Copy, Debug, Default)]
pub struct ViNormal {
    count: Option<usize>,
    operator: Option<Operator>,
    motion: Option<Motion>,
    text_object: Option<TextObject>,
    selection: bool,
}

impl ViNormal {
    /// Repeat the provided function count times, resetting count after
    pub fn repeat<F: FnMut(usize)>(&mut self, mut f: F) {
        for i in 0..self.count.take().unwrap_or(1) {
            f(i);
        }
    }

    /// Set motion
    pub fn motion<F: FnMut(Event)>(&mut self, motion: Motion, f: &mut F) {
        self.motion = Some(motion);
        self.run(f);
    }

    /// Set operator, may set motion if operator is doubled like `dd`
    pub fn operator<F: FnMut(Event)>(&mut self, operator: Operator, f: &mut F) {
        if self.operator == Some(operator) {
            self.motion = Some(Motion::Line);
        } else {
            self.operator = Some(operator);
        }
        self.run(f);
    }

    /// Set text object and return true if supported by the motion
    pub fn text_object<F: FnMut(Event)>(&mut self, text_object: TextObject, f: &mut F) -> bool {
        if !self.motion.map_or(false, |motion| motion.text_object()) {
            // Did not need text object
            return false;
        }

        // Needed text object
        self.text_object = Some(text_object);
        self.run(f);
        true
    }

    /// Run operation, resetting it to defaults if it runs
    pub fn run<F: FnMut(Event)>(&mut self, f: &mut F) -> bool {
        match self.motion {
            Some(motion) => {
                if motion.text_object() && self.text_object.is_none() {
                    // After or inside requires a text object
                    return false;
                }
            }
            None => {
                if !self.selection {
                    // No motion requires a selection
                    return false;
                }
            }
        }

        let count = self.count.take().unwrap_or(1);
        let operator = self.operator.take().unwrap_or(Operator::Move);
        let motion = self.motion.take().unwrap_or(Motion::Selection);
        let text_object = self.text_object.take();

        f(Event::Operator(count, operator, motion, text_object));

        true
    }
}

#[derive(Debug)]
pub enum ViMode {
    /// Normal mode
    Normal(ViNormal),
    /// Waiting for g command
    LowerG(ViNormal),
    /// Waiting for z command
    LowerZ(ViNormal),
    /// Waiting for z command
    UpperZ(ViNormal),
    /// Insert mode
    Insert,
    /// Replace mode
    Replace,
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
        // Makes managing callbacks easier
        let f = &mut f;
        match self.mode {
            ViMode::Normal(ref mut normal) => {
                //TODO: is there a better way to store this?
                normal.selection = selection;
                match c {
                    // Enter insert mode after cursor (if not awaiting text object)
                    'a' => {
                        if normal.operator.is_some() {
                            normal.motion(Motion::Around, f);
                        } else {
                            f(Event::Right);
                            self.mode = ViMode::Insert;
                        }
                    }
                    // Enter insert mode at end of line
                    'A' => {
                        f(Event::End);
                        self.mode = ViMode::Insert;
                    }
                    // Previous word (if not text object)
                    'b' => {
                        if !normal.text_object(TextObject::Block, f) {
                            normal.motion(Motion::PreviousWordStart(Word::Lower), f);
                        }
                    }
                    // Previous WORD (if not text object)
                    //TODO: should this TextObject be different?
                    'B' => {
                        if !normal.text_object(TextObject::Block, f) {
                            normal.motion(Motion::PreviousWordStart(Word::Upper), f);
                        }
                    }
                    // Change mode
                    'c' => {
                        normal.operator(Operator::Change, f);
                    }
                    //TODO: Change to end of line
                    'C' => {}
                    // Delete mode
                    'd' => {
                        normal.operator(Operator::Delete, f);
                    }
                    //TODO: Delete to end of line
                    'D' => {}
                    // End of word
                    'e' => normal.motion(Motion::NextWordEnd(Word::Lower), f),
                    // End of WORD
                    'E' => normal.motion(Motion::NextWordEnd(Word::Upper), f),
                    //TODO: Find char forwards
                    'f' => {}
                    //TODO: Find char backwords
                    'F' => {}
                    // g commands
                    'g' => {
                        self.mode = ViMode::LowerG(*normal);
                    }
                    // Goto line (or end of file)
                    'G' => match normal.count.take() {
                        Some(line) => normal.motion(Motion::GotoLine(line), f),
                        None => normal.motion(Motion::GotoEof, f),
                    },
                    // Left
                    'h' | BACKSPACE => normal.motion(Motion::Left, f),
                    // Top of screen
                    'H' => normal.motion(Motion::ScreenHigh, f),
                    // Enter insert mode at cursor (if not awaiting text object)
                    'i' => {
                        if normal.operator.is_some() {
                            normal.motion(Motion::Inside, f);
                        } else {
                            self.mode = ViMode::Insert;
                        }
                    }
                    // Enter insert mode at start of line
                    'I' => {
                        f(Event::SoftHome);
                        self.mode = ViMode::Insert;
                    }
                    // Down
                    'j' => normal.motion(Motion::Down, f),
                    //TODO: Join lines
                    'J' => {}
                    // Up
                    'k' => normal.motion(Motion::Up, f),
                    //TODO: Look up keyword (vim looks up word under cursor in man pages)
                    'K' => {}
                    // Right
                    'l' | ' ' => normal.motion(Motion::Right, f),
                    // Bottom of screen
                    'L' => normal.motion(Motion::ScreenLow, f),
                    //TODO: Set mark
                    'm' => {}
                    // Middle of screen
                    'M' => normal.motion(Motion::ScreenMiddle, f),
                    // Next search item
                    'n' => normal.motion(Motion::NextSearch, f),
                    // Previous search item
                    'N' => normal.motion(Motion::PreviousSearch, f),
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
                    // Paste after (if not text object)
                    'p' => {
                        if !normal.text_object(TextObject::Paragraph, f) {
                            f(Event::Right);
                            f(Event::Paste);
                        }
                    }
                    // Paste before
                    'P' => {
                        f(Event::Paste);
                    }
                    //TODO: q, Q
                    //TODO: Replace char
                    'r' => {}
                    //TODO: Replace mode
                    'R' => {
                        self.mode = ViMode::Replace;
                    }
                    // Substitute char (if not text object)
                    's' => {
                        if !normal.text_object(TextObject::Sentence, f) {
                            normal.repeat(|_| f(Event::Delete));
                            self.mode = ViMode::Insert;
                        }
                    }
                    //TODO: Substitute line
                    'S' => {}
                    //TODO: Until character forwards (if not text object)
                    't' => if !normal.text_object(TextObject::Tag, f) {},
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
                    // Next word (if not text object)
                    'w' => {
                        if !normal.text_object(TextObject::Word(Word::Lower), f) {
                            normal.motion(Motion::NextWordStart(Word::Lower), f);
                        }
                    }
                    // Next WORD (if not text object)
                    'W' => {
                        if !normal.text_object(TextObject::Word(Word::Upper), f) {
                            normal.motion(Motion::NextWordStart(Word::Upper), f);
                        }
                    }
                    // Remove character at cursor
                    'x' | DELETE => normal.repeat(|_| f(Event::Delete)),
                    // Remove character before cursor
                    'X' => normal.repeat(|_| f(Event::Backspace)),
                    // Yank
                    'y' => normal.operator(Operator::Yank, f),
                    //TODO: Yank line
                    'Y' => {}
                    // z commands
                    'z' => {
                        self.mode = ViMode::LowerZ(*normal);
                    }
                    // Z commands
                    'Z' => {
                        self.mode = ViMode::UpperZ(*normal);
                    }
                    // Go to start of line
                    '0' => match normal.count {
                        Some(ref mut count) => {
                            *count = count.saturating_mul(10);
                        }
                        None => {
                            normal.motion(Motion::Home, f);
                        }
                    },
                    // Count of next action
                    '1'..='9' => {
                        let number = (c as u32).saturating_sub('0' as u32) as usize;
                        normal.count = Some(match normal.count.take() {
                            Some(count) => count.saturating_mul(10).saturating_add(number),
                            None => number,
                        });
                    }
                    // TODO (if not text object)
                    '`' => if !normal.text_object(TextObject::Ticks, f) {},
                    // Swap case
                    '~' => normal.operator(Operator::SwapCase, f),
                    // Go to end of line
                    '$' => normal.motion(Motion::End, f),
                    // Go to start of line after whitespace
                    '^' => normal.motion(Motion::SoftHome, f),
                    // TODO (if not text object)
                    '(' => if !normal.text_object(TextObject::Parentheses, f) {},
                    // TODO (if not text object)
                    ')' => if !normal.text_object(TextObject::Parentheses, f) {},
                    // Auto indent
                    '=' => normal.operator(Operator::AutoIndent, f),
                    // TODO (if not text object)
                    '[' => if !normal.text_object(TextObject::SquareBrackets, f) {},
                    // TODO (if not text object)
                    '{' => if !normal.text_object(TextObject::CurlyBrackets, f) {},
                    // TODO (if not text object)
                    ']' => if !normal.text_object(TextObject::SquareBrackets, f) {},
                    // TODO (if not text object)
                    '}' => if !normal.text_object(TextObject::CurlyBrackets, f) {},
                    // Enter command mode
                    ':' => {
                        self.mode = ViMode::Command {
                            value: String::new(),
                        };
                    }
                    //TODO: ';'
                    //TODO (if not text object)
                    '\'' => if !normal.text_object(TextObject::SingleQuotes, f) {},
                    '"' => if !normal.text_object(TextObject::DoubleQuotes, f) {},
                    // Unindent (if not text object)
                    '<' => {
                        if !normal.text_object(TextObject::AngleBrackets, f) {
                            normal.operator(Operator::ShiftLeft, f);
                        }
                    }
                    // Indent (if not text object)
                    '>' => {
                        if !normal.text_object(TextObject::AngleBrackets, f) {
                            normal.operator(Operator::ShiftRight, f);
                        }
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
                    ESCAPE => {
                        *normal = ViNormal::default();
                        f(Event::Escape);
                    }
                    _ => {}
                }
            }
            ViMode::LowerG(mut normal) => {
                //TODO: is there a better way to store this?
                normal.selection = selection;
                match c {
                    // Previous word end
                    'e' => normal.motion(Motion::PreviousWordEnd(Word::Lower), f),
                    // Prevous WORD end
                    'E' => normal.motion(Motion::PreviousWordEnd(Word::Upper), f),
                    'g' => match normal.count.take() {
                        Some(line) => normal.motion(Motion::GotoLine(line), f),
                        None => normal.motion(Motion::GotoLine(1), f),
                    },
                    //TODO: more g commands
                    _ => {}
                }
                self.mode = ViMode::Normal(normal);
            }
            ViMode::LowerZ(mut normal) => {
                //TODO: is there a better way to store this?
                normal.selection = selection;
                match c {
                    //TODO: more z commands
                    _ => {}
                }
                self.mode = ViMode::Normal(normal);
            }
            ViMode::UpperZ(mut normal) => {
                //TODO: is there a better way to store this?
                normal.selection = selection;
                match c {
                    //TODO: more Z commands
                    _ => {}
                }
                self.mode = ViMode::Normal(normal);
            }
            ViMode::Insert => match c {
                ESCAPE => {
                    f(Event::Left);
                    self.mode = ViMode::normal();
                }
                _ => f(Event::Insert(c)),
            },
            ViMode::Replace => match c {
                ESCAPE => {
                    f(Event::Left);
                    self.mode = ViMode::normal();
                }
                _ => f(Event::Replace(c)),
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
