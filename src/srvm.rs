/// Server manger, establish and maintain connect with manager server.
/// Put IntApi Resource, perform periodic poll, generate PollEvent.

use bevy_ecs::prelude::*;
use std::{io::Error, time::Instant};

use crate::{data_types::{Cert, data_server::PollAnsw}, utils::siapi::{self, IntApi}, stages, events, configm::ConfigBase};

#[derive(Resource)]
pub struct Server {
	pub api: IntApi,
	pub is_connect: bool,
	pub tl_poll: Instant
}

fn sys_poll(
    mut srv: ResMut<Server>,
    config: Res<ConfigBase>,
    mut evw_not_reg: EventWriter<events::NotReg>,
    mut evw_cmd: EventWriter<events::Cmd>,
    mut evw_pcua: EventWriter<events::PointUpdateAvailable>,
    mut evw_pua: EventWriter<events::ProgramUpdateAvailable>,
    mut evw_stream: EventWriter<events::Stream>
) {
    if srv.tl_poll.elapsed() < config.poll_period {
        return;
    }
    srv.tl_poll = Instant::now();
    match srv.api.poll() {
        Ok(answ) => {
            srv.is_connect = true;
            match answ {
                PollAnsw::Nothing => (),
                PollAnsw::NotReg => evw_not_reg.send(events::NotReg),
                PollAnsw::Cmd(id, cmd) => evw_cmd.send(events::Cmd {id: id, ctype: cmd}),
                PollAnsw::PointConfigChanged => evw_pcua.send(events::PointUpdateAvailable),
                PollAnsw::ProgramDataChanged => evw_pua.send(events::ProgramUpdateAvailable),
                PollAnsw::Stream(id, program_id) => evw_stream.send(events::Stream {id: id, program_id: program_id}),
            }
        },
        Err(_) => srv.is_connect = false
    }
}

fn startup(mut cmd: Commands, cert: Res<Cert>) {
    println!("[SRVM] startup..");
    let auth = cert.auth.clone().unwrap();
    let api = siapi::IntApi::new(
        cert.host.clone(),
        cert.data_port,
        cert.file_port,
        auth.id,
        auth.token
    );
    let server_api = Server {api: api, is_connect: false, tl_poll: Instant::now()}; 
    cmd.insert_resource(server_api);
}

pub fn init(_world: &mut World, schedule: &mut Schedule) -> Result<(), Error> {
    schedule.add_system_to_stage(stages::Startup::InitServerApi, startup);
    schedule.add_system_to_stage(stages::Core::PollServer, sys_poll);

	Ok(())
}