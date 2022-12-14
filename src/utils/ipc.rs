use std::{io::{Error, Write, Read}, os::unix::net::UnixStream, time::Duration};
use bevy_ecs::system::Resource;
use serde::{Deserialize, Serialize};

use crate::data_types::data_server::{ProgramStatus, Stat, ModuleStatus, IpcType};
use super::{rmp_decode, json_decode};

const WRITE_TIMEOUT: Duration = Duration::from_millis(1000);
const READ_TIMEOUT: Duration = Duration::from_millis(2000);


#[derive(Serialize)]
pub enum RequestToProgram {
	GetStatus,			// -> ProgramStatus
	Terminate(bool)		// is_hard -> OkAnsw
}

#[derive(Deserialize, Clone)]
pub struct Log {
	pub name: String,
	pub level: i16,
	pub module: ModuleStatus
}

#[derive(Deserialize)]
pub enum RequestFromProgram {
	Log(Log),			// -> ResAnsw
	Stat(Stat)			// -> ResAnsw
}

#[derive(Deserialize, Serialize)]
pub enum ResAnsw {
	Ok,
	Err
}

#[derive(Resource)]
pub struct Ipc {
	ipc_dir: String
}

impl Ipc {
	pub fn new(ipc_dir: &str) -> Self {
		Self {
			ipc_dir: String::from(ipc_dir)
		}
	}

	pub fn terminate(&self, pname: &str, hard: bool, ipc_type: &IpcType) -> Result<(), Error> {
		let answ_raw = self.request(RequestToProgram::Terminate(hard), pname, ipc_type)?;
		let _: ResAnsw = match ipc_type {
			IpcType::Msgpack => rmp_decode(&answ_raw)?,
			IpcType::Json => json_decode(&answ_raw)?
		};
		Ok(())
	}

	pub fn get_status(&self, pname: &str, ipc_type: &IpcType) -> Result<ProgramStatus, Error> {
		let answ_raw = self.request(RequestToProgram::GetStatus, pname, ipc_type)?;
		Ok(match ipc_type {
			IpcType::Msgpack => rmp_decode(&answ_raw)?,
			IpcType::Json => json_decode(&answ_raw)?
		})
	}

	fn request(&self, req: RequestToProgram, pname: &str, ipc_type: &IpcType) -> Result<Vec<u8>, Error> {
		println!("[IPC] request to {}", self.format_ipc(pname));
		let mut stream = UnixStream::connect(self.format_ipc(pname))?;
		stream.set_write_timeout(Some(WRITE_TIMEOUT))?;
		stream.set_read_timeout(Some(READ_TIMEOUT))?;
		let req_raw = match ipc_type {
			IpcType::Msgpack => rmp_serde::to_vec(&req).unwrap(),
			IpcType::Json => serde_json::to_vec(&req).unwrap()
		};
		stream.write_all(&req_raw)?;
		let mut answ_raw = Vec::new();
		stream.read_to_end(&mut answ_raw)?;
		Ok(answ_raw)
	}

	fn format_ipc(&self, pname: &str) -> String {
		format!("{}/{}", self.ipc_dir, pname)
	}
}