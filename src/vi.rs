use alloc::string::String;
use core::{fmt, mem};

use crate::{Event, Key, Motion, Operator, Parser, TextObject, Word};

#[derive(Clone, Copy, Debug, Default)]
pub struct ViCmd {
    count: Option<usize>,
    operator: Option<Operator>,
    motion: Option<Motion>,
    text_object: Option<TextObject>,
    selection: bool,
    enter_insert_mode: bool,
}

impl fmt::Display for ViCmd {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if let Some(count) = self.count {
            write!(f, "{count}")?;
        }
        if let Some(operator) = self.operator {
            write!(f, "{operator:?}")?;
        }
        if let Some(motion) = self.motion {
            write!(f, "{motion:?}")?;
        }
        if let Some(text_object) = self.text_object {
            write!(f, "{text_object:?}")?;
        }
        Ok(())
    }
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
        let motion = self.motion.take().unwrap_or(Motion::Selection);
        let text_object = self.text_object.take();

        //TODO: clean up logic of Motion, such that actual motions and references to
        // text objects and selections are not in the same enum
        match self.operator.take() {
            Some(operator) => {
                match motion {
                    Motion::Around => f(Event::SelectTextObject(
                        text_object.expect("no text object"),
                        true,
                    )),
                    Motion::Inside => f(Event::SelectTextObject(
                        text_object.expect("no text object"),
                        false,
                    )),
                    Motion::Line => {
                        f(Event::Motion(Motion::SoftHome));
                        f(Event::SelectStart);
                        f(Event::Motion(Motion::End));
                    }
                    Motion::Selection => {}
                    _ => {
                        f(Event::SelectStart);
                        for _ in 0..count {
                            f(Event::Motion(motion));
                        }
                    }
                }

                match operator {
                    Operator::AutoIndent => {
                        f(Event::AutoIndent);
                    }
                    Operator::Change => {
                        f(Event::Delete);
                        self.enter_insert_mode = true;
                    }
                    Operator::Delete => {
                        f(Event::Delete);
                    }
                    Operator::ShiftLeft => {
                        f(Event::ShiftLeft);
                    }
                    Operator::ShiftRight => {
                        f(Event::ShiftRight);
                    }
                    Operator::SwapCase => {
                        f(Event::SwapCase);
                    }
                    Operator::Yank => {
                        f(Event::Copy);
                    }
                }
            }
            None => match motion {
                Motion::Around => f(Event::SelectTextObject(
                    text_object.expect("no text object"),
                    true,
                )),
                Motion::Inside => f(Event::SelectTextObject(
                    text_object.expect("no text object"),
                    false,
                )),
                _ => {
                    for _ in 0..count {
                        f(Event::Motion(motion));
                    }
                }
            },
        }

        true
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ViMode {
    /// Normal mode
    Normal,
    /// Waiting for another character to complete command
    Extra(char),
    /// Insert mode
    Insert,
    /// Replace mode
    Replace,
    /// Visual mode
    Visual,
    /// Visual line mode
    VisualLine,
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

    fn parse<F: FnMut(Event)>(&mut self, key: Key, selection: bool, mut f: F) {
        // Normalize key, so we don't deal with control characters below
        let key = key.normalize();
        //TODO: is there a better way to store this?
        self.cmd.selection = selection;
        // Makes managing callbacks easier
        let f = &mut f;
        // Makes composint commands easier
        let cmd = &mut self.cmd;
        match self.mode {
            ViMode::Normal | ViMode::Visual | ViMode::VisualLine => match key {
                Key::Backspace => cmd.motion(Motion::Left, f),
                Key::Delete => cmd.repeat(|_| f(Event::Delete)),
                Key::Down => cmd.motion(Motion::Down, f),
                Key::Enter => {
                    cmd.motion(Motion::Down, f);
                    cmd.motion(Motion::SoftHome, f);
                }
                Key::Escape => {
                    self.reset();
                    f(Event::Escape);
                }
                Key::Left => cmd.motion(Motion::Left, f),
                Key::Right => cmd.motion(Motion::Right, f),
                //TODO: what should tab do?
                Key::Tab => (),
                Key::Up => cmd.motion(Motion::Up, f),
                Key::Char(c) => match c {
                    // Enter insert mode after cursor (if not awaiting text object)
                    'a' => {
                        if cmd.operator.is_some() || self.mode != ViMode::Normal {
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
                    // Change to end of line
                    'C' => {
                        cmd.operator(Operator::Change, f);
                        cmd.motion(Motion::End, f);
                    }
                    // Delete mode
                    'd' => {
                        cmd.operator(Operator::Delete, f);
                    }
                    // Delete to end of line
                    'D' => {
                        cmd.operator(Operator::Change, f);
                        cmd.motion(Motion::End, f);
                    }
                    // End of word
                    'e' => cmd.motion(Motion::NextWordEnd(Word::Lower), f),
                    // End of WORD
                    'E' => cmd.motion(Motion::NextWordEnd(Word::Upper), f),
                    // Find char forwards
                    'f' => {
                        self.mode = ViMode::Extra(c);
                    }
                    // Find char backwords
                    'F' => {
                        self.mode = ViMode::Extra(c);
                    }
                    // g commands
                    'g' => {
                        self.mode = ViMode::Extra(c);
                    }
                    // Goto line (or end of file)
                    'G' => match cmd.count.take() {
                        Some(line) => cmd.motion(Motion::GotoLine(line), f),
                        None => cmd.motion(Motion::GotoEof, f),
                    },
                    // Left
                    'h' => cmd.motion(Motion::Left, f),
                    // Top of screen
                    'H' => cmd.motion(Motion::ScreenHigh, f),
                    // Enter insert mode at cursor (if not awaiting text object)
                    'i' => {
                        if cmd.operator.is_some() || self.mode != ViMode::Normal {
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
                    // Replace char
                    'r' => {
                        self.mode = ViMode::Extra(c);
                    }
                    // Replace mode
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
                    // Substitute line
                    'S' => {
                        cmd.operator(Operator::Change, f);
                        cmd.motion(Motion::Line, f);
                    }
                    // Until character forwards (if not text object)
                    't' => {
                        if !cmd.text_object(TextObject::Tag, f) {
                            self.mode = ViMode::Extra(c);
                        }
                    }
                    // Until character backwards
                    'T' => {
                        self.mode = ViMode::Extra(c);
                    }
                    // Undo
                    'u' => {
                        f(Event::Undo);
                    }
                    //TODO: U
                    // Enter visual mode
                    'v' => {
                        //TODO: this is very hacky and has bugs
                        if self.mode == ViMode::Visual {
                            f(Event::SelectClear);
                            self.mode = ViMode::Normal;
                        } else {
                            f(Event::SelectStart);
                            self.mode = ViMode::Visual;
                        }
                    }
                    // Enter line visual mode
                    'V' => {
                        if self.mode == ViMode::VisualLine {
                            f(Event::SelectClear);
                            self.mode = ViMode::Normal;
                        } else {
                            //TODO: select by line
                            f(Event::SelectStart);
                            self.mode = ViMode::VisualLine;
                        }
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
                    'x' => cmd.repeat(|_| f(Event::Delete)),
                    // Remove character before cursor
                    'X' => cmd.repeat(|_| f(Event::Backspace)),
                    // Yank
                    'y' => cmd.operator(Operator::Yank, f),
                    // Yank line
                    'Y' => {
                        cmd.operator(Operator::Yank, f);
                        cmd.motion(Motion::Line, f);
                    }
                    // z commands
                    'z' => {
                        self.mode = ViMode::Extra(c);
                    }
                    // Z commands
                    'Z' => {
                        self.mode = ViMode::Extra(c);
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
                    // TODO: !, @, #
                    // Go to end of line
                    '$' => cmd.motion(Motion::End, f),
                    //TODO: %
                    // Go to start of line after whitespace
                    '^' => cmd.motion(Motion::SoftHome, f),
                    //TODO &, *
                    // TODO (if not text object)
                    '(' => if !cmd.text_object(TextObject::Parentheses, f) {},
                    // TODO (if not text object)
                    ')' => if !cmd.text_object(TextObject::Parentheses, f) {},
                    // Move up and soft home
                    '-' => {
                        cmd.motion(Motion::Up, f);
                        cmd.motion(Motion::SoftHome, f);
                    }
                    // Move down and soft home
                    '+' => {
                        cmd.motion(Motion::Down, f);
                        cmd.motion(Motion::SoftHome, f);
                    }
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
                    //TODO (if not text object)
                    '\'' => if !cmd.text_object(TextObject::SingleQuotes, f) {},
                    //TODO (if not text object)
                    '"' => if !cmd.text_object(TextObject::DoubleQuotes, f) {},
                    // Reverse f/F/t/T
                    ',' => {
                        if let Some(motion) = self.semicolon_motion {
                            if let Some(reverse) = motion.reverse() {
                                cmd.motion(reverse, f);
                            }
                        }
                    }
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
                    _ => {}
                },
            },
            ViMode::Extra(extra) => match extra {
                'f' | 'F' | 't' | 'T' => {
                    match key {
                        Key::Char(c) => {
                            let motion = match extra {
                                'f' => Motion::NextChar(c),
                                'F' => Motion::PreviousChar(c),
                                't' => Motion::NextCharTill(c),
                                'T' => Motion::PreviousCharTill(c),
                                _ => unreachable!(),
                            };
                            cmd.motion(motion, f);
                            self.semicolon_motion = Some(motion);
                        }
                        _ => {}
                    }
                    self.reset();
                }
                'g' => {
                    match key {
                        Key::Char(c) => match c {
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
                        },
                        //TODO: what do control keys do in this mode?
                        _ => {}
                    }
                    self.reset();
                }
                _ => {
                    //TODO
                    log::info!("TODO: extra command {:?}{:?}", extra, key);
                    self.reset();
                }
            },
            ViMode::Insert => match key {
                Key::Backspace => f(Event::Backspace),
                Key::Delete => f(Event::Delete),
                Key::Escape => {
                    ViCmd::default().motion(Motion::Left, f);
                    self.reset();
                }
                Key::Char(c) => f(Event::Insert(c)),
                _ => {
                    //TODO: more keys
                }
            },
            ViMode::Replace => match key {
                Key::Backspace => f(Event::Backspace),
                Key::Delete => f(Event::Delete),
                Key::Escape => {
                    ViCmd::default().motion(Motion::Left, f);
                    self.reset();
                }
                Key::Char(c) => {
                    f(Event::Delete);
                    f(Event::Insert(c));
                }
                _ => {
                    //TODO: more keys
                }
            },
            ViMode::Command { ref mut value } => match key {
                Key::Escape => {
                    self.reset();
                }
                Key::Enter => {
                    //TODO: run command
                    self.reset();
                }
                Key::Backspace => {
                    if value.pop().is_none() {
                        self.reset();
                    }
                }
                Key::Char(c) => {
                    value.push(c);
                }
                _ => {
                    //TODO: more keys
                }
            },
            ViMode::Search {
                ref mut value,
                forwards,
            } => match key {
                Key::Escape => {
                    self.reset();
                }
                Key::Enter => {
                    // Swap search value to avoid allocations
                    let mut tmp = String::new();
                    mem::swap(value, &mut tmp);
                    f(Event::SetSearch(tmp, forwards));
                    self.reset();
                    ViCmd::default().motion(Motion::NextSearch, f);
                }
                Key::Backspace => {
                    if value.pop().is_none() {
                        self.reset();
                    }
                }
                Key::Char(c) => {
                    value.push(c);
                }
                _ => {
                    //TODO: more keys
                }
            },
        }

        // Enter insert mode, for example, after Change operator
        if self.cmd.enter_insert_mode {
            self.cmd.enter_insert_mode = false;
            self.mode = ViMode::Insert;
        }

        //TODO: optimize redraw
        f(Event::Redraw);
    }
}
