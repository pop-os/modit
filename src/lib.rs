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
    /// Notify of a mode change requiring redraw
    Redraw,
    /// Escape key
    Escape,
    /// Insert character at cursor
    Insert(char),
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
    /// A fully composed command
    Cmd(usize, Operator, Motion, Option<TextObject>),
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

#[derive(Clone, Copy, Debug, Default)]
pub struct ViCmd {
    count: Option<usize>,
    operator: Option<Operator>,
    motion: Option<Motion>,
    text_object: Option<TextObject>,
    selection: bool,
}

impl ViCmd {
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

        f(Event::Cmd(count, operator, motion, text_object));

        true
    }
}

#[derive(Debug)]
pub enum ViMode {
    /// Normal mode
    Normal,
    /// Waiting for f character
    LowerF,
    /// Waiting for F character
    UpperF,
    /// Waiting for g command
    LowerG,
    /// Waiting for t character
    LowerT,
    /// Waiting for T character
    UpperT,
    /// Waiting for z command
    LowerZ,
    /// Waiting for z command
    UpperZ,
    /// Insert mode
    Insert,
    /// Replace mode
    Replace,
    /// Command mode
    Command { value: String },
    /// Search mode
    Search { value: String, forwards: bool },
}

#[derive(Debug)]
pub struct ViParser {
    pub mode: ViMode,
    pub cmd: ViCmd,
    pub semicolon_motion: Option<Motion>,
}

impl ViParser {
    pub fn new() -> Self {
        Self {
            mode: ViMode::Normal,
            cmd: ViCmd::default(),
            semicolon_motion: None,
        }
    }
}

impl Parser for ViParser {
    fn reset(&mut self) {
        self.mode = ViMode::Normal;
        self.cmd = ViCmd::default();
    }

    fn parse<F: FnMut(Event)>(&mut self, c: char, selection: bool, mut f: F) {
        // Makes managing callbacks easier
        let f = &mut f;
        //TODO: is there a better way to store this?
        let mut cmd = &mut self.cmd;
        cmd.selection = selection;
        match self.mode {
            ViMode::Normal => {
                match c {
                    // Enter insert mode after cursor (if not awaiting text object)
                    'a' => {
                        if cmd.operator.is_some() {
                            cmd.motion(Motion::Around, f);
                        } else {
                            ViCmd::default().motion(Motion::Right, f);
                            self.mode = ViMode::Insert;
                        }
                    }
                    // Enter insert mode at end of line
                    'A' => {
                        ViCmd::default().motion(Motion::End, f);
                        self.mode = ViMode::Insert;
                    }
                    // Previous word (if not text object)
                    'b' => {
                        if !cmd.text_object(TextObject::Block, f) {
                            cmd.motion(Motion::PreviousWordStart(Word::Lower), f);
                        }
                    }
                    // Previous WORD (if not text object)
                    //TODO: should this TextObject be different?
                    'B' => {
                        if !cmd.text_object(TextObject::Block, f) {
                            cmd.motion(Motion::PreviousWordStart(Word::Upper), f);
                        }
                    }
                    // Change mode
                    'c' => {
                        cmd.operator(Operator::Change, f);
                    }
                    //TODO: Change to end of line
                    'C' => {}
                    // Delete mode
                    'd' => {
                        cmd.operator(Operator::Delete, f);
                    }
                    //TODO: Delete to end of line
                    'D' => {}
                    // End of word
                    'e' => cmd.motion(Motion::NextWordEnd(Word::Lower), f),
                    // End of WORD
                    'E' => cmd.motion(Motion::NextWordEnd(Word::Upper), f),
                    // Find char forwards
                    'f' => {
                        self.mode = ViMode::LowerF;
                    }
                    // Find char backwords
                    'F' => {
                        self.mode = ViMode::UpperF;
                    }
                    // g commands
                    'g' => {
                        self.mode = ViMode::LowerG;
                    }
                    // Goto line (or end of file)
                    'G' => match cmd.count.take() {
                        Some(line) => cmd.motion(Motion::GotoLine(line), f),
                        None => cmd.motion(Motion::GotoEof, f),
                    },
                    // Left
                    'h' | BACKSPACE => cmd.motion(Motion::Left, f),
                    // Top of screen
                    'H' => cmd.motion(Motion::ScreenHigh, f),
                    // Enter insert mode at cursor (if not awaiting text object)
                    'i' => {
                        if cmd.operator.is_some() {
                            cmd.motion(Motion::Inside, f);
                        } else {
                            self.mode = ViMode::Insert;
                        }
                    }
                    // Enter insert mode at start of line
                    'I' => {
                        ViCmd::default().motion(Motion::SoftHome, f);
                        self.mode = ViMode::Insert;
                    }
                    // Down
                    'j' => cmd.motion(Motion::Down, f),
                    //TODO: Join lines
                    'J' => {}
                    // Up
                    'k' => cmd.motion(Motion::Up, f),
                    //TODO: Look up keyword (vim looks up word under cursor in man pages)
                    'K' => {}
                    // Right
                    'l' | ' ' => cmd.motion(Motion::Right, f),
                    // Bottom of screen
                    'L' => cmd.motion(Motion::ScreenLow, f),
                    //TODO: Set mark
                    'm' => {}
                    // Middle of screen
                    'M' => cmd.motion(Motion::ScreenMiddle, f),
                    // Next search item
                    'n' => cmd.motion(Motion::NextSearch, f),
                    // Previous search item
                    'N' => cmd.motion(Motion::PreviousSearch, f),
                    // Create line after and enter insert mode
                    'o' => {
                        ViCmd::default().motion(Motion::End, f);
                        f(Event::NewLine);
                        self.mode = ViMode::Insert;
                    }
                    // Create line before and enter insert mode
                    'O' => {
                        ViCmd::default().motion(Motion::Home, f);
                        f(Event::NewLine);
                        ViCmd::default().motion(Motion::Up, f);
                        self.mode = ViMode::Insert;
                    }
                    // Paste after (if not text object)
                    'p' => {
                        if !cmd.text_object(TextObject::Paragraph, f) {
                            ViCmd::default().motion(Motion::Right, f);
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
                        if !cmd.text_object(TextObject::Sentence, f) {
                            cmd.repeat(|_| f(Event::Delete));
                            self.mode = ViMode::Insert;
                        }
                    }
                    //TODO: Substitute line
                    'S' => {}
                    // Until character forwards (if not text object)
                    't' => {
                        if !cmd.text_object(TextObject::Tag, f) {
                            self.mode = ViMode::LowerT;
                        }
                    }
                    // Until character backwards
                    'T' => {
                        self.mode = ViMode::UpperT;
                    }
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
                        if !cmd.text_object(TextObject::Word(Word::Lower), f) {
                            cmd.motion(Motion::NextWordStart(Word::Lower), f);
                        }
                    }
                    // Next WORD (if not text object)
                    'W' => {
                        if !cmd.text_object(TextObject::Word(Word::Upper), f) {
                            cmd.motion(Motion::NextWordStart(Word::Upper), f);
                        }
                    }
                    // Remove character at cursor
                    'x' | DELETE => cmd.repeat(|_| f(Event::Delete)),
                    // Remove character before cursor
                    'X' => cmd.repeat(|_| f(Event::Backspace)),
                    // Yank
                    'y' => cmd.operator(Operator::Yank, f),
                    //TODO: Yank line
                    'Y' => {}
                    // z commands
                    'z' => {
                        self.mode = ViMode::LowerZ;
                    }
                    // Z commands
                    'Z' => {
                        self.mode = ViMode::UpperZ;
                    }
                    // Go to start of line
                    '0' => match cmd.count {
                        Some(ref mut count) => {
                            *count = count.saturating_mul(10);
                        }
                        None => {
                            cmd.motion(Motion::Home, f);
                        }
                    },
                    // Count of next action
                    '1'..='9' => {
                        let number = (c as u32).saturating_sub('0' as u32) as usize;
                        cmd.count = Some(match cmd.count.take() {
                            Some(count) => count.saturating_mul(10).saturating_add(number),
                            None => number,
                        });
                    }
                    // TODO (if not text object)
                    '`' => if !cmd.text_object(TextObject::Ticks, f) {},
                    // Swap case
                    '~' => cmd.operator(Operator::SwapCase, f),
                    // Go to end of line
                    '$' => cmd.motion(Motion::End, f),
                    // Go to start of line after whitespace
                    '^' => cmd.motion(Motion::SoftHome, f),
                    // TODO (if not text object)
                    '(' => if !cmd.text_object(TextObject::Parentheses, f) {},
                    // TODO (if not text object)
                    ')' => if !cmd.text_object(TextObject::Parentheses, f) {},
                    // Auto indent
                    '=' => cmd.operator(Operator::AutoIndent, f),
                    // TODO (if not text object)
                    '[' => if !cmd.text_object(TextObject::SquareBrackets, f) {},
                    // TODO (if not text object)
                    '{' => if !cmd.text_object(TextObject::CurlyBrackets, f) {},
                    // TODO (if not text object)
                    ']' => if !cmd.text_object(TextObject::SquareBrackets, f) {},
                    // TODO (if not text object)
                    '}' => if !cmd.text_object(TextObject::CurlyBrackets, f) {},
                    // Repeat f/F/t/T
                    ';' => {
                        if let Some(motion) = self.semicolon_motion {
                            cmd.motion(motion, f);
                        }
                    }
                    // Enter command mode
                    ':' => {
                        self.mode = ViMode::Command {
                            value: String::new(),
                        };
                    }
                    //TODO: ';'
                    //TODO (if not text object)
                    '\'' => if !cmd.text_object(TextObject::SingleQuotes, f) {},
                    '"' => if !cmd.text_object(TextObject::DoubleQuotes, f) {},
                    // Unindent (if not text object)
                    '<' => {
                        if !cmd.text_object(TextObject::AngleBrackets, f) {
                            cmd.operator(Operator::ShiftLeft, f);
                        }
                    }
                    // Indent (if not text object)
                    '>' => {
                        if !cmd.text_object(TextObject::AngleBrackets, f) {
                            cmd.operator(Operator::ShiftRight, f);
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
                        cmd.motion(Motion::Down, f);
                        cmd.motion(Motion::SoftHome, f);
                    }
                    ESCAPE => {
                        *cmd = ViCmd::default();
                        f(Event::Escape);
                    }
                    _ => {}
                }
            }
            ViMode::LowerF => {
                match c {
                    BACKSPACE | DELETE | ESCAPE => {}
                    _ => {
                        let motion = Motion::NextChar(c);
                        cmd.motion(motion, f);
                        self.semicolon_motion = Some(motion);
                    }
                }
                self.reset();
            }
            ViMode::UpperF => {
                match c {
                    BACKSPACE | DELETE | ESCAPE => {}
                    _ => {
                        let motion = Motion::PreviousChar(c);
                        cmd.motion(motion, f);
                        self.semicolon_motion = Some(motion);
                    }
                }
                self.reset();
            }
            ViMode::LowerG => {
                match c {
                    // Previous word end
                    'e' => cmd.motion(Motion::PreviousWordEnd(Word::Lower), f),
                    // Prevous WORD end
                    'E' => cmd.motion(Motion::PreviousWordEnd(Word::Upper), f),
                    'g' => match cmd.count.take() {
                        Some(line) => cmd.motion(Motion::GotoLine(line), f),
                        None => cmd.motion(Motion::GotoLine(1), f),
                    },
                    //TODO: more g commands
                    _ => {}
                }
                self.reset();
            }
            ViMode::LowerT => {
                match c {
                    BACKSPACE | DELETE | ESCAPE => {}
                    _ => {
                        let motion = Motion::NextCharTill(c);
                        cmd.motion(motion, f);
                        self.semicolon_motion = Some(motion);
                    }
                }
                self.reset();
            }
            ViMode::UpperT => {
                match c {
                    BACKSPACE | DELETE | ESCAPE => {}
                    _ => {
                        let motion = Motion::PreviousCharTill(c);
                        cmd.motion(motion, f);
                        self.semicolon_motion = Some(motion);
                    }
                }
                self.reset();
            }
            ViMode::LowerZ => {
                match c {
                    //TODO: more z commands
                    _ => {}
                }
                self.reset();
            }
            ViMode::UpperZ => {
                match c {
                    //TODO: more Z commands
                    _ => {}
                }
                self.reset();
            }
            ViMode::Insert => match c {
                BACKSPACE => {
                    f(Event::Backspace);
                }
                DELETE => {
                    f(Event::Delete);
                }
                ESCAPE => {
                    ViCmd::default().motion(Motion::Left, f);
                    self.reset();
                }
                _ => f(Event::Insert(c)),
            },
            ViMode::Replace => match c {
                BACKSPACE => {
                    f(Event::Backspace);
                }
                DELETE => {
                    f(Event::Delete);
                }
                ESCAPE => {
                    ViCmd::default().motion(Motion::Left, f);
                    self.reset();
                }
                _ => {
                    f(Event::Delete);
                    f(Event::Insert(c));
                }
            },
            ViMode::Command { ref mut value } => match c {
                ESCAPE => {
                    self.reset();
                }
                ENTER => {
                    //TODO: run command
                    self.reset();
                }
                BACKSPACE => {
                    if value.pop().is_none() {
                        self.reset();
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
                    self.reset();
                }
                ENTER => {
                    //TODO: run search
                    self.reset();
                }
                BACKSPACE => {
                    if value.pop().is_none() {
                        self.reset();
                    }
                }
                _ => {
                    value.push(c);
                }
            },
        }
        //TODO: optimize redraw
        f(Event::Redraw);
    }
}
