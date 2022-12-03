use bevy_ecs::schedule::StageLabel;

#[derive(StageLabel)]
pub enum Startup {
	InitCert,
	InitServerApi,
	InitSendManager,
	RebootReport,
	InitConfigManager,
	InitIpcManager,
	InitExecManager,
	InitProgramUpdater,
	InitStreamer,
	InitAdminCli
}

#[derive(StageLabel)]
pub enum Core {
	PollServer,
	HandlePollEvents,
	Main,
	Save
}