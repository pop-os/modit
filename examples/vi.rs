use modit::{Event, Parser, ViParser, ESCAPE};

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
    println!("{:#?}", parse(&format!("iHello, World!{ESCAPE}")));

    println!("{:#?}", parse(&format!("12w")));
}
