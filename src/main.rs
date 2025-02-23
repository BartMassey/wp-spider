use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::sync::mpsc;
use std::thread;
use std::time;

use clap::Parser;
use serde::Serialize;
use serde_json::Value as JsonValue;
use threadpool::ThreadPool;
use wikipedia::{
    http::{default::Client, HttpClient},
    Wikipedia,
};

const WIKIPEDIA_KEYFILE: &str = ".wikimedia-api-key";

#[derive(Debug, Parser)]
struct Args {
    /// Title of starting wikipedia page.
    #[arg(short, long, default_value = "Rust (programming language)")]
    root: String,
    /// Recursion depth including root node.
    #[arg(short, long, default_value = "1")]
    depth: usize,
    /// Rate limit in requests per second.
    #[arg(short, long, default_value = "1.5")]
    limit: f32,
    /// Worker count for thread pool.
    #[arg(short, long, default_value = "20")]
    workers: usize,
    /// Output JSON file.
    #[arg(short, long, default_value = "map.json")]
    outfile: String,
}

fn get_wikimedia_info() -> Option<(String, String)> {
    #[allow(deprecated)]
    let mut keypath = std::env::home_dir()?;
    keypath.push(WIKIPEDIA_KEYFILE);
    let f = File::open(keypath).ok()?;
    let value: JsonValue = serde_json::from_reader(f).ok()?;

    if let JsonValue::Object(map) = value {
        let value = map.get("Access token")?;
        if let JsonValue::String(token) = value {
            let value = map.get("Email address")?;
            if let JsonValue::String(email) = value {
                return Some((token.into(), email.into()));
            }
        }
    }

    None
}

#[derive(Debug, Clone, PartialOrd, Ord, PartialEq, Eq, Serialize)]
struct Link {
    depth: usize,
    title: String,
}

impl Link {
    fn new(depth: usize, title: String) -> Self {
        Self { depth, title }
    }
}

#[derive(Debug, Serialize)]
struct Entry {
    title: String,
    links: BTreeSet<Link>,
}

impl Entry {
    fn new(title: String, links: BTreeSet<Link>) -> Self {
        Self { title, links }
    }
}

type EntrySender = mpsc::Sender<Entry>;

struct State {
    tp: ThreadPool,
    tx: EntrySender,
    st: time::Instant,
    bt: Option<(String, String)>,
}

impl State {
    fn new(tx: EntrySender, nworkers: usize) -> Self {
        let tp = ThreadPool::new(nworkers);
        let st = time::Instant::now();
        let bt = get_wikimedia_info();
        Self { tp, tx, st, bt }
    }

    fn step(&self, depth: usize, title: String) {
        let tx = self.tx.clone();
        let mut client = Client::default();
        if let Some((ref bt, ref email)) = self.bt {
            client.bearer_token(bt.clone());
            client.user_agent(email.clone());
        }
        self.tp.execute(move || {
            let wp = Wikipedia::new(client);

            let page = wp.page_from_title(title.clone());
            let links = page
                .get_links()
                .unwrap()
                .map(|l| Link::new(depth + 1, l.title))
                .collect();
            let entry = Entry::new(title, links);
            let _ = tx.send(entry);
        });
    }
}

fn main() {
    let args = Args::parse();
    let root = args.root;
    let max_depth = args.depth;
    let limit = args.limit;

    let mut s = BTreeMap::new();
    let (tx, rx) = mpsc::channel();
    let state = State::new(tx, args.workers);
    let mut outstanding = 1;
    state.step(0, root);

    for page_no in 1.. {
        if outstanding == 0 {
            break;
        }
        let entry = rx.recv().unwrap();
        for l in &entry.links {
            if l.depth >= max_depth || s.contains_key(&l.title) {
                continue;
            }

            eprintln!("{} → {} ({})", entry.title, l.title, l.depth);

            let secs = state.st.elapsed().as_secs_f32();
            if page_no as f32 > secs * limit {
                let st = page_no as f32 / limit - secs;
                eprintln!("{}s sleep", st);
                thread::sleep(time::Duration::from_secs_f32(st));
            }

            outstanding += 1;
            state.step(l.depth, l.title.clone());

            let secs = state.st.elapsed().as_secs_f32();
            eprintln!("{} rps", page_no as f32 / secs);
        }

        if s.insert(entry.title, entry.links).is_some() {
            panic!("internal error: re-inserted link");
        }
        outstanding -= 1;
    }

    let output = File::create(args.outfile).unwrap();
    serde_json::to_writer(output, &s).unwrap();
}
