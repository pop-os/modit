use modit::{Event, Key, Motion, Parser, ViMode, ViParser};
use std::{
    env, fs,
    io::{self, Write},
};
use termion::{
    event::Key as TermionKey, input::TermRead, raw::IntoRawMode, screen::IntoAlternateScreen,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Cursor {
    pub line: usize,
    pub index: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LayoutCursor {
    pub row: usize,
    pub col: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InsertError {
    InvalidLine(Cursor),
    InvalidIndex(Cursor),
}

struct Editor {
    lines: Vec<String>,
    width: usize,
    height: usize,
    redraw: bool,
    scroll: LayoutCursor,
}

impl Editor {
    pub fn delete_char(&mut self, cursor: &mut Cursor) -> Result<Option<char>, InsertError> {
        let line = self
            .lines
            .get_mut(cursor.line)
            .ok_or(InsertError::InvalidLine(*cursor))?;
        if cursor.index >= line.len() {
            return Ok(None);
        }
        if !line.is_char_boundary(cursor.index) {
            return Err(InsertError::InvalidIndex(*cursor));
        }
        Ok(Some(line.remove(cursor.index)))
    }

    pub fn insert_char(&mut self, cursor: &mut Cursor, c: char) -> Result<(), InsertError> {
        let line = self
            .lines
            .get_mut(cursor.line)
            .ok_or(InsertError::InvalidLine(*cursor))?;
        if !line.is_char_boundary(cursor.index) {
            return Err(InsertError::InvalidIndex(*cursor));
        }
        match c {
            '\n' => {
                let after = line.split_off(cursor.index);
                cursor.line += 1;
                cursor.index = 0;
                self.lines.insert(cursor.line, after);
            }
            _ => {
                line.insert(cursor.index, c);
                cursor.index += c.len_utf8();
            }
        }
        Ok(())
    }

    pub fn layout<T, F: FnMut(Cursor, LayoutCursor, Option<char>) -> Option<T>>(
        &self,
        mut callback: F,
    ) -> Option<T> {
        let mut layout_cursor = LayoutCursor { row: 0, col: 0 };
        for (line_i, line) in self.lines.iter().enumerate() {
            for (char_i, c) in line.char_indices() {
                match unicode_width::UnicodeWidthChar::width(c) {
                    Some(char_width) => {
                        if layout_cursor.col + char_width >= self.width {
                            layout_cursor.row += 1;
                            layout_cursor.col = 0;
                        }

                        if let Some(t) = callback(
                            Cursor {
                                line: line_i,
                                index: char_i,
                            },
                            layout_cursor,
                            Some(c),
                        ) {
                            return Some(t);
                        }

                        layout_cursor.col += char_width;
                    }
                    None => {
                        eprintln!("no char width for {:?}", c);
                    }
                }
            }

            if let Some(t) = callback(
                Cursor {
                    line: line_i,
                    index: line.len(),
                },
                layout_cursor,
                None,
            ) {
                return Some(t);
            }

            layout_cursor.row += 1;
            layout_cursor.col = 0;
        }

        None
    }

    pub fn cursor(&self, layout_cursor: LayoutCursor) -> Option<Cursor> {
        self.layout(|cursor, other_layout_cursor, _c| {
            if layout_cursor == other_layout_cursor {
                Some(cursor)
            } else {
                None
            }
        })
    }

    pub fn layout_cursor(&self, cursor: Cursor) -> Option<LayoutCursor> {
        self.layout(|other_cursor, layout_cursor, _c| {
            if cursor == other_cursor {
                Some(layout_cursor)
            } else {
                None
            }
        })
    }

    pub fn motion(&mut self, mut cursor: Cursor, motion: Motion) -> Option<Cursor> {
        match motion {
            Motion::Down => {
                let mut layout_cursor = self.layout_cursor(cursor)?;
                layout_cursor.row = layout_cursor.row.checked_add(1)?;
                return self.cursor(layout_cursor);
            }
            Motion::End => {
                let line = self.lines.get(cursor.line)?;
                cursor.index = line.len();
                return Some(cursor);
            }
            Motion::Home => {
                cursor.index = 0;
                return Some(cursor);
            }
            Motion::Left | Motion::LeftInLine => {
                let line = self.lines.get(cursor.line)?;
                match line
                    .get(..cursor.index)
                    .and_then(|x| x.chars().rev().next())
                {
                    Some(c) => {
                        cursor.index = cursor.index.checked_sub(c.len_utf8())?;
                        return Some(cursor);
                    }
                    None => {
                        if let Motion::Left = motion {
                            cursor.line = cursor.line.checked_sub(1)?;
                            if let Some(new_line) = self.lines.get(cursor.line) {
                                cursor.index = new_line.len();
                                return Some(cursor);
                            }
                        }
                    }
                }
            }
            Motion::PageDown => {
                self.scroll.row = self
                    .scroll
                    .row
                    .saturating_add(self.height.saturating_sub(1));
                let mut max_row = 0;
                self.layout(|_, layout_cursor, _| {
                    if layout_cursor.row > max_row {
                        max_row = layout_cursor.row.saturating_add(1);
                    }
                    None::<()>
                });
                let max_scroll_row = max_row.saturating_sub(self.height.saturating_sub(1));
                if self.scroll.row > max_scroll_row {
                    self.scroll.row = max_scroll_row;
                }
            }
            Motion::PageUp => {
                self.scroll.row = self
                    .scroll
                    .row
                    .saturating_sub(self.height.saturating_sub(1));
            }
            Motion::Right | Motion::RightInLine => {
                let line = self.lines.get(cursor.line)?;
                match line.get(cursor.index..).and_then(|x| x.chars().next()) {
                    Some(c) => {
                        cursor.index = cursor.index.checked_add(c.len_utf8())?;
                        return Some(cursor);
                    }
                    None => {
                        if let Motion::Right = motion {
                            cursor.line = cursor.line.checked_add(1)?;
                            if cursor.line < self.lines.len() {
                                cursor.index = 0;
                                return Some(cursor);
                            }
                        }
                    }
                }
            }
            Motion::SoftHome => {
                let line = self.lines.get(cursor.line)?;
                cursor.index = 0;
                for (i, c) in line.char_indices() {
                    if !c.is_whitespace() {
                        cursor.index = i;
                        break;
                    }
                }
                return Some(cursor);
            }
            Motion::Up => {
                let mut layout_cursor = self.layout_cursor(cursor)?;
                layout_cursor.row = layout_cursor.row.checked_sub(1)?;
                return self.cursor(layout_cursor);
            }
            _ => {
                eprintln!("TODO: {:?}", motion);
            }
        }
        None
    }

    pub fn draw<W: Write>(&self, w: &mut W, cursor: Cursor, parser: &ViParser) -> io::Result<()> {
        write!(w, "{}{}", termion::clear::All, termion::cursor::Goto(1, 1))?;
        if let Some(err) = self.layout(|draw_cursor, layout_cursor, c_opt| {
            if layout_cursor.row >= self.scroll.row
                && layout_cursor.row
                    < self
                        .height
                        .saturating_add(self.scroll.row)
                        .saturating_sub(1)
                && layout_cursor.col >= self.scroll.col
                && layout_cursor.col < self.width.saturating_add(self.scroll.col)
            {
                let row = layout_cursor.row.saturating_sub(self.scroll.row);
                let col = layout_cursor.col.saturating_sub(self.scroll.col);
                let write_res = if cursor == draw_cursor {
                    write!(
                        w,
                        "{}{}{}{}",
                        termion::style::Invert,
                        termion::cursor::Goto(col as u16 + 1, row as u16 + 1),
                        c_opt.unwrap_or(' '),
                        termion::style::Reset,
                    )
                } else {
                    match c_opt {
                        Some(c) => write!(
                            w,
                            "{}{}",
                            termion::cursor::Goto(col as u16 + 1, row as u16 + 1),
                            c
                        ),
                        None => Ok(()),
                    }
                };
                if let Err(err) = write_res {
                    return Some(err);
                }
            }
            None
        }) {
            return Err(err);
        }

        write!(w, "{}", termion::cursor::Goto(1, self.height as u16))?;
        match &parser.mode {
            ViMode::Normal => {
                write!(w, "{}", parser.cmd)?;
            }
            ViMode::Insert => {
                write!(w, "-- INSERT --")?;
            }
            ViMode::Extra(extra) => {
                write!(w, "{}{}", parser.cmd, extra)?;
            }
            ViMode::Replace => {
                write!(w, "-- REPLACE --")?;
            }
            ViMode::Visual => {
                write!(w, "-- VISUAL -- {}", parser.cmd)?;
            }
            ViMode::VisualLine => {
                write!(w, "-- VISUAL LINE -- {}", parser.cmd)?;
            }
            ViMode::Command { value } => {
                write!(
                    w,
                    ":{value}{} {}",
                    termion::style::Invert,
                    termion::style::Reset
                )?;
            }
            ViMode::Search { value, forwards } => {
                if *forwards {
                    write!(
                        w,
                        "/{value}{} {}",
                        termion::style::Invert,
                        termion::style::Reset
                    )?;
                } else {
                    write!(
                        w,
                        "?{value}{} {}",
                        termion::style::Invert,
                        termion::style::Reset
                    )?;
                }
            }
        }

        w.flush()
    }
}

fn main() {
    let mut lines = Vec::new();
    if let Some(arg) = env::args().nth(1) {
        match fs::read_to_string(&arg) {
            Ok(data) => {
                for line in data.lines() {
                    lines.push(line.to_string());
                }
            }
            Err(err) => {
                eprintln!("failed to read {:?}: {}", arg, err);
            }
        }
    }

    // Ensure lines has at least one line
    if lines.is_empty() {
        lines.push(String::new());
    }

    let (width, height) = match termion::terminal_size() {
        Ok(ok) => ok,
        Err(err) => {
            eprintln!("failed to get terminal size: {}", err);
            (80, 24)
        }
    };

    let mut stdout = termion::cursor::HideCursor::from(
        io::stdout()
            .into_alternate_screen()
            .unwrap()
            .into_raw_mode()
            .unwrap(),
    );

    let mut cursor = Cursor { line: 0, index: 0 };
    let mut parser = ViParser::new();
    let mut editor = Editor {
        lines,
        width: width.into(),
        height: height.into(),
        redraw: false,
        scroll: LayoutCursor { row: 0, col: 0 },
    };

    editor.draw(&mut stdout, cursor, &parser).unwrap();

    for termion_key_res in io::stdin().keys() {
        let termion_key = match termion_key_res {
            Ok(ok) => ok,
            Err(err) => {
                eprintln!("error reading keys: {}", err);
                break;
            }
        };
        let key = match termion_key {
            TermionKey::Backspace => Key::Backspace,
            TermionKey::Left => Key::Left,
            TermionKey::Right => Key::Right,
            TermionKey::Up => Key::Up,
            TermionKey::Down => Key::Down,
            TermionKey::Home => Key::Home,
            TermionKey::End => Key::End,
            TermionKey::PageUp => Key::PageUp,
            TermionKey::PageDown => Key::PageDown,
            TermionKey::BackTab => Key::Backtab,
            TermionKey::Delete => Key::Delete,
            TermionKey::Insert => continue,
            TermionKey::F(n) => continue,
            TermionKey::Char(c) => Key::Char(c),
            TermionKey::Alt(c) => continue,
            TermionKey::Ctrl(c) => match c {
                'c' => break,
                _ => Key::Ctrl(c),
            },
            TermionKey::Null => continue,
            TermionKey::Esc => Key::Escape,
            _ => continue,
        };
        eprintln!("Key: {:?}", key);
        parser.parse(key, false, |event| {
            eprintln!("Event: {:?}", event);
            match event {
                Event::Delete => {
                    match editor.delete_char(&mut cursor) {
                        Ok(Some(_)) => {}
                        Ok(None) => {
                            // Join lines
                            if let Some(next_line_i) = cursor.line.checked_add(1) {
                                if next_line_i < editor.lines.len() {
                                    let next_line = editor.lines.remove(next_line_i);
                                    if let Some(line) = editor.lines.get_mut(cursor.line) {
                                        cursor.index = line.len();
                                        line.insert_str(cursor.index, &next_line);
                                    }
                                }
                            }
                        }
                        Err(err) => {
                            eprintln!("failed to delete: {:?}", err);
                        }
                    }
                }
                Event::DeleteInLine => match editor.delete_char(&mut cursor) {
                    Ok(_) => {}
                    Err(err) => {
                        eprintln!("failed to delete: {:?}", err);
                    }
                },
                Event::Insert(c) => match editor.insert_char(&mut cursor, c) {
                    Ok(()) => {}
                    Err(err) => {
                        eprintln!("failed to insert {:?}: {:?}", c, err);
                    }
                },
                Event::Motion(motion) => {
                    if let Some(new_cursor) = editor.motion(cursor, motion) {
                        cursor = new_cursor;
                    }
                }
                Event::NewLine => match editor.insert_char(&mut cursor, '\n') {
                    Ok(()) => {}
                    Err(err) => {
                        eprintln!("failed to insert new line: {:?}", err);
                    }
                },
                Event::Redraw => {
                    editor.redraw = true;
                }
                _ => {
                    eprintln!("TODO {:?}", event);
                }
            }
        });
        if editor.redraw {
            editor.draw(&mut stdout, cursor, &parser).unwrap();
            editor.redraw = false;
        }
    }
}
