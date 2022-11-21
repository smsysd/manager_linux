use std::io::{Read, Seek};
use std::path::{PathBuf, Path};
use std::process::{Command};
use std::{fs, io::Write};
use std::fs::{File, OpenOptions};
use ring::digest::{Context, SHA256};
use rmp_serde as rmps;

use zstd::Decoder;
use tar::Archive;
use sysinfo::{ProcessExt, System, SystemExt, Pid, PidExt};

use crate::siapi::PointConfig;
use crate::siapi::Program;
use crate::siapi::ProgramConfig;

#[derive(Serialize, Deserialize, Debug)]
pub struct Cert {
	pub host: String,
	pub data_port: u16,
	pub file_port: u16,
	pub stream_port: u16,
    pub id: Option<u32>,
    pub name: Option<String>,
    pub firm_id: Option<u32>,
    pub firm_name: Option<String>,
    pub pasw: Option<String>
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Hash)]
pub struct ProgramHashes {
	pub id: u32,
	pub build: Vec<u8>,
	pub asset: Option<Vec<u8>>
}

const DEFAULT_INDICATE_SH_PATH: &str = "./indicate.sh";
const CERT_PATH: &str = "./cert.json";
const CONFIG_PATH: &str = "./config.json";
const HASHES_PATH: &str = "./hashes.dat";
const TEMP_ARCH_PATH: &str = "./temp_download";
const TEMP_SEND_DATA_PATH: &str = "./temp_send_data";
const ARCH_TYPE: &str = "tar.zst";

const HASH_CALC_BUFFER_SIZE: usize = 4096;

fn unpack(arch: &str, to: &str) -> Result<(), String> {
    let tar_zstd = match File::open(arch) {
		Ok(f) => f,
		_ => return Err(String::from("fail to open file"))
	};

    let tar = match Decoder::new(tar_zstd) {
		Ok(s) => s,
		_ => return Err(String::from("fail create zstd decoder stream"))
	};
    let mut archive = Archive::new(tar);
    match archive.unpack(to) {
		Err(_) => Err(String::from("fail unpack")),
		_ => Ok(())
	}
}

pub fn read_cert() -> Result<Cert, String> {
	let data = fs::read(CERT_PATH).unwrap();
	match serde_json::from_slice::<Cert>(&data[..]) {
		Err(e) => Err(e.to_string()),
		Ok(cert) => Ok(cert)
	}
}

pub fn write_cert(cert: &Cert) -> Result<(), String> {
	println!("\tWRITE CERT..");
	let res = serde_json::to_vec_pretty(cert);
	match res {
		Ok(data) => {
			match fs::write(CERT_PATH, data) {
				Ok(()) => Ok(()),
				Err(e) => Err(e.to_string())
			}
		},
		Err(e) => Err(e.to_string())
	}
}

pub fn read_config() -> Result<PointConfig, String> {
	match fs::read(CONFIG_PATH) {
		Ok(data) => {
			match serde_json::from_slice::<PointConfig>(&data[..]) {
				Ok(config) => Ok(config),
				Err(e) => Err(e.to_string())
			}
		},
		Err(e) => Err(e.to_string())
	}
}

pub fn write_config(config: &PointConfig) -> Result<(), String> {
	println!("\tWRITE CONFIG..");
	let res = serde_json::to_vec_pretty(config);
	match res {
		Ok(data) => {
			match fs::write(CONFIG_PATH, data) {
				Ok(()) => Ok(()),
				Err(e) => Err(e.to_string())
			}
		},
		Err(e) => Err(e.to_string())
	}
}

pub fn read_programs_hashes() -> Vec<ProgramHashes> {
	let data = match fs::read(HASHES_PATH) {
		Ok(data) => data,
		Err(e) => {
			println!("[MOS] fail to read file with program hashes: {} make empty vec", e.to_string());
			return Vec::new();
		}
	};
	match rmps::decode::from_slice(&data) {
		Ok(data) => data,
		Err(e) => {
			println!("[MOS] fail to read programs hashes: {}", e.to_string());
			Vec::new()
		}
	}
}

pub fn write_programs_hashes(hashes: &Vec<ProgramHashes>) -> Result<(), String> {
	println!("\tWRITE HASHES..");
	let res = rmps::encode::to_vec(hashes);
	match res {
		Ok(data) => {
			match fs::write(HASHES_PATH, data) {
				Ok(()) => Ok(()),
				Err(e) => Err(e.to_string())
			}
		},
		Err(e) => Err(e.to_string())
	}
}

pub fn is_manager_already_run() -> bool {
	let mut sys = System::new_all();
	let exe_path = std::env::current_exe().unwrap();
	let exe_name = exe_path.file_name().unwrap();
	let mut self_name = exe_name.to_str().unwrap();
	if self_name.len() > 16 {
		self_name = &self_name[0..16];
	}
	println!("proc name: {}", self_name);
	sys.refresh_all();
	if sys.processes_by_name(self_name).count() > 1 {
		true
	} else {
		false
	}
}

pub fn get_hostname() -> String {
	let mut sys = System::new_all();
	sys.refresh_all();
	match sys.host_name() {
		Some(name) => name,
		_ => String::from("")
	}
}

pub fn get_bin_programs(bin_path: &str) -> Vec<String> {
	let mut dirs = Vec::<String>::new();
	let entries = match fs::read_dir(bin_path) {
		Ok(res) => res,
		_ => return dirs
	};

	for entry in entries {
		match entry {
			Ok(res) => {
				let path = res.path();
				if path.is_dir() {
					match res.file_name().into_string() {
						Ok(name) => dirs.push(name),
						_ => ()
					}
				}
			},
			_ => ()
		}
	}

	dirs
}

pub fn is_program_run(pid: u32) -> bool {
	let mut sys = System::new_all();
	sys.refresh_all();
	match sys.process(Pid::from_u32(pid)) {
		Some(_) => {
			true
		},
		None => false
	}
}

pub fn pkill_program(program_name: &str) {
	let name = if program_name.len() > 16 {
		&program_name[..16]
	} else {
		program_name
	};

	let mut sys = System::new_all();
	sys.refresh_all();
	for p in sys.processes_by_name(name) {
		p.kill();
	}
}

pub fn kill_program(pid: u32) {
	let mut sys = System::new_all();
	sys.refresh_all();
	match sys.process(Pid::from_u32(pid)) {
		Some(p) => {
			p.kill();
		},
		None => ()
	}
}

pub fn format_program_path(program_name: &str, bin_path: &str) -> String {
	format!("{}/{}", bin_path, program_name)
}

pub fn format_entry_path(program_name: &str, entry: &str, bin_path: &str) -> String {
	format!("{}/{}", format_program_path(program_name, bin_path), entry)
}

pub fn get_entry_dir(program_name: &str, entry: &str, bin_path: &str) -> PathBuf {
	let full = PathBuf::from(format_entry_path(program_name, entry, bin_path));
	PathBuf::from(full.parent().unwrap())
}

pub fn format_temp_arch_path(arch_name: &str) -> String {
	format!("{}/{}.{}", TEMP_ARCH_PATH, arch_name, ARCH_TYPE)
}

pub fn remove_program(program_name: &str, bin_path: &str) {
	let dir_path = format_program_path(program_name, bin_path);
	match fs::remove_dir_all(dir_path) {
		_ => ()
	}
}

pub fn remove_asset(program_name: &str, bin_path: &str) {
	let dir_path = format!("{}/asset", format_program_path(program_name, bin_path));
	match fs::remove_dir_all(dir_path) {
		_ => ()
	}
}

pub fn remove_config(program_name: &str, path: &str, bin_path: &str) {
	let file_path = format!("{}/{}", format_program_path(program_name, bin_path), path);
	match fs::remove_file(file_path) {
		_ => ()
	}
}

pub fn setup_program(program: &Program, temp_arch: &str, bin_path: &str, entry: Option<String>) -> Result<(), String> {
	let program_spath = format_program_path(&program.name, bin_path);
	let program_temp_spath = format!("{}.temp", program_spath);
	let program_path = Path::new(&program_spath);
	let program_temp_path = Path::new(&program_temp_spath);

	// if old dir exists
	let path_pairs = if program_path.exists() {
		// if temp dir already exists - delete
		if program_temp_path.exists() {
			match fs::remove_dir_all(&program_temp_path) {
				Err(e) => return Err(format!("fail delete temp dir: {}", e.to_string())),
				_ => ()
			}
		}
		// create clear temp dir
		match fs::create_dir_all(&program_temp_path) {
			Err(e) => return Err(format!("fail create temp dir: {}", e.to_string())),
			_ => ()
		}

		// create orig paths and temp paths of exists configs and asset dir
		let mut configs_path_pairs = Vec::<(PathBuf, PathBuf)>::new();
		for csp in &program.configs {
			let full_spath = format!("{}/{}", program_spath, csp.path);
			let path = PathBuf::from(&full_spath);
			let full_temp_spath = format!("{}/{}", program_temp_spath, csp.path);
			let temp_path = PathBuf::from(&full_temp_spath);
			if path.exists() {
				configs_path_pairs.push((path, temp_path));
			}
		}
		let asset_spath = format!("{}/asset", program_spath);
		let asset_path = PathBuf::from(&asset_spath);

		// move asset and configs which exists to temp
		let asset_temp_spath = format!("{}/asset", program_temp_spath);
		let asset_temp_path = PathBuf::from(&asset_temp_spath);
		if asset_path.exists() {
			match fs::rename(&asset_path, &asset_temp_path) {
				Err(e) => return Err(format!("fail move asset to temp dir: {}", e.to_string())),
				_ => ()
			}
		}
		for cp in &configs_path_pairs {
			// create path
			match cp.1.parent() {
				Some(parent) => {
					match fs::create_dir_all(parent) {
						Err(e) => return Err(format!("fail create path for config in temp: {}", e.to_string())),
						_ => ()
					}
				},
				None => ()
			}
			match fs::rename(&cp.0, &cp.1) {
				Err(e) => return Err(format!("fail move config to temp: {}", e.to_string())),
				_ => ()
			}
		}

		Some((configs_path_pairs, asset_path, asset_temp_path))
	} else {
		None
	};

	// perform clear setup - delete current dir, create empty dir and unpack arch into
	if program_path.exists() {
		match fs::remove_dir_all(program_path) {
			Err(e) => return Err(format!("fail remove current program dir: {}", e.to_string())),
			_ => ()
		}
	}
	match fs::create_dir_all(program_path) {
		Err(e) => return Err(format!("fail create new clear program dir: {}", e.to_string())),
		_ => ()
	}
	let arch_path = format_temp_arch_path(temp_arch);
	match unpack(&arch_path, &program_spath) {
		Err(e) => return Err(format!("fail to unpack: {}", e)),
		_ => ()
	}

	// if old dir was exists - return copied files with full paths to program dir
	match path_pairs {
		Some(paths) => {
			// move asset
			if paths.2.exists() {
				match fs::rename(&paths.2, &paths.1) {
					Err(e) => return Err(format!("fail move asset from temp to new program dir: {}", e.to_string())),
					_ => ()
				}
			}
			for cp in paths.0 {
				match cp.0.parent() {
					Some(parent) => {
						match fs::create_dir_all(parent) {
							Err(e) => return Err(format!("fail create config path in new program dir: {}", e.to_string())),
							_ => ()
						}
					},
					None => ()
				}
				match fs::rename(&cp.1, &cp.0) {
					Err(e) => return Err(format!("fail to move old config ({:?}) to new program dir ({:?}): {}", cp.1, cp.0, e.to_string())),
					_ => ()
				}
			}
		},
		None => ()
	}

	// delete temp dir
	if program_temp_path.exists() {
		match fs::remove_dir_all(&program_temp_path) {
			_ => ()
		}
	}
	
	match entry {
		Some(entry) => {
			let entry_path = format_entry_path(&program.name, &entry, bin_path);
			if !entry_exists(program, bin_path) {
				return Err(format!("program setup success, but entry in program dir not found: {}\n\tbuild on server may be corrupted", entry_path));
			}
		},
		None => ()
	}
	Ok(())
}

pub fn setup_asset(program: &Program, temp_arch: &str, bin_path: &str) -> Result<(), String> {
	let arch_path = format_temp_arch_path(temp_arch);
	let asset_path = format!("{}/asset", format_program_path(&program.name, bin_path));
	match fs::remove_dir_all(&asset_path) {
		_ => ()
	}
	match fs::create_dir_all(&asset_path) {
		Ok(()) => (),
		Err(e) => return Err(format!("fail to create asset dir: {}", e.to_string()))
	}
	unpack(&arch_path, &asset_path)
}

pub fn is_asset_exists(program: &Program, bin_path: &str) -> bool {
	let dir_path = format!("{}/asset", format_program_path(&program.name, bin_path));
	Path::new(&dir_path).exists()
}

pub fn setup_config(program: &Program, config: &ProgramConfig, data: Vec<u8>, bin_path: &str) -> Result<(), String> {
	let full_spath = format!("{}/{}", format_program_path(&program.name, bin_path), config.path);
	let full_path = Path::new(&full_spath);
	match full_path.parent() {
		Some(parent) => {
			match fs::create_dir_all(parent) {
				Err(e) => return Err(e.to_string()),
				_ => ()
			}
		},
		None => ()
	}
	let mut opt = fs::OpenOptions::new();
	opt.write(true);
	opt.create(true);
	opt.truncate(true);
	let mut file = match opt.open(&full_path) {
		Ok(f) => f,
		_ => return Err(String::from("failt to open file"))
	};

	match file.write_all(&data[..]) {
		Ok(_) => Ok(()),
		Err(e) => Err(format!("fail write to file: {}", e.to_string()))
	}
}

pub fn create_temp_arch(name: &str) -> Result<File, String> {
	let parent_path = PathBuf::from(TEMP_ARCH_PATH);
	if !parent_path.exists() {
		match fs::create_dir_all(parent_path) {
			Err(e) => return Err(e.to_string()),
			_ => ()
		}
	}
	let mut opt = OpenOptions::new();
	opt.write(true);
	opt.read(true);
	opt.create(true);
	opt.append(false);
	opt.truncate(true);
	match opt.open(format_temp_arch_path(name)) {
		Ok(f) => Ok(f),
		Err(e) => Err(e.to_string())
	}
}

pub fn temp_send_data_push(id: i64, data: &Vec<u8>) {
	let file_path = format!("{}/{}", TEMP_SEND_DATA_PATH, id);
	let mut opt = OpenOptions::new();
	opt.write(true);
	opt.create(true);
	opt.truncate(true);
	if !Path::new(TEMP_SEND_DATA_PATH).exists() {
		fs::create_dir(TEMP_SEND_DATA_PATH).unwrap();
	}
	let mut file = opt.open(&file_path).unwrap();
	file.write_all(&data).unwrap();
	std::thread::sleep(std::time::Duration::from_millis(2));
}

pub fn temp_send_data_pop() -> Option<(i64,Vec<u8>)> {
	let mut files = Vec::<(PathBuf, i64)>::new();
	let entries = match fs::read_dir(TEMP_SEND_DATA_PATH) {
		Ok(res) => res,
		_ => return None
	};

	for entry in entries {
		match entry {
			Ok(res) => {
				let path = res.path();
				if path.is_file() {
					match path.file_name() {
						Some(osstr) => {
							match osstr.to_str() {
								Some(s) => {
									match i64::from_str_radix(s, 10) {
										Ok(id) => files.push((path, id)),
										_ => ()
									}
								},
								None => ()
							}
						},
						None => ()
					}
				}
			},
			_ => ()
		}
	}

	if files.len() > 0 {
		let mut min_id_i: usize = 0;
		let mut min_val: i64 = 0;
		for i in 0..files.len() {
			if files[i].1 < min_val {
				min_id_i = i;
				min_val = files[i].1;
			}
		}
		let mut file = File::open(&files[min_id_i].0).unwrap();
		let mut buf = Vec::<u8>::new();
		file.read_to_end(&mut buf).unwrap();
		drop(file);
		fs::remove_file(&files[min_id_i].0).unwrap();
		Some((min_val, buf))
	} else {
		None
	}
}

pub fn hash_vec(data: &Vec<u8>) -> Vec<u8> {
	let mut context = Context::new(&SHA256);
	context.update(&data);
	context.finish().as_ref().to_vec()
}

pub fn hash_file(file: &mut File) -> Vec<u8> {
	let mut context = Context::new(&SHA256);
	let mut buf: [u8;HASH_CALC_BUFFER_SIZE] = [0;HASH_CALC_BUFFER_SIZE];
	file.rewind().unwrap();
	loop {
		let len = file.read(&mut buf).unwrap();
		if len == 0 {
			break;
		} else {
			context.update(&buf[..len]);
		}
	}
	context.finish().as_ref().to_vec()
}

pub fn config_hash(program_name: &str, config: &str, bin_path: &str) -> Vec<u8> {
	let full_spath = format!("{}/{}", format_program_path(program_name, bin_path), config);
	let full_path = Path::new(&full_spath);
	match File::open(full_path) {
		Ok(mut f) => hash_file(&mut f),
		Err(_) => {
			// println!("[MOS][CONFIG_HASH] fail to open config file by path {} : {}", full_spath, e.to_string());
			Vec::new()
		}
	}
}

pub fn entry_exists(program: &Program, bin_path: &str) -> bool {
	let entry_spath = format_entry_path(&program.name, &program.entry, bin_path);
	let path = Path::new(&entry_spath);
	if !path.exists() {
		return false;
	}
	path.is_file()
}

pub fn indicate() -> Result<(), String> {
	let path = Path::new(DEFAULT_INDICATE_SH_PATH);
	if path.exists() {
		let mut cmd = Command::new("sh");
		cmd.arg("indicate.sh");
		match cmd.status() {
			Ok(rc) => {
				if rc.success() {
					Ok(())
				} else {
					Err(rc.to_string())
				}
			},
			Err(e) => Err(e.to_string())
		}
	} else {
		Err(format!("default indicate not implemented"))
	}
}