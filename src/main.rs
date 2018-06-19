extern crate chrono;
extern crate failure;
extern crate libc;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;
extern crate uuid;

use std::env;
use std::net::Ipv4Addr;
use std::ptr;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

mod grains;

use chrono::DateTime;
use failure::Error;

#[derive(Debug, Serialize, Deserialize)]
struct Connection {
    pid: u32,
    name: String,
    source_ip: Ipv4Addr,
    destination_ip: Ipv4Addr,
    destination_port: u16,
}

impl From<data_t> for Connection {
    fn from(data: data_t) -> Connection {
        Connection {
            pid: data.id as u32,
            name: get_string(&data.comm),
            source_ip: to_ip(data.saddr),
            destination_ip: to_ip(data.daddr),
            destination_port: data.dport,
        }
    }
}

//
// Define the struct the BPF code writes in Rust.
// This must match `struct data_t` in `bpf.c`
#[repr(C)]
#[derive(Debug)]
struct data_t {
    id: u64,
    ts: u64,
    comm: [u8; 16], // TASK_COMM_LEN
    saddr: u32,
    daddr: u32,
    dport: u16,
}

impl<'a> From<&'a [u8]> for data_t {
    fn from(x: &'a [u8]) -> data_t {
        unsafe { ptr::read(x.as_ptr() as *const data_t) }
    }
}

fn to_ip(bytes: u32) -> Ipv4Addr {
    let d = (bytes >> 24) as u8;
    let c = (bytes >> 16) as u8;
    let b = (bytes >> 8) as u8;
    let a = bytes as u8;

    Ipv4Addr::new(a, b, c, d)
}

fn get_string(x: &[u8]) -> String {
    match x.iter().position(|&r| r == 0) {
        Some(zero_pos) => String::from_utf8_lossy(&x[0..zero_pos]).to_string(),
        None => String::from_utf8_lossy(x).to_string(),
    }
}

fn main() -> Result<(), Error> {
    // let instance_name =
    //     env::var("TCPSNIFF_ID").expect("Need to set INSTANCE_NAME environment variable");
    // let url_base = env::var("TCPSNIFF_URL").expect("Need to set SIFT_URL environment variable");
    // let events: Arc<Mutex<Vec<Connection>>> = Arc::new(Mutex::default());
    // let mut module = BPF::new(BPF_CODE)?;

    // // load + attach kprobes!
    // let return_probe = module.load_kprobe("trace_outbound_return")?;
    // let entry_probe = module.load_kprobe("trace_outbound_entry")?;
    // module.attach_kprobe("tcp_v4_connect", entry_probe)?;
    // module.attach_kretprobe("tcp_v4_connect", return_probe)?;

    // // the "events" table is where the "open file" events get sent
    // let table = module.table("events");

    // // install a callback to print out file open events when they happen
    // let mut perf_map = init_perf_map(table, || {
    //     let events = events.clone();
    //     Box::new(move |x| {
    //         // This callback
    //         let data = Connection::from(data_t::from(x));
    //         println!("{:-7} {:-16}: {:#?}", data.pid, &data.name, data);

    //         events
    //             .lock()
    //             .map(|mut e| {
    //                 e.push(data);
    //             })
    //             .unwrap();
    //     })
    // })?;

    // let reporter = thread::spawn(move || {
    //     let events = events.clone();

    //     loop {
    //         thread::sleep(Duration::from_secs(60));

    //         events
    //             .lock()
    //             .map(|mut data| {
    //                 Envelope::new(instance_name.clone(), Report::new(&data)).send(&url_base);
    //                 data.clear();
    //             })
    //             .unwrap();
    //     }
    // });

    // loop {
    //     perf_map.poll(200);
    // }
    Ok(())
}
