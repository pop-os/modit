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
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("info"));

    println!("{:#?}", parse(&format!("iHello, World!{ESCAPE}")));

    println!("{:#?}", parse(&format!("10w")));

    println!("{:#?}", parse(&format!("cw")));

    println!("{:#?}", parse(&format!("diw")));

    println!("{:#?}", parse(&format!("dap")));
}
