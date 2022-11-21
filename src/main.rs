
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::process::Command;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;
use std::time::Instant;
use bevy_ecs::prelude::*;
use std::io::Error;

pub mod data_types;
pub mod utils;
pub mod ipcs;
pub mod exec;
pub mod streamer;
pub mod configm;

use mos::Cert;
use mos::ProgramHashes;
use siapi::{CmdAnsw, ModuleStatus, Report, Status};
use siapi::PointConfig;
use siapi::IntApi;
use siapi::return_codes::*;
use siapi::event_types::*;
use siapi::report_types::*;
use siapi::cmd_types::*;
use ipcs::IpcEvent;
use exec::Exec;

use crate::streamer::Streamer;

const TRY_REGISTRATION_PERIOD: Duration = Duration::from_secs(30);
const MAIN_SLEEP: Duration = Duration::from_millis(1000);

fn hash_of_programs_hashes(hashes: &Vec<ProgramHashes>) -> u64 {
    let mut s = DefaultHasher::new();
    hashes.hash(&mut s);
    s.finish()
}

impl PartialEq for ModuleStatus {
    fn eq(&self, other: &Self) -> bool {
        if self.lstype != other.lstype {
            false
        } else
        if self.module != other.module {
            false
        } else {
            true
        }        
    }
}

fn find_status_module<'a>(modules: &'a Vec<ModuleStatus>, module: &str) -> Option<&'a ModuleStatus> {
    for m in modules {
        if m.module == module {
            return Some(m);
        }
    }
    None
}

impl PartialEq for Status {
    fn eq(&self, other: &Self) -> bool {
        if self.name != other.name {
            return false;
        }
        for m in &self.modules {
            match find_status_module(&other.modules, &m.module) {
                Some(om) => {
                    if m != om {
                        return false;
                    }
                },
                None => return false
            }
        }

        true
    }
}

fn is_cmd_for_program(cmd: u8) -> bool {
    match cmd {
        CMD_ASSET_UPDATE_FORCE => true,
        CMD_BUILD_UPDATE_FORCE => true,
        CMD_CONFIG_UPDATE_FORCE => true,
        CMD_RESTART_PROGRAM_HARD => true,
        CMD_RESTART_PROGRAM_SOFT => true,
        CMD_START_PROGRAM => true,
        CMD_STOP_PROGRAM_HARD => true,
        CMD_STOP_PROGRAM_SOFT => true,
        _ => false
    }
}

fn is_cmd_for_exec(cmd: u8) -> bool {
    match cmd {
        CMD_RESTART_PROGRAM_HARD => true,
        CMD_RESTART_PROGRAM_SOFT => true,
        CMD_START_PROGRAM => true,
        CMD_STOP_PROGRAM_HARD => true,
        CMD_STOP_PROGRAM_SOFT => true,
        _ => false
    }
}

fn indicate(config: &PointConfig) -> Result<(), String> {
    match config.indicate_program_id {
        Some(id) => {
            match config.find_program(id) {
                Some(p) => {
                    match ipcs::indicate(&p.name, &config.ipc_dir) {
                        Err(e) => Err(e),
                        _ => Ok(())
                    }
                },
                None => Err(format!("indicator program with id {} not found", id))
            }
        },
        None => {
            Err(format!("indicator program not set"))
        }
    }
}

fn register(cert: Cert) {
    let firm_name = match cert.firm_name {
        Some(name) => name.clone(),
        _ => String::from("")
    };
    let point_name = match cert.name {
        Some(name) => name,
        _ => mos::get_hostname()
    };
    let api = siapi::IntApi::new(cert.host, cert.data_port, cert.file_port, cert.stream_port, 0, 20);
    loop {
        println!("SEND REGISTER REQUEST TO '{}'", api.get_host());
        match api.register(point_name.clone(), firm_name.clone()) {
            Ok(reg_data) => {
                println!("REGISTRATION SUCCESS, WRITE CERT..");
                let new_cert = Cert {
                    host: api.get_host(),
                    data_port: cert.data_port,
                    file_port: cert.file_port,
                    stream_port: cert.stream_port,
                    id: Some(reg_data.id),
                    name: Some(reg_data.name),
                    firm_id: Some(reg_data.firm_id),
                    firm_name: Some(reg_data.firm_name),
                    pasw: Some(reg_data.pasw)};
                
                match mos::write_cert(&new_cert) {
                    Err(e) => {
                        println!("FAIL TO WRITE NEW CERT: {}", e);
                        return;
                    },
                    _ => {
                        println!("WRITE CERT SUCCESS, RESTART..");
                        return;
                    }
                }
            },
            Err(rc) => {
                if rc == RC_REG_PROCEED {
                    println!("REGISTRATION PROCEED");
                } else
                if rc == RC_REG_PROCEED_INDICATE {
                    println!("REGISTRATION PROCEED WITH INDICATE");
                    match mos::indicate() {
                        Err(e) => println!("fail to indicate: {}", e),
                        _ => ()
                    }
                } else {
                    println!("FAIL TO REGISTRATION REQUEST: {}", siapi::get_rc_name(rc));
                }
                std::thread::sleep(TRY_REGISTRATION_PERIOD);
            }
        }
    }
}

fn reset_cert(host: &str, data_port: u16, file_port: u16, stream_port: u16, point_name: Option<String>, firm_name: Option<String>) {
    let new_cert = Cert {
        host: String::from(host),
        data_port: data_port,
        file_port: file_port,
        stream_port: stream_port,
        name: point_name,
        firm_name: firm_name,
        firm_id: None,
        id: None,
        pasw: None
    };
    mos::write_cert(&new_cert).unwrap();
}

pub fn reboot(api: Arc<Mutex<IntApi>>) {
    drop(api.lock().unwrap());
    let mut cmd = Command::new("sudo");
    cmd.arg("reboot");
    cmd.status().unwrap();
    thread::sleep(Duration::from_secs(10));
}

fn perform_cmd(cmd: CmdAnsw, config: &PointConfig, api: Arc<Mutex<IntApi>>, execs: Vec<Arc<Mutex<Exec>>>, program_hashes: &mut Vec<ProgramHashes>) {
    let bin_path = config.bin_path.clone();
    let ipc_dir = config.ipc_dir.clone();
    let program = if is_cmd_for_program(cmd.cmd) {
        match cmd.program_id {
            Some(id) => {
                match config.find_program(id) {
                    Some(p) => Some(p),
                    None => {
                        api.lock().unwrap().send_report(Report {rtype: REPORT_INTERNAL_ERROR, program_id: cmd.program_id, cmd_id: Some(cmd.id), descr: Some(format!("PROGRAM NOT FOUND")), delay: 0}, true);
                        return;
                    }
                }
            },
            None => {
                api.lock().unwrap().send_report(Report {rtype: REPORT_INTERNAL_ERROR, program_id: cmd.program_id, cmd_id: Some(cmd.id), descr: Some(format!("PROGRAM_ID NOT SET")), delay: 0}, true);
                return;
            }
        }
    } else {
        None
    };

    let exec = match &program {
        Some(p) => {
            if is_cmd_for_exec(cmd.cmd) {
                if exec::is_exec_program(p.ptype) {
                    match exec::exec_find(&execs, p.id) {
                        Some(ex) => Some(ex),
                        None => {
                            api.lock().unwrap().send_report(Report {rtype: REPORT_INTERNAL_ERROR, program_id: cmd.program_id, cmd_id: Some(cmd.id), descr: Some(format!("PROGRAM NOT DOUND IN EXECS")), delay: 0}, true);
                            return;
                        }
                    }
                } else {
                    api.lock().unwrap().send_report(Report {rtype: REPORT_INTERNAL_ERROR, program_id: cmd.program_id, cmd_id: Some(cmd.id), descr: Some(format!("EXEC CMD FOR NON EXEC-TYPE PROGRAM")), delay: 0}, true);
                    return;
                }
            } else {
                None
            }
        },
        None => None
    };

    let res: Result<(), String> = match cmd.cmd {
        CMD_REBOOT_HARD => {
            reboot(api);
            Ok(())
        },
        CMD_REBOOT_SOFT => {
            match exec::stop_program_all(execs.clone(), api.clone(), Some(cmd.id), false, &ipc_dir) {
                Err(e) => Err(e),
                _ => {
                    reboot(api.clone());
                    Ok(())
                }
            }
        },
        CMD_BUILD_UPDATE_FORCE => {
            updater::update_program_build(exec, program.unwrap(), api, Some(cmd.id), &bin_path, &ipc_dir, program_hashes)
        },
        CMD_ASSET_UPDATE_FORCE => {
            updater::update_program_asset(exec, program.unwrap(), api, Some(cmd.id), &bin_path, &ipc_dir, program_hashes)
        },
        CMD_CONFIG_UPDATE_FORCE => {
            updater::update_program_config_all(exec, program.unwrap(), api, Some(cmd.id), &bin_path, &ipc_dir)
        },
        CMD_START_PROGRAM => {
            exec::start_program(exec.unwrap(), api, Some(cmd.id), &bin_path)
        },
        CMD_STOP_PROGRAM_HARD => {
            exec::stop_program(exec.unwrap(), api, Some(cmd.id), Some(false), false, &ipc_dir)
        },
        CMD_STOP_PROGRAM_SOFT => {
            exec::stop_program(exec.unwrap(), api, Some(cmd.id), Some(true), false, &ipc_dir)
        },
        CMD_RESTART_PROGRAM_HARD => {
            exec::restart_program(exec.unwrap(), api, Some(cmd.id), Some(false), &bin_path, &ipc_dir)
        },
        CMD_RESTART_PROGRAM_SOFT => {
            exec::restart_program(exec.unwrap(), api, Some(cmd.id), Some(true), &bin_path, &ipc_dir)
        },
        CMD_INDICATE => {
            match indicate(config) {
                Ok(()) => {
                    api.lock().unwrap().send_report(Report {rtype: REPORT_INTERNAL_ERROR, program_id: cmd.program_id, cmd_id: Some(cmd.id), descr: None, delay: 0}, true);
                    Ok(())
                },
                Err(e) => {
                    api.lock().unwrap().send_report(Report {rtype: REPORT_INTERNAL_ERROR, program_id: cmd.program_id, cmd_id: Some(cmd.id), descr: Some(e.clone()), delay: 0}, true);
                    Err(e)
                }
            }
        },
        _ => {
            api.lock().unwrap().send_report(Report {rtype: REPORT_INTERNAL_ERROR, program_id: cmd.program_id, cmd_id: Some(cmd.id), descr: Some(format!("UNKNOWN CMD")), delay: 0}, true);
            Err(format!("UNKNOWN CMD"))
        }
    };

    match res {
        Err(e) => println!("FAIL PERFORM CMD: {}", e),
        _ => {
            println!("PERFORM CMD CPLT");
        }
    }
}

fn get_and_handle_cmd(api: Arc<Mutex<IntApi>>, config: &mut PointConfig, execs: Vec<Arc<Mutex<Exec>>>, program_hashes: &mut Vec<ProgramHashes>) {
    let res = api.lock().unwrap().get_cmd();
    match res {
        Ok(cmd) => {
            perform_cmd(cmd, config, api, execs, program_hashes)
        },
        Err(e) => {
            println!("fail get cmd: {}", siapi::get_rc_name(e))
        }
    }
}

fn perform_poll(poll_res: Result<u8, u8>, api: Arc<Mutex<IntApi>>, mut config: PointConfig, mut execs: &mut Vec<Arc<Mutex<Exec>>>, cert: &Cert, program_hashes: &mut Vec<ProgramHashes>, streamer: &mut Streamer) -> Option<PointConfig> {
    match poll_res {
        Ok(event) => {
            // handle event
            println!("POLL: {}", siapi::get_event_name(event));
            match event {
                EVENT_NOTHING => (),
                EVENT_NEW_CMD => {
                    get_and_handle_cmd(api.clone(), &mut config, execs.clone(), program_hashes);
                },
                EVENT_NEW_STREAM_REQUEST => {
                    streamer.get_and_begin_stream(&execs, api.clone(), &config.bin_path);
                },
                EVENT_POINT_UPDATE_AVAILABLE => {
                    match updater::upload_point_config(api.clone()) {
                        Ok(mut config_new) => {
                            println!("POINT CONFIG LOAD SUCCESS");
                            match updater::sync_config_with_real(Some(execs.clone()), &mut config_new, api.clone(), None, Some(&config), program_hashes) {
                                Err(e) => {
                                    let msg = format!("fail to sync new config with real: {}", e);
                                    println!("{}", msg);
                                    api.lock().unwrap().send_report(Report {
                                        rtype: REPORT_INTERNAL_ERROR,
                                        program_id: None,
                                        cmd_id: None,
                                        descr: Some(msg),
                                        delay: 0
                                    }, false);
                                    exec::stop_program_all(execs.clone(), api.clone(), None, false, &config.ipc_dir).unwrap();
                                    return None;
                                },
                                _ => {
                                    config = config_new;
                                    exec::load_execs(&config, &mut execs);
                                }
                            }
                        },
                        Err(e) => println!("FAIL TO LOAD POINT CONFIG: {}:{}", e, siapi::get_rc_name(e))
                    }
                },
                EVENT_UPDATE_AVAILABLE => {

                    match updater::check_update_all(Some(execs.clone()), &mut config, api.clone(), program_hashes) {
                        Err(e) => println!("FAIL TO UPDATE: {}", e),
                        Ok(changes) => {
                            println!("UPDATE OK");
                            if changes {
                                println!("\tWRITE CONFIG");
                                mos::write_config(&config).unwrap();
                            }
                        }
                    }
                },
                _ => println!("UNKNOWN EVENT: {}", event)
            }
        },
        Err(e) => {
            println!("POLL FAIL: {}", siapi::get_rc_name(e));
            if e == RC_ACCESS_DENIED {
                println!("ERROR CAUSED BY UNREG ERROR, FOR THIS REASON CERT WILL BE RESET");
                reset_cert(&api.lock().unwrap().get_host(), cert.data_port, cert.file_port, cert.stream_port, cert.name.clone(), cert.firm_name.clone());
                exec::stop_program_all(execs.clone(), api.clone(), None, false, &config.ipc_dir).unwrap();
                return None;
            }
        }
    }

    Some(config)
}

fn main() -> Result<(), Error> {
    if utils::mos::is_manager_already_run() {
        return;
    }

    let mut world = World::default();

    // read cert
    let cert: Cert = match mos::read_cert() {
        Ok(r) => {
            println!("CERT SUCCESS READ");
            r
        },
        Err(e) => {
            print!("FAIL TO READ CERT: {}", e);
            return;
        }
    };

    // register point if already not
    if cert.id.is_none() {
        println!("CERT IS NOT REGISTERED, BEING REGISTRATION..");
        register(cert);
        return;
    }

    let api = Arc::new(Mutex::new(siapi::IntApi::new(
        cert.host.clone(),
        cert.data_port,
        cert.file_port,
        cert.stream_port,
        cert.id.unwrap(),
        20
    )));

    // send reboot report
    api.lock().unwrap().send_report(Report {rtype: REPORT_REBOOT, program_id: None, cmd_id: None, descr: None, delay: 0}, true);

    // get point config
    let mut config: PointConfig = match mos::read_config() {
        Ok(c) => {
            println!("POINT CONFIG SUCCESS READ");
            c
        },
        Err(e) => {
            println!("FAIL READ POINT CONFIG: {}", e);
            match updater::upload_point_config(api.clone()) {
                Ok(config) => {
                    println!("POINT CONFIG LOAD SUCCESS");
                    config
                }
                Err(e) => {
                    println!("FAIL TO LOAD POINT CONFIG: {}:{}", e, siapi::get_rc_name(e));
                    if e == RC_ACCESS_DENIED {
                        println!("ERROR CAUSED BY UNREG ERROR, FOR THIS REASON CERT WILL BE RESET");
                        reset_cert(&api.lock().unwrap().get_host(), cert.data_port, cert.file_port, cert.stream_port, cert.name, cert.firm_name);
                    }
                    return;
                }
            }
        }
    };

    // get programs hashes
    let mut program_hashes = mos::read_programs_hashes();
    let mut program_hashes_hash = hash_of_programs_hashes(&program_hashes);

    // create Exec map
    let mut execs = Vec::<Arc<Mutex<Exec>>>::new();
    exec::load_execs(&config, &mut execs);

    // create streamer
    let mut streamer = Streamer::new();

    // start ipc server and wait for end entries
    println!("START IPC SERVER AND WAIT FOR END ENTRIES..");
    let ipc_event = ipcs::run(api.clone(), &config.ipc_dir);
    let mut tl_ipc_event = Instant::now();
    let tstart = Instant::now();
    loop {
        match ipc_event.try_recv() {
            Ok(data) => {
                match data {
                    IpcEvent::SendStat => {
                        tl_ipc_event = Instant::now();
                    },
                    _ => ()
                }
            },
            _ => ()
        }

        if tl_ipc_event.elapsed() > Duration::from_secs(config.startup_wait_ipc as u64) {
            println!("IPC ENTRIES END - GO FURTHER");
            break;
        }
        if tstart.elapsed() > Duration::from_secs(config.startup_wait_ipc_timeout as u64) {
            println!("IPC ENTRIES NOT END, BUT TIMEOUT HAS COME - GO FURTHER");
            break;
        }
    }
    drop(ipc_event);
    
    exec::kill_program_all(&config);

    // poll
    println!("STARTUP HANDLING..");
    loop {
        let poll_res = api.lock().unwrap().poll();
        match poll_res {
            Ok(event) => {
                // if nothing - sync point config with exists files
                if event == EVENT_NOTHING {
                    println!("POLL WAS NOTHING - SYNC CONFIG WITH REAL AND UPDATE ALL");
                    match updater::sync_config_with_real(None, &mut config, api.clone(), None, None, &mut program_hashes) {
                        Ok(res) => {
                            println!("SYNC CONFIG WITH REAL OK: {:?} UPDATED", res);
                            match updater::check_update_all(None, &mut config, api.clone(), &mut program_hashes) {
                                Err(e) => println!("FAIL TO UPDATE ALL: {}", e),
                                _ => ()
                            }
                            break;
                        },
                        Err(e) => {
                            println!("FAIL TO SYNC: {}", e);
                            break;
                        }
                    }
                } else {
                    config = match perform_poll(Ok(event), api.clone(), config, &mut execs, &cert, &mut program_hashes, &mut streamer) {
                        Some(config) => config,
                        None => return
                    }
                }
            },
            Err(e) => {
                println!("FAIL TO POLL: {}\nSTARTUP HANDLER WILL BE SKIPPED", siapi::get_rc_name(e));
                break;
            }
        }

        thread::sleep(MAIN_SLEEP);
    }

    // main loop - poll remote server, perform command, handle Exec
    let mut tl_poll = Instant::now();
    let poll_period = Duration::from_secs(config.poll_period as u64);
    println!("POLL PER {} secs, begin MAIN LOOP", config.poll_period);
    loop {
        // periodic poll
        if tl_poll.elapsed() > poll_period {
            let poll_res = api.lock().unwrap().poll();
            config = match perform_poll(poll_res, api.clone(), config, &mut execs, &cert, &mut program_hashes, &mut streamer) {
                Some(config) => config,
                None => return
            };
            tl_poll = Instant::now();
        }

        exec::exec_handler(api.clone(), execs.clone(), &config.bin_path, &config.ipc_dir, &mut streamer);
        streamer.update_streams(&execs);

        let program_hashes_hash_now = hash_of_programs_hashes(&program_hashes);
        if program_hashes_hash_now != program_hashes_hash {
            mos::write_programs_hashes(&program_hashes).unwrap();
            program_hashes_hash = program_hashes_hash_now;
        }

        thread::sleep(MAIN_SLEEP);
    }
}
