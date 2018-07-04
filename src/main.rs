#![cfg_attr(feature = "cargo-clippy", allow(clippy))]

#[macro_use]
extern crate actix;
extern crate failure;
extern crate futures;
extern crate libc;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate cadence;
extern crate redbpf;
extern crate rusoto_core;
extern crate rusoto_s3;
extern crate serde_json;
extern crate uuid;

use std::env;
use std::thread;
use std::time::Duration;

mod backends;
mod grains;
mod metrics;
use grains::*;

use actix::Actor;

use backends::{s3, s3::S3, statsd::Statsd};

fn main() {
    let system = actix::System::new("outbound");
    // let statsd = Statsd::new("127.0.0.1", 8125);
    // let statsd_backend = statsd.start().recipient();

    let s3_addr = S3::create(|ctx| {
        use actix::prelude::*;
        let bucket = env::var("AWS_BUCKET").unwrap();
        let interval = u16::from_str_radix(&env::var("AWS_INTERVAL").unwrap(), 10).unwrap();

        ctx.run_interval(Duration::from_secs(interval), |_, ctx| {
            ctx.address().do_send(backends::Flush)
        });
        S3::new(s3::Region::EuWest2, bucket)
    }).recipient();

    thread::spawn(move || {
        let mut mod_tcp4 = Grain::<tcpv4::TCP4>::load().unwrap().bind(&s3_addr);
        let mut mod_udp = Grain::<udp::UDP>::load().unwrap().bind(&s3_addr);

        loop {
            mod_tcp4.poll();
            mod_udp.poll();
        }
    });

    system.run();
}
