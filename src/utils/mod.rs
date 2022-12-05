use std::io::Error;
use std::io::ErrorKind;

use rmp_serde as rmps;
use serde::Deserialize;
use serde::Serialize;

pub mod mos;
pub mod siapi;
pub mod ipc;

pub fn err(e: &str) -> Error {
	Error::new(ErrorKind::Other, e)
}

pub fn rmp_encode<'a, T>(val: & T) -> Result<Vec<u8>, Error>
where T: Serialize {
	match rmps::encode::to_vec(val) {
		Ok(data) => Ok(data),
		Err(e) => Err(err(&e.to_string()))
	}
}

pub fn rmp_decode<'a, T>(val: &'a [u8]) -> Result<T, Error>
where T: Deserialize<'a> {
	match rmps::decode::from_slice(val) {
		Ok(val) => Ok(val),
		Err(e) => Err(err(&e.to_string()))
	}
}

pub fn json_decode<'a, T>(val: &'a [u8]) -> Result<T, Error>
where T: Deserialize<'a> {
	match serde_json::from_slice(val) {
		Ok(val) => Ok(val),
		Err(e) => Err(err(&e.to_string()))
	}
}

pub fn count<T>(data: T) -> usize
where T: IntoIterator {
    let mut cnt = 0;
    for _ in data {
        cnt += 1;
    }
    cnt
}

pub fn some_str(val: &str) -> Option<String> {
	Some(format!("{}", val))
}