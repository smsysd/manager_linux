/// Executable manager, main feature of manager, spawn possible executable entity(Exec).
/// Start execute of Exec, check the run status, stop Exec.

use bevy_ecs::prelude::*;
use std::io::Read;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::sync::mpsc::{Receiver, channel};
use std::thread;
use std::time::{Instant, Duration};
use std::{io::Error, process, process::Child};
use crate::configm::ConfigBase;
use crate::data_types;
use crate::data_types::data_server::{ProgramType, IpcType};
use crate::data_types::data_server::Report;
use crate::data_types::data_server::ReportType;
use crate::sendm::SendManager;
use crate::utils::ipc::Ipc;
use crate::utils::mos;
use data_types::data_server::GetPointConfigAnsw as PointConfig;

use crate::{stages, events};

const CLEAR_CNT_MAX: i32 = 5;
pub const TERMINATE_CHECK_PERIOD: Duration = Duration::from_millis(5000);
pub const TERMINATE_REQ_REPEAT_PERIOD: Duration = Duration::from_millis(5000);
const STDOUT_BUFSIZE: usize = 4096;


#[derive(Component, Debug)]
pub struct Exec {
    pub pid: i32,
    pub keep_run: bool,
    pub name: String,
    pub ipc_type: Option<IpcType>,
    pub entry: String,
    pub args_before: Option<String>,
    pub args_after: Option<String>,
    pub is_custom: bool
}

#[derive(Component)]
pub struct Run {
    pub child: Child,
    pub stdout: Option<Arc<Mutex<Receiver<Vec<u8>>>>>
}

#[derive(Component)]
pub struct Terminate {
    pub hard: bool,
    pub tl_req: Option<Instant>,
    pub clear_cnt: i32
}

pub mod utils {
    use bevy_ecs::prelude::*;
    use crate::events::TerminateRequest;

    use super::Exec;
    use super::Run;

    pub fn is_run(execs: &Query<(Entity, &Exec), With<Run>>, pid: i32) -> bool {
        for (_, ex) in execs {
            if ex.pid == pid {
                return true;
            }
        }
        false
    }

    pub fn terminate(execs: &Query<(Entity, &Exec), With<Run>>, evw: &mut EventWriter<TerminateRequest>, pid: i32, hard: bool) -> bool {
        evw.send(TerminateRequest {pid: pid, hard: hard});

        !is_run(execs, pid)
    }
}

fn terminate_adder(
    mut cmd: Commands,
    mut execs: Query<(Entity, &Exec, Option<&mut Terminate>)>,
    mut evr: EventReader<events::TerminateRequest>
) {
    for ev in evr.iter() {
        for (ex_e, ex, t) in &mut execs {
            if ev.pid == ex.pid {
                match t {
                    Some(mut t) => {
                        t.clear_cnt = 0;
                        if ev.hard {
                            t.hard = true;
                        }
                    },
                    None => {
                        println!("[EXECM] terminate program {}..", ex.name);
                        cmd.entity(ex_e).insert(Terminate {
                            hard: ev.hard,
                            clear_cnt: 0,
                            tl_req: None
                        });
                    }
                }
            }
        } 
    }
}

fn terminate_cleaner(mut cmd: Commands, mut execs: Query<(Entity, &Exec, &mut Terminate)>,) {
    for (ex_e, ex, mut t) in &mut execs {
        if t.clear_cnt >= CLEAR_CNT_MAX {
            println!("[EXECM] clean terminator for program {}", ex.name);
            cmd.entity(ex_e).remove::<Terminate>();
        } else {
            t.clear_cnt += 1;
        }
    }
}

fn terminator(mut execs: Query<(&Exec, &mut Terminate), With<Run>>, ipc: Res<Ipc>) {
    for (ex, mut t) in &mut execs {
        match &ex.ipc_type {
            Some(ipc_type) => {
                let req_allow = match &t.tl_req {
                    Some(val) => val.elapsed() >= TERMINATE_REQ_REPEAT_PERIOD,
                    None => true
                };
                if req_allow {
                    match ipc.terminate(&ex.name, t.hard, ipc_type) {
                        Err(e) => println!("[EXECM] fail to send terminate req for program {}: {:?}", ex.name, e),
                        Ok(()) => {
                            // println!("[EXECM] {} terminate request sended to program {}", if t.hard {"hard"} else {"soft"}, ex.name);
                        }
                    }
                    t.tl_req = Some(Instant::now());
                }
            },
            None => {
                let req_allow = match &t.tl_req {
                    Some(val) => val.elapsed() >= TERMINATE_CHECK_PERIOD,
                    None => true
                };
                if req_allow {
                    mos::pkill(&ex.name);
                    t.tl_req = Some(Instant::now());
                }
            }
        }
    }
}

fn run_checker(mut cmd: Commands, mut execs: Query<(Entity, &Exec, &mut Run)>, mut sm: ResMut<SendManager>) {
    for (ex_e, ex, mut rund) in &mut execs {
        if !is_child_run(&mut rund.child) {
            println!("[EXECM] program {} stopped", ex.name);
            cmd.entity(ex_e).remove::<Run>();
            sm.report(Report {delay: 0, rtype: ReportType::StopProgram, program_id: Some(ex.pid), descr: None});
        }
    }
}

fn runner(
    mut cmd: Commands,
    execs: Query<(Entity, &Exec), (Without<Run>, Without<Terminate>)>,
    config: Res<ConfigBase>,
    mut evr: EventReader<events::RunRequest>,
    mut sm: ResMut<SendManager>
) {
    let mut run_ex = None;
    'main_for: for (ex_e, ex) in &execs {
        if ex.keep_run {
            run_ex = Some((ex_e, ex));
            break;
        }
        for ev in evr.iter() {
            if ev.0 == ex.pid {
                run_ex = Some((ex_e, ex));
                break 'main_for;
            }
        }
    }

    match run_ex {
        Some((ex_e, ex)) => {
            match run(ex, &config.bin_path) {
                Some(r) => {
                    println!("[EXECM] start program {}", ex.name);
                    cmd.entity(ex_e).insert(r);
                    sm.report(Report {delay: 0, rtype: ReportType::StartProgram, program_id: Some(ex.pid), descr: None});
                },
                None => println!("[EXECM] fail to start program {}", ex.name)
            }
        },
        None => ()
    }
}

fn indicator() {

}

fn startup(mut cmd: Commands, config: Res<PointConfig>) {
    println!("[EXECM] startup..");
    // TODO: terminate previous session, if exists
    for p in &config.programs {
        let ex = Exec {
            pid: p.id,
            name: p.name.clone(),
            keep_run: p.keep_run,
            ipc_type: p.ipc_type.clone(),
            entry: p.entry.clone(),
            args_after: p.args_after.clone(),
            args_before: p.args_before.clone(),
            is_custom: p.ptype.is_custom()
        };
        println!("\t[EXECM] new: {:?}", ex);
        cmd.spawn(ex);
    }
}

fn is_child_run(child: &mut Child) -> bool {
    match child.try_wait() {
        Ok(res) => match res {
            Some(_) => false,
            None => true
        },
        _ => false
    }
}

fn run(exec: &Exec, bin_path: &str) -> Option<Run> {
    // collect args
    let mut args = Vec::<String>::new();
    match exec.args_before.clone() {
        Some(args_b) => {
            for a in args_b.split(" ") {
                args.push(String::from(a));
            }
        },
        None => ()
    }
    if exec.is_custom {
        args.push(mos::format_entry_path(&exec.name, &exec.entry, bin_path));
    } else {
        args.push(exec.entry.clone());
    }
    match exec.args_after.clone() {
        Some(args_a) => {
            for a in args_a.split(" ") {
                args.push(String::from(a));
            }
        },
        None => ()
    }

    // set args to cmd
    let mut cmd =  process::Command::new(args.remove(0));
    for a in args {
        cmd.arg(a);
    }

    // set exec dir
    if exec.is_custom {
        cmd.current_dir(mos::format_entry_dir(&exec.name, &exec.entry, bin_path));
    }

    cmd.stdout(Stdio::piped());
    cmd.stdin(Stdio::piped());

    match cmd.spawn() {
        Ok(mut child) => {
            let mut stdout = child.stdout.take();
            let (tx, rx) = channel::<Vec<u8>>();
            thread::spawn(move || {
                let mut buf: [u8;STDOUT_BUFSIZE] = [0;STDOUT_BUFSIZE];
                loop {
                    match &mut stdout {
                        Some(stdout) => {
                            match stdout.read(&mut buf) {
                                Ok(len) => {
                                    if len == 0 {
                                        break;
                                    }
                                    match tx.send(buf[..len].to_vec()) {
                                        Err(_) => break,
                                        _ => ()
                                    }
                                },
                                Err(_) => break
                            }
                        },
                        None => break
                    }
                }
                stdout
            });
            Some(Run {
                child: child,
                stdout: Some(Arc::new(Mutex::new(rx)))
            })
        },
        Err(_) => None
    }
}

pub fn init(_world: &mut World, schedule: &mut Schedule) -> Result<(), Error> {
    schedule.add_system_to_stage(stages::Startup::InitExecManager, startup);
	schedule.add_system_to_stage(stages::Core::Main, terminate_adder);
    schedule.add_system_to_stage(stages::Core::Main, terminate_cleaner);
    schedule.add_system_to_stage(stages::Core::Main, terminator);
    schedule.add_system_to_stage(stages::Core::Main, run_checker);
    schedule.add_system_to_stage(stages::Core::Main, runner);
    schedule.add_system_to_stage(stages::Core::Main, indicator);
    Ok(())
}