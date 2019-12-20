use std::collections::HashMap;

use serde_json;

use super::{Kind, Measurement, Message, Unit};

#[derive(Serialize, Deserialize, Debug)]
pub enum Encoding {
    JSON,
    #[cfg(feature = "capnp")]
    Capnp,
}

pub type Encoder = Box<dyn Fn(&[Measurement]) -> Vec<u8>>;

impl Encoding {
    pub fn to_encoder(&self) -> Encoder {
        Box::new(match self {
            Encoding::JSON => to_json,
            #[cfg(feature = "capnp")]
            Encoding::Capnp => to_capnp,
        })
    }
}

#[cfg(feature = "capnp-encoding")]
pub fn to_capnp(src: &[Measurement]) -> Vec<u8> {
    use crate::ingraind_capnp::*;
    use capnp::serialize;
    use std::io::Cursor;

    let mut message = ::capnp::message::Builder::new_default();
    let payload = message.init_root::<ingrain_payload::Builder>();

    let mut data = payload.init_data(src.len() as u32);
    for (i, source) in src.iter().enumerate() {
        let mut m = data.reborrow().get(i as u32);
        m.set_timestamp(source.timestamp);
        m.set_kind(source.kind);
        m.set_name(&serialized_name(&source));
        m.set_measurement(source.value.get() as f64);

        let mut tags = m.init_tags(source.tags.0.len() as u32);
        for (i, source) in source.tags.0.iter().enumerate() {
            let mut tag = tags.reborrow().get(i as u32);
            tag.set_key(&source.0);
            tag.set_value(&source.1);
        }
    }

    let mut buffer = Cursor::new(Vec::new());
    serialize::write_message(&mut buffer, &message).unwrap();
    buffer.into_inner()
}

pub fn to_json(measurements: &[Measurement]) -> Vec<u8> {
    serde_json::to_vec(&measurements.iter().map(SerializedMeasurement::from).collect::<Vec<_>>()).unwrap()
}

fn serialized_name(msg: &Measurement) -> String {
    let type_str = match msg.value {
        Unit::Byte(_) => "byte",
        Unit::Count(_) => "count",
        Unit::Str(_) => "string",
    };

    format!("{}_{}", &msg.name, type_str)
}

#[derive(Serialize, Deserialize, Debug)]
struct SerializedMeasurement {
    timestamp: u64,
    pub kind: Kind,
    pub name: String,
    pub measurement: u64,
    pub tags: HashMap<String, String>,
}

impl From<&Measurement> for SerializedMeasurement {
    fn from(msg: &Measurement) -> SerializedMeasurement {
        let name = serialized_name(msg);

        SerializedMeasurement {
            timestamp: msg.timestamp,
            kind: msg.kind,
            measurement: msg.value.get(),
            tags: msg.tags.iter().cloned().collect(),
            name,
        }
    }
}
