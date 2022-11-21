pub mod data_server {
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
	const CMD_SOFT_RESTART_PROGRAM: i16 = 23;
	const CMD_HARD_RESTART_PROGRAM: i16 = 24;
	const CMD_SOFT_REBOOT: i16 = 25;
	const CMD_HARD_REBOOT: i16 = 26;
	const CMD_INDICATE: i16 = 40;


	#[derive(Serialize, Deserialize)]
	pub enum Request {
		Poll(Auth),
		GetUpdateData(Auth, Vec<ProgramUpdateData>),
		GetPointConfig(Auth),
		GetProgramConfig(Auth, i32),				// config_id
		AddLog(Auth, Log),
		AddStat(Auth, Stat),
		AddReport(Auth, Report),
		Register(String, Option<String>),			// point_name, firm_name
		SetStatus(Auth, ProgramStatus),
		SetRunStatus(Auth, ProgramRunStatus)
	}
	
	#[derive(Serialize, Deserialize)]
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
		Stream(i32, i32)		// stream_id, point_program_id
	}
	
	// - - - - - - - UPDATE DATA - - - - - - - - - //
	
	#[derive(Serialize, Deserialize)]
	pub struct ProgramUpdateData {
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
	#[derive(Serialize, Deserialize)]
	pub struct ProgramCustom {
		pub autoupdate: bool,
		pub config_autoupdate: bool,
		pub asset_autoupdate: bool,
		pub use_ipc: bool,
		pub log_level: i16,
		pub configs: Vec<(i32, String)>
	}
	
	#[derive(Serialize, Deserialize)]
	pub enum ProgramType {
		Custom(ProgramCustom),
		Builtin
	}
	
	#[derive(Serialize, Deserialize)]
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
	
	#[derive(Serialize, Deserialize)]
	pub struct GetPointConfigAnsw {
		pub poll_period: i64,
		pub bin_path: String,
		pub ipc_dir: String,
		pub programs: Vec<Program>
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
	
	#[derive(Serialize, Deserialize, PartialEq)]
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
		pub status: RunStatus
	}
	
	#[derive(Serialize, Deserialize)]
	pub enum RunStatus {
		Stopped(Option<String>),	// Last words
		Run,
		Stopping,
		Crashing(Option<String>)	// Last words
	}
	
	impl RunStatus {
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
		SoftRestartProgram(i32),
		HardRestartProgram(i32),
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
				Self::SoftRestartProgram(_) => CMD_START_PROGRAM,
				Self::HardRestartProgram(_) => CMD_HARD_RESTART_PROGRAM,
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
				CMD_SOFT_RESTART_PROGRAM => Ok(Self::SoftRestartProgram(Self::opt(program_id)?)),
				CMD_HARD_RESTART_PROGRAM => Ok(Self::HardRestartProgram(Self::opt(program_id)?)),
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
	
	#[derive(Serialize, Deserialize, PartialEq)]
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
	
	#[derive(Serialize, Deserialize)]
	pub struct Report {
		pub delay: i64,
		pub rtype: ReportType,
		pub program_id: Option<i32>,
		pub cmd_id: Option<i32>,
		pub descr: Option<String>
	}
	
	#[derive(Serialize, Deserialize, PartialEq)]
	pub struct ModuleStatus {
		pub lstype: StatusCode,
		pub module: String,
		pub descr: String	
	}
	
	#[derive(Serialize, Deserialize)]
	pub struct Log {
		pub program_id: i32,
		pub delay: i64,
		pub level: i16,
		pub module: ModuleStatus
	}
	
	#[derive(Serialize, Deserialize)]
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
