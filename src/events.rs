use crate::stages;
use crate::data_types;
use bevy_ecs::prelude::*;
use data_types::data_server::*;

pub struct PointUpdateAvailable;
pub struct ProgramUpdateAvailable;
pub struct Cmd {
	pub id: i32,
	pub ctype: CmdType
}

pub struct Stream {
	pub id: i32,
	pub program_id: i32
}

pub struct NotReg;

pub struct TerminateRequest {
	pub pid: i32,
	pub hard: bool
}

pub struct RunRequest(pub i32);

pub struct ProgramHashesChanged;

// pub struct Error();

pub fn init(world: &mut World, schedule: &mut Schedule) {
	world.init_resource::<Events<PointUpdateAvailable>>();
	schedule.add_system_to_stage(stages::Core::HandlePollEvents, Events::<PointUpdateAvailable>::update_system);

	world.init_resource::<Events<ProgramUpdateAvailable>>();
	schedule.add_system_to_stage(stages::Core::HandlePollEvents, Events::<ProgramUpdateAvailable>::update_system);
	
	world.init_resource::<Events<Cmd>>();
	schedule.add_system_to_stage(stages::Core::HandlePollEvents, Events::<Cmd>::update_system);
	
	world.init_resource::<Events<Stream>>();
	schedule.add_system_to_stage(stages::Core::HandlePollEvents, Events::<Stream>::update_system);
	
	world.init_resource::<Events<NotReg>>();
	schedule.add_system_to_stage(stages::Core::HandlePollEvents, Events::<NotReg>::update_system);
	
	world.init_resource::<Events<TerminateRequest>>();
	schedule.add_system_to_stage(stages::Core::Main, Events::<TerminateRequest>::update_system);
	
	world.init_resource::<Events<RunRequest>>();
	schedule.add_system_to_stage(stages::Core::Main, Events::<RunRequest>::update_system);
	
	world.init_resource::<Events<ProgramHashesChanged>>();
	schedule.add_system_to_stage(stages::Core::Main, Events::<ProgramHashesChanged>::update_system);
}