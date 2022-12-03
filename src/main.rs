use std::thread;
use std::time::Duration;
use bevy_ecs::prelude::*;
use bevy_ecs::schedule::ShouldRun;
use sendm::SendManager;
use std::io::Error;

pub mod data_types;
pub mod utils;
pub mod stages;
pub mod events;
pub mod certm;
pub mod srvm;
pub mod sendm;
pub mod configm;
pub mod ipcm;
pub mod execm;
pub mod program_updater;
pub mod streamer;
pub mod admin_clim;

use data_types::data_server::{Report, ReportType};

use crate::utils::err;

const MAIN_DELAY: Duration = Duration::from_millis(25);

fn reboot_report(mut sm: ResMut<SendManager>) {
    sm.report(Report {delay: 0, rtype: ReportType::Reboot, program_id: None, descr: None})
}

fn main() -> Result<(), Error> {
    if utils::mos::is_manager_already_run() {
        return Err(err("manager already run"));
    }

    let mut world = World::default();
    let mut schedule = Schedule::default();
    world.insert_resource(data_types::AppState::default());
    schedule.add_stage(stages::Startup::InitCert, SystemStage::parallel().with_run_criteria(ShouldRun::once));
    schedule.add_stage(stages::Startup::InitServerApi, SystemStage::parallel().with_run_criteria(ShouldRun::once));
    schedule.add_stage(stages::Startup::InitSendManager, SystemStage::parallel().with_run_criteria(ShouldRun::once));
    schedule.add_stage(stages::Startup::RebootReport, SystemStage::parallel().with_run_criteria(ShouldRun::once));
    schedule.add_system_to_stage(stages::Startup::RebootReport, reboot_report);
    schedule.add_stage(stages::Startup::InitConfigManager, SystemStage::parallel().with_run_criteria(ShouldRun::once));
    schedule.add_stage(stages::Startup::InitIpcManager, SystemStage::parallel().with_run_criteria(ShouldRun::once));
    schedule.add_stage(stages::Startup::InitExecManager, SystemStage::parallel().with_run_criteria(ShouldRun::once));
    schedule.add_stage(stages::Startup::InitProgramUpdater, SystemStage::parallel().with_run_criteria(ShouldRun::once));
    schedule.add_stage(stages::Startup::InitStreamer, SystemStage::parallel().with_run_criteria(ShouldRun::once));
    schedule.add_stage(stages::Startup::InitAdminCli, SystemStage::parallel().with_run_criteria(ShouldRun::once));
    schedule.add_stage(stages::Core::PollServer, SystemStage::parallel());
    schedule.add_stage(stages::Core::HandlePollEvents, SystemStage::parallel());
    schedule.add_stage(stages::Core::Main, SystemStage::parallel());
    schedule.add_stage(stages::Core::Save, SystemStage::parallel());
    
    events::init(&mut world, &mut schedule);
    certm::init(&mut world, &mut schedule)?;
    srvm::init(&mut world, &mut schedule)?;
    sendm::init(&mut world, &mut schedule)?;
    configm::init(&mut world, &mut schedule)?;
    ipcm::init(&mut world, &mut schedule)?;
    execm::init(&mut world, &mut schedule)?;
    program_updater::init(&mut world, &mut schedule)?;
    streamer::init(&mut world, &mut schedule)?;
    admin_clim::init(&mut world, &mut schedule)?;

    loop {
        schedule.run(&mut world);
        thread::sleep(MAIN_DELAY);
    }
}
