use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::sync::mpsc;
use std::thread;
use std::time;

use clap::Parser;
use serde_json::Value as JsonValue;
use threadpool::ThreadPool;
use wikipedia::{http::{default::Client, HttpClient}, Wikipedia};

const WIKIPEDIA_KEYFILE: &str = ".wikimedia-api-key";

#[derive(Debug, Parser)]
struct Args {
    /// Title of starting wikipedia page.
    #[arg(short, long, default_value="Rust (programming language)")]
    root: String,
    /// Recursion depth including root node.
    #[arg(short, long, default_value="3")]
    depth: usize,
    /// Rate limit in requests per second.
    #[arg(short, long, default_value="1.5")]
    limit: f32,
    /// Worker count for thread pool.
    #[arg(short, long, default_value="20")]
    workers: usize,
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

type EdgeSender = mpsc::Sender<Option<(usize, String, String)>>;

struct State {
    tp: ThreadPool,
    tx: EdgeSender,
    st: time::Instant,
    bt: Option<(String, String)>,
}

impl State {
    fn new(tx: EdgeSender, nworkers: usize) -> Self {
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
                .map(|l| l.title);
            for l in links {
                tx.send(Some((depth + 1, title.clone(), l))).unwrap();
            }
            tx.send(None).unwrap();
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
    let mut nreqs = 1;

    while let Ok(r) = rx.recv() {
        match r {
            Some((depth, page_title, link_title)) => {
                if depth >= max_depth || s.contains_key(&link_title) {
                    continue;
                }

                eprintln!("{} → {}", page_title, link_title);

                s.entry(page_title)
                    .and_modify(|v: &mut BTreeSet<String>| {
                        v.insert(link_title.clone());
                    })
                    .or_insert_with(|| {
                        let mut vs = BTreeSet::new();
                        vs.insert(link_title.clone());
                        vs
                    });

                nreqs += 1;
                let page_no = nreqs as f32;
                let secs = state.st.elapsed().as_secs_f32();
                if page_no > secs * limit {
                    let st = page_no / limit - secs;
                    eprintln!("{}s sleep", st);
                    thread::sleep(time::Duration::from_secs_f32(st));
                }

                outstanding += 1;
                state.step(depth, link_title);

                let secs = state.st.elapsed().as_secs_f32();
                eprintln!("{} rps", nreqs as f32 / secs);
            }
            None => {
                outstanding -= 1;
                if outstanding == 0 {
                    break;
                }
            }
        }
    }

    for (k, vs) in s {
        println!("{:?}", k);
        for v in vs {
            println!("  {:?}", v);
        }
    }
}
