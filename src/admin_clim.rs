use bevy_ecs::prelude::*;
use mio::{Events, Poll, Interest, Token, net::UdpSocket, net::UnixListener, net::UnixStream, event::Event};
use std::{io::Error, time::Duration, path::Path, fs};

use crate::{stages, configm::ConfigBase};

const POLL_TIMEOUT: Duration = Duration::from_millis(1);

#[derive(Resource)]
struct AdminCliServer {
	poll: Poll,
	srv: UnixListener
}

fn server(cmd: Commands, mut aclisrv: ResMut<AdminCliServer>) {
	let mut events = Events::with_capacity(1);
	aclisrv.poll.poll(&mut events, Some(POLL_TIMEOUT)).unwrap();
	for ev in events.iter() {

	}
}

fn startup(mut cmd: Commands, config: Res<ConfigBase>) {
    println!("[ADMIN_CLIM] startup..");
	let poll = Poll::new().unwrap();
	let sock_path = format!("{}/admin_cli", config.ipc_dir);
	if Path::new(&sock_path).exists() {
		fs::remove_file(&sock_path).unwrap();
	}
	let mut srv = UnixListener::bind(sock_path).unwrap();
	poll.registry().register(&mut srv, Token(0), Interest::READABLE | Interest::WRITABLE).unwrap();
	cmd.insert_resource(AdminCliServer {
		poll: poll,
		srv: srv
	});
}

pub fn init(_world: &mut World, schedule: &mut Schedule) -> Result<(), Error> {
    schedule.add_system_to_stage(stages::Startup::InitAdminCli, startup);
    schedule.add_system_to_stage(stages::Core::PollServer, server);
	Ok(())
}