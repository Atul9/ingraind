use actix::prelude::*;
use futures::Future;
pub use rusoto_core::region::Region;
use rusoto_s3::{PutObjectRequest, S3 as RusotoS3, S3Client};
use serde_json;

use serde::Serialize;

use backends::Message;
use metrics::{kind::Kind, timestamp_now, Measurement, Tags, Unit};

pub struct S3 {
    hostname: String,
    client: S3Client,
    bucket: String,
}

impl S3 {
    pub fn new(region: Region, bucket: impl Into<String>) -> S3 {
        use redbpf::uname::*;

        S3 {
            hostname: get_fqdn().unwrap(),
            client: S3Client::simple(region),
            bucket: bucket.into(),
        }
    }
}

impl Actor for S3 {
    type Context = Context<Self>;
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SerializedMeasurement {
    timestamp: u64,
    pub kind: Kind,
    pub name: String,
    pub measurement: u64,
    pub tags: Tags,
}

fn format_by_type(msg: &Measurement) -> impl Serialize {
    let (type_str, measurement) = match msg.value {
        Unit::Byte(x) => ("byte", x),
        Unit::Count(x) => ("count", x),
    };

    let name = format!("{}_{}", &msg.name, type_str);

    SerializedMeasurement {
        timestamp: msg.timestamp,
        kind: msg.kind,
        name,
        measurement,
        tags: msg.tags.clone(),
    }
}

impl Handler<Message> for S3 {
    type Result = ();

    fn handle(&mut self, msg: Message, _ctx: &mut Context<Self>) -> Self::Result {
        let body = match msg {
            Message::List(lst) => format!(
                "[{}]",
                lst.iter()
                    .map(|e| serde_json::to_string(&format_by_type(e)).unwrap())
                    .collect::<Vec<String>>()
                    .join(",\n")
            ),
            Message::Single(msg) => serde_json::to_string(&[&msg]).unwrap(),
        }.into();

        ::actix::spawn(
            self.client
                .put_object(&PutObjectRequest {
                    bucket: self.bucket.clone(),
                    key: format!("{}_{}", &self.hostname, timestamp_now()),
                    body: Some(body),
                    ..Default::default()
                })
                .and_then(|_| Ok(()))
                .or_else(|_| Ok(())),
        );
    }
}
