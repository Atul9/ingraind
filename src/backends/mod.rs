use actix::{Message, Recipient};

pub mod s3;
pub mod statsd;
pub mod console;

use metrics::Measurement;

pub type Backend = Recipient<Measurement>;
impl Message for Measurement {
    type Result = ();
}

#[derive(Message)]
pub struct Flush;
