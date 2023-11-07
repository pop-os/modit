use modit::{Word, WordIter};

fn main() {
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("info"));

    println!("Testing words");
    for (i, word) in WordIter::new(".test.some....words    ", Word::Lower) {
        println!("{}: {:?}", i, word);
    }

    println!("Testing WORDs");
    for (i, word) in WordIter::new(".test.some    words    ", Word::Upper) {
        println!("{}: {:?}", i, word);
    }
}
