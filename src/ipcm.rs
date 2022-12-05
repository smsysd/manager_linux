/// IPC manager create and handle IPC server, put IPC server Resource.
/// Handle IPC connections, redirect to send manager.

use bevy_ecs::prelude::*;
use mio::{Poll, Token, Interest};
use mio::net::{UnixListener, UnixStream};
use std::fs;
use std::io::{Error, Read, ErrorKind, Write};
use std::path::Path;
use std::time::Duration;

use crate::configm::ConfigBase;
use crate::data_types::data_server::{self, IpcType};
use crate::sendm::SendManager;
use crate::stages;
use crate::utils::ipc::{self, RequestFromProgram, ResAnsw};
use crate::utils::{rmp_decode, json_decode};
use data_server::GetPointConfigAnsw as PointConfig;

const SERVER_MP: Token = Token(0);
const SERVER_JSON: Token = Token(1);
const POLL_TIMEOUT: Duration = Duration::from_micros(2500);
const EVENTS_CAP: usize = 16;
const BUFSIZE: usize = 4096;

pub enum ClientState {
	Read([u8;BUFSIZE]),
	Write(Vec<u8>)
}

pub enum HandleResult {
	Request(RequestFromProgram),
	Ok
}

#[derive(Component)]
pub struct IpcClient {
	pub stream: UnixStream,
	pub state: ClientState,
	pub ipc_type: IpcType
}

#[derive(Resource)]
pub struct IpcServer {
	pub srv_mp: UnixListener,
	pub srv_json: UnixListener,
	pub poll: Poll,
	pub events: mio::Events
}

fn handle_client(client: &mut IpcClient) -> Result<HandleResult, Error> {
	let res = match client.state {
		ClientState::Read(ref mut buf) => {
			let len = client.stream.read(buf)?;
			if len == 0 {
				return Err(Error::new(ErrorKind::BrokenPipe, "zero len"));
			}
			let req = match &client.ipc_type {
				IpcType::Msgpack => rmp_decode(&buf[..len])?,
				IpcType::Json => json_decode(&buf[..len])?
			};
			HandleResult::Request(req)
		},
		ClientState::Write(ref data) => {
			client.stream.write_all(data)?;
			HandleResult::Ok
		}
	};
	Ok(res)
}

fn incoming_handler(mut cmd: Commands, mut sm: ResMut<SendManager>, mut clients: Query<(Entity, &mut IpcClient)>, config: Res<PointConfig>) {
	for (ent, mut client) in &mut clients {
		match handle_client(&mut client) {
			Ok(res) => match res {
				HandleResult::Request(req) => match req {
					RequestFromProgram::Log(log) => {
						if let Some(pid) = config.find_program_by_name(&log.name) {
							sm.log(data_server::Log {
								delay: 0,
								level: log.level,
								module: log.module,
								program_id: pid
							});
						}
						let answ_raw = match &client.ipc_type {
							IpcType::Msgpack => rmp_serde::to_vec(&ResAnsw::Ok).unwrap(),
							IpcType::Json => serde_json::to_vec(&ResAnsw::Ok).unwrap()
						};
						client.state = ClientState::Write(answ_raw);
					},
					RequestFromProgram::Stat(stat) => {
						sm.stat(stat);
						let answ_raw = match &client.ipc_type {
							IpcType::Msgpack => rmp_serde::to_vec(&ResAnsw::Ok).unwrap(),
							IpcType::Json => serde_json::to_vec(&ResAnsw::Ok).unwrap()
						};
						client.state = ClientState::Write(answ_raw);
					}
				},
				HandleResult::Ok => {
					cmd.entity(ent).despawn();
				}
			},
			Err(ref e) if e.kind() == ErrorKind::WouldBlock => (),
			Err(_) => {
				cmd.entity(ent).despawn();
			}
		}
	}
}

fn server(mut cmd: Commands, mut srv: ResMut<IpcServer>) {
	let srv = &mut *srv;
	srv.poll.poll(&mut srv.events, Some(POLL_TIMEOUT)).unwrap();
	for ev in srv.events.iter() {
		match ev.token() {
			SERVER_MP => {
				match srv.srv_mp.accept() {
					Ok((stream, _)) => {
						cmd.spawn(IpcClient {
							stream: stream,
							state: ClientState::Read([0;BUFSIZE]),
							ipc_type: IpcType::Msgpack
						});
					}
					_ => ()
				}
			},
			SERVER_JSON => {
				match srv.srv_json.accept() {
					Ok((stream, _)) => {
						cmd.spawn(IpcClient {
							stream: stream,
							state: ClientState::Read([0;BUFSIZE]),
							ipc_type: IpcType::Json
						});
					}
					_ => ()
				}
			},
			_ => ()
		}
	}
}

fn startup(mut cmd: Commands, config: Res<ConfigBase>) {
	let ipc = ipc::Ipc::new(&config.ipc_dir);
	cmd.insert_resource(ipc);

	let sock_path_mp = format!("{}/manager_mp", config.ipc_dir);
	let sock_path_json = format!("{}/manager_json", config.ipc_dir);
	let sock_path_mp = Path::new(&sock_path_mp);
	let sock_path_json = Path::new(&sock_path_json);
	if sock_path_mp.exists() {
		fs::remove_file(&sock_path_mp).unwrap();
	}
	if sock_path_json.exists() {
		fs::remove_file(&sock_path_json).unwrap();
	}
	match sock_path_mp.parent() {
		Some(dir_path) => {
			fs::create_dir_all(dir_path).unwrap();
		},
		None => ()
	}
	let poll = Poll::new().unwrap();
	let mut srv_mp = UnixListener::bind(sock_path_mp).unwrap();
	poll.registry().register(&mut srv_mp, SERVER_MP, Interest::READABLE | Interest::WRITABLE).unwrap();
	let mut srv_json = UnixListener::bind(sock_path_json).unwrap();
	poll.registry().register(&mut srv_json, SERVER_JSON, Interest::READABLE | Interest::WRITABLE).unwrap();
	cmd.insert_resource(IpcServer {
		poll: poll,
		srv_mp: srv_mp,
		srv_json: srv_json,
		events: mio::Events::with_capacity(EVENTS_CAP)
	});
}

pub fn init(_world: &mut World, schedule: &mut Schedule) -> Result<(), Error> {
	schedule.add_system_to_stage(stages::Startup::InitIpcManager, startup);
	schedule.add_system_to_stage(stages::Core::PollServer, server);
	schedule.add_system_to_stage(stages::Core::HandlePollEvents, incoming_handler);
	Ok(())
}