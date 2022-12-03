/// Send manager, handle SendData - send data to server, save SendData on disk if needed.

use bevy_ecs::prelude::*;
use chrono::prelude::*;
use serde::{Deserialize, Serialize};
use std::{io::Error, time::{Instant, Duration}};

use crate::{utils::{mos}, data_types::{data_server::{Report, Stat, Log}, AppState}, srvm::Server, stages};

pub const MAX_SEND_QUEUE: usize = 25;
pub const DISK_CHECK_PERIOD: Duration = Duration::from_secs(10);
pub const TRY_SEND_PERIOD: Duration = Duration::from_millis(2000);

#[derive(Deserialize, Serialize)]
pub enum SendDataType {
	Report(Report),
	Stat(Stat),
	Log(Log)
}

impl SendDataType {
    pub fn is_necessary(&self) -> bool {
        match self {
            Self::Report(_) => true,
            Self::Stat(_) => true,
            Self::Log(_) => false,
        }
    }
}

pub struct SendData {
	dt: DateTime<Utc>,
	dtype: SendDataType
}

#[derive(Resource)]
pub struct SendManager {
    pub queue: Vec<SendData>,
    tl_disk_check: Instant,
    tl_try_send: Instant
}

impl Default for SendManager {
    fn default() -> Self {
        Self {
            queue: Vec::new(),
            tl_disk_check: Instant::now(),
            tl_try_send: Instant::now()
        }
    }
}

impl SendManager {
    pub fn report(&mut self, val: Report) {
        self.queue.push(SendData {
            dt: Utc::now(),
            dtype: SendDataType::Report(val)
        })
    }

    pub fn stat(&mut self, val: Stat) {
        self.queue.push(SendData {
            dt: Utc::now(),
            dtype: SendDataType::Stat(val)
        })
    }

    pub fn log(&mut self, val: Log) {
        self.queue.push(SendData {
            dt: Utc::now(),
            dtype: SendDataType::Log(val)
        })
    }
}

fn elapsed(dt: &DateTime<Utc>) -> i64 {
    Utc::now().timestamp_millis() - dt.timestamp_millis() 
}

fn sys_send_manager(server: Res<Server>, mut sm: ResMut<SendManager>) {
    if sm.tl_try_send.elapsed() < TRY_SEND_PERIOD {
        return;
    }
    if sm.queue.len() > 0 {
        let first = &mut sm.queue[0];
        let res = match first.dtype {
            SendDataType::Report(ref mut r) => {
                r.delay = elapsed(&first.dt);
                server.api.send_report(r.clone())
            },
            SendDataType::Stat(ref mut s) => {
                s.delay = elapsed(&first.dt);
                server.api.send_stat(s.clone())
            },
            SendDataType::Log(ref mut l) => {
                l.delay = elapsed(&first.dt);
                match server.api.send_log(l.clone()) {_ => ()}
                Ok(())
            }
        };

        match res {
            Ok(()) => {
                sm.queue.remove(0);
            },
            Err(_) => sm.tl_try_send = Instant::now()
        }
    }
}

fn sys_disk_manager(mut sm: ResMut<SendManager>, st: Res<AppState>) {
    if st.is_terminate() {
        match sm.queue.pop() {
            Some(val) => {
                if val.dtype.is_necessary() {
                    mos::temp_send_data_push(val.dt.timestamp_millis(), &rmp_serde::to_vec(&val.dtype).unwrap()).unwrap();
                }
            },
            None => ()
        }
        return;
    }
    if sm.tl_disk_check.elapsed() < DISK_CHECK_PERIOD {
        return;
    }
    sm.tl_disk_check = Instant::now();
    if sm.queue.len() == 0 {
        match mos::temp_send_data_pop().unwrap() {
            Some((dt, data)) => {
                sm.queue.push(SendData {
                    dt: Utc.timestamp_millis_opt(dt).unwrap(),
                    dtype: match rmp_serde::from_slice(&data) {
                        Ok(data) => data,
                        _ => return
                    }
                })
            },
            None => ()
        }
    }
}

fn startup(mut cmd: Commands) {
    println!("[SENDM] startup..");
    cmd.insert_resource(SendManager::default());
}

pub fn init(_world: &mut World, schedule: &mut Schedule) -> Result<(), Error> {
    schedule.add_system_to_stage(stages::Startup::InitSendManager, startup);
    schedule.add_system_to_stage(stages::Core::HandlePollEvents, sys_send_manager);
    schedule.add_system_to_stage(stages::Core::Save, sys_disk_manager);
	Ok(())
}