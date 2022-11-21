use std::io::prelude::*;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::sync::mpsc::{Sender, channel, Receiver};
use std::{thread, fs};
use std::os::unix::net::{UnixStream, UnixListener};
use std::time::Duration;
use rmp_serde as rmps;

use crate::siapi::IntApi;

// use super::siapi::Log;
// use super::siapi::Stat;
use super::siapi::Status;

const SOCK_NAME: &str = "manager";
const READ_TIMEOUT: Duration = Duration::from_millis(1000);
const READ_BUF_SIZE: usize = 32_768;

const M2PREQ_SOFT_TERMINATE: u8 = 1;
const M2PREQ_GET_STATUS: u8 = 2;
const M2PREQ_INDICATE: u8 = 3;

const P2MREQ_LOG: u8 = 1;
const P2MREQ_STAT: u8 = 2;

pub enum IpcEvent {
	SendLog,
	SendStat
}

// - - - - - - - GENERAL TYPES - - - - - - - //
#[derive(Serialize, Deserialize, Debug)]
struct GenReq {
	req: u8,
	name: String,
	body: Vec<u8>
}

// - - - - - - - PROGRAM TO MANAGER REQUEST TYPES - - - - - - - //



// - - - - - - - PROGRAM TO MANAGER REQUESTS HANDLERS - - - - - //

fn ipc_handler(stream: UnixStream, req: GenReq, tx: &Sender<IpcEvent>, api: &Arc<Mutex<IntApi>>) {
	match req.req {
		P2MREQ_LOG => {
			match rmps::decode::from_slice(&req.body) {
				Ok(log) => {
					match api.lock().unwrap().send_log(&log) {
						Err(e) => println!("fail to send log: {}", e),
						_ => {
							match tx.send(IpcEvent::SendLog) {
								_ => ()
							}
						}
					}
				},
				_ => {
					println!("[IPC][SERVER] FAIL HANDLE PM REQ: INC_REQ_DATA(LOG)");
					return;
				}
			}
		},
		P2MREQ_STAT => {
			match rmps::decode::from_slice(&req.body) {
				Ok(stat) => {
					api.lock().unwrap().send_stat(stat);
					match tx.send(IpcEvent::SendStat) {
						_ => ()
					}
				},
				_ => {
					println!("[IPC][SERVER] FAIL HANDLE PM REQ: INC_REQ_DATA(STAT)");
					return;
				}
			}
		},
		_ => {
			println!("[IPC][SERVER] FAIL HANDLE PM REQ: INC_REQ_CODE");
			return;
		}
	};

	stream.shutdown(std::net::Shutdown::Both).unwrap();
}

fn ipc_server(tx: Sender<IpcEvent>, api: Arc<Mutex<IntApi>>, sock_path_dir: String) {
	let sock_spath = format!("{}/{}", sock_path_dir, SOCK_NAME);
	let sock_path = Path::new(&sock_spath);
	if sock_path.exists() {
		fs::remove_file(&sock_path).unwrap();
	}
	if !Path::new(&sock_path_dir).exists() {
		fs::create_dir_all(sock_path_dir).unwrap();
	}
	
    let listener = UnixListener::bind(&sock_path).unwrap();
	for stream in listener.incoming() {
		println!("[IPC_SERVER] incoming..");
        match stream {
            Ok(mut stream) => {
                stream.set_read_timeout(Some(READ_TIMEOUT)).unwrap();
				let mut buf:[u8;READ_BUF_SIZE] = [0;READ_BUF_SIZE];
				match stream.read(&mut buf) {
					Err(e) => {
						println!("[IPC][SERVER] FAIL TO READ: {}", e.to_string());
					},
					Ok(len) => {
						match rmps::decode::from_slice(&buf[..len]) {
							Ok(req) => ipc_handler(stream, req, &tx, &api),
							_ => println!("[IPC][SERVER] FAIL TO DECODE")
						}
					}
				}
            }
            Err(e) => {
                println!("[IPC][SERVER] CONNECT FAILED: {}", e.to_string());
            }
        }
    }
}

pub fn run(api: Arc<Mutex<IntApi>>, sock_path_dir: &str) -> Receiver<IpcEvent> {
	let (tx, rx) = channel::<IpcEvent>();
	let sock_path_dir = String::from(sock_path_dir);
	thread::spawn(|| ipc_server(tx, api, sock_path_dir));
	rx
}


// - - - - - - - MANAGER TO PROGRAM REQUESTS - - - - - - - - - //

fn perform_request(sock: &str, req: u8, body: Vec<u8>) -> Result<Vec<u8>, String> {
	let mut sock = match UnixStream::connect(sock) {
		Ok(sock) => sock,
		Err(e) => return Err(format!("fail to connect to {}: {}", sock, e.to_string()))
	};

	let req = GenReq {req: req, name: String::new(), body: body};
	let req_raw = rmps::encode::to_vec(&req).unwrap();
	match sock.write_all(&req_raw) {
		Err(e) => return Err(format!("fail to write req: {}", e.to_string())),
		_ => ()
	}
	sock.set_read_timeout(Some(READ_TIMEOUT)).unwrap();
	let mut buf = Vec::<u8>::new();
	match sock.read_to_end(&mut buf) {
		Err(e) => Err(format!("fail to read answ: {}", e.to_string())),
		_ => Ok(buf)
	}
}

pub fn format_sock_path(program_name: &str, sock_path_dir: &str) -> String {
	format!("{}/{}", sock_path_dir, program_name)
}

pub fn get_status(program_name: &str, sock_path_dir: &str) -> Result<Status, String> {
	let sock = format_sock_path(program_name, sock_path_dir);
	match perform_request(&sock, M2PREQ_GET_STATUS, Vec::<u8>::new()) {
		Ok(data) => {
			match rmps::decode::from_slice(&data) {
				Ok(data) => Ok(data),
				Err(e) => Err(e.to_string())
			}
		},
		Err(e) => Err(e)
	}
}

pub fn soft_terminate(program_name: &str, sock_path_dir: &str) -> Result<bool, String> {
	let sock = format_sock_path(program_name, sock_path_dir);
	match perform_request(&sock, M2PREQ_SOFT_TERMINATE, Vec::<u8>::new()) {
		Ok(data) => {
			let answ: bool = match rmps::decode::from_slice(&data) {
				Ok(data) => data,
				Err(e) => return Err(e.to_string())
			};

			Ok(answ)
		},
		Err(e) => Err(e)
	}
}

pub fn indicate(program_name: &str, sock_path_dir: &str) -> Result<(), String> {
	let sock = format_sock_path(program_name, sock_path_dir);
	match perform_request(&sock, M2PREQ_INDICATE, Vec::<u8>::new()) {
		Ok(data) => {
			let answ: bool = match rmps::decode::from_slice(&data) {
				Ok(data) => data,
				Err(e) => return Err(e.to_string())
			};
			if answ {
				Ok(())
			} else {
				Err(format!("program not supported indicate func"))
			}
		},
		Err(e) => Err(e)
	}
}