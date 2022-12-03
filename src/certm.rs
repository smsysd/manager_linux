/// Cert manager, read cert from disk, try registration if cert only base.
/// Put Cert Resource. Reset Cert if receive corresponding PollEvent(NotReg).

use bevy_ecs::prelude::*;
use std::io::Error;
use std::thread;
use std::time::Duration;
use crate::data_types::Cert;
use crate::data_types::AppState;
use crate::data_types::AppStateCode;
use crate::data_types::data_server::Auth;
use crate::data_types::data_server::RegisterAnsw;
use crate::events::NotReg;
use crate::utils;
use utils::mos;
use utils::siapi;
use crate::stages;

const REGISTER_REQ_DELAY: Duration = Duration::from_millis(5000);

fn sys_reset(ev: EventReader<NotReg>, mut state: ResMut<AppState>, cert: Res<Cert>) {
    if !ev.is_empty() {
        reset_cert(&cert).unwrap();
        state.code = AppStateCode::Shutdown;
    }
}

pub fn init(world: &mut World, schedule: &mut Schedule) -> Result<(), Error> {
    println!("[CERTM] startup..");
    // read cert
    let cert: Cert = mos::read_cert()?;

    // register point if already not
    let cert = if cert.auth.is_none() {
        println!("\t[CERTM] cert only base, register..");
        let new_cert = register(cert)?;
        mos::write_cert(&new_cert)?;
        new_cert
    } else {
        println!("\t[CERTM] cert full loaded from disk");
        cert
    };

    println!("\t[CERTM] cert sucess loaded");
    world.insert_resource(cert);
    schedule.add_system_to_stage(stages::Core::Save, sys_reset);

    Ok(())
}

fn register(cert: Cert) -> Result<Cert, Error> {
    let point_name = match cert.name {
        Some(name) => name,
        _ => mos::get_hostname()
    };
    let api = siapi::IntApi::new(cert.host.clone(), cert.data_port, cert.file_port, 0, Vec::new());
    loop {
        let answ = api.register(point_name.clone(), cert.firm_name.clone())?;
        match answ {
            RegisterAnsw::Ok(data) => {
                let new_cert = Cert {
                    host: cert.host,
                    name: Some(data.name),
                    firm_id: Some(data.firm_id),
                    firm_name: Some(data.firm_name),
                    auth: Some(Auth {id: data.id, token: data.token}),
                    data_port: cert.data_port,
                    file_port: cert.file_port,
                    stream_port: cert.stream_port
                };
                return Ok(new_cert);
            },
            RegisterAnsw::ProceedIndicate => mos::indicate(),
            RegisterAnsw::Proceed => thread::sleep(REGISTER_REQ_DELAY)
        }
    }
}

fn reset_cert(cert: &Cert) -> Result<(), Error> {
    let new_cert = Cert {
        host: cert.host.clone(),
        data_port: cert.data_port,
        file_port: cert.file_port,
        stream_port: cert.stream_port,
        name: cert.name.clone(),
        firm_name: cert.firm_name.clone(),
        firm_id: None,
        auth: None
    };
    mos::write_cert(&new_cert)
}