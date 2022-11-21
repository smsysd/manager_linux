
use std::sync::Arc;
use std::sync::Mutex;

use crate::mos;
use crate::siapi;
use crate::exec;
use siapi::PointConfig;
use siapi::Program;
use siapi::ProgramConfig;
use siapi::IntApi;
use siapi::Report;
use siapi::report_types::*;
use siapi::UpdateDataReq;
use mos::ProgramHashes;
use exec::Exec;

struct ConfigHash {
    config_id: u32,
    program_id: u32,
    hash: Vec<u8>
}

fn update_config_hashes(config: &PointConfig) -> Vec<ConfigHash> {
    let mut hashes = Vec::new();
    for p in &config.programs {
        for c in &p.configs {
            hashes.push(ConfigHash {
                config_id: c.id,
                program_id: p.id,
                hash: mos::config_hash(&p.name, &c.path, &config.bin_path)
            });
        }
    }
    hashes
}

fn find_config_hash(id: u32, hashes: &Vec<ConfigHash>) -> Option<&ConfigHash> {
    for h in hashes {
        if h.config_id == id {
            return Some(h);
        }
    }
    None
}

fn find_program_hash(id: u32, hashes: &Vec<ProgramHashes>) -> Option<&ProgramHashes> {
    for h in hashes {
        if h.id == id {
            return Some(h);
        }
    }
    None
}

fn update_program_build_hash(id: u32, hashes: &mut Vec<ProgramHashes>, new_hash: &Vec<u8>) {
    for i in 0..hashes.len() {
        if hashes[i].id == id {
            hashes[i].build = new_hash.clone();
            return;
        }
    }
    hashes.push(ProgramHashes {id: id, build: new_hash.clone(), asset: None});
}

fn update_program_asset_hash(id: u32, hashes: &mut Vec<ProgramHashes>, new_hash: &Option<Vec<u8>>) {
    for i in 0..hashes.len() {
        if hashes[i].id == id {
            hashes[i].asset = new_hash.clone();
            return;
        }
    }
    hashes.push(ProgramHashes {id: id, build: Vec::new(), asset: new_hash.clone()});
}

impl Program {
    pub fn find_config(&self, id: u32) -> Option<&ProgramConfig> {
        for c in &self.configs {
            if c.id == id {
                return Some(c);
            }
        }
        None
    }
}

impl PointConfig {
    pub fn find_program(&self, id: u32) -> Option<&Program> {
        for p in &self.programs {
            if p.id == id {
                return Some(p);
            }
        }
    
        None
    }

    pub fn find_program_by_name(&self, name: &str) -> Option<&Program> {
        for p in &self.programs {
            if p.name == name {
                return Some(p);
            }
        }
    
        None
    }

    pub fn find_program_mut(&mut self, id: u32) -> Option<&mut Program> {
        for p in &mut self.programs {
            if p.id == id {
                return Some(p);
            }
        }
    
        None
    }

    pub fn find_config(&self, program_id: u32, config_id: u32) -> Option<&ProgramConfig> {
        for p in &self.programs {
            if p.id == program_id {
                for c in &p.configs {
                    if c.id == config_id {
                        return Some(c);
                    }
                }
            }
        }
    
        None
    }

    pub fn program_exists(&self, id: Option<u32>, name: Option<&str>) -> bool {
        if id.is_none() && name.is_none() {
            return false;
        }
        for p in &self.programs {
            let id_res = match id {
                Some(id) => p.id == id,
                None => true
            };
            let name_res = match name {
                Some(name) => p.name == name,
                None => true
            };
            if id_res && name_res {
                return true;
            }
        }

        false
    }
}

// return true if corrupted
fn check_corrupt_program(program: &Program, bin_path: &str) -> bool {
    if exec::is_exec_program(program.ptype) {
        if !exec::is_builtin(program.ptype) {
            !mos::entry_exists(program, bin_path)
        } else {
            true
        }
    } else {
        true
    }
}

pub fn update_program_build(exec: Option<Arc<Mutex<Exec>>>, program: &Program, api: Arc<Mutex<IntApi>>, cmd: Option<u32>, bin_path: &str, ipc_dir: &str, program_hashes: &mut Vec<ProgramHashes>) -> Result<(), String> {
    println!("UPDATE PROGRAM BUILD FOR {}", program.name);
    
    match exec {
        Some(ex) => exec::stop_program(ex.clone(), api.clone(), cmd, None, true, ipc_dir)?,
        None => ()
    };
    let res = match api.lock().unwrap().download_program(program.id) {
        Ok((fname, hash)) => {
            println!("BUILD DOWNLOADED SUCCESS, SETUP..");
            let entry = if exec::is_exec_program(program.ptype) {
                Some(program.entry.clone())
            } else {
                None
            };
            match mos::setup_program(program, &fname, bin_path, entry) {
                Err(e) => {
                    Err(e)
                },
                Ok(()) => {
                    update_program_build_hash(program.id, program_hashes, &hash);
                    Ok(())
                }
            }
        },
        Err(e) => return Err(format!("fail download file from server: {}", siapi::get_rc_name(e)))
    };

    match res.clone() {
        Err(e) => {
            let must_have = cmd.is_some();
            api.lock().unwrap().send_report(Report {rtype: REPORT_INTERNAL_ERROR, program_id: Some(program.id), cmd_id: cmd, descr: Some(e), delay: 0}, must_have);
        },
        _ => {
            api.lock().unwrap().send_report(Report {rtype: REPORT_BUILD_UPDATE, program_id: Some(program.id), cmd_id: cmd, descr: None, delay: 0}, true);
        }
    }
    res
}

pub fn update_program_asset(exec: Option<Arc<Mutex<Exec>>>, program: &Program, api: Arc<Mutex<IntApi>>, cmd: Option<u32>, bin_path: &str, ipc_dir: &str, program_hashes: &mut Vec<ProgramHashes>) -> Result<(), String> {
    match exec {
        Some(ex) => exec::stop_program(ex, api.clone(), cmd, None, true, ipc_dir)?,
        None => ()
    }
    let res = match api.lock().unwrap().download_asset(program.id) {
        Ok((fname, hash)) => {
            match mos::setup_asset(program, &fname, bin_path) {
                Ok(()) => {
                    update_program_asset_hash(program.id, program_hashes, &Some(hash));
                    Ok(())
                },
                Err(e) => Err(e)
            }
        },
        Err(e) => return Err(format!("fail download file from server: {}", siapi::get_rc_name(e)))
    };
    match res.clone() {
        Err(e) => {
            let must_have = cmd.is_some();
            api.lock().unwrap().send_report(Report {rtype: REPORT_INTERNAL_ERROR, program_id: Some(program.id), cmd_id: cmd, descr: Some(e), delay: 0}, must_have);
        },
        _ => {
            api.lock().unwrap().send_report(Report {rtype: REPORT_ASSET_UPDATE, program_id: Some(program.id), cmd_id: cmd, descr: None, delay: 0}, true);
        }
    }
    res
}

pub fn update_program_config(exec: Option<Arc<Mutex<Exec>>>, program: &Program, config: &ProgramConfig, api: Arc<Mutex<IntApi>>, cmd: Option<u32>, bin_path: &str, ipc_dir: &str) -> Result<(), String> {
    match exec {
        Some(ex) => exec::stop_program(ex, api.clone(), cmd, None, true, ipc_dir)?,
        None => ()
    }
    let res = match api.lock().unwrap().get_program_config(config.id) {
        Ok(answ) => mos::setup_config(program, config, answ.data, bin_path),
        Err(e) => return Err(format!("fail get config from server: {}", siapi::get_rc_name(e)))
    };
    match res.clone() {
        Err(e) => {
            let must_have = cmd.is_some();
            api.lock().unwrap().send_report(Report {rtype: REPORT_INTERNAL_ERROR, program_id: Some(program.id), cmd_id: cmd, descr: Some(e), delay: 0}, must_have);
        },
        _ => {
            api.lock().unwrap().send_report(Report {rtype: REPORT_CONFIG_UPDATE, program_id: Some(program.id), cmd_id: cmd, descr: None, delay: 0}, true);
        }
    }
    res
}

pub fn update_program_config_all(exec: Option<Arc<Mutex<Exec>>>, program: &Program, api: Arc<Mutex<IntApi>>, cmd: Option<u32>, bin_path: &str, ipc_dir: &str) -> Result<(), String> {
    let mut res: Result<(), String> = Ok(());
    for c in &program.configs {
        match update_program_config(exec.clone(), program, c, api.clone(), cmd, bin_path, ipc_dir) {
            Err(e) => {
                res = Err(e);
                break;
            },
            _ => ()
        }
    }
    res
}

fn find_program_fnvec(rprogram: &Vec<String>, name: &str) -> bool {
    for p in rprogram {
        if p == name {
            return true;
        }
    }

    return false;
}

// delete not used programs, download missing or corrupted
// sync deleted config files, rename program folders if found same id with diff names
pub fn sync_config_with_real(execs: Option<Vec<Arc<Mutex<Exec>>>>, config: &mut PointConfig, api: Arc<Mutex<IntApi>>, cmd: Option<u32>, old: Option<&PointConfig>, program_hashes: &mut Vec<ProgramHashes>) -> Result<Vec<u32>, String> {
    let rprograms = mos::get_bin_programs(&config.bin_path);

    // remove unused programs
    println!("REMOVE UNUSED PROGRAMS..");
    for rp in &rprograms {
        match config.find_program_by_name(rp) {
            Some(p) => {
                // check asset
                if p.asset_id.is_none() {
                    if mos::is_asset_exists(p, &config.bin_path) {
                        println!("REMOVE ASSET FOR {} program", rp);
                        mos::remove_asset(rp, &config.bin_path);
                    }
                }
            },
            None => {
                println!("REMOVE {} program", rp);
                mos::remove_program(rp, &config.bin_path);
            }
        }
    }

    println!("CHECK CORRUPTED PROGRAMS..");
    let rprograms = mos::get_bin_programs(&config.bin_path);
    let mut upg_programs = Vec::<u32>::new();
    for i in 0..config.programs.len() {
        let need_upg = if find_program_fnvec(&rprograms, &config.programs[i].name) {
            check_corrupt_program(&config.programs[i], &config.bin_path)
        } else {
            true
        };
        if need_upg {
            println!("PROGRAM {} NEED UPGRADE: CORRUPTED", config.programs[i].name);
            upg_programs.push(config.programs[i].id);
            let exec = match execs.clone() {
                Some(execs) => {
                    exec::exec_find(&execs, config.programs[i].id)
                },
                None => None
            };
            match update_program_build(exec, &mut config.programs[i], api.clone(), cmd, &config.bin_path, &config.ipc_dir, program_hashes) {
                Err(e) => return Err(e),
                _ => ()
            }
        } else {
            println!("\tprogram {} not courrupted", config.programs[i].name);
        }

        // delete old configs; if in old - exists and in new not
        match old {
            Some(old_config) => {
                // if in old program was exists
                println!("\tOLD CONFIG EXISTS - CLEAR CONFIGS..");
                match old_config.find_program(config.programs[i].id) {
                    Some(p) => {
                        println!("\t\tCLEAR CONFIGS FOR PROGRAM {}", p.name);
                        for c in &p.configs {
                            if config.find_config(config.programs[i].id, c.id).is_none() {
                                println!("\t\tREMOVE CONFIG {} FOR {} program", c.name, config.programs[i].name);
                                mos::remove_config(&config.programs[i].name, &c.path, &config.bin_path);
                            }
                        }
                    },
                    None => ()
                }
            },
            None => ()
        }
    }
    Ok(upg_programs)
}

// get update data and perform all updates
pub fn check_update_all(execs: Option<Vec<Arc<Mutex<Exec>>>>, config: &PointConfig, api: Arc<Mutex<IntApi>>, program_hashess: &mut Vec<ProgramHashes>) -> Result<bool, String> {
    println!("\t[UPDATE_ALL] UPDATE HASHES..");
    let config_hashes = update_config_hashes(config);
    println!("\t[UPDATE_ALL] COLLECT HASHES..");
    let mut hashes = UpdateDataReq {programs: Vec::<siapi::ProgramUpdateDataReq>::new()};
    for p in &config.programs {
        let phashes = match find_program_hash(p.id, program_hashess) {
            Some(res) => ProgramHashes {id: p.id, build: res.build.clone(), asset: res.asset.clone()},
            None => ProgramHashes {id: p.id, build: Vec::new(), asset: None}
        };
        let mut pudr = siapi::ProgramUpdateDataReq {
            id: p.id,
            program_hash: phashes.build,
            asset_hash: phashes.asset,
            configs: Vec::<siapi::ConfigUpdateDataReq>::new()
        };
        for c in &p.configs {
            pudr.configs.push(siapi::ConfigUpdateDataReq {
                id: c.id,
                hash: {
                    match find_config_hash(c.id, &config_hashes) {
                        Some(h) => h.hash.clone(),
                        None => Vec::new()
                    }
                }
            });
        }
        hashes.programs.push(pudr);
    }
    println!("{:?}", hashes);
    let bin_path = config.bin_path.clone();
    let ipc_dir = config.ipc_dir.clone();
    println!("\t[UPDATE_ALL] GET UPDATE DATA..");
    let get_update_data_res = api.lock().unwrap().get_update_data(&hashes); 
    match get_update_data_res {
        Ok(answ) => {
            println!("\t[UPDATE_ALL] GET UPDATE DATA OK, PERFORM UPDATES..");
            let mut any_updates = false;
            for p in &answ.programs {
                match config.find_program(p.id) {
                    Some(config_program) => {
                        // get exec
                        let exec = match execs.clone() {
                            Some(execs) => {
                                exec::exec_find(&execs, p.id)
                            },
                            None => None
                        };
                        
                        let is_run = match exec.clone() {
                            Some(ex) => {
                                ex.lock().unwrap().is_run()
                            },
                            None => false
                        };
                        let mut is_updated = false;
                        if p.need_program_update && config_program.autoupdate {
                            println!("\t[UPDATE_ALL] program {} need build update..", config_program.name);
                            is_updated = true;
                            match update_program_build(exec.clone(), config_program, api.clone(), None, &bin_path, &ipc_dir, program_hashess) {
                                Err(e) => return Err(e),
                                _ => ()
                            }
                        }
                        if p.need_asset_update && config_program.autoupdate_asset {
                            println!("\t[UPDATE_ALL] program {} need asset update..", config_program.name);
                            is_updated = true;
                            match update_program_asset(exec.clone(), config_program, api.clone(), None, &bin_path, &ipc_dir, program_hashess) {
                                Err(e) => return Err(e),
                                _ => ()
                            }
                        }
                        if config_program.autoupdate_config {
                            for c in &p.configs {
                                match config_program.find_config(c.id) {
                                    Some(config_config) => {
                                        if c.need_update {
                                            println!("\t[UPDATE_ALL] program {} need update config {}..", config_program.name, config_config.name);
                                            is_updated = true;
                                            match update_program_config(exec.clone(), config_program, config_config, api.clone(), None, &bin_path, &ipc_dir) {
                                                Ok(()) => (),
                                                Err(e) => return Err(e)
                                            }
                                        }
                                    },
                                    None => return Err(format!("config with id {} not found", c.id))
                                }
                            }
                        }
                        if is_updated {
                            any_updates = true;
                        }
                        match exec {
                            Some(ex) => {
                                if is_updated && is_run {
                                    match exec::start_program(ex, api.clone(), None, &config.bin_path) {
                                        Err(e) => return Err(e),
                                        _ => ()
                                    }
                                }
                            },
                            None => ()
                        }
                    },
                    None => return Err(format!("program with {} id not found in config", p.id))
                }
            }
            Ok(any_updates)
        },
        Err(e) => Err(format!("fail get update data: {}", siapi::get_rc_name(e)))
    }
}

pub fn upload_point_config(api: Arc<Mutex<IntApi>>) -> Result<PointConfig, u8> {
    println!("upload point config from server..");
    let res = match api.lock().unwrap().get_point_config() {
        Ok(config) => {
            println!("point config downloaded success");
            mos::write_config(&config).unwrap();
            Ok(config)
        },
        Err(e) => Err(e)
    };

    match res {
        Ok(config) => {
            api.lock().unwrap().send_report(Report {rtype: REPORT_POINT_CONFIG_UPDATE, program_id: None, cmd_id: None, descr: None, delay: 0}, true);
            Ok(config)
        },
        Err(e) => {
            api.lock().unwrap().send_report(Report {rtype: REPORT_ERROR, program_id: None, cmd_id: None, descr: Some(siapi::get_rc_name(e)), delay: 0}, false);
            Err(e)
        }
    }
}