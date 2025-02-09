use std::collections::{BTreeMap, BTreeSet, VecDeque};

use wikipedia::{http::default::Client, Wikipedia};

const MAX_DEPTH: usize = 1;

struct State {
    wp: Wikipedia<Client>,
    q: VecDeque<(usize, String)>,
    s: BTreeMap<String, BTreeSet<String>>,
}

impl State {
    fn new(start: &str) -> Self {
        let wp = Wikipedia::new(Client::default());
        let q = VecDeque::new();
        let s = BTreeMap::new();
        let mut result = Self { wp, q, s };
        result.step(start.into(), 0);
        result
    }

    fn step(&mut self, title: String, depth: usize) {
        if depth >= MAX_DEPTH {
            return;
        }
        self.s.entry(title).or_insert_with_key(|k| {
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
        state.step(title, depth);
    }

    for (k, vs) in state.s {
        println!("{:?}", k);
        for v in vs {
            println!("  {:?}", v);
        }
    }
}
