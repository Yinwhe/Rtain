use cgroups_rs::Cgroup;
use log::{debug, error};

use crate::{RMArgs, RECORD_MANAGER};

use super::image::delete_workspace;

pub fn remove_container(rm_args: RMArgs) {
    let bindings = RECORD_MANAGER.lock().unwrap();
    let cr = match bindings.container_with_name(&rm_args.name) {
        Ok(cr) => cr,
        Err(e) => {
            error!("Failed to stop container {}, due to: {}", &rm_args.name, e);
            return;
        }
    };

    if cr.status.is_running() {
        error!(
            "Failed to rm container {}, it's still running",
            &rm_args.name
        );
        return;
    }

    // Do some clean up.
    let name_id = format!("{}-{}", cr.name, cr.id);
    let root_path = format!("/tmp/rtain/{}", name_id);
    let mnt_path = format!("/tmp/rtain/{}/mnt", name_id);

    let id = cr.id.clone();
    drop(bindings);

    let hier = cgroups_rs::hierarchies::auto();
    let cg = Cgroup::load(hier, name_id);

    debug!("Delete cgroup: {:?}", &cg);
    if let Err(e) = cg.delete() {
        error!(
            "Failed to rm container {}, cannot clean up cgroup: {}",
            &rm_args.name, e
        );
    }

    // TODO: volume support needed.
    debug!("Delete workspace: {:?}", &root_path);
    if let Err(e) = delete_workspace(&root_path, &mnt_path, &None) {
        error!(
            "Failed to rm container {}, cannot clean up workspace: {}",
            &rm_args.name, e
        );
    }

    debug!("Deregister container: {:?}", &id);
    if let Err(e) = RECORD_MANAGER.lock().unwrap().deregister(&id) {
        error!(
            "Failed to rm container {}, cannot deregister: {}",
            &rm_args.name, e
        );
    }
}
