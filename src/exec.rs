use std::process::{Child};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::mpsc::Sender;
use std::thread;
use std::thread::JoinHandle;
use std::time::Duration;
use std::process::Command;
use std::process::Stdio;
use std::time::Instant;

use crate::mos;
use crate::siapi;
use crate::ipcs;
use crate::streamer;
use siapi::Status;
use siapi::PointConfig;
use siapi::IntApi;
use siapi::Report;
use siapi::report_types::*;
use siapi::run_statuses::*;
use siapi::prog_types::*;
use streamer::Streamer;

const DEFAULT_UTILITY_MAX_RUN_TIME: Duration = Duration::from_secs(20);
const SOFT_TERMINATE_CHECK_PERIOD: Duration = Duration::from_millis(2000);
const SUFFICIENT_RUN_TIME_WITHOUT_CRASH: Duration = Duration::from_secs(20);
const STATUS_POLL_PERIOD: Duration = Duration::from_secs(5);

pub fn is_exec_program(ptype: u8) -> bool {
    match ptype {
        PROG_TYPE_EXEC => true,
        PROG_TYPE_UTILITY => true,
        PROG_TYPE_EXEC_BUILTIN => true,
        PROG_TYPE_UTILITY_BUILTIN => true,
        _ => false
    }
}

pub fn is_utility(ptype: u8) -> bool {
    match ptype {
        PROG_TYPE_UTILITY => true,
        PROG_TYPE_UTILITY_BUILTIN => true,
        _ => false
    }
}

pub fn is_builtin(ptype: u8) -> bool {
    match ptype {
        PROG_TYPE_EXEC_BUILTIN => true,
        PROG_TYPE_UTILITY_BUILTIN => true,
        _ => false
    }
}

pub fn exec_find(execs: &Vec<Arc<Mutex<Exec>>>, id: u32) -> Option<Arc<Mutex<Exec>>> {
    for ex in execs {
        if ex.lock().unwrap().program_id == id {
            return Some(ex.clone());
        }
    }
    None
}

pub struct Exec {
    pub program_id: u32,
    pub name: String,
    pub ptype: u8,
    pub run_status: u8,
    pub proc: Option<Child>,
    run_status_old: u8,
    tl_crash: Option<Instant>,
    tl_status_poll: Option<Instant>,
    crash_report: Option<String>,
    keep_run: bool,
    entry: String,
	args_after: Option<String>,
	args_before: Option<String>,
	use_ipc: bool,
    max_run_time: Option<Duration>,
    max_stopping_time: Option<Duration>,
    stream: Option<(JoinHandle<Child>, Sender<bool>)>,
    service_stop: bool, // if true means program will be started in exec_handler
    status_descr: Option<Status>
}

impl Exec {
    pub fn is_run(&mut self) -> bool {
        match &mut self.proc {
            Some(proc) => {
                match proc.try_wait() {
                    Ok(res) => {
                        match res {
                            Some(_) => false,
                            None => true
                        }
                    },
                    _ => false
                }
            },
            None => self.stream.is_some()
        }
    }

    pub fn spawn(&mut self, bin_path: &str) -> Result<(), String> {
        if self.is_run() {
            return Ok(());
        }
        // assert necessary fields
        if !self.is_builtin() && self.entry.is_empty() {
            return Err(format!("NON BUILTIN EXEC MUST HAVE ENTRY_PATH"));
        }
    
        // collect args
        let mut args = Vec::<String>::new();
        match self.args_before.clone() {
            Some(args_b) => {
                for a in args_b.split(" ") {
                    args.push(String::from(a));
                }
            },
            None => ()
        }
        if !self.entry.is_empty() {
            args.push(mos::format_entry_path(&self.name, &self.entry, bin_path));
        }
        match self.args_after.clone() {
            Some(args_a) => {
                for a in args_a.split(" ") {
                    args.push(String::from(a));
                }
            },
            None => ()
        }

        if args.len() < 1 {
            return Err(format!("NO ARGS FOR EXEC"));
        }
        // set args to cmd
        let mut cmd = Command::new(args.remove(0));
        for a in args {
            cmd.arg(a);
        }

        // set exec dir
        cmd.current_dir(mos::get_entry_dir(&self.name, &self.entry, bin_path));

        cmd.stdout(Stdio::piped());
        cmd.stdin(Stdio::piped());

        match cmd.spawn() {
            Ok(child) => {
                self.proc = Some(child);
                Ok(())
            },
            Err(e) => Err(e.to_string())
        }
    }

    pub fn is_utility(&self) -> bool {
        is_utility(self.ptype)
    }

    pub fn is_builtin(&self) -> bool {
        is_utility(self.ptype)
    }
}


pub fn start_program(exec: Arc<Mutex<Exec>>, api: Arc<Mutex<IntApi>>, cmd: Option<u32>, bin_path: &str) -> Result<(), String> {
    println!("START PROGRAM: {}", exec.lock().unwrap().name);
    let mut exec_unmux = exec.lock().unwrap();
    let res = if !exec_unmux.is_run() {
        match exec_unmux.spawn(bin_path) {
            Err(e) => Err(e),
            _ => Ok(None)
        }
    } else {
        Ok(Some(format!("ALREADY")))
    };

    match res {
        Err(e) => {
            println!("FAIL START PROGRAM: {}", e);
            if cmd.is_some() {
                api.lock().unwrap().send_report(Report {
                    rtype: REPORT_INTERNAL_ERROR,
                    program_id: Some(exec_unmux.program_id),
                    cmd_id: cmd,
                    descr: Some(e.clone()),
                    delay: 0
                }, true);
            }
            exec_unmux.crash_report = Some(e.clone());
            Err(e)
        },
        Ok(descr) => {
            println!("PROGRAM STARTED: {}", {match descr.clone() {
                Some(text) => text,
                None => String::from("OK")
            }});
            api.lock().unwrap().send_report(Report {
                rtype: REPORT_START_PROGRAM,
                program_id: Some(exec_unmux.program_id),
                cmd_id: cmd,
                descr: descr,
                delay: 0
            }, true);
            Ok(())
        }
    }
}

pub fn stop_program(exec: Arc<Mutex<Exec>>, api: Arc<Mutex<IntApi>>, cmd: Option<u32>, may_soft: Option<bool>, is_service_stop: bool, ipc_dir: &str) -> Result<(), String> {
    println!("STOP PROGRAM {}..", exec.lock().unwrap().name);
    let res: Result<Option<String>, String> = if exec.lock().unwrap().is_run() {
        let soft = match may_soft {
            Some(soft) => soft,
            None => exec.lock().unwrap().use_ipc
        };
        if soft {
            if exec.lock().unwrap().use_ipc {
                let start = Instant::now();
                exec.lock().unwrap().run_status = RUN_STATUS_STOPPING;
                let mut run_status_sended = false;
                loop {
                    let mut exec_unmux = exec.lock().unwrap();
                    match ipcs::soft_terminate(&exec_unmux.name, ipc_dir) {
                        Ok(state) => {
                            if state && !run_status_sended {
                                match api.lock().unwrap().send_run_status(exec_unmux.program_id, RUN_STATUS_STOPPING, None) {
                                    Ok(()) => {
                                        run_status_sended = true;
                                    },
                                    _ => ()
                                }
                            }
                        },
                        Err(_) => {
                            match exec_unmux.proc.take() {
                                Some(mut child) => {
                                    match child.kill() {
                                        _ => break Ok(None)
                                    }
                                },
                                _ => break Ok(None)
                            }
                        }
                    }
                    if exec_unmux.max_stopping_time.is_some() {
                        if start.elapsed() > exec_unmux.max_stopping_time.unwrap() {
                            break Err(format!("TIMEOUT STOP WAITING"));
                        }
                    }
                    drop(exec_unmux);
                    thread::sleep(SOFT_TERMINATE_CHECK_PERIOD);
                }
            } else {
                Err(format!("SOFT TERMINATE COULD NOT PERFORM FOR NON IPC PROGRAMS"))
            }
        } else {
            match exec.lock().unwrap().proc.take().unwrap().kill() {
                Err(e) => Err(e.to_string()),
                _ => Ok(None)
            }
        }
    } else {
        Ok(Some(format!("ALREADY")))
    };
    match res {
        Err(e) => {
            println!("FAIL STOP PROGRAM: {}", e);
            exec.lock().unwrap().run_status = RUN_STATUS_RUN;
            let must_have = cmd.is_some();
            api.lock().unwrap().send_report(Report {
                rtype: REPORT_INTERNAL_ERROR,
                program_id: Some(exec.lock().unwrap().program_id),
                cmd_id: cmd,
                descr: Some(e.clone()),
                delay: 0
            }, must_have);
            Err(e)
        },
        Ok(descr) => {
            println!("PROGRAM STOPPED: {}", {match descr.clone() {
                Some(text) => text,
                None => String::from("OK")
            }});
            exec.lock().unwrap().run_status = RUN_STATUS_STOPPED;
            exec.lock().unwrap().service_stop = is_service_stop;
            api.lock().unwrap().send_report(Report {
                rtype: REPORT_STOP_PROGRAM,
                program_id: Some(exec.lock().unwrap().program_id),
                cmd_id: cmd,
                descr: descr,
                delay: 0
            }, true);
            Ok(())
        }
    }
}

pub fn restart_program(exec: Arc<Mutex<Exec>>, api: Arc<Mutex<IntApi>>, cmd: Option<u32>, soft: Option<bool>, bin_path: &str, ipc_dir: &str) -> Result<(), String> {
    match stop_program(exec.clone(), api.clone(), cmd, soft, false, ipc_dir) {
        Err(e) => return Err(e),
        _ => ()
    }
    start_program(exec, api, cmd, bin_path)
}

pub fn stop_program_all(execs: Vec<Arc<Mutex<Exec>>>, api: Arc<Mutex<IntApi>>, cmd: Option<u32>, is_service_stop: bool, ipc_dir: &str) -> Result<(), String> {
    for ex in execs {
        match stop_program(ex, api.clone(), cmd, None, is_service_stop, ipc_dir) {
            Err(e) => return Err(e),
            _ => ()
        }
    }
    Ok(())
}

pub fn load_execs(config: &PointConfig, execs: &mut Vec<Arc<Mutex<Exec>>>) {
    println!("RELOAD EXECS..");
    let mut updated: usize = 0;
    let mut added: usize = 0;
    for p in &config.programs {
        if is_exec_program(p.ptype) {
            match exec_find(execs, p.id) {
                Some(ex) => {
                    // update fields
                    println!("UPDATE {} EXEC..", p.name);
                    updated += 1;
                    let mut ex_unmux = ex.lock().unwrap();
                    ex_unmux.args_after = p.args_after.clone();
                    ex_unmux.args_before = p.args_before.clone();
                    ex_unmux.entry = p.entry.clone();
                    ex_unmux.keep_run = p.keep_run;
                    ex_unmux.max_run_time = {
                        match p.max_run_time {
                            Some(secs) => Some(Duration::from_secs(secs as u64)),
                            None => Some(DEFAULT_UTILITY_MAX_RUN_TIME)
                        }
                    };
                    ex_unmux.max_stopping_time = {
                        match p.max_stopping_time {
                            Some(secs) => Some(Duration::from_secs(secs as u64)),
                            None => None
                        }
                    };
                    ex_unmux.name = p.name.clone();
                    ex_unmux.ptype = p.ptype;
                },
                None => {
                    // add with default
                    println!("ADD {} EXEC..", p.name);
                    added += 1;
                    execs.push(Arc::new(Mutex::new(Exec {
                        program_id: p.id,
                        ptype: p.ptype,
                        run_status: RUN_STATUS_STOPPED,
                        run_status_old: RUN_STATUS_STOPPED,
                        tl_crash: None,
                        crash_report: None,
                        proc: None,
                        keep_run: p.keep_run,
                        entry: p.entry.clone(),
                        args_after: p.args_after.clone(),
                        args_before: p.args_before.clone(),
                        use_ipc: p.use_ipc,
                        stream: None,
                        max_run_time: {
                            match p.max_run_time {
                                Some(secs) => Some(Duration::from_secs(secs as u64)),
                                None => Some(DEFAULT_UTILITY_MAX_RUN_TIME)
                            }
                        },
                        max_stopping_time: {
                            match p.max_stopping_time {
                                Some(secs) => Some(Duration::from_secs(secs as u64)),
                                None => None
                            }
                        },
                        name: p.name.clone(),
                        service_stop: true,
                        status_descr: None,
                        tl_status_poll: None
                    })));
                }
            }
        }
    }
    println!("RELOAD EXECS OK, UPDATED: {}, ADDED AS DEFAULT: {}", updated, added);
}

pub fn exec_handler(api: Arc<Mutex<IntApi>>, execs: Vec<Arc<Mutex<Exec>>>, bin_path: &str, ipc_dir: &str, streamer: &mut Streamer) {
    for exec in execs {
        let mut ex_unmux = exec.lock().unwrap();
        if !ex_unmux.is_utility() {
            if ex_unmux.run_status_old != ex_unmux.run_status {
                let mut crash_report: Option<String> = None;
                if ex_unmux.run_status == RUN_STATUS_CRASHING {
                    crash_report = ex_unmux.crash_report.take();
                }
                match api.lock().unwrap().send_run_status(ex_unmux.program_id, ex_unmux.run_status, crash_report) {
                    Err(e) => println!("fail to send run status: {}", siapi::get_rc_name(e)),
                    _ => {
                        ex_unmux.run_status_old = ex_unmux.run_status;
                    }
                }
            }
            if ex_unmux.run_status == RUN_STATUS_RUN {
                if ex_unmux.keep_run {
                    if ex_unmux.is_run() {
                        if ex_unmux.use_ipc {
                            let need_status_poll = match ex_unmux.tl_status_poll {
                                Some(tlp) => {
                                    tlp.elapsed() > STATUS_POLL_PERIOD
                                },
                                None => true
                            };
                            if need_status_poll {
                                match ipcs::get_status(&ex_unmux.name, ipc_dir) {
                                    Ok(status) => {
                                        ex_unmux.tl_status_poll = Some(Instant::now());
                                        let mut need_send = false;
                                        match ex_unmux.status_descr.take() {
                                            Some(old_status) => {
                                                if old_status != status {
                                                    println!("NEED SEND STATUS FOR {}: \n\tOLD: {:?}\n\tNEW: {:?}", ex_unmux.name, old_status, status);
                                                    need_send = true;
                                                } else {
                                                    ex_unmux.status_descr = Some(old_status);
                                                }
                                            },
                                            None => {
                                                need_send = true;
                                            }
                                        }
                                        if need_send {
                                            match api.lock().unwrap().send_status(&status) {
                                                Err(e) => {
                                                    println!("fail to send status: {}", e);
                                                },
                                                _ => {
                                                    println!("STATUS FOR {} SENDED: {:?}", ex_unmux.name, status);
                                                    ex_unmux.status_descr = Some(status);
                                                }
                                            }
                                        }
                                    },
                                    Err(e) => {
                                        println!("fail to poll status for program {}: {}", ex_unmux.name, e);
                                    }
                                }
                            }
                        }
                    } else {
                        ex_unmux.run_status = RUN_STATUS_CRASHING;
                        ex_unmux.tl_crash = Some(Instant::now());
                        ex_unmux.crash_report = Some(streamer.last_words(ex_unmux.program_id));
                    }
                }
            } else
            if ex_unmux.run_status == RUN_STATUS_CRASHING {
                if ex_unmux.is_run() {
                    // remove crash
                    match ex_unmux.tl_crash {
                        Some(tl_crash) => {
                            if tl_crash.elapsed() > SUFFICIENT_RUN_TIME_WITHOUT_CRASH {
                                ex_unmux.run_status = RUN_STATUS_RUN;
                                ex_unmux.crash_report = None;
                            }
                        },
                        None => {
                            ex_unmux.run_status = RUN_STATUS_RUN;
                            ex_unmux.crash_report = None;
                        }
                    }
                } else {
                    // try start
                    drop(ex_unmux);
                    match start_program(exec, api.clone(), None, bin_path) {
                        Err(e) => println!("fail start program: {}", e),
                        _ => ()
                    }
                }
            } else
            if ex_unmux.run_status == RUN_STATUS_STOPPED {
                if ex_unmux.keep_run && ex_unmux.service_stop {
                    ex_unmux.service_stop = false;
                    drop(ex_unmux);
                    match start_program(exec.clone(), api.clone(), None, bin_path) {
                        Err(e) => {
                            exec.lock().unwrap().run_status = RUN_STATUS_CRASHING;
                            println!("fail start program: {}", e)
                        },
                        _ => {
                            exec.lock().unwrap().run_status = RUN_STATUS_RUN;
                        }
                    }
                }
            }
        }
    }
}

pub fn kill_program_all(config: &PointConfig) {
    println!("KILL ALL PROGRAMS, SOFT IF POSSIBLE..");
    for p in &config.programs {
        if is_exec_program(p.ptype) {
            if p.use_ipc {
                println!("SOFT TERMINATE {}..", p.name);
                let start = Instant::now();
                loop {
                    match ipcs::soft_terminate(&p.name, &config.ipc_dir) {
                        Ok(_) => (),
                        _ => break
                    }
                    if p.max_stopping_time.is_some() {
                        if start.elapsed() > Duration::from_secs(p.max_stopping_time.unwrap() as u64) {
                            println!("PKILL TERMINATE {} BECAUSE SOFT TERMINATE TIMEOUT HAS COME", p.name);
                            mos::pkill_program(&p.name);
                            break;
                        }
                    }
                    thread::sleep(SOFT_TERMINATE_CHECK_PERIOD);
                }
            } else {
                println!("PKILL TERMINATE {} BECAUSE NOT USE IPC", p.name);
                mos::pkill_program(&p.name);
            }
        }
    }
}