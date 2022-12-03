use bevy_ecs::{system::Resource};
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Resource)]
pub struct Cert {
	pub host: String,
	pub data_port: u16,
	pub file_port: u16,
	pub stream_port: u16,

	pub name: Option<String>,
    pub firm_id: Option<i32>,
    pub firm_name: Option<String>,

	pub auth: Option<data_server::Auth>
}

#[derive(PartialEq)]
pub enum AppStateCode {
	Init,
	Normal,
	Emergency,
	Shutdown
}

#[derive(Resource)]
pub struct AppState {
	pub code: AppStateCode
}

impl Default for AppState {
	fn default() -> Self {
		Self {
			code: AppStateCode::Init
		}
	}
}

impl AppState {
	pub fn is_terminate(&self) -> bool {
		self.code == AppStateCode::Emergency || self.code == AppStateCode::Shutdown
	}
}

pub mod data_server {
	use bevy_ecs::{prelude::Component, system::Resource};
	use serde::{Serialize, Deserialize};

	const PSTATUS_OK: i16 = 0;
	const PSTATUS_WARNING: i16 = 1;
	const PSTATUS_ERROR: i16 = 2;
	
	const PRUNSTATUS_STOPPED: i16 = 0;
	const PRUNSTATUS_RUN: i16 = 1;
	const PRUNSTATUS_STOPPING: i16 = 2;
	const PRUNSTATUS_CRASHING: i16 = 3;
	
	const RTYPE_REBOOT: i16 = 1;
	const RTYPE_SELFUPDATE: i16 = 2;
	const RTYPE_BUILD_UPDATE: i16 = 3;
	const RTYPE_CONFIG_UPDATE: i16 = 4;
	const RTYPE_ASSET_UPDATE: i16 = 5;
	const RTYPE_STOP_PROGRAM: i16 = 7;
	const RTYPE_START_PROGRAM: i16 = 8;
	const RTYPE_POINT_CONFIG_UPDATE: i16 = 9;
	const RTYPE_INTERNAL_ERROR: i16 = 20;
	
	const CMD_SELFUPDATE: i16 = 2;
	const CMD_FORCE_SELFUPDATE: i16 = 3;
	const CMD_FORCE_BUILD_UPDATE: i16 = 5;
	const CMD_FORCE_ASSET_UPDATE: i16 = 7;
	const CMD_FORCE_CONFIG_UPDATE: i16 = 9;
	const CMD_START_PROGRAM: i16 = 20;
	const CMD_SOFT_STOP_PROGRAM: i16 = 21;
	const CMD_HARD_STOP_PROGRAM: i16 = 22;
	const CMD_SOFT_REBOOT: i16 = 25;
	const CMD_HARD_REBOOT: i16 = 26;
	const CMD_INDICATE: i16 = 40;


	#[derive(Serialize, Deserialize)]
	pub enum Request {
		Poll(Auth),
		GetUpdateData(Auth, Vec<ProgramHashes>),
		GetPointConfig(Auth),
		GetProgramConfig(Auth, i32),				// config_id
		AddLog(Auth, Log),
		AddStat(Auth, Stat),
		AddReport(Auth, Report),
		Register(String, Option<String>),			// point_name, firm_name
		SetStatus(Auth, ProgramStatus),
		SetRunStatus(Auth, ProgramRunStatus)
	}
	
	#[derive(Serialize, Deserialize, Clone)]
	pub struct Auth {
		pub id: i32,
		pub token: Vec<u8>
	}
	
	#[derive(Serialize, Deserialize)]
	pub struct OkAnsw {
		
	}
	
	#[derive(Serialize, Deserialize)]
	pub enum PollAnsw {
		Nothing,
		PointConfigChanged,
		ProgramDataChanged,
		Cmd(i32, CmdType),		// cmd_id, cmd_data
		Stream(i32, i32),		// stream_id, point_program_id
		NotReg
	}
	
	// - - - - - - - UPDATE DATA - - - - - - - - - //
	
	#[derive(Serialize, Deserialize, Clone, Debug)]
	pub struct ProgramHashes {
		pub id: i32,
		pub build_hash: Vec<u8>,
		pub asset_hash: Option<Vec<u8>>,
		pub configs: Vec<(i32, Vec<u8>)>
	}
	
	#[derive(Serialize, Deserialize)]
	pub enum GetUpdateDataAnsw {
		Build(i32),		// program_id
		Asset(i32, bool),		// program_id, is_exists
		Config(i32),	// config_id
		Nothing
	}
	
	// - - - - - - - POINT CONFIG - - - - - - - - //
	#[derive(Serialize, Deserialize, Clone)]
	pub struct ProgramCustom {
		pub autoupdate: bool,
		pub config_autoupdate: bool,
		pub asset_autoupdate: bool,
		pub use_ipc: bool,
		pub log_level: i16,
		pub configs: Vec<(i32, String)>
	}

	impl ProgramCustom {
		pub fn get_config_path_by_id(&self, cid: i32) -> Option<String> {
			for c in &self.configs {
				if c.0 == cid {
					return Some(c.1.clone());
				}
			}
			None
		}
	}
	
	#[derive(Serialize, Deserialize, Clone)]
	pub enum ProgramType {
		Custom(ProgramCustom),
		Builtin
	}

	impl ProgramType {
		pub fn is_custom(&self) -> bool {
			match self {
				Self::Builtin => false,
				Self::Custom(_) => true
			}
		}
	}
	
	#[derive(Serialize, Deserialize, Clone)]
	pub struct Program {
		pub id: i32,
		pub name: String,
		pub keep_run: bool,
		pub entry: String,
		pub args_after: Option<String>,
		pub args_before: Option<String>,
		pub is_indicate: bool,
	
		pub ptype: ProgramType
	}
	
	#[derive(Serialize, Deserialize, Resource, Clone)]
	pub struct GetPointConfigAnsw {
		pub poll_period: i64,
		pub bin_path: String,
		pub ipc_dir: String,
		pub programs: Vec<Program>
	}

	impl GetPointConfigAnsw {
		pub fn find_program_by_config(&self, config_id: i32) -> Option<i32> {
			for i in 0..self.programs.len() {
				match &self.programs[i].ptype {
					ProgramType::Custom(data) => {
						for j in 0..data.configs.len() {
							if data.configs[j].0 == config_id {
								return Some(self.programs[i].id);
							}
						}
					},
					_ => ()
				}
			}
			None
		}

		pub fn find_program_by_name(&self, name: &str) -> Option<i32> {
			for p in &self.programs {
				if p.name == name {
					return Some(p.id);
				}
			}
			None
		}

		pub fn get_program_by_id(&self, pid: i32) -> Option<Program> {
			for p in &self.programs {
				if p.id == pid {
					return Some(p.clone());
				}
			}
			None
		}
	}
	
	// - - - - - - - PROGRAM CONFIG - - - - - - - //
	#[derive(Serialize, Deserialize)]
	pub struct GetProgramConfigAnsw {
		pub hash: Vec<u8>,
		pub data: Vec<u8>
	}
	
	// - - - - - - - REGISTER - - - - - - - - - - //
	#[derive(Serialize, Deserialize)]
	pub struct RegisterData {
		pub id: i32,
		pub name: String,
		pub firm_id: i32,
		pub firm_name: String,
		pub token: Vec<u8>
	}
	
	#[derive(Serialize, Deserialize)]
	pub enum RegisterAnsw {
		Proceed,
		ProceedIndicate,
		Ok(RegisterData)
	}
	
	// - - - - - - - - REPORT LOG STAT - - - - - - - - //
	
	#[derive(Serialize, Deserialize, PartialEq, Clone)]
	pub enum StatusCode {
		Ok,
		Warning,
		Error
	}
	
	impl StatusCode {
		pub fn to_code(&self) -> i16 {
			match self {
				Self::Ok => PSTATUS_OK,
				Self::Warning => PSTATUS_WARNING,
				Self::Error => PSTATUS_ERROR
			}
		}
	}
	
	#[derive(Serialize, Deserialize)]
	pub struct ProgramRunStatus {
		pub id: i32,
		pub status: RunStatusCode
	}
	
	#[derive(Serialize, Deserialize)]
	pub enum RunStatusCode {
		Stopped(Option<String>),	// Last words
		Run,
		Stopping,
		Crashing(Option<String>)	// Last words
	}
	
	impl RunStatusCode {
		pub fn to_code(&self) -> i16 {
			match self {
				Self::Stopped(_) => PRUNSTATUS_STOPPED,
				Self::Run => PRUNSTATUS_RUN,
				Self::Stopping => PRUNSTATUS_STOPPING,
				Self::Crashing(_) => PRUNSTATUS_CRASHING
			}
		}
	
		pub fn last_words(self) -> Option<String> {
			match self {
				Self::Stopped(lw) => lw,
				Self::Run => None,
				Self::Stopping => None,
				Self::Crashing(lw) => lw
			}
		}
	}
	
	#[derive(Serialize, Deserialize)]
	pub enum CmdType {
		Selfupdate,
		ForceSelfupdate,
		ForceBuildUpdate(i32),
		ForceAssetUpdate(i32),
		ForceConfigUpdate(i32),
		StartProgram(i32),
		SoftStopProgram(i32),
		HardStopProgram(i32),
		SoftReboot,
		HardReboot,
		Indicate
	}
	
	impl CmdType {
		pub fn to_code(&self) -> i16 {
			match self {
				Self::Selfupdate => CMD_SELFUPDATE,
				Self::ForceSelfupdate => CMD_FORCE_SELFUPDATE,
				Self::ForceBuildUpdate(_) => CMD_FORCE_BUILD_UPDATE,
				Self::ForceAssetUpdate(_) => CMD_FORCE_ASSET_UPDATE,
				Self::ForceConfigUpdate(_) => CMD_FORCE_CONFIG_UPDATE,
				Self::StartProgram(_) => CMD_START_PROGRAM,
				Self::SoftStopProgram(_) => CMD_SOFT_STOP_PROGRAM,
				Self::HardStopProgram(_) => CMD_HARD_STOP_PROGRAM,
				Self::SoftReboot => CMD_SOFT_REBOOT,
				Self::HardReboot => CMD_HARD_REBOOT,
				Self::Indicate => CMD_INDICATE
			}
		}
	
		pub fn from_code(code: i16, program_id: Option<i32>) -> Result<Self, std::io::Error> {
			match code {
				CMD_SELFUPDATE => Ok(Self::Selfupdate),
				CMD_FORCE_SELFUPDATE => Ok(Self::ForceSelfupdate),
				CMD_FORCE_BUILD_UPDATE => Ok(Self::ForceBuildUpdate(Self::opt(program_id)?)),
				CMD_FORCE_ASSET_UPDATE => Ok(Self::ForceAssetUpdate(Self::opt(program_id)?)),
				CMD_FORCE_CONFIG_UPDATE => Ok(Self::ForceConfigUpdate(Self::opt(program_id)?)),
				CMD_START_PROGRAM => Ok(Self::StartProgram(Self::opt(program_id)?)),
				CMD_SOFT_STOP_PROGRAM => Ok(Self::SoftStopProgram(Self::opt(program_id)?)),
				CMD_HARD_STOP_PROGRAM => Ok(Self::HardStopProgram(Self::opt(program_id)?)),
				CMD_SOFT_REBOOT => Ok(Self::SoftReboot),
				CMD_HARD_REBOOT => Ok(Self::HardReboot),
				CMD_INDICATE => Ok(Self::Indicate),
				cmd => Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, format!("unknown cmd code: {}", cmd)))
			}
		}
	
		fn opt<T>(val: Option<T>) -> Result<T, std::io::Error> {
			match val {
				Some(val) => Ok(val),
				None => Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "no program_id"))
			}
		}
	}
	
	#[derive(Serialize, Deserialize, PartialEq, Clone)]
	pub enum ReportType {
		Reboot,
		Selfupdate,
		BuildUpdate,
		ConfigUpdate,
		AssetUpdate,
		StopProgram,
		StartProgram,
		PointConfigUpdate,
		InternalError
	}
	
	impl ReportType {
		pub fn to_code(&self) -> i16 {
			match self {
				ReportType::Reboot => RTYPE_REBOOT,
				ReportType::Selfupdate => RTYPE_SELFUPDATE,
				ReportType::BuildUpdate => RTYPE_BUILD_UPDATE,
				ReportType::ConfigUpdate => RTYPE_CONFIG_UPDATE,
				ReportType::AssetUpdate => RTYPE_ASSET_UPDATE,
				ReportType::StopProgram => RTYPE_STOP_PROGRAM,
				ReportType::StartProgram => RTYPE_START_PROGRAM,
				ReportType::PointConfigUpdate => RTYPE_POINT_CONFIG_UPDATE,
				ReportType::InternalError => RTYPE_INTERNAL_ERROR
			}
		}
	}
	
	#[derive(Serialize, Deserialize, Component, Clone)]
	pub struct Report {
		pub delay: i64,
		pub rtype: ReportType,
		pub program_id: Option<i32>,
		pub descr: Option<String>
	}
	
	#[derive(Serialize, Deserialize, PartialEq, Clone)]
	pub struct ModuleStatus {
		pub lstype: StatusCode,
		pub module: String,
		pub descr: String	
	}
	
	#[derive(Serialize, Deserialize, Clone)]
	pub struct Log {
		pub program_id: i32,
		pub delay: i64,
		pub level: i16,
		pub module: ModuleStatus
	}
	
	#[derive(Serialize, Deserialize, Clone)]
	pub struct Stat {
		pub delay: i64,
		pub name: String,
		pub data: Vec<u8>
	}
	
	#[derive(Serialize, Deserialize, PartialEq)]
	pub struct ProgramStatus {
		pub name: String,
		pub modules: Vec<ModuleStatus>
	}
}

pub mod file_server {
	use serde::{Serialize, Deserialize};

	#[derive(Serialize, Deserialize)]
	pub enum ResourceType {
		Build,
		Asset
	}

	#[derive(Serialize, Deserialize)]
	pub struct Request {
		pub point_id: i32,
		pub point_program_id: i32,
		pub token: Vec<u8>,
		pub res_type: ResourceType
	}

	#[derive(Serialize, Deserialize)]
	pub struct Answer {
		pub hash: Vec<u8>,
		pub fsize: u32
	}
}


pub mod stream_api {
	use serde::{Serialize, Deserialize};

	#[derive(Serialize, Deserialize)]
	pub struct Request {
		pub id: i32,
		pub initiator: bool
	}
}