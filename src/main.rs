use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::thread;
use std::time;

use wikipedia::{http::default::Client, Wikipedia};

const MAX_DEPTH: usize = 2;

// Requests per second.
const RATE_LIMIT: usize = 10;

struct State {
    wp: Wikipedia<Client>,
    q: VecDeque<(usize, String)>,
    s: BTreeMap<String, BTreeSet<String>>,
    st: time::Instant,
}

impl State {
    fn new(start: &str) -> Self {
        let wp = Wikipedia::new(Client::default());
        let q = VecDeque::new();
        let s = BTreeMap::new();
        let st = time::Instant::now();
        let mut result = Self { wp, q, s, st };
        result.step(start.into(), 0);
        result
    }

    fn step(&mut self, title: String, depth: usize) {
        if depth >= MAX_DEPTH {
            return;
        }

        
        let nfound = self.s.len();
        self.s.entry(title).or_insert_with_key(|k| {
            let dt = self.st.elapsed().as_secs_f32();
            let page_no = nfound as f32 + 1.0;
            let rl = RATE_LIMIT as f32;
            if dt * rl > 1.0 && page_no > dt * rl {
                let st = page_no / rl - dt;
                eprintln!("{}s sleep", st);
                thread::sleep(time::Duration::from_secs_f32(st));
            }

            let page = self.wp.page_from_title(k.into());
            let links: BTreeSet<String> = page
                .get_links()
                .unwrap()
                .map(|l| l.title)
                .collect();
            for l in &links {
                self.q.push_back((depth + 1, l.into()));
            }
            links
        });
    }
}

fn main() {
    let mut state = State::new("Rust (programming language)");
    while let Some((depth, title)) = state.q.pop_front() {
        let rate = state.s.len() as f32 / state.st.elapsed().as_secs_f32();
        eprintln!("{} rps", rate);
        state.step(title, depth);
    }

    for (k, vs) in state.s {
        println!("{:?}", k);
        for v in vs {
            println!("  {:?}", v);
        }
    }
}
