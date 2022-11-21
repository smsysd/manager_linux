use std::thread::{self, JoinHandle};
use std::io::{prelude::*};
use std::net::TcpStream;
use std::time::{Duration, Instant};
use chrono::{Utc};

use std::sync::{Arc, Mutex};
use std::sync::mpsc::{channel, Receiver, Sender};

use pbr::ProgressBar;
use rmp_serde as rmps;

// - - - - - - - API DATA TYPES - - - - - - - - //

#[derive(Serialize, Deserialize, Debug)]
struct GenReq {
    req: u8,
    id: u32,
    body: Vec<u8>
}

#[derive(Serialize, Deserialize, Debug)]
struct GenAnsw {
    rc: u8,
    event: u8,
    body: Vec<u8>
}


// - - - - - - - UPDATE DATA - - - - - - - - - //
#[derive(Serialize, Deserialize, Debug)]
pub struct ConfigUpdateDataAnsw {
    pub id: u32,
    pub need_update: bool
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ProgramUpdateDataAnsw {
    pub id: u32,
    pub need_program_update: bool,
    pub need_asset_update: bool,
    pub configs: Vec<ConfigUpdateDataAnsw>
}

#[derive(Serialize, Deserialize, Debug)]
pub struct UpdateDataAnsw {
    pub programs: Vec<ProgramUpdateDataAnsw>
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ConfigUpdateDataReq {
    pub id: u32,
    pub hash: Vec<u8>
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ProgramUpdateDataReq {
    pub id: u32,
    pub program_hash: Vec<u8>,
    pub asset_hash: Option<Vec<u8>>,
    pub configs: Vec<ConfigUpdateDataReq>
}

#[derive(Serialize, Deserialize, Debug)]
pub struct UpdateDataReq {
    pub programs: Vec<ProgramUpdateDataReq>
}

// - - - - - - - POINT CONFIG - - - - - - - - //
#[derive(Serialize, Deserialize, Debug)]
pub struct ProgramConfig {
	pub id: u32,
	pub name: String,
	pub path: String
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Program {
    pub id: u32,
    pub name: String,
    pub ptype: u8,
	pub autoupdate: bool,
	pub autoupdate_config: bool,
    pub autoupdate_asset: bool,
	pub keep_run: bool,
	pub entry: String,
	pub args_after: Option<String>,
	pub args_before: Option<String>,
    pub use_ipc: bool,
	pub configs: Vec<ProgramConfig>,
	pub asset_id: Option<u32>,
	pub asset_name: Option<String>,
    pub max_run_time: Option<u32>,
    pub max_stopping_time: Option<u32>
}

#[derive(Serialize, Deserialize, Debug)]
pub struct PointConfig {
    pub poll_period: u32,
    pub indicate_program_id: Option<u32>,
    pub startup_wait_ipc: u32,
	pub startup_wait_ipc_timeout: u32,
    pub bin_path: String,
    pub ipc_dir: String,
    pub programs: Vec<Program>
}

// - - - - - - - PROGRAM CONFIG - - - - - - - //
#[derive(Serialize, Deserialize, Debug)]
struct ProgramConfigReq {
    config_id: u32
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ProgramConfigAnsw {
    pub hash: Vec<u8>,
    pub data: Vec<u8>
}

// - - - - - - - REGISTER - - - - - - - - - - //
#[derive(Serialize, Deserialize, Debug)]
struct RegisterReq {
    name: String,
    firm: String
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RegisterAnsw {
    pub id: u32,
    pub name: String,
    pub firm_id: u32,
    pub firm_name: String,
    pub pasw: String
}


// - - - - - - - - REPORT LOG STAT STATUS - - - - - - - - //

#[derive(Serialize, Deserialize, Debug)]
pub struct Report {
    pub delay: u32,
	pub rtype: u8,
	pub program_id: Option<u32>,
    pub cmd_id: Option<u32>,
	pub descr: Option<String>
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ModuleStatus {
	pub lstype: u8,
	pub module: String,
	pub descr: String	
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Log {
	pub name: String,
    pub module: ModuleStatus
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Stat {
	pub delay: u32,
	pub name: String,
	pub data: Vec<u8>
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Status {
	pub name: String,
    pub modules: Vec<ModuleStatus>
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RunStatus {
	pub program_id: u32,
    pub run_status: u8,
    pub crash_report: Option<String>
}

// - - - - - - - - OTHER - - - - - - - - //
#[derive(Serialize, Deserialize, Debug)]
pub struct CmdAnsw {
    pub id: u32,
	pub cmd: u8,
	pub program_id: Option<u32>
}

#[derive(Serialize, Deserialize, Debug)]
pub struct StreamAnsw {
	pub stream_id: u32,
	pub point_program_id: u32
}

// it sync with stream_api, not with data_api
#[derive(Serialize, Deserialize, Debug)]
struct StreamConnectReq {
	stream_id: u32,
	initiator: bool
}

#[derive(Serialize, Deserialize, Debug)]
enum SendData {
    Report(Report),
    Stat(Stat)
} 

#[derive(Serialize, Deserialize, Debug)]
struct DelayedSend {
    instatnt: i64,  // timestamp millis
    must_have: bool,
    data: SendData
}


// - - - - - - - FILE SERVER TYPES - - - - - - - - //
#[derive(Serialize, Deserialize, Debug)]
struct FileReq {
	point_id: u32,
	point_program_id: u32,
	res_type: u8
}

#[derive(Serialize, Deserialize, Debug)]
struct FileAnsw {
	rc: u8,
	hash: Vec<u8>,
    fsize: u32
}

// - - - - - - - INTERNAL SETTINGS - - - - - - - - //

// const DATA_PORT_BUFFER_SIZE: usize = 65536;


// - - - - - - - API ENUM TYPES - - - - - - - - -  //

const REQ_POLL: u8 = 1;
const REQ_GET_UPDATE_DATA: u8 = 2;
const REQ_GET_POINT_CONFIG: u8 = 3;
const REQ_GET_PROGRAM_CONFIG: u8 = 4;
const REQ_ADD_LOG: u8 = 5;
const REQ_ADD_STAT: u8 = 6;
const REQ_ADD_REPORT: u8 = 7;
const REQ_REGISTER: u8 = 8;
const REQ_GET_CMD: u8 = 9;
const REQ_GET_STREAM_REQUEST: u8 = 10;
const REQ_SET_STATUS: u8 = 11;
const REQ_SET_RUN_STATUS: u8 = 12;

pub mod return_codes {
    pub const RC_OK: u8 = 0;
    pub const RC_ACCESS_DENIED: u8 = 1;
    pub const RC_REG_PROCEED: u8 = 3;
    pub const RC_REG_PROCEED_INDICATE: u8 = 4;
    pub const RC_NOT_FOUND: u8 = 5;
    pub const RC_POINT_CONFIG_UNSYNC: u8 = 6;
    pub const RC_NET_ERROR: u8 = 64;
    pub const RC_INC_REQ_DATA: u8 = 65;
    pub const RC_INC_ANSW_DATA: u8 = 66;
    pub const RC_INTEGRITY_ERROR: u8 = 67;
    pub const RC_FS_ERROR: u8 = 68;
    pub const RC_INTERNAL: u8 = 127;
}

use return_codes::*;

pub mod event_types {
    pub const EVENT_NOTHING: u8 = 0;
    pub const EVENT_NEW_CMD: u8 = 1;
    pub const EVENT_POINT_UPDATE_AVAILABLE: u8 = 2;
    pub const EVENT_UPDATE_AVAILABLE: u8 = 3;
    pub const EVENT_NEW_STREAM_REQUEST: u8 = 4;
}

use event_types::*;

pub mod prog_types {
    pub const PROG_TYPE_EXEC: u8 = 1;
    pub const PROG_TYPE_UTILITY: u8 = 2;
    pub const PROG_TYPE_EXEC_BUILTIN: u8 = 3;
    pub const PROG_TYPE_UTILITY_BUILTIN: u8 = 4;
    pub const PROG_TYPE_LIB: u8 = 5;
    pub const PROG_TYPE_FIRMWARE: u8 = 6;
}

pub mod run_statuses {
    pub const RUN_STATUS_STOPPED: u8 = 0;
    pub const RUN_STATUS_RUN: u8 = 1;
    pub const RUN_STATUS_STOPPING: u8 = 2;
    pub const RUN_STATUS_CRASHING: u8 = 3;
}

pub mod report_types {
    pub const REPORT_REBOOT: u8 = 1;
    pub const REPORT_SELFUPDATE: u8 = 2;
    pub const REPORT_BUILD_UPDATE: u8 = 3;
    pub const REPORT_CONFIG_UPDATE: u8 = 4;
    pub const REPORT_ASSET_UPDATE: u8 = 5;
    pub const REPORT_STOP_PROGRAM: u8 = 7;
    pub const REPORT_START_PROGRAM: u8 = 8;
    pub const REPORT_POINT_CONFIG_UPDATE: u8 = 9;
    pub const REPORT_INTERNAL_ERROR: u8 = 20;
    pub const REPORT_ERROR: u8 = 21;
}

pub mod cmd_types {
    pub const CMD_SELFUPDATE: u8 = 2;
    pub const CMD_SELFUPDATE_FORCE: u8 = 3;
    pub const CMD_BUILD_UPDATE_FORCE: u8 = 5;
    pub const CMD_ASSET_UPDATE_FORCE: u8 = 7;
    pub const CMD_CONFIG_UPDATE_FORCE: u8 = 9;
    pub const CMD_START_PROGRAM: u8 = 20;
    pub const CMD_STOP_PROGRAM_SOFT: u8 = 21;
    pub const CMD_STOP_PROGRAM_HARD: u8 = 22;
    pub const CMD_RESTART_PROGRAM_SOFT: u8 = 23;
    pub const CMD_RESTART_PROGRAM_HARD: u8 = 24;
    pub const CMD_REBOOT_SOFT: u8 = 25;
    pub const CMD_REBOOT_HARD: u8 = 26;
    pub const CMD_INDICATE: u8 = 40;
}

const RES_TYPE_BUILD: u8 = 1;
const RES_TYPE_ASSET: u8 = 2;

const FILE_BUF_SIZE: usize = 4096;
const SEND_MANAGER_SLEEP: Duration = Duration::from_millis(10);
const SEND_MANAGER_ERROR_SLEEP: Duration = Duration::from_millis(1000);
const FIRST_DATA_DELAY: Duration = Duration::from_millis(1000);
const READ_TIMEOUT: Duration = Duration::from_millis(5000);
const WRITE_TIMEOUT: Duration = Duration::from_millis(5000);
const WAIT_SEND_MANAGER_TIMEOUT: Duration = Duration::from_millis(2500);

pub fn get_rc_name(rc: u8) -> String {
    match rc {
        RC_OK => format!("OK({})", rc),
        RC_ACCESS_DENIED => format!("ACCESS_DENIED({})", rc),
        RC_INTERNAL => format!("INTERNAL({})", rc),
        RC_NET_ERROR => format!("NET_ERROR({})", rc),
        RC_INC_REQ_DATA => format!("INC_REQ_DATA({})", rc),
        RC_INC_ANSW_DATA => format!("INC_ANSW_DATA({})", rc),
        RC_FS_ERROR => format!("FS_ERROR({})", rc),
        RC_NOT_FOUND => format!("NOT_FOUND({})", rc),
        RC_INTEGRITY_ERROR => format!("INTEGRITY_ERROR({})", rc),
        RC_POINT_CONFIG_UNSYNC => format!("POINT_CONFIG_UNSYNC({})", rc),
        _ => format!("UNKNOWN({})", rc)
    }
}

pub fn get_event_name(event: u8) -> String {
    match event {
        EVENT_NOTHING => format!("NOTHING({})", event),
        EVENT_NEW_CMD => format!("NEW_CMD({})", event),
        EVENT_UPDATE_AVAILABLE => format!("UPDATE_AVAILABLE({})", event),
        EVENT_POINT_UPDATE_AVAILABLE => format!("POINT_UPDATE_AVAILABLE({})", event),
        EVENT_NEW_STREAM_REQUEST => format!("NEW_STREAM_REQUEST({})", event),
        _ => format!("UNKNOWN({})", event)
    }
}

fn tcp_request(host: &str, data_port: u16, data: &Vec<u8>) -> Result<Vec<u8>, u8> {
    let mut stream = match TcpStream::connect(format!("{}:{}", host, data_port)) {
        Ok(s) => s,
        Err(_) => {
            return Err(RC_NET_ERROR);
        }
    };

    stream.set_read_timeout(Some(READ_TIMEOUT)).unwrap();
    stream.set_write_timeout(Some(WRITE_TIMEOUT)).unwrap();

    match stream.write_all(data) {
        Ok(()) => (),
        Err(e) => {
            println!("fail to perform tcp_request: fail send request: {}", e.to_string());
            return Err(RC_NET_ERROR)
        }
    }

    let mut buf = Vec::<u8>::new();
    match stream.read_to_end(&mut buf) {
        Ok(_) => Ok(buf),
        Err(e) => {
            println!("fail to perform tcp_request: fail receive answer: {}", e.to_string());
            return Err(RC_NET_ERROR)
        }
    }
}

fn wrap_aser_req(id: u32, req: u8, body: &Vec<u8>) -> Result<Vec<u8>, ()> {
    let wrap_req = GenReq {id: id, req: req, body: body.clone()};
    match rmps::encode::to_vec(&wrap_req) {
        Ok(data) => Ok(data),
        _ => Err(())
    }
}

fn perform_request(url: &str, data_port: u16, id: u32, req: u8, body: &Vec<u8>) -> Result<GenAnsw, u8> {
    let req_data = match wrap_aser_req(id, req, body) {
        Ok(data) => data,
        _ => return Err(RC_INC_REQ_DATA)
    };

    let answ_data = match tcp_request(url, data_port, &req_data) {
        Ok(data) => data,
        Err(e) => return Err(e)
    };

    let answ_gen: GenAnsw = match rmps::decode::from_slice(&answ_data[..]) {
        Ok(data) => data,
        _ => return Err(RC_INC_ANSW_DATA)
    };

    if answ_gen.rc != RC_OK {
        Err(answ_gen.rc)
    } else {
        Ok(answ_gen)
    }
}

fn download_file(url: &str, file_port: u16, req: FileReq, temp_file_name: &str) -> Result<FileAnsw, u8> {
    if req.res_type == RES_TYPE_BUILD {
        println!("DOWNLOAD BUILD FILE as {}", temp_file_name);
    } else
    if req.res_type == RES_TYPE_ASSET {
        println!("DOWNLOAD ASSET FILE as {}", temp_file_name);
    } else {
        panic!("INCORRECT RES_TYPE");
    }
    let req_raw = match rmps::encode::to_vec(&req) {
        Ok(data) => data,
        _ => return Err(RC_INC_REQ_DATA)
    };

    println!("\t CONNECT TO SERVER..");
    let mut stream = match TcpStream::connect(format!("{}:{}", url, file_port)) {
        Ok(s) => s,
        _ => return Err(RC_NET_ERROR)
    };

    println!("\t SEND REQUEST..");
    match stream.write_all(&req_raw) {
        Err(_) => return Err(RC_NET_ERROR),
        _ => ()
    }

    println!("\t RECEIVE ANSWER..");
    let mut buf: [u8;256] = [0;256];
    stream.set_read_timeout(Some(FIRST_DATA_DELAY)).unwrap();
    let answ: FileAnsw = match stream.read(&mut buf) {
        Ok(len) => {
            match rmps::decode::from_slice(&buf[..len]) {
                Ok(answ) => answ,
                Err(e) => {
                    println!("\t FAIL DESEREALIZE ANSW({}): {}", len, e.to_string());
                    return Err(RC_INC_ANSW_DATA);
                }
            }
        },
        _ => return Err(RC_NET_ERROR)
    };

    if answ.rc != RC_OK {
        return Err(answ.rc);
    }

    stream.set_read_timeout(Some(READ_TIMEOUT)).unwrap();
    let mut file_buf: [u8;FILE_BUF_SIZE] = [0;FILE_BUF_SIZE];
    let mut file = match super::mos::create_temp_arch(&temp_file_name) {
        Ok(f) => f,
        Err(e) => {
            println!("\t FAIL CREATE TEMP ARCH: {}", e);
            return Err(RC_FS_ERROR);
        }
    };

    println!("\t DOWNLOAD FILE..");
    let mut pb = ProgressBar::new(answ.fsize as u64);
    loop {
        match stream.read(&mut file_buf) {
            Ok(len) => {
                if len == 0 {
                    break;
                }
                match file.write_all(&file_buf[..len]) {
                    Err(_) => return Err(RC_FS_ERROR),
                    _ => {
                        pb.add(len as u64);
                    }
                }
            },
            _ => break
        }
    }
    pb.finish_println("\tFILE DOWNLOADED, CHECK HASH..");

    let hash = super::mos::hash_file(&mut file);
    if hash == answ.hash {
        println!("\t HASH OK");
        Ok(answ)
    } else {
        println!("\t HASH IS DIFF");
        Err(RC_INTEGRITY_ERROR)
    }
}

pub fn send_report(url: &str, data_port: u16, id: u32, report: &Report) -> Result<(), u8> {
    let body = match rmps::encode::to_vec(&report) {
        Ok(data) => data,
        _ => return Err(RC_INC_REQ_DATA)
    };

    match perform_request(url, data_port, id, REQ_ADD_REPORT, &body) {
        Ok(_) => Ok(()),
        Err(rc) => return Err(rc)
    }
}

pub fn send_stat(url: &str, data_port: u16, id: u32, stat: &Stat) -> Result<(), u8> {
    let body = match rmps::encode::to_vec(&stat) {
        Ok(data) => data,
        _ => return Err(RC_INC_REQ_DATA)
    };

    match perform_request(url, data_port, id, REQ_ADD_STAT, &body) {
        Ok(_) => Ok(()),
        Err(rc) => return Err(rc)
    }
}

fn send_manager(rx: Receiver<Option<DelayedSend>>, send_queue: Arc<Mutex<usize>>, max_queue_len: usize, url: String, data_port: u16, id: u32) {
    let mut queue = Vec::<DelayedSend>::new();
    let mut cash_exists = true;
    loop {
        match rx.try_recv() {
            Ok(rx_data) => {
                match rx_data {
                    Some(data) => {
                        if queue.len() >= max_queue_len {
                            if data.must_have {
                                let raw_sd = rmps::encode::to_vec(&data).unwrap();
                                super::mos::temp_send_data_push(data.instatnt, &raw_sd);
                                cash_exists = true;
                                println!("[SIAPI][SEND_MANAGER] push send data to cash");
                            }
                        } else {
                            *send_queue.lock().unwrap() = queue.len() + 1;
                            println!("[SEND_MANAGER] add new element, total in buf: {}", queue.len()+1);
                            // println!("[SEND_MANGER] add new element, must_have: {}", data.must_have);
                            queue.push(data);
                        }
                    },
                    None => {
                        for sd in &queue {
                            if sd.must_have {
                                let raw_sd = rmps::encode::to_vec(sd).unwrap();
                                super::mos::temp_send_data_push(sd.instatnt, &raw_sd);
                                println!("[SIAPI][SEND_MANAGER] push send data to cash");
                            }
                        }
                        queue.clear();
                        return;
                    }
                }
            },
            _ => ()
        }

        if queue.len() > 0 {
            let res = match &queue[0].data {
                SendData::Report(data) => {
                    match send_report(&url, data_port, id, data) {
                        Ok(()) => {
                            true
                        },
                        Err(e) => {
                            if e != RC_NET_ERROR {
                                println!("[SEND_MANAGER] fail to send not because there is NET_ERROR - remove report");
                                true
                            } else {
                                !queue[0].must_have
                            }
                        }
                    }
                },
                SendData::Stat(data) => {
                    match send_stat(&url, data_port, id, data) {
                        Ok(()) => {
                            true
                        },
                        _ => !&queue[0].must_have
                    }
                }
            };

            if res {
                queue.remove(0);
                println!("[SEND_MANAGER] remove queue element, total in buf: {}", queue.len());
                if queue.len() == 0 && cash_exists {
                    match super::mos::temp_send_data_pop() {
                        Some(data) => {
                            let res: Result<DelayedSend, _> = rmps::decode::from_slice(&data.1);
                            match res {
                                Ok(data) => {
                                    println!("[SEND_MANAGER] add element from cash");
                                    queue.push(data);
                                },
                                _ => ()
                            }
                        },
                        None => {
                            cash_exists = false;
                        }
                    }
                }
                *send_queue.lock().unwrap() = queue.len();
            } else {
                thread::sleep(SEND_MANAGER_ERROR_SLEEP);
            }
        } else
        if cash_exists {
            match super::mos::temp_send_data_pop() {
                Some(data) => {
                    let res: Result<DelayedSend, _> = rmps::decode::from_slice(&data.1);
                    match res {
                        Ok(data) => {
                            queue.push(data);
                        },
                        _ => ()
                    }
                },
                None => {
                    cash_exists = false;
                }
            }
        } else {
            thread::sleep(SEND_MANAGER_SLEEP);
        }
    }
}

pub struct IntApi {
    host: String,
	data_port: u16,
	file_port: u16,
	stream_port: u16,
    id: u32,
    send_queue: Arc<Mutex<usize>>,
    send_manager_handle: Option<JoinHandle<()>>,
    send_manager_tx: Sender<Option<DelayedSend>>
}

impl Drop for IntApi {
    fn drop(&mut self) {
        self.send_manager_tx.send(None).unwrap();
        self.send_manager_handle.take().unwrap().join().unwrap();
    }
}

impl IntApi {
	pub fn new(host: String, data_port: u16, file_port: u16, stream_port: u16, id: u32, max_send_queue_len: usize) -> Self {
        let send_queue = Arc::new(Mutex::new(0));
        let (tx, rx) = channel::<Option<DelayedSend>>();
        let send_queue_cpy = send_queue.clone();
        let host_cpy = host.clone();
        let handle = thread::spawn(move || send_manager(
            rx,
            send_queue_cpy,
            max_send_queue_len,
            host,
            data_port,
            id
        ));
        Self {
            host: host_cpy,
            data_port: data_port,
            file_port: file_port,
            stream_port: stream_port,
            id: id,
            send_queue: send_queue,
            send_manager_handle: Some(handle),
            send_manager_tx: tx
        }
    }

    pub fn get_host(&self) -> String {
        self.host.clone()
    }

    // return true if manager send all data, else false
    fn try_wait_send_manager(&self, timeout: Duration) -> bool {
        let start = Instant::now();
        loop {
            if *self.send_queue.lock().unwrap() > 0 {
                if start.elapsed() > timeout {
                    return false;
                }
                thread::sleep(Duration::from_millis(100));
            } else {
                return true;
            }
        }
    }

    pub fn send_report(&self, report: Report, must_have: bool) {
        self.send_manager_tx.send(Some(DelayedSend {must_have: must_have, instatnt: Utc::now().timestamp_millis(), data: SendData::Report(report)})).unwrap();
    }

    pub fn send_stat(&self, stat: Stat) {
        self.send_manager_tx.send(Some(DelayedSend {must_have: true, instatnt: Utc::now().timestamp_millis(), data: SendData::Stat(stat)})).unwrap();
    }

    pub fn send_log(&self, log: &Log) -> Result<(), u8> {
        let body = match rmps::encode::to_vec(&log) {
            Ok(data) => data,
            _ => return Err(RC_INC_REQ_DATA)
        };

        match perform_request(&self.host, self.data_port, self.id, REQ_ADD_LOG, &body) {
            Ok(_) => Ok(()),
            Err(rc) => return Err(rc)
        }
    }

    pub fn send_status(&self, status: &Status) -> Result<(), u8> {
        let body = match rmps::encode::to_vec(&status) {
            Ok(data) => data,
            _ => return Err(RC_INC_REQ_DATA)
        };

        match perform_request(&self.host, self.data_port, self.id, REQ_SET_STATUS, &body) {
            Ok(_) => Ok(()),
            Err(rc) => return Err(rc)
        }
    }

    pub fn send_run_status(&self, program_id: u32, status: u8, crash_report: Option<String>) -> Result<(), u8> {
        let run_status = RunStatus {program_id: program_id, run_status: status, crash_report: crash_report};
        let body = match rmps::encode::to_vec(&run_status) {
            Ok(data) => data,
            _ => return Err(RC_INC_REQ_DATA)
        };

        match perform_request(&self.host, self.data_port, self.id, REQ_SET_RUN_STATUS, &body) {
            Ok(_) => Ok(()),
            Err(rc) => return Err(rc)
        }
    }
    
    pub fn poll(&self) -> Result<u8, u8> {
        if !self.try_wait_send_manager(WAIT_SEND_MANAGER_TIMEOUT) {
            return Err(RC_NET_ERROR);
        }
        let body = Vec::<u8>::new();

        let answ_gen = match perform_request(&self.host, self.data_port, self.id, REQ_POLL, &body) {
            Ok(data) => data,
            Err(rc) => return Err(rc)
        };

        Ok(answ_gen.event)
    }
    
    pub fn get_update_data(&self, hashes: &UpdateDataReq) -> Result<UpdateDataAnsw, u8> {
        if !self.try_wait_send_manager(WAIT_SEND_MANAGER_TIMEOUT) {
            return Err(RC_NET_ERROR);
        }
        let body = match rmps::encode::to_vec(&hashes) {
            Ok(data) => data,
            _ => return Err(RC_INC_REQ_DATA)
        };

        let answ_gen = match perform_request(&self.host, self.data_port, self.id, REQ_GET_UPDATE_DATA, &body) {
            Ok(data) => data,
            Err(rc) => return Err(rc)
        };

        match rmps::decode::from_slice(&answ_gen.body[..]) {
            Ok(data) => Ok(data),
            _ => Err(RC_INC_ANSW_DATA)
        }
    }
    
    pub fn get_point_config(&self) -> Result<PointConfig, u8>{
        let body = Vec::<u8>::new();

        let answ_gen = match perform_request(&self.host, self.data_port, self.id, REQ_GET_POINT_CONFIG, &body) {
            Ok(data) => data,
            Err(rc) => return Err(rc)
        };

        match rmps::decode::from_slice(&answ_gen.body[..]) {
            Ok(data) => Ok(data),
            _ => Err(RC_INC_ANSW_DATA)
        }
    }
    
    pub fn get_program_config(&self, config_id: u32) -> Result<ProgramConfigAnsw, u8> {
        let req = ProgramConfigReq {config_id: config_id};
        let body = match rmps::encode::to_vec(&req) {
            Ok(data) => data,
            _ => return Err(RC_INC_REQ_DATA)
        };

        let answ_gen = match perform_request(&self.host, self.data_port, self.id, REQ_GET_PROGRAM_CONFIG, &body) {
            Ok(data) => data,
            Err(rc) => return Err(rc)
        };

        let answ: ProgramConfigAnsw = match rmps::decode::from_slice(&answ_gen.body[..]) {
            Ok(data) => data,
            _ => return Err(RC_INC_ANSW_DATA)
        };

        if super::mos::hash_vec(&answ.data) == answ.hash {
            Ok(answ)
        } else {
            Err(RC_INTEGRITY_ERROR)
        }
    }
    
    pub fn get_cmd(&self) -> Result<CmdAnsw, u8> {
        let body = Vec::<u8>::new();

        let answ_gen = match perform_request(&self.host, self.data_port, self.id, REQ_GET_CMD, &body) {
            Ok(data) => data,
            Err(rc) => return Err(rc)
        };

        match rmps::decode::from_slice(&answ_gen.body[..]) {
            Ok(data) => Ok(data),
            _ => Err(RC_INC_ANSW_DATA)
        }
    }
    
    pub fn get_stream(&self) -> Result<(StreamAnsw,TcpStream), u8> {
        let body = Vec::<u8>::new();

        let answ_gen = match perform_request(&self.host, self.data_port, self.id, REQ_GET_STREAM_REQUEST, &body) {
            Ok(data) => data,
            Err(rc) => return Err(rc)
        };

        let answ: StreamAnsw = match rmps::decode::from_slice(&answ_gen.body[..]) {
            Ok(data) => data,
            _ => return Err(RC_INC_ANSW_DATA)
        };

        let con_req = StreamConnectReq {stream_id: answ.stream_id, initiator: false};
        let raw_con_req = rmps::encode::to_vec(&con_req).unwrap();

        let mut stream = match TcpStream::connect(format!("{}:{}", &self.host, self.stream_port)) {
            Ok(s) => s,
            _ => return Err(RC_NET_ERROR)
        };
    
        match stream.write_all(&raw_con_req) {
            Ok(_) => (),
            _ => return Err(RC_NET_ERROR)
        }

        thread::sleep(FIRST_DATA_DELAY);

        Ok((answ, stream))
    }

    pub fn download_asset(&self, program_id: u32) -> Result<(String, Vec<u8>), u8> {
        let req = FileReq {point_id: self.id, point_program_id: program_id, res_type: RES_TYPE_ASSET};
        let file_name = format!("{}_{}", "asset", program_id);
        match download_file(&self.host, self.file_port, req, &file_name) {
            Ok(answ) => Ok((file_name, answ.hash)),
            Err(e) => Err(e)
        }
    }

    pub fn download_program(&self, program_id: u32) -> Result<(String, Vec<u8>), u8> {
        let req = FileReq {point_id: self.id, point_program_id: program_id, res_type: RES_TYPE_BUILD};
        let file_name = format!("{}_{}", "build", program_id);
        match download_file(&self.host, self.file_port, req, &file_name) {
            Ok(answ) => Ok((file_name, answ.hash)),
            Err(e) => Err(e)
        }
    }

    pub fn register(&self, name: String, firm: String) -> Result<RegisterAnsw, u8> {
        let req = RegisterReq {firm: firm, name: name};
        let body = match rmps::encode::to_vec(&req) {
            Ok(data) => data,
            _ => return Err(RC_INC_REQ_DATA)
        };

        let answ_gen = match perform_request(&self.host, self.data_port, self.id, REQ_REGISTER, &body) {
            Ok(data) => data,
            Err(rc) => {
                return Err(rc)
            }
        };

        match rmps::decode::from_slice(&answ_gen.body[..]) {
            Ok(data) => Ok(data),
            _ => Err(RC_INC_ANSW_DATA)
        }
    }
}
