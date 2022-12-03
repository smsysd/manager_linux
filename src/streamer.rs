/// Stream manager, handle corresponding PollEvent(StreamReq) - 
/// get stream from Exec, connect Exec stream with server stream, 
/// perform transfer data for Stream.

use bevy_ecs::prelude::*;
use std::{io::{Error, Write, Read, ErrorKind}, net::TcpStream, sync::mpsc::TryRecvError};

use crate::{stages, events, execm::{Exec, self}, data_types::{Cert, self}, utils::rmp_encode};

const BUFSIZE: usize = 1024;

#[derive(Component)]
pub struct Stream {
	pub stream_id: i32,
	pub program_id: i32,
	pub tcp: TcpStream
}

#[derive(Component)]
pub struct StreamStateRun;

#[derive(Component)]
pub struct StreamStateTransfer;

fn terminator(mut cmd: Commands, execs: Query<(Entity, &Stream), (With<StreamStateTransfer>, Without<execm::Run>)>) {
	for (e, s) in &execs {
		cmd.entity(e).remove::<Stream>();
		cmd.entity(e).remove::<StreamStateTransfer>();
		println!("[STREAMER] stream({}) for {} transfer terminated because program not run", s.stream_id, s.program_id);
	}
}

fn transfer(mut cmd: Commands, mut execs: Query<(Entity, &Exec, &mut execm::Run, &mut Stream), With<StreamStateTransfer>>) {
	let mut buf: [u8;BUFSIZE] = [0;BUFSIZE];
	for (ex_e, _ex, mut run, mut s) in &mut execs {
		let mut disonnect = false;
		match &mut run.stdout {
			Some(stdout) => {
				match stdout.lock().unwrap().try_recv() {
					Ok(data) => {
						match s.tcp.write_all(&data) {
							Err(ref e) if e.kind() == ErrorKind::WouldBlock => (),
							Err(_) => disonnect = true,
							_ => ()
						}
					},
					Err(TryRecvError::Disconnected) => disonnect = true,
					_ => ()
				}
			},
			None => disonnect = true
		}
		match s.tcp.read(&mut buf) {
			Ok(0) => disonnect = true,
			Ok(len) => {
				match &mut run.child.stdin {
					Some(stdin) => match stdin.write_all(&buf[..len]) {
						_ => ()
					},
					None => ()
				}
			},
			Err(ref e) if e.kind() == ErrorKind::WouldBlock => (),
			Err(_) => disonnect = true,
		}
		if disonnect {
			cmd.entity(ex_e).remove::<Stream>();
			cmd.entity(ex_e).remove::<StreamStateTransfer>();
			println!("[STREAMER] stream transfer terminated because program terminated or master cause");
		}
	}
}

fn runner(mut cmd: Commands, mut evw: EventWriter<events::RunRequest>, execs: Query<(Entity, &Exec, Option<&execm::Run>), With<StreamStateRun>>) {
	for (ex_e, ex, run) in &execs {
		match run {
			Some(_) => {
				cmd.entity(ex_e).remove::<StreamStateRun>();
				cmd.entity(ex_e).insert(StreamStateTransfer);
			},
			None => {
				evw.send(events::RunRequest(ex.pid));
			}
		}
	}
}

fn adder(
	mut cmd: Commands,
	mut evr: EventReader<events::Stream>,
	execs: Query<(Entity, &Exec), Without<Stream>>,
	cert: Res<Cert>
) {
	if !evr.is_empty() {
		let ev = evr.iter().next().unwrap();
		for (ex_e, ex) in &execs {
			if ex.pid == ev.program_id {
				let tcp = match connect(ev.id, &cert.host, cert.stream_port) {
					Ok(tcp) => tcp,
					Err(e) => {
						println!("[STREAMER] fail to connect: {:?}", e);
						return;
					}
				};
				tcp.set_nonblocking(true).unwrap();
				
				cmd.entity(ex_e).insert((
					Stream {
						stream_id: ev.id,
						program_id: ev.program_id,
						tcp: tcp,

					},
					StreamStateRun
				));
				println!("[STREAMER] new stream({}) for {}", ev.id, ev.program_id);
				break;
			}
		}
	}
}

fn connect(id: i32, host: &str, port: u16) -> Result<TcpStream, Error> {
	let mut tcp = std::net::TcpStream::connect(format!("{}:{}", host, port))?;
	let req_raw = rmp_encode(&data_types::stream_api::Request {id: id, initiator: false})?;
	tcp.write_all(&req_raw)?;
	Ok(tcp)
}

fn setup(mut _cmd: Commands) {

}

pub fn init(_world: &mut World, schedule: &mut Schedule) -> Result<(), Error> {
	schedule.add_system_to_stage(stages::Startup::InitStreamer, setup);
	schedule.add_system_to_stage(stages::Core::HandlePollEvents, adder);
	schedule.add_system_to_stage(stages::Core::Main, runner);
	schedule.add_system_to_stage(stages::Core::Main, transfer);
	schedule.add_system_to_stage(stages::Core::Main, terminator);
	Ok(())
}