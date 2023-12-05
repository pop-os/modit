use modit::{Event, Parser, ViParser, BACKSPACE, DELETE, ESCAPE};
use std::io::{self, Read, Write};
use termion::{event::Key, input::TermRead, raw::IntoRawMode};

fn parse(string: &str) -> Vec<Event> {
    let mut parser = ViParser::new();
    let mut events = Vec::new();
    //TODO: what to do with selection
    let selection = false;
    for c in string.chars() {
        parser.parse(c, selection, |event| events.push(event));
    }
    events
}

fn main() {
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("info"));

    let mut stdout = io::stdout().into_raw_mode().unwrap();
    let mut stdin = io::stdin();

    let mut parser = ViParser::new();
    for key_res in stdin.keys() {
        let key = key_res.unwrap();
        write!(stdout, "{:?}\r\n", key).unwrap();
        let c = match key {
            Key::Backspace => BACKSPACE,
            Key::Ctrl('c') => break,
            Key::Char(c) => c,
            Key::Delete => DELETE,
            Key::Esc => ESCAPE,
            _ => continue,
        };
        parser.parse(c, false, |event| {
            write!(stdout, "  {:?}\r\n", event).unwrap();
            stdout.flush().unwrap();
        });
    }

    /*
    println!("{:#?}", parse(&format!("iHello, World!{ESCAPE}")));

    println!("{:#?}", parse(&format!("10w")));

    println!("{:#?}", parse(&format!("cw")));

    println!("{:#?}", parse(&format!("diw")));

    println!("{:#?}", parse(&format!("dap")));
    */
}
