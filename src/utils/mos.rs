use std::io::{Read, Seek};
use std::path::{PathBuf, Path};
use std::process::{Command};
use std::thread;
use std::time::Duration;
use std::{fs, io::Write};
use std::fs::{File, OpenOptions};
use ring::digest::{Context, SHA256};
use rmp_serde as rmps;
use std::io::Error;

use zstd::Decoder;
use tar::Archive;
use sysinfo::{ProcessExt, System, SystemExt, Pid, PidExt};

use crate::data_types::data_server::{GetPointConfigAnsw as PointConfig, ProgramCustom};
use crate::data_types::data_server::Program;
use crate::data_types::Cert;
use crate::data_types::data_server::ProgramHashes;

use crate::utils::err;

const DEFAULT_INDICATE_SH_PATH: &str = "./indicate.sh";
const CERT_PATH: &str = "./cert.json";
const CONFIG_PATH: &str = "./config.json";
const HASHES_PATH: &str = "./hashes.dat";
const TEMP_ARCH_PATH: &str = "./temp_download";
const TEMP_SEND_DATA_PATH: &str = "./temp_send_data";
const ARCH_TYPE: &str = "tar.zst";

const HASH_CALC_BUFFER_SIZE: usize = 4096;

fn unpack(arch: &str, to: &str) -> Result<(), Error> {
    let tar_zstd = File::open(arch)?;

    let tar = Decoder::new(tar_zstd)?;
    let mut archive = Archive::new(tar);
    archive.unpack(to)?;
	Ok(())
}

pub fn read_cert() -> Result<Cert, Error> {
	let data = fs::read(CERT_PATH)?;
	match serde_json::from_slice::<Cert>(&data[..]) {
		Err(e) => Err(err(&e.to_string())),
		Ok(cert) => Ok(cert)
	}
}

pub fn write_cert(cert: &Cert) -> Result<(), Error> {
	let res = serde_json::to_vec_pretty(cert);
	match res {
		Ok(data) => {
			fs::write(CERT_PATH, data)?;
			Ok(())
		},
		Err(e) => Err(err(&e.to_string()))
	}
}

pub fn read_config() -> Result<PointConfig, Error> {
	let data = fs::read(CONFIG_PATH)?;
	match serde_json::from_slice::<PointConfig>(&data[..]) {
		Ok(config) => Ok(config),
		Err(e) => Err(err(&e.to_string()))
	}
}

pub fn write_config(config: &PointConfig) -> Result<(), Error> {
	match serde_json::to_vec_pretty(config) {
		Ok(data) => {
			fs::write(CONFIG_PATH, data)?;
			Ok(())
		},
		Err(e) => Err(err(&e.to_string()))
	}
}

pub fn read_programs_hashes() -> Vec<ProgramHashes> {
	let data = match fs::read(HASHES_PATH) {
		Ok(data) => data,
		Err(_) => return Vec::new()
	};
	match rmps::decode::from_slice(&data) {
		Ok(data) => data,
		Err(_) => Vec::new()
	}
}

pub fn write_programs_hashes(hashes: &Vec<ProgramHashes>) -> Result<(), Error> {
	match rmps::encode::to_vec(hashes) {
		Ok(data) => {
			fs::write(HASHES_PATH, data)?;
			Ok(())
		},
		Err(e) => Err(err(&e.to_string()))
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
	sys.processes_by_name(self_name).count() > 1
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

pub fn is_run_by_name(name: &str) -> bool {
	let mut sys = System::new_all();
	let mut name = String::from(name);
	if name.len() > 16 {
		name = String::from(&name[0..16]);
	}
	sys.refresh_all();
	sys.processes_by_name(&name).count() > 1
}

pub fn is_run_by_pid(pid: u32) -> bool {
	let mut sys = System::new_all();
	sys.refresh_all();
	match sys.process(Pid::from_u32(pid)) {
		Some(_) => {
			true
		},
		None => false
	}
}

pub fn pkill(program_name: &str) {
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

pub fn kill(pid: u32) {
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

pub fn format_entry_dir(program_name: &str, entry: &str, bin_path: &str) -> PathBuf {
	let full = PathBuf::from(format_entry_path(program_name, entry, bin_path));
	PathBuf::from(full.parent().unwrap())
}

pub fn format_temp_arch_path(arch_name: &str) -> String {
	format!("{}/{}.{}", TEMP_ARCH_PATH, arch_name, ARCH_TYPE)
}

pub fn remove_program(program_name: &str, bin_path: &str) -> Result<(), Error> {
	let dir_path = format_program_path(program_name, bin_path);
	fs::remove_dir_all(dir_path)?;
	Ok(())
}

pub fn remove_asset(program_name: &str, bin_path: &str) -> Result<(), Error> {
	let dir_path = format!("{}/asset", format_program_path(program_name, bin_path));
	fs::remove_dir_all(dir_path)?;
	Ok(())
}

pub fn remove_config(program_name: &str, path: &str, bin_path: &str) -> Result<(), Error> {
	let file_path = format!("{}/{}", format_program_path(program_name, bin_path), path);
	fs::remove_file(file_path)?;
	Ok(())
}

pub fn setup_program(program: &ProgramCustom, name: &str, temp_arch: &str, bin_path: &str) -> Result<(), Error> {
	let program_spath = format_program_path(name, bin_path);
	let program_temp_spath = format!("{}.temp", program_spath);
	let program_path = Path::new(&program_spath);
	let program_temp_path = Path::new(&program_temp_spath);

	// if old dir exists
	let path_pairs = if program_path.exists() {
		// if temp dir already exists - delete
		if program_temp_path.exists() {
			fs::remove_dir_all(&program_temp_path)?;
		}
		// create clear temp dir
		fs::create_dir_all(&program_temp_path)?;

		// create orig paths and temp paths of exists configs and asset dir
		let mut configs_path_pairs = Vec::<(PathBuf, PathBuf)>::new();
		for csp in &program.configs {
			let full_spath = format!("{}/{}", program_spath, csp.1);
			let path = PathBuf::from(&full_spath);
			let full_temp_spath = format!("{}/{}", program_temp_spath, csp.1);
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
			fs::rename(&asset_path, &asset_temp_path)?;
		}
		for cp in &configs_path_pairs {
			// create path
			match cp.1.parent() {
				Some(parent) => fs::create_dir_all(parent)?,
				None => ()
			}
			fs::rename(&cp.0, &cp.1)?;
		}

		Some((configs_path_pairs, asset_path, asset_temp_path))
	} else {
		None
	};

	// perform clear setup - delete current dir, create empty dir and unpack arch into
	if program_path.exists() {
		fs::remove_dir_all(program_path)?;
	}
	fs::create_dir_all(program_path)?;
	let arch_path = format_temp_arch_path(temp_arch);
	unpack(&arch_path, &program_spath)?;

	// if old dir was exists - return copied files with full paths to program dir
	match path_pairs {
		Some(paths) => {
			// move asset
			if paths.2.exists() {
				fs::rename(&paths.2, &paths.1)?;
			}
			for cp in paths.0 {
				match cp.0.parent() {
					Some(parent) => {
						fs::create_dir_all(parent)?;
					},
					None => ()
				}
				fs::rename(&cp.1, &cp.0)?;
			}
		},
		None => ()
	}

	// delete temp dir
	if program_temp_path.exists() {
		fs::remove_dir_all(&program_temp_path)?;
	}

	Ok(())
}

pub fn setup_asset(name: &str, temp_arch: &str, bin_path: &str) -> Result<(), Error> {
	let arch_path = format_temp_arch_path(temp_arch);
	let asset_path = format!("{}/asset", format_program_path(name, bin_path));
	fs::remove_dir_all(&asset_path)?;
	fs::create_dir_all(&asset_path)?;
	unpack(&arch_path, &asset_path)
}

pub fn is_asset_exists(program: &Program, bin_path: &str) -> bool {
	let dir_path = format!("{}/asset", format_program_path(&program.name, bin_path));
	Path::new(&dir_path).exists()
}

pub fn setup_config(name: &str, config_path: &str, data: &Vec<u8>, bin_path: &str) -> Result<(), Error> {
	let full_spath = format!("{}/{}", format_program_path(name, bin_path), config_path);
	let full_path = Path::new(&full_spath);
	match full_path.parent() {
		Some(parent) => fs::create_dir_all(parent)?,
		None => ()
	}
	let mut opt = fs::OpenOptions::new();
	opt.write(true);
	opt.create(true);
	opt.truncate(true);
	let mut file = opt.open(&full_path)?;
	file.write_all(&data[..])
}

pub fn create_temp_arch(name: &str) -> Result<File, Error> {
	let parent_path = PathBuf::from(TEMP_ARCH_PATH);
	if !parent_path.exists() {
		fs::create_dir_all(parent_path)?;
	}
	let mut opt = OpenOptions::new();
	opt.write(true);
	opt.read(true);
	opt.create(true);
	opt.append(false);
	opt.truncate(true);
	opt.open(format_temp_arch_path(name))
}

pub fn temp_send_data_push(id: i64, data: &Vec<u8>) -> Result<(), Error> {
	let file_path = format!("{}/{}", TEMP_SEND_DATA_PATH, id);
	let mut opt = OpenOptions::new();
	opt.write(true);
	opt.create(true);
	opt.truncate(true);
	if !Path::new(TEMP_SEND_DATA_PATH).exists() {
		fs::create_dir(TEMP_SEND_DATA_PATH)?;
	}
	let mut file = opt.open(&file_path)?;
	file.write_all(&data)?;
	std::thread::sleep(std::time::Duration::from_millis(2));
	Ok(())
}

pub fn temp_send_data_pop() -> Result<Option<(i64,Vec<u8>)>, Error> {
	let mut files = Vec::<(PathBuf, i64)>::new();
	let entries = fs::read_dir(TEMP_SEND_DATA_PATH)?;

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
		let mut file = File::open(&files[min_id_i].0)?;
		let mut buf = Vec::<u8>::new();
		file.read_to_end(&mut buf)?;
		drop(file);
		fs::remove_file(&files[min_id_i].0)?;
		Ok(Some((min_val, buf)))
	} else {
		Ok(None)
	}
}

pub fn hash_vec(data: &Vec<u8>) -> Vec<u8> {
	let mut context = Context::new(&SHA256);
	context.update(&data);
	context.finish().as_ref().to_vec()
}

pub fn hash_file(file: &mut File) -> Result<Vec<u8>, Error> {
	let mut context = Context::new(&SHA256);
	let mut buf: [u8;HASH_CALC_BUFFER_SIZE] = [0;HASH_CALC_BUFFER_SIZE];
	file.rewind()?;
	loop {
		let len = file.read(&mut buf)?;
		if len == 0 {
			break;
		} else {
			context.update(&buf[..len]);
		}
	}
	Ok(context.finish().as_ref().to_vec())
}

pub fn config_hash(program_name: &str, config: &str, bin_path: &str) -> Vec<u8> {
	let full_spath = format!("{}/{}", format_program_path(program_name, bin_path), config);
	let full_path = Path::new(&full_spath);
	match File::open(full_path) {
		Ok(mut f) => match hash_file(&mut f) {
			Ok(hash) => hash,
			Err(_) => Vec::new()
		},
		Err(_) => Vec::new()
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

pub fn indicate() {
	let path = Path::new(DEFAULT_INDICATE_SH_PATH);
	if path.exists() {
		let mut cmd = Command::new("sh");
		cmd.arg("indicate.sh");
		match cmd.status() {
			_ => ()
		}
	}
}

pub fn reboot() -> Result<(), Error> {
    let mut cmd = Command::new("sudo");
    cmd.arg("reboot");
    cmd.status()?;
    thread::sleep(Duration::from_secs(10));
	Ok(())
}