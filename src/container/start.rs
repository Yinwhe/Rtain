use log::error;

use crate::{StartArgs, RECORD_MANAGER};

pub fn start_container(start_args: StartArgs) {
    let bindings = RECORD_MANAGER.lock().unwrap();
    let cr = match bindings.container_with_name(&start_args.name) {
        Ok(cr) => cr,
        Err(e) => {
            error!(
                "Failed to start container {}, due to: {}",
                &start_args.name, e
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
