/// Command manager, handle SrvCmd entities - perform command.
/// Rreceive corresponding PollEvent(Cmd).

use bevy_ecs::prelude::*;
use std::io::Error;

pub fn init(world: &mut World, schedule: &mut Schedule) -> Result<(), Error> {
	Ok(())
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