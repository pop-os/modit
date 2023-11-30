use alloc::{string::String, vec::Vec};
use core::{fmt, mem};

use crate::{Event, Key, Motion, Operator, Parser, TextObject, Word};

pub const VI_DEFAULT_REGISTER: char = '"';

#[derive(Debug)]
pub struct ViContext<F: FnMut(Event)> {
    callback: F,
    selection: bool,
    pending_change: Option<Vec<Event>>,
    change: Option<Vec<Event>>,
    set_mode: Option<ViMode>,
}

impl<F: FnMut(Event)> ViContext<F> {
    fn start_change(&mut self) {
        if self.pending_change.is_none() {
            self.pending_change = Some(Vec::new());
        }
        (self.callback)(Event::ChangeStart);
    }

    fn finish_change(&mut self) {
        self.change = self.pending_change.take();
        (self.callback)(Event::ChangeFinish);
    }

    fn e(&mut self, event: Event) {
        match &mut self.pending_change {
            Some(change) => change.push(event.clone()),
            None => {}
        }
        (self.callback)(event);
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ViCmd {
    register: Option<char>,
    count: Option<usize>,
    operator: Option<Operator>,
    motion: Option<Motion>,
    text_object: Option<TextObject>,
}

impl fmt::Display for ViCmd {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if let Some(register) = self.register {
            write!(f, "\"{register}")?;
        }
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
    pub fn motion<F: FnMut(Event)>(&mut self, motion: Motion, ctx: &mut ViContext<F>) {
        self.motion = Some(motion);
        self.run(ctx);
    }

    /// Set operator, may set motion if operator is doubled like `dd`
    pub fn operator<F: FnMut(Event)>(&mut self, operator: Operator, ctx: &mut ViContext<F>) {
        if self.operator == Some(operator) {
            self.motion = Some(Motion::Line);
        } else {
            self.operator = Some(operator);
        }
        self.run(ctx);
    }

    /// Set text object and return true if supported by the motion
    pub fn text_object<F: FnMut(Event)>(
        &mut self,
        text_object: TextObject,
        ctx: &mut ViContext<F>,
    ) -> bool {
        if !self.motion.map_or(false, |motion| motion.text_object()) {
            // Did not need text object
            return false;
        }

        // Needed text object
        self.text_object = Some(text_object);
        self.run(ctx);
        true
    }

    /// Run operation, resetting it to defaults if it runs
    pub fn run<F: FnMut(Event)>(&mut self, ctx: &mut ViContext<F>) -> bool {
        match self.motion {
            Some(motion) => {
                if motion.text_object() && self.text_object.is_none() {
                    // After or inside requires a text object
                    return false;
                }
            }
            None => {
                if !ctx.selection {
                    // No motion requires a selection
                    return false;
                }
            }
        }

        let register = self.register.take().unwrap_or(VI_DEFAULT_REGISTER);
        let count = self.count.take().unwrap_or(1);
        let motion = self.motion.take().unwrap_or(Motion::Selection);
        let text_object = self.text_object.take();

        //TODO: clean up logic of Motion, such that actual motions and references to
        // text objects and selections are not in the same enum
        match self.operator.take() {
            Some(operator) => {
                ctx.start_change();

                match motion {
                    Motion::Around => ctx.e(Event::SelectTextObject(
                        text_object.expect("no text object"),
                        true,
                    )),
                    Motion::Inside => ctx.e(Event::SelectTextObject(
                        text_object.expect("no text object"),
                        false,
                    )),
                    Motion::Line => {
                        ctx.e(Event::SelectLineStart);
                    }
                    Motion::Selection => {}
                    _ => {
                        ctx.e(Event::SelectStart);
                        for _ in 0..count {
                            ctx.e(Event::Motion(motion));
                        }
                    }
                }

                let mut enter_insert_mode = false;
                match operator {
                    Operator::AutoIndent => {
                        ctx.e(Event::AutoIndent);
                    }
                    Operator::Change => {
                        ctx.e(Event::Yank { register });
                        ctx.e(Event::Delete);
                        enter_insert_mode = true;
                    }
                    Operator::Delete => {
                        ctx.e(Event::Yank { register });
                        ctx.e(Event::Delete);
                    }
                    Operator::ShiftLeft => {
                        ctx.e(Event::ShiftLeft);
                    }
                    Operator::ShiftRight => {
                        ctx.e(Event::ShiftRight);
                    }
                    Operator::SwapCase => {
                        ctx.e(Event::SwapCase);
                    }
                    Operator::Yank => {
                        ctx.e(Event::Yank { register });
                    }
                }

                ctx.e(Event::SelectClear);
                if enter_insert_mode {
                    ctx.set_mode = Some(ViMode::Insert);
                } else {
                    ctx.finish_change();
                    ctx.set_mode = Some(ViMode::Normal);
                }
            }
            None => match motion {
                Motion::Around => ctx.e(Event::SelectTextObject(
                    text_object.expect("no text object"),
                    true,
                )),
                Motion::Inside => ctx.e(Event::SelectTextObject(
                    text_object.expect("no text object"),
                    false,
                )),
                _ => {
                    for _ in 0..count {
                        ctx.e(Event::Motion(motion));
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
    pub register_mode: ViMode,
    pub semicolon_motion: Option<Motion>,
    pub pending_change: Option<Vec<Event>>,
    pub last_change: Option<Vec<Event>>,
}

impl ViParser {
    pub fn new() -> Self {
        Self {
            mode: ViMode::Normal,
            cmd: ViCmd::default(),
            register_mode: ViMode::Normal,
            semicolon_motion: None,
            pending_change: None,
            last_change: None,
        }
    }
}

impl Parser for ViParser {
    fn reset(&mut self) {
        self.mode = ViMode::Normal;
        self.cmd = ViCmd::default();
    }

    fn parse<F: FnMut(Event)>(&mut self, key: Key, selection: bool, callback: F) {
        // Makes composing commands easier
        let cmd = &mut self.cmd;
        // Normalize key, so we don't deal with control characters below
        let key = key.normalize();
        // Makes managing callbacks easier
        let mut ctx = ViContext {
            selection,
            callback,
            pending_change: self.pending_change.take(),
            change: None,
            set_mode: None,
        };
        let ctx = &mut ctx;
        match self.mode {
            ViMode::Normal | ViMode::Visual | ViMode::VisualLine => match key {
                Key::Backspace => cmd.motion(Motion::Left, ctx),
                //TODO: what should backtab do?
                Key::Backtab => (),
                Key::Delete => cmd.repeat(|_| ctx.e(Event::Delete)),
                Key::Down => cmd.motion(Motion::Down, ctx),
                Key::End => cmd.motion(Motion::End, ctx),
                Key::Enter => {
                    cmd.motion(Motion::Down, ctx);
                    cmd.motion(Motion::SoftHome, ctx);
                }
                Key::Escape => {
                    self.reset();
                    ctx.e(Event::Escape);
                }
                Key::Home => cmd.motion(Motion::Home, ctx),
                Key::Left => cmd.motion(Motion::LeftInLine, ctx),
                Key::PageDown => cmd.motion(Motion::PageDown, ctx),
                Key::PageUp => cmd.motion(Motion::PageUp, ctx),
                Key::Right => cmd.motion(Motion::RightInLine, ctx),
                //TODO: what should tab do?
                Key::Tab => (),
                Key::Up => cmd.motion(Motion::Up, ctx),
                Key::Char(c) => match c {
                    // Enter insert mode after cursor (if not awaiting text object)
                    'a' => {
                        if cmd.operator.is_some() || self.mode != ViMode::Normal {
                            cmd.motion(Motion::Around, ctx);
                        } else {
                            ctx.start_change();
                            ViCmd::default().motion(Motion::Right, ctx);
                            self.mode = ViMode::Insert;
                        }
                    }
                    // Enter insert mode at end of line
                    'A' => {
                        ctx.start_change();
                        ViCmd::default().motion(Motion::End, ctx);
                        self.mode = ViMode::Insert;
                    }
                    // Previous word (if not text object)
                    'b' => {
                        if !cmd.text_object(TextObject::Block, ctx) {
                            cmd.motion(Motion::PreviousWordStart(Word::Lower), ctx);
                        }
                    }
                    // Previous WORD (if not text object)
                    //TODO: should this TextObject be different?
                    'B' => {
                        if !cmd.text_object(TextObject::Block, ctx) {
                            cmd.motion(Motion::PreviousWordStart(Word::Upper), ctx);
                        }
                    }
                    // Change mode
                    'c' => {
                        cmd.operator(Operator::Change, ctx);
                    }
                    // Change to end of line
                    'C' => {
                        cmd.operator(Operator::Change, ctx);
                        cmd.motion(Motion::End, ctx);
                    }
                    // Delete mode
                    'd' => {
                        cmd.operator(Operator::Delete, ctx);
                    }
                    // Delete to end of line
                    'D' => {
                        cmd.operator(Operator::Delete, ctx);
                        cmd.motion(Motion::End, ctx);
                    }
                    // End of word
                    'e' => cmd.motion(Motion::NextWordEnd(Word::Lower), ctx),
                    // End of WORD
                    'E' => cmd.motion(Motion::NextWordEnd(Word::Upper), ctx),
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
                        Some(line) => cmd.motion(Motion::GotoLine(line), ctx),
                        None => cmd.motion(Motion::GotoEof, ctx),
                    },
                    // Left (in line)
                    'h' => cmd.motion(Motion::LeftInLine, ctx),
                    // Top of screen
                    'H' => cmd.motion(Motion::ScreenHigh, ctx),
                    // Enter insert mode at cursor (if not awaiting text object)
                    'i' => {
                        if cmd.operator.is_some() || self.mode != ViMode::Normal {
                            cmd.motion(Motion::Inside, ctx);
                        } else {
                            ctx.start_change();
                            self.mode = ViMode::Insert;
                        }
                    }
                    // Enter insert mode at start of line
                    'I' => {
                        ctx.start_change();
                        ViCmd::default().motion(Motion::SoftHome, ctx);
                        self.mode = ViMode::Insert;
                    }
                    // Down
                    'j' => cmd.motion(Motion::Down, ctx),
                    //TODO: Join lines
                    'J' => {}
                    // Up
                    'k' => cmd.motion(Motion::Up, ctx),
                    //TODO: Look up keyword (vim looks up word under cursor in man pages)
                    'K' => {}
                    // Right (in line)
                    'l' => cmd.motion(Motion::RightInLine, ctx),
                    // Bottom of screen
                    'L' => cmd.motion(Motion::ScreenLow, ctx),
                    //TODO: Set mark
                    'm' => {}
                    // Middle of screen
                    'M' => cmd.motion(Motion::ScreenMiddle, ctx),
                    // Next search item
                    'n' => cmd.motion(Motion::NextSearch, ctx),
                    // Previous search item
                    'N' => cmd.motion(Motion::PreviousSearch, ctx),
                    // Create line after and enter insert mode
                    'o' => {
                        ctx.start_change();
                        ViCmd::default().motion(Motion::End, ctx);
                        ctx.e(Event::NewLine);
                        self.mode = ViMode::Insert;
                    }
                    // Create line before and enter insert mode
                    'O' => {
                        ctx.start_change();
                        ViCmd::default().motion(Motion::Home, ctx);
                        ctx.e(Event::NewLine);
                        ViCmd::default().motion(Motion::Up, ctx);
                        self.mode = ViMode::Insert;
                    }
                    // Paste after (if not text object)
                    'p' => {
                        if !cmd.text_object(TextObject::Paragraph, ctx) {
                            let register = cmd.register.unwrap_or(VI_DEFAULT_REGISTER);
                            ctx.e(Event::Put {
                                register,
                                after: true,
                            });
                        }
                    }
                    // Paste before
                    'P' => {
                        let register = cmd.register.unwrap_or(VI_DEFAULT_REGISTER);
                        ctx.e(Event::Put {
                            register,
                            after: false,
                        });
                    }
                    //TODO: q, Q
                    // Replace char
                    'r' => {
                        self.mode = ViMode::Extra(c);
                    }
                    // Replace mode
                    'R' => {
                        ctx.start_change();
                        self.mode = ViMode::Replace;
                    }
                    // Substitute char (if not text object)
                    's' => {
                        if !cmd.text_object(TextObject::Sentence, ctx) {
                            ctx.start_change();
                            cmd.repeat(|_| ctx.e(Event::Delete));
                            self.mode = ViMode::Insert;
                        }
                    }
                    // Substitute line
                    'S' => {
                        cmd.operator(Operator::Change, ctx);
                        cmd.motion(Motion::Line, ctx);
                    }
                    // Until character forwards (if not text object)
                    't' => {
                        if !cmd.text_object(TextObject::Tag, ctx) {
                            self.mode = ViMode::Extra(c);
                        }
                    }
                    // Until character backwards
                    'T' => {
                        self.mode = ViMode::Extra(c);
                    }
                    // Undo
                    'u' => {
                        ctx.e(Event::Undo);
                    }
                    //TODO: U
                    // Enter visual mode
                    'v' => {
                        //TODO: this is very hacky and has bugs
                        if self.mode == ViMode::Visual {
                            ctx.e(Event::SelectClear);
                            self.mode = ViMode::Normal;
                        } else {
                            ctx.e(Event::SelectStart);
                            self.mode = ViMode::Visual;
                        }
                    }
                    // Enter line visual mode
                    'V' => {
                        if self.mode == ViMode::VisualLine {
                            ctx.e(Event::SelectClear);
                            self.mode = ViMode::Normal;
                        } else {
                            ctx.e(Event::SelectLineStart);
                            self.mode = ViMode::VisualLine;
                        }
                    }
                    // Next word (if not text object)
                    'w' => {
                        if !cmd.text_object(TextObject::Word(Word::Lower), ctx) {
                            cmd.motion(Motion::NextWordStart(Word::Lower), ctx);
                        }
                    }
                    // Next WORD (if not text object)
                    'W' => {
                        if !cmd.text_object(TextObject::Word(Word::Upper), ctx) {
                            cmd.motion(Motion::NextWordStart(Word::Upper), ctx);
                        }
                    }
                    // Remove character at cursor
                    'x' => cmd.repeat(|_| ctx.e(Event::Delete)),
                    // Remove character before cursor
                    'X' => cmd.repeat(|_| ctx.e(Event::Backspace)),
                    // Yank
                    'y' => cmd.operator(Operator::Yank, ctx),
                    // Yank line
                    'Y' => {
                        cmd.operator(Operator::Yank, ctx);
                        cmd.motion(Motion::Line, ctx);
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
                            cmd.motion(Motion::Home, ctx);
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
                    '`' => if !cmd.text_object(TextObject::Ticks, ctx) {},
                    // Swap case
                    '~' => cmd.operator(Operator::SwapCase, ctx),
                    // TODO: !, @, #
                    // Go to end of line
                    '$' => cmd.motion(Motion::End, ctx),
                    //TODO: %
                    // Go to start of line after whitespace
                    '^' => cmd.motion(Motion::SoftHome, ctx),
                    //TODO &, *
                    // TODO (if not text object)
                    '(' => if !cmd.text_object(TextObject::Parentheses, ctx) {},
                    // TODO (if not text object)
                    ')' => if !cmd.text_object(TextObject::Parentheses, ctx) {},
                    // Move up and soft home
                    '-' => {
                        cmd.motion(Motion::Up, ctx);
                        cmd.motion(Motion::SoftHome, ctx);
                    }
                    // Move down and soft home
                    '+' => {
                        cmd.motion(Motion::Down, ctx);
                        cmd.motion(Motion::SoftHome, ctx);
                    }
                    // Auto indent
                    '=' => cmd.operator(Operator::AutoIndent, ctx),
                    // TODO (if not text object)
                    '[' => if !cmd.text_object(TextObject::SquareBrackets, ctx) {},
                    // TODO (if not text object)
                    '{' => if !cmd.text_object(TextObject::CurlyBrackets, ctx) {},
                    // TODO (if not text object)
                    ']' => if !cmd.text_object(TextObject::SquareBrackets, ctx) {},
                    // TODO (if not text object)
                    '}' => if !cmd.text_object(TextObject::CurlyBrackets, ctx) {},
                    // Repeat f/F/t/T
                    ';' => {
                        if let Some(motion) = self.semicolon_motion {
                            cmd.motion(motion, ctx);
                        }
                    }
                    // Enter command mode
                    ':' => {
                        self.mode = ViMode::Command {
                            value: String::new(),
                        };
                    }
                    //TODO (if not text object)
                    '\'' => if !cmd.text_object(TextObject::SingleQuotes, ctx) {},
                    // Select register (if not text object)
                    '"' => {
                        if !cmd.text_object(TextObject::DoubleQuotes, ctx) {
                            self.register_mode = self.mode.clone();
                            self.mode = ViMode::Extra(c);
                        }
                    }
                    // Reverse f/F/t/T
                    ',' => {
                        if let Some(motion) = self.semicolon_motion {
                            if let Some(reverse) = motion.reverse() {
                                cmd.motion(reverse, ctx);
                            }
                        }
                    }
                    // Unindent (if not text object)
                    '<' => {
                        if !cmd.text_object(TextObject::AngleBrackets, ctx) {
                            cmd.operator(Operator::ShiftLeft, ctx);
                        }
                    }
                    // Repeat change
                    '.' => {
                        if let Some(change) = &self.last_change {
                            ctx.start_change();
                            for event in change.iter() {
                                ctx.e(event.clone());
                            }
                            ctx.finish_change();
                        }
                    }
                    // Indent (if not text object)
                    '>' => {
                        if !cmd.text_object(TextObject::AngleBrackets, ctx) {
                            cmd.operator(Operator::ShiftRight, ctx);
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
                    // Right
                    ' ' => cmd.motion(Motion::Right, ctx),
                    _ => {}
                },
            },
            ViMode::Extra(extra) => match extra {
                // Find/till character
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
                            cmd.motion(motion, ctx);
                            self.semicolon_motion = Some(motion);
                        }
                        _ => {}
                    }
                    self.reset();
                }
                // Extra commands
                'g' => {
                    match key {
                        Key::Char(c) => match c {
                            // Previous word end
                            'e' => cmd.motion(Motion::PreviousWordEnd(Word::Lower), ctx),
                            // Prevous WORD end
                            'E' => cmd.motion(Motion::PreviousWordEnd(Word::Upper), ctx),
                            'g' => match cmd.count.take() {
                                Some(line) => cmd.motion(Motion::GotoLine(line), ctx),
                                None => cmd.motion(Motion::GotoLine(1), ctx),
                            },
                            'n' => {
                                cmd.motion(Motion::Inside, ctx);
                                cmd.text_object(TextObject::Search { forwards: true }, ctx);
                            }
                            'N' => {
                                cmd.motion(Motion::Inside, ctx);
                                cmd.text_object(TextObject::Search { forwards: false }, ctx);
                            }
                            //TODO: more g commands
                            _ => {}
                        },
                        //TODO: what do control keys do in this mode?
                        _ => {}
                    }
                    self.reset();
                }
                // Replace character
                'r' => {
                    match key {
                        Key::Char(c) => {
                            //TODO: a visual selection allows replacing all characters
                            ctx.start_change();
                            ctx.e(Event::Delete);
                            ctx.e(Event::Insert(c));
                            ViCmd::default().motion(Motion::LeftInLine, ctx);
                            ctx.finish_change();
                        }
                        _ => {}
                    }
                    self.reset();
                }
                // Select register
                '"' => {
                    match key {
                        Key::Char(c) => {
                            cmd.register = Some(c);
                        }
                        _ => {}
                    }
                    self.mode = self.register_mode.clone();
                    self.register_mode = ViMode::Normal;
                }
                _ => {
                    //TODO
                    log::info!("TODO: extra command {:?}{:?}", extra, key);
                    self.reset();
                }
            },
            ViMode::Insert | ViMode::Replace => match key {
                //TODO: FINISH CHANGE ON MOTION?
                Key::Backspace => ctx.e(Event::Backspace),
                Key::Backtab => ctx.e(Event::ShiftLeft),
                Key::Char(c) => {
                    if self.mode == ViMode::Replace {
                        ctx.e(Event::Delete);
                    }
                    ctx.e(Event::Insert(c));
                }
                Key::Down => ViCmd::default().motion(Motion::Down, ctx),
                Key::Delete => ctx.e(Event::Delete),
                Key::End => ViCmd::default().motion(Motion::End, ctx),
                Key::Enter => ctx.e(Event::NewLine),
                Key::Escape => {
                    ViCmd::default().motion(Motion::LeftInLine, ctx);
                    ctx.finish_change();
                    self.reset();
                }
                Key::Home => ViCmd::default().motion(Motion::Home, ctx),
                Key::Left => ViCmd::default().motion(Motion::LeftInLine, ctx),
                Key::PageDown => ViCmd::default().motion(Motion::PageDown, ctx),
                Key::PageUp => ViCmd::default().motion(Motion::PageUp, ctx),
                Key::Right => ViCmd::default().motion(Motion::RightInLine, ctx),
                Key::Tab => ctx.e(Event::ShiftRight),
                Key::Up => ViCmd::default().motion(Motion::Up, ctx),
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
                    ctx.e(Event::SetSearch(tmp, forwards));
                    self.reset();
                    ViCmd::default().motion(Motion::NextSearch, ctx);
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

        // Reset mode after operators
        if let Some(mode) = ctx.set_mode.take() {
            self.mode = mode;
        }

        // Save change state
        self.pending_change = ctx.pending_change.take();
        if let Some(change) = ctx.change.take() {
            self.last_change = Some(change);
        }

        //TODO: optimize redraw
        ctx.e(Event::Redraw);
    }
}
