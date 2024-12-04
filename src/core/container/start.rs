use log::error;

use crate::core::cmd::StartArgs;
use crate::core::RECORD_MANAGER;

pub fn start_container(start_args: StartArgs) {
    let cr = match RECORD_MANAGER.get_record(&start_args.name) {
        Some(cr) => cr,
        None => {
            error!(
                "Failed to start container {}, record does not exist",
                &start_args.name
            );
            return;
        }
    };

    if cr.status.is_running() {
        error!(
            "Failed to start container {}, it's already running",
            &start_args.name
        );
        return;
    }

    // TODO: run it with command and detach by default.
}
