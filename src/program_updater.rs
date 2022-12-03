/// Program updater manager, handle corresponding PollEvent(ProgramUpdateAvailable), perform periodic check updates.
/// Observe change program hashes and save its on disk.

use bevy_ecs::prelude::*;
use std::io::Error;
use std::thread::{JoinHandle, self};

use crate::execm::{Exec, self};
use crate::sendm::SendManager;
use crate::{events, stages};
use crate::data_types::data_server::{GetPointConfigAnsw as PointConfig, GetUpdateDataAnsw, GetProgramConfigAnsw, Report, ReportType, ProgramType, ProgramHashes};
use crate::srvm::Server;
use crate::utils::{mos, some_str};

#[derive(Resource, Clone)]
pub struct ProgramHashesRes(Vec<ProgramHashes>);

impl ProgramHashesRes {
	pub fn set_build(&mut self, pid: i32, hash: Vec<u8>) {
		for ph in &mut self.0 {
			if ph.id == pid {
				ph.build_hash = hash;
				return;
			}
		}
		self.0.push(ProgramHashes {
			asset_hash: None,
			build_hash: hash,
			id: pid,
			configs: Vec::new()
		});
	}

	pub fn set_asset(&mut self, pid: i32, hash: Option<Vec<u8>>) {
		for ph in &mut self.0 {
			if ph.id == pid {
				ph.asset_hash = hash;
				return;
			}
		}
		self.0.push(ProgramHashes {
			asset_hash: hash,
			build_hash: Vec::new(),
			id: pid,
			configs: Vec::new()
		});
	}

	pub fn set_config(&mut self, pid: i32, cid: i32, hash: Vec<u8>) -> bool {
		for ph in &mut self.0 {
			if ph.id == pid {
				for ch in &mut ph.configs {
					if ch.0 == cid {
						if ch.1 != hash {
							ch.1 = hash;
							return true;
						}
						return false;
					}
				}
				ph.configs.push((cid, hash));
				return true;
			}
		}
		self.0.push(ProgramHashes {
			id: pid,
			asset_hash: None,
			build_hash: Vec::new(),
			configs: vec![(cid, hash)]
		});
		true
	}

	pub fn clear_configs(&mut self) {
		for p in &mut self.0 {
			p.configs.clear();
		}
	}

	pub fn insert_builtin(&mut self, config: &PointConfig) {
		for p in &config.programs {
			if !p.ptype.is_custom() {
				let mut exists = false;
				for p_self in &self.0 {
					if p.id == p_self.id {
						exists = true;
						break;
					} 
				}
				if !exists {
					self.0.push(ProgramHashes {
						id: p.id,
						asset_hash: None,
						build_hash: Vec::new(),
						configs: Vec::new()
					})
				}
			}
		}
	}
}

pub enum UpdateData {
	Build(String, Vec<u8>),
	Asset(Option<(String, Vec<u8>)>),
	Config(GetProgramConfigAnsw)	
}

pub enum GetDataResult {
	Build(JoinHandle<Result<(String, Vec<u8>), Error>>),
	Asset(JoinHandle<Result<(String, Vec<u8>), Error>>),
	Config(JoinHandle<Result<GetProgramConfigAnsw, Error>>)
}

#[derive(Component)]
pub struct UpdateStateNew;

#[derive(Component)]
pub struct UpdateStateGetData(Option<GetDataResult>);

#[derive(Component)]
pub struct UpdateStateTerminate;

#[derive(Component)]
pub struct UpdateStateApply;

#[derive(Component)]
pub struct UpdateStateReport;

#[derive(PartialEq)]
pub enum UpdateType {
	Build,
	Asset(bool),
	Config(i32)
}

#[derive(Component)]
pub struct ProgramUpdate {
	pub pid: i32,
	pub utype: UpdateType,
	pub data: Option<UpdateData>
}

impl PartialEq for ProgramUpdate {
	fn eq(&self, other: &Self) -> bool {
		if self.pid != other.pid {
			return false;
		}
		if self.utype != other.utype {
			return false;
		}
		true
	}
}

fn get_config_hashes(config: &PointConfig) -> Vec<(i32, i32, Vec<u8>)> {
    let mut hashes = Vec::new();
    for p in &config.programs {
		let cdata = match &p.ptype {
			ProgramType::Custom(cdata) => cdata,
			_ => continue
		};
        for c in &cdata.configs {
            hashes.push((
                p.id,
				c.0,
                mos::config_hash(&p.name, &c.1, &config.bin_path)
			));
        }
    }
    hashes
}

fn update_config_hashes(config: &PointConfig, hashes: &mut ProgramHashesRes) -> bool {
	let rhashes = get_config_hashes(config);
	hashes.clear_configs();
	let mut changes = false;
	for (pid, cid, hash) in rhashes {
		if hashes.set_config(pid, cid, hash) {
			changes = true;
		}
	}
	changes
}

fn sys_update_checker(
	mut cmd: Commands,
	evr: EventReader<events::ProgramUpdateAvailable>,
	config: Res<PointConfig>,
	server: Res<Server>,
	hashes: ResMut<ProgramHashesRes>,
	cur_upd: Query<&ProgramUpdate>
) {
	if !evr.is_empty() || config.is_changed() {
		println!("[PU] get update data..");
		let mut hashes_cpy = hashes.clone();
		hashes_cpy.insert_builtin(&config);
		update_config_hashes(&config, &mut hashes_cpy);
		println!("[PU] get update data with hashes: {:?}", hashes_cpy.0);
		let to_spawn = match server.api.get_update_data(hashes_cpy.0) {
			Ok(answ) => match answ {
				GetUpdateDataAnsw::Build(pid) => {
					println!("\t[PU] build for program {} update available", pid);
					Some(ProgramUpdate {pid: pid, utype: UpdateType::Build, data: None})
				},
				GetUpdateDataAnsw::Asset(pid, exists) => {
					println!("\t[PU] asset for program {} update available, exists: {}", pid, exists);
					Some(ProgramUpdate {pid: pid, utype: UpdateType::Asset(exists), data: None})
				},
				GetUpdateDataAnsw::Config(cid) => {
					match config.find_program_by_config(cid) {
						Some(pid) => {
							println!("\t [PU] config {} for program {} update available", cid, pid);
							Some(ProgramUpdate {pid: pid, utype: UpdateType::Config(cid), data: None})
						},
						None => None
					}
				},
				GetUpdateDataAnsw::Nothing => {
					println!("\t [PU] nothing to update");
					None
				}
			},
			_ => None
		};
		match to_spawn {
			Some(val) => {
				for pu in &cur_upd {
					if &val == pu {
						println!("\t [PU] update task the same type already exists, return");
						return;
					}
				}
				println!("\t [PU] spawn update task..");
				cmd.spawn((val, UpdateStateNew));
			},
			None => ()
		}
	}
}

fn sys_new_handler(mut cmd: Commands, server: Res<Server>, mut query: Query<(Entity, &mut ProgramUpdate), With<UpdateStateNew>>) {
	for (e, mut d) in &mut query {
		let api = server.api.clone();
		let pid = d.pid;
		match d.utype {
			UpdateType::Build => {
				cmd.entity(e).insert(UpdateStateGetData {0: Some(GetDataResult::Build(thread::spawn(move || api.download_program(pid))))});
			},
			UpdateType::Asset(exists) => {
				if exists {
					cmd.entity(e).insert(UpdateStateGetData {0: Some(GetDataResult::Asset(thread::spawn(move || api.download_asset(pid))))});
				} else {
					d.data = Some(UpdateData::Asset(None));
					cmd.entity(e).insert(UpdateStateTerminate);
				}
			},
			UpdateType::Config(cid) => {
				cmd.entity(e).insert(UpdateStateGetData {0: Some(GetDataResult::Config(thread::spawn(move || api.get_program_config(cid))))});
			}
		};
		cmd.entity(e).remove::<UpdateStateNew>();
	}
}

fn sys_get_handler(mut cmd: Commands, mut query: Query<(Entity, &mut ProgramUpdate, &mut UpdateStateGetData)>) {
	for (e, mut u, mut d) in &mut query {
		d.0 = match d.0.take() {
			Some(h) => match h {
				GetDataResult::Build(h) => if h.is_finished() {
					let res = h.join().unwrap();
					match res {
						Ok(data) => u.data = Some(UpdateData::Build(data.0, data.1)),
						_ => ()
					}
					None
				} else {Some(GetDataResult::Build(h))},
				GetDataResult::Asset(h) => if h.is_finished() {
					let res = h.join().unwrap();
					match res {
						Ok(data) => u.data = Some(UpdateData::Asset(Some((data.0, data.1)))),
						_ => ()
					}
					None
				} else {Some(GetDataResult::Asset(h))},
				GetDataResult::Config(h) => if h.is_finished() {
					let res = h.join().unwrap();
					match res {
						Ok(data) => u.data = Some(UpdateData::Config(data)),
						_ => ()
					}
					None
				} else {Some(GetDataResult::Config(h))}
			},
			None => None
		};
		if d.0.is_none() {
			if u.data.is_none() {
				cmd.entity(e).despawn();
			} else {
				cmd.entity(e).remove::<UpdateStateGetData>();
				cmd.entity(e).insert(UpdateStateTerminate);
			}
		}
	}
}

fn sys_terminate_handler(
	mut cmd: Commands,
	mut query: Query<(Entity, &mut ProgramUpdate, Option<&UpdateStateApply>), With<UpdateStateTerminate>>,
	execs: Query<(Entity, &Exec), With<execm::Run>>,
	mut evw: EventWriter<events::TerminateRequest>
) {
	for (ue, u, applys) in &mut query {
		if execm::utils::terminate(&execs, &mut evw, u.pid, false) {
			if applys.is_none() {
				cmd.entity(ue).insert(UpdateStateApply);
			}
		}
	}
}

fn sys_apply_handler(
	mut cmd: Commands,
	mut query: Query<(Entity, &mut ProgramUpdate), With<UpdateStateApply>>,
	config: Res<PointConfig>,
	mut sm: ResMut<SendManager>,
	mut hashes: ResMut<ProgramHashesRes>,
	mut evw: EventWriter<events::ProgramHashesChanged>
) {
	for (ue, mut u) in &mut query {
		let p = match config.get_program_by_id(u.pid) {
			Some(p) => p,
			None => {
				cmd.entity(ue).despawn();
				sm.report(Report {delay: 0, rtype: ReportType::InternalError, program_id: Some(u.pid), descr: some_str("program not found")});
				continue;
			}
		};
		let pc = match p.ptype {
			ProgramType::Custom(pc) => pc,
			_ => {
				cmd.entity(ue).despawn();
				sm.report(Report {delay: 0, rtype: ReportType::InternalError, program_id: Some(p.id), descr: some_str("update for non custom program not supported")});
				continue;
			}
		};
		let cid = match u.utype {
			UpdateType::Config(cid) => Some(cid),
			_ => None
		};
		match u.data.as_mut().unwrap() {
			UpdateData::Build(name, hash) => {
				match mos::setup_program(&pc, &p.name, &name, &config.bin_path) {
					Err(e) => {
						cmd.entity(ue).despawn();
						sm.report(Report {delay: 0, rtype: ReportType::InternalError, program_id: Some(p.id), descr: Some(format!("fail to build update: {:?}", e))});
					},
					_ => {
						cmd.entity(ue).despawn();
						sm.report(Report {delay: 0, rtype: ReportType::BuildUpdate, program_id: Some(p.id), descr: None});
						hashes.set_build(p.id, hash.clone());
						evw.send(events::ProgramHashesChanged);
					}
				}
			},
			UpdateData::Asset(data) => {
				match data {
					Some((arch_name, hash)) => {
						match mos::setup_asset(&p.name, &arch_name, &config.bin_path) {
							Err(e) => {
								cmd.entity(ue).despawn();
								sm.report(Report {delay: 0, rtype: ReportType::InternalError, program_id: Some(p.id), descr: Some(format!("fail to asset update: {:?}", e))});
							},
							_ => {
								cmd.entity(ue).despawn();
								sm.report(Report {delay: 0, rtype: ReportType::AssetUpdate, program_id: Some(p.id), descr: None});
								hashes.set_asset(p.id, Some(hash.clone()));
								evw.send(events::ProgramHashesChanged);
							}
						}
					},
					None => {
						match mos::remove_asset(&p.name, &config.bin_path) {
							Err(e) => {
								cmd.entity(ue).despawn();
								sm.report(Report {delay: 0, rtype: ReportType::InternalError, program_id: Some(p.id), descr: Some(format!("fail to asset delete: {:?}", e))});
							},
							_ => {
								cmd.entity(ue).despawn();
								sm.report(Report {delay: 0, rtype: ReportType::AssetUpdate, program_id: Some(p.id), descr: None});
								hashes.set_asset(p.id, None);
								evw.send(events::ProgramHashesChanged);
							}
						}
					}
				}
			},
			UpdateData::Config(data) => {
				let cid: i32 = cid.unwrap();
				let config_path = match pc.get_config_path_by_id(cid) {
					Some(path) => path,
					None => {
						cmd.entity(ue).despawn();
						sm.report(Report {delay: 0, rtype: ReportType::InternalError, program_id: Some(u.pid), descr: Some(format!("config with id {} not found", cid))});
						continue;
					}
				};
				match mos::setup_config(&p.name, &config_path, &data.data, &config.bin_path) {
					Err(e) => {
						cmd.entity(ue).despawn();
						sm.report(Report {delay: 0, rtype: ReportType::InternalError, program_id: Some(u.pid), descr: Some(format!("fail to build update: {:?}", e))});
					},
					_ => {
						cmd.entity(ue).despawn();
						sm.report(Report {delay: 0, rtype: ReportType::BuildUpdate, program_id: Some(u.pid), descr: None});
					}
				}
			}
		}
	}
}

fn sys_hash_saver(mut hashes: ResMut<ProgramHashesRes>, evr: EventReader<events::ProgramHashesChanged>) {
	if !evr.is_empty() {
		println!("[PU] write program hashes.");
		hashes.clear_configs();
		mos::write_programs_hashes(&hashes.0).unwrap();
	}
}

fn startup(mut cmd: Commands) {
	println!("[PROGRAM_UPDATER] startup..");
	let hashes = mos::read_programs_hashes();
	let hashes = ProgramHashesRes {0: hashes};
	cmd.insert_resource(hashes);
}

pub fn init(_world: &mut World, schedule: &mut Schedule) -> Result<(), Error> {
	schedule.add_system_to_stage(stages::Startup::InitProgramUpdater, startup);
	schedule.add_system_to_stage(stages::Core::HandlePollEvents, sys_update_checker);
	schedule.add_system_to_stage(stages::Core::Main, sys_new_handler);
	schedule.add_system_to_stage(stages::Core::Main, sys_get_handler);
	schedule.add_system_to_stage(stages::Core::Main, sys_terminate_handler);
	schedule.add_system_to_stage(stages::Core::Main, sys_apply_handler);
	schedule.add_system_to_stage(stages::Core::Save, sys_hash_saver);
	Ok(())
}