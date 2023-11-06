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
    /// Replace character at cursor, moving cursor to the next
    Replace(char),
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
    //TODO: special hack, clean up!
    Operator(u32, ViOperator, ViMotion, Option<ViTextObject>),
}

pub trait Parser {
    fn reset(&mut self);
    fn parse<F: FnMut(Event)>(&mut self, c: char, selection: bool, f: F);
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ViOperator {
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
pub enum ViWord {
    Small,
    Big,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ViMotion {
    Around,
    Down,
    Inside,
    Left,
    Line,
    NextWordEnd(ViWord),
    NextWordStart(ViWord),
    PreviousWordEnd(ViWord),
    PreviousWordStart(ViWord),
    Right,
    Selection,
    Up,
}

impl ViMotion {
    /// Returns true if text object is needed
    pub fn text_object(&self) -> bool {
        match self {
            Self::Around | Self::Inside => true,
            _ => false,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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
    Word(ViWord),
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ViNormal {
    count: Option<u32>,
    operator: Option<ViOperator>,
    motion: Option<ViMotion>,
    text_object: Option<ViTextObject>,
    selection: bool,
}

impl ViNormal {
    /// Repeat the provided function count times, resetting count after
    pub fn repeat<F: FnMut(u32)>(&mut self, mut f: F) {
        for i in 0..self.count.take().unwrap_or(1) {
            f(i);
        }
    }

    /// Set motion
    pub fn motion<F: FnMut(Event)>(&mut self, motion: ViMotion, f: &mut F) {
        self.motion = Some(motion);
        self.run(f);
    }

    /// Set operator, may set motion if operator is doubled like `dd`
    pub fn operator<F: FnMut(Event)>(&mut self, operator: ViOperator, f: &mut F) {
        if self.operator == Some(operator) {
            self.motion = Some(ViMotion::Line);
        } else {
            self.operator = Some(operator);
        }
        self.run(f);
    }

    /// Set text object and return true if supported by the motion
    pub fn text_object<F: FnMut(Event)>(&mut self, text_object: ViTextObject, f: &mut F) -> bool {
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
        let operator = self.operator.unwrap_or(ViOperator::Move);
        let motion = self.motion.take().unwrap_or(ViMotion::Selection);
        let text_object = self.text_object.take();

        f(Event::Operator(count, operator, motion, text_object));

        true
    }
}

#[derive(Debug)]
pub enum ViMode {
    /// Normal mode
    Normal(ViNormal),
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
                            normal.motion(ViMotion::Around, f);
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
                        if !normal.text_object(ViTextObject::Block, f) {
                            normal.motion(ViMotion::PreviousWordStart(ViWord::Small), f);
                        }
                    }
                    // Previous WORD (if not text object)
                    //TODO: should this TextObject be different?
                    'B' => {
                        if !normal.text_object(ViTextObject::Block, f) {
                            normal.motion(ViMotion::PreviousWordStart(ViWord::Big), f);
                        }
                    }
                    // Change mode
                    'c' => {
                        normal.operator(ViOperator::Change, f);
                    }
                    //TODO: Change to end of line
                    'C' => {}
                    // Delete mode
                    'd' => {
                        normal.operator(ViOperator::Delete, f);
                    }
                    //TODO: Delete to end of line
                    'D' => {}
                    // End of word
                    'e' => normal.motion(ViMotion::NextWordEnd(ViWord::Small), f),
                    // End of WORD
                    'E' => normal.motion(ViMotion::NextWordEnd(ViWord::Big), f),
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
                    'h' | BACKSPACE => normal.motion(ViMotion::Left, f),
                    // Top of screen
                    'H' => f(Event::ScreenHigh),
                    // Enter insert mode at cursor (if not awaiting text object)
                    'i' => {
                        if normal.operator.is_some() {
                            normal.motion(ViMotion::Inside, f);
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
                    'j' => normal.motion(ViMotion::Down, f),
                    //TODO: Join lines
                    'J' => {}
                    // Up
                    'k' => normal.motion(ViMotion::Up, f),
                    //TODO: Look up keyword (vim looks up word under cursor in man pages)
                    'K' => {}
                    // Right
                    'l' | ' ' => normal.motion(ViMotion::Right, f),
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
                    // Paste after (if not text object)
                    'p' => {
                        if !normal.text_object(ViTextObject::Paragraph, f) {
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
                        if !normal.text_object(ViTextObject::Sentence, f) {
                            normal.repeat(|_| f(Event::Delete));
                            self.mode = ViMode::Insert;
                        }
                    }
                    //TODO: Substitute line
                    'S' => {}
                    //TODO: Until character forwards (if not text object)
                    't' => if !normal.text_object(ViTextObject::Tag, f) {},
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
                        if !normal.text_object(ViTextObject::Word(ViWord::Small), f) {
                            normal.motion(ViMotion::NextWordStart(ViWord::Small), f);
                        }
                    }
                    // Next WORD (if not text object)
                    'W' => {
                        if !normal.text_object(ViTextObject::Word(ViWord::Big), f) {
                            normal.motion(ViMotion::NextWordStart(ViWord::Big), f);
                        }
                    }
                    // Remove character at cursor
                    'x' => normal.repeat(|_| f(Event::Delete)),
                    // Remove character before cursor
                    'X' => normal.repeat(|_| f(Event::Backspace)),
                    // Yank
                    'y' => normal.operator(ViOperator::Yank, f),
                    //TODO: Yank line
                    'Y' => {}
                    //TODO: z, Z
                    // Go to start of line
                    '0' => match normal.count {
                        Some(ref mut count) => {
                            *count = count.saturating_mul(10);
                        }
                        None => {
                            f(Event::Home);
                        }
                    },
                    // Count of next action
                    '1'..='9' => {
                        let number = (c as u32).saturating_sub('0' as u32);
                        normal.count = Some(match normal.count.take() {
                            Some(count) => count.saturating_mul(10).saturating_add(number),
                            None => number,
                        });
                    }
                    // TODO (if not text object)
                    '`' => if !normal.text_object(ViTextObject::Ticks, f) {},
                    // Swap case
                    '~' => normal.operator(ViOperator::SwapCase, f),
                    // Go to end of line
                    '$' => f(Event::End),
                    // Go to start of line after whitespace
                    '^' => f(Event::SoftHome),
                    // TODO (if not text object)
                    '(' => if !normal.text_object(ViTextObject::Parentheses, f) {},
                    // TODO (if not text object)
                    ')' => if !normal.text_object(ViTextObject::Parentheses, f) {},
                    // Auto indent
                    '=' => normal.operator(ViOperator::AutoIndent, f),
                    // TODO (if not text object)
                    '[' => if !normal.text_object(ViTextObject::SquareBrackets, f) {},
                    // TODO (if not text object)
                    '{' => if !normal.text_object(ViTextObject::CurlyBrackets, f) {},
                    // TODO (if not text object)
                    ']' => if !normal.text_object(ViTextObject::SquareBrackets, f) {},
                    // TODO (if not text object)
                    '}' => if !normal.text_object(ViTextObject::CurlyBrackets, f) {},
                    // Enter command mode
                    ':' => {
                        self.mode = ViMode::Command {
                            value: String::new(),
                        };
                    }
                    //TODO: ';'
                    //TODO (if not text object)
                    '\'' => if !normal.text_object(ViTextObject::SingleQuotes, f) {},
                    '"' => if !normal.text_object(ViTextObject::DoubleQuotes, f) {},
                    // Unindent (if not text object)
                    '<' => {
                        if !normal.text_object(ViTextObject::AngleBrackets, f) {
                            normal.operator(ViOperator::ShiftLeft, f);
                        }
                    }
                    // Indent (if not text object)
                    '>' => {
                        if !normal.text_object(ViTextObject::AngleBrackets, f) {
                            normal.operator(ViOperator::ShiftRight, f);
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
                    ESCAPE => f(Event::Escape),
                    _ => {}
                }
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
