use std::collections::HashMap;

use actix::prelude::*;
use futures::Future;
use regex::Regex as RegexMatcher;

use backends::Message;
use metrics::Measurement;

pub struct Regex(HashMap<String, (RegexMatcher, String)>, Recipient<Message>);
impl Regex {
    pub fn launch(mut config: Vec<(String, String, String)>, upstream: Recipient<Message>) -> Recipient<Message> {
        let rules = config
            .drain(..)
            .map(|(key, replace, regex)| (key, (RegexMatcher::new(&regex).unwrap(), replace)))
            .collect();

        Regex(rules, upstream).start().recipient()
    }

    fn filter_tags(&self, msg: &mut Measurement) {
        for (key, value) in msg.tags.iter_mut() {
            if let Some((regex, replace)) = self.0.get(key) {
                if regex.is_match(value) {
                    *value = replace.clone();
                }
            }
        }
    }
}

impl Actor for Regex {
    type Context = Context<Self>;
}

impl Handler<Message> for Regex {
    type Result = ();

    fn handle(&mut self, mut msg: Message, _ctx: &mut Context<Self>) -> Self::Result {
        match msg {
            Message::List(ref mut ms) => for mut m in ms {
                self.filter_tags(&mut m);
            },
            Message::Single(ref mut m) => self.filter_tags(m),
        }

        ::actix::spawn(self.1.send(msg).map_err(|_| ()));
    }
}
