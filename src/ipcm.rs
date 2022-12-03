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
use crate::data_types::data_server;
use crate::sendm::SendManager;
use crate::stages;
use crate::utils::ipc::{self, RequestFromProgram, ResAnsw};
use crate::utils::{rmp_decode, rmp_encode};
use data_server::GetPointConfigAnsw as PointConfig;

const SERVER: Token = Token(0);
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
	pub state: ClientState
}

#[derive(Resource)]
pub struct IpcServer {
	pub srv: UnixListener,
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
			HandleResult::Request(rmp_decode(&buf[..len])?)
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
						client.state = ClientState::Write(rmp_encode(&ResAnsw::Ok).unwrap())
					},
					RequestFromProgram::Stat(stat) => {
						sm.stat(stat);
						client.state = ClientState::Write(rmp_encode(&ResAnsw::Ok).unwrap())
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
			SERVER => {
				match srv.srv.accept() {
					Ok((stream, _)) => {
						cmd.spawn(IpcClient {
							stream: stream,
							state: ClientState::Read([0;BUFSIZE])
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

	let sock_path = format!("{}/manager", config.ipc_dir);
	let sock_path = Path::new(&sock_path);
	if sock_path.exists() {
		fs::remove_file(&sock_path).unwrap();
	}
	match sock_path.parent() {
		Some(dir_path) => {
			fs::create_dir_all(dir_path).unwrap();
		},
		None => ()
	}
	let poll = Poll::new().unwrap();
	let mut srv = UnixListener::bind(sock_path).unwrap();
	poll.registry().register(&mut srv, SERVER, Interest::READABLE | Interest::WRITABLE).unwrap();
	cmd.insert_resource(IpcServer {
		poll: poll,
		srv: srv,
		events: mio::Events::with_capacity(EVENTS_CAP)
	});
}

pub fn init(_world: &mut World, schedule: &mut Schedule) -> Result<(), Error> {
	schedule.add_system_to_stage(stages::Startup::InitIpcManager, startup);
	schedule.add_system_to_stage(stages::Core::PollServer, server);
	schedule.add_system_to_stage(stages::Core::HandlePollEvents, incoming_handler);
	Ok(())
}