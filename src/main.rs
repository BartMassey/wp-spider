use std::collections::{BTreeMap, BTreeSet};
use std::sync::mpsc;
use std::thread;
use std::time;

use threadpool::ThreadPool;
use wikipedia::{http::default::Client, Wikipedia};

const NWORKERS: usize = 20;

const MAX_DEPTH: usize = 2;

// Requests per second.
const RATE_LIMIT: f32 = 10.0;

type EdgeSender = mpsc::Sender<(usize, String, String)>;

struct State {
    tp: ThreadPool,
    tx: EdgeSender,
    st: time::Instant,
}

impl State {
    fn new(tx: EdgeSender) -> Self {
        let tp = ThreadPool::new(NWORKERS);
        let st = time::Instant::now();
        Self { tp, tx, st }
    }

    fn step(&self, depth: usize, title: String) {
        let tx = self.tx.clone();
        self.tp.execute(move || {
            let wp = Wikipedia::new(Client::default());
            let page = wp.page_from_title(title.clone());
            let links = page
                .get_links()
                .unwrap()
                .map(|l| l.title);
            for l in links {
                tx.send((depth + 1, title.clone(), l)).unwrap();
            }
        });
    }
}

fn main() {
    let mut s = BTreeMap::new();
    let (tx, rx) = mpsc::channel();
    let state = State::new(tx);
    state.step(0, "Rust (programming language)".into());
    let mut nreqs = 1;

    while let Ok((depth, page_title, link_title)) = rx.recv() {
        if depth >= MAX_DEPTH || s.contains_key(&link_title) {
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
        if page_no > secs * RATE_LIMIT {
            let st = page_no / RATE_LIMIT - secs;
            eprintln!("{}s sleep", st);
            thread::sleep(time::Duration::from_secs_f32(st));
        }

        state.step(depth, link_title);

        let secs = state.st.elapsed().as_secs_f32();
        eprintln!("{} rps", nreqs as f32 / secs);
    }

    for (k, vs) in s {
        println!("{:?}", k);
        for v in vs {
            println!("  {:?}", v);
        }
    }
}
