#![allow(non_camel_case_types)]

use crate::grains::{self, *};

use ingraind_probes::connection::{Connection, Ipv6Addr, Message};
use redbpf_probes::bindings::{IPPROTO_TCP, IPPROTO_UDP};

use std::net;

pub struct TCP4;

impl EBPFProbe for Grain<TCP4> {
    fn attach(&mut self) -> MessageStreams {
        self.attach_kprobes()
    }
}

impl EBPFGrain<'static> for TCP4 {
    fn code() -> &'static [u8] {
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/ingraind-probes/target/release/bpf-programs/connection/connection.elf"
        ))
    }

    fn get_handler(&self, id: &str) -> EventCallback {
        match id {
            "ip_connections" => Box::new(|raw| {
                let event = unsafe { std::ptr::read(raw.as_ptr() as *const Connection) };

                Some(grains::Message::Single(Measurement::new(
                    COUNTER | HISTOGRAM | METER,
                    "connection.out".to_string(),
                    Unit::Count(1),
                    conn_tags(&event),
                )))
            }),

            "ip_volume" => Box::new(|raw| {
                let event = unsafe { std::ptr::read(raw.as_ptr() as *const Message) };
		let (name, conn, vol) = match event {
		    Message::Send(conn, size) => ("volume.out", conn, size),
		    Message::Receive(conn, size) => ("volume.in", conn, size)
		};

		let proto = match conn.typ {
		    IPPROTO_TCP => "tcp",
		    IPPROTO_UDP => "udp",
		    _ => "unknown"
		};

		let mut tags = conn_tags(&conn);
		tags.insert("proto", proto);

                Some(grains::Message::Single(Measurement::new(
                    COUNTER | HISTOGRAM,
                    name.to_string(),
                    Unit::Byte(vol),
                    tags,
                )))
            }),
            _ => unreachable!(),
        }
    }
}

fn conn_tags(event: &Connection) -> Tags {
    let mut tags = Tags::new();
    tags.insert("process_str", to_string(&event.comm));
    tags.insert("process_id", event.pid.to_string());
    tags.insert("d_ip", to_ipv6(&event.daddr).to_string());
    tags.insert("s_ip", to_ipv6(&event.saddr).to_string());
    tags.insert("d_port", to_le(event.dport as u16).to_string());
    tags.insert("s_port", to_le(event.sport as u16).to_string());

    tags
}

fn to_ipv6(addr: &Ipv6Addr) -> &net::Ipv6Addr {
    unsafe { std::mem::transmute(addr) }
}
