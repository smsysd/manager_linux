/// Load config from disk or from server.
/// Write config, hashes to disk if it was were changed.
/// Update config if receive corresponding PollEvent (PointConfigUpdateAvailable)

use std::io::Error;
use bevy_ecs::prelude::*;


/// Load config from disk or from server.
/// 
pub fn init(world: &mut World, schedule: &mut Schedule) -> Result<(), Error> {

	Ok(())
}