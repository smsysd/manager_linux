use std::net::TcpStream;
use std::time::Duration;
use pbr::ProgressBar;
use rmp_serde as rmps;
use std::io::{Error, Write, Read};

use crate::data_types;
use crate::utils::{err, rmp_decode};
use data_types::data_server::*;
use data_types::file_server;
use crate::utils::mos;
use data_types::data_server::GetPointConfigAnsw as PointConfig;
use data_types::data_server::GetProgramConfigAnsw as ProgramConfig;

const FILE_BUF_SIZE: usize = 4096;
const FIRST_DATA_DELAY: Duration = Duration::from_millis(1000);
const READ_TIMEOUT: Duration = Duration::from_millis(5000);
const WRITE_TIMEOUT: Duration = Duration::from_millis(5000);

#[derive(Clone)]
pub struct IntApi {
    host: String,
	data_port: u16,
	file_port: u16,
    auth: Auth
}

impl IntApi {
	pub fn new(host: String, data_port: u16, file_port: u16, id: i32, token: Vec<u8>) -> Self {
        Self {
            host: host,
            data_port: data_port,
            file_port: file_port,
            auth: Auth {id: id, token: token}
        }
    }

    pub fn get_host(&self) -> String {
        self.host.clone()
    }
    
    pub fn send_report(&self, report: Report) -> Result<(), Error> {
        let req = Request::AddReport(self.auth.clone(), report);
        let answ_raw = self.data_request(&req)?;
        let _: OkAnsw = rmp_decode(&answ_raw)?;
        Ok(())
    }
    
    pub fn send_stat(&self, stat: Stat) -> Result<(), Error> {
        let req = Request::AddStat(self.auth.clone(), stat);
        let answ_raw = self.data_request(&req)?;
        let _: OkAnsw = rmp_decode(&answ_raw)?;
        Ok(())
    }

    pub fn send_log(&self, log: Log) -> Result<(), Error> {
        let req = Request::AddLog(self.auth.clone(), log);
        let answ_raw = self.data_request(&req)?;
        let _: OkAnsw = rmp_decode(&answ_raw)?;
        Ok(())
    }

    pub fn send_status(&self, status: ProgramStatus) -> Result<(), Error> {
        let req = Request::SetStatus(self.auth.clone(), status);
        self.data_request(&req)?;
        Ok(())
    }

    pub fn send_run_status(&self, status: ProgramRunStatus) -> Result<(), Error> {
        let req = Request::SetRunStatus(self.auth.clone(), status);
        self.data_request(&req)?;
        Ok(())
    }
    
    pub fn poll(&self) -> Result<PollAnsw, Error> {
        let req = Request::Poll(self.auth.clone());
        let answ_raw = self.data_request(&req)?;
        Ok(rmp_decode(&answ_raw)?)
    }
    
    pub fn get_update_data(&self, hashes: Vec<ProgramHashes>) -> Result<GetUpdateDataAnsw, Error> {
        let req = Request::GetUpdateData(self.auth.clone(), hashes);
        let answ_raw = self.data_request(&req)?;
        Ok(rmp_decode(&answ_raw)?)
    }
    
    pub fn get_point_config(&self) -> Result<PointConfig, Error> {
        let req = Request::GetPointConfig(self.auth.clone());
        let answ_raw = self.data_request(&req)?;
        Ok(rmp_decode(&answ_raw)?)
    }
    
    pub fn get_program_config(&self, config_id: i32) -> Result<ProgramConfig, Error> {
        let req = Request::GetProgramConfig(self.auth.clone(), config_id);
        let answ_raw = self.data_request(&req)?;
        Ok(rmp_decode(&answ_raw)?)
    }

    pub fn download_asset(&self, program_id: i32) -> Result<(String, Vec<u8>), Error> {
        let req = file_server::Request {
            point_id: self.auth.id,
            token: self.auth.token.clone(),
            point_program_id: program_id,
            res_type: file_server::ResourceType::Asset
        };
        let fname = format!("{}_asset", program_id);
        let answ = self.download_file(req, &fname)?;
        Ok((fname, answ.hash))
    }

    pub fn download_program(&self, program_id: i32) -> Result<(String, Vec<u8>), Error> {
        let req = file_server::Request {
            point_id: self.auth.id,
            token: self.auth.token.clone(),
            point_program_id: program_id,
            res_type: file_server::ResourceType::Build
        };
        let fname = format!("{}_build", program_id);
        let answ = self.download_file(req, &fname)?;
        Ok((fname, answ.hash))
    }

    pub fn register(&self, name: String, firm: Option<String>) -> Result<RegisterAnsw, Error> {
        let answ_raw = self.data_request(&Request::Register(name, firm))?;
        Ok(rmp_decode(&answ_raw)?)
    }

    fn data_request(&self, req: &Request) -> Result<Vec<u8>, Error> {
        let mut stream = TcpStream::connect(format!("{}:{}", &self.host, self.data_port))?;
        stream.set_read_timeout(Some(READ_TIMEOUT))?;
        stream.set_write_timeout(Some(WRITE_TIMEOUT))?;
        let data = rmps::encode::to_vec(&req).unwrap();
        stream.write_all(&data)?;
        let mut buf = Vec::new();
        stream.read_to_end(&mut buf)?;
        Ok(buf)
    }
    
    fn download_file(&self, req: file_server::Request, temp_file_name: &str) -> Result<file_server::Answer, Error> {
        let req_raw = rmps::encode::to_vec(&req).unwrap();
        let mut stream = TcpStream::connect(format!("{}:{}", &self.host, self.file_port))?;
        stream.write_all(&req_raw)?;
        let mut buf: [u8;512] = [0;512];
        stream.set_read_timeout(Some(FIRST_DATA_DELAY))?;
        let len = stream.read(&mut buf)?;
        if len == 0 {
            return Err(err("broken"));
        }
        let answ: file_server::Answer = rmp_decode(&buf[..len])?;
        stream.set_read_timeout(Some(READ_TIMEOUT))?;
        let mut file_buf: [u8;FILE_BUF_SIZE] = [0;FILE_BUF_SIZE];
        let mut file = mos::create_temp_arch(&temp_file_name)?;
        let mut pb = ProgressBar::new(answ.fsize as u64);
        loop {
            match stream.read(&mut file_buf) {
                Ok(len) => {
                    if len == 0 {
                        break;
                    }
                    file.write_all(&file_buf[..len])?;
                    pb.add(len as u64);
                },
                _ => break
            }
        }
    
        let hash = mos::hash_file(&mut file)?;
        if hash == answ.hash {
            pb.finish_println("\tFILE DOWNLOADED, HASH OK\n");
            Ok(answ)
        } else {
            pb.finish_println("\tFILE DOWNLOADED, HASH FAIL\n");
            Err(err("integrity error"))
        }
    }
}
