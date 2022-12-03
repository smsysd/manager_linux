/// Load config from disk or from server.
/// Write config, hashes to disk if it was were changed.
/// Update config if receive corresponding PollEvent (PointConfigUpdateAvailable)

use std::{io::Error, time::Duration};
use bevy_ecs::prelude::*;

use crate::utils::siapi::IntApi;
use crate::{events, stages};
use crate::sendm::SendManager;
use crate::{utils::mos, srvm::Server};
use crate::data_types::data_server::{GetPointConfigAnsw as PointConfig, Report, ReportType};

#[derive(Resource)]
pub struct ConfigBase {
	pub poll_period: Duration,
	pub bin_path: String,
	pub ipc_dir: String,
}

fn sys_config_updater(
    server: Res<Server>,
    mut sm: ResMut<SendManager>,
    mut config: ResMut<PointConfig>,
    evr: EventReader<events::PointUpdateAvailable>
) {
    if evr.is_empty() {
        return;
    }
    println!("[CONFIGM] PointConfigUpdateAvailable, upload config..");
    let new_config = match upload_config(&server.api, &mut sm) {
        Ok(c) => c,
        _ => return
    };
    println!("\t[CONFIGM] PointConfig updated");
    *config = new_config;
}

fn startup(mut cmd: Commands, server: Res<Server>, mut sm: ResMut<SendManager>) {
    println!("[CONFIGM] startup..");
    let config = match mos::read_config() {
        Ok(c) => {
            println!("\t[CONFIGM] success load config from disk");
            c
        },
        Err(e) => {
            println!("\t[CONFIGM] fail to load config from disk: {:?}, load from server..", e);
            match upload_config(&server.api, &mut sm) {
                Ok(c) => {
                    println!("\t[CONFIGM] success load config from server");
                    c
                },
                _ => {
                    panic!("\t[ERROR][CONFIGM] FAIL TO LOAD CONFIG");
                }
            }
        }
    };

    let config_base = ConfigBase {
        bin_path: config.bin_path.clone(),
        ipc_dir: config.ipc_dir.clone(),
        poll_period: Duration::from_millis(config.poll_period as u64)
    };

    cmd.insert_resource(config);
	cmd.insert_resource(config_base);
}

pub fn init(_world: &mut World, schedule: &mut Schedule) -> Result<(), Error> {
    schedule.add_system_to_stage(stages::Startup::InitConfigManager, startup);
    schedule.add_system_to_stage(stages::Core::HandlePollEvents, sys_config_updater);

	Ok(())
}

fn upload_config(api: &IntApi, sm: &mut SendManager) -> Result<PointConfig, Error> {
    let config = api.get_point_config()?;
    mos::write_config(&config)?;
    sm.report(Report {
        delay: 0,
        descr: None,
        program_id: None,
        rtype: ReportType::PointConfigUpdate
    });
    Ok(config)
}