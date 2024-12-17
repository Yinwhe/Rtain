use cgroups_rs::Cgroup;
use log::{debug, error};

use super::image::delete_workspace;
use crate::core::cmd::RMArgs;
use crate::core::{RECORD_MANAGER, ROOT_PATH};

pub fn remove_container(rm_args: RMArgs) {
    let cr = match RECORD_MANAGER.get_record(&rm_args.name) {
        Some(cr) => cr,
        None => {
            error!(
                "Failed to rm container {}, record does not exist",
                &rm_args.name
            );
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
    let root_path = format!("{}/{}", ROOT_PATH, name_id);
    let mnt_path = format!("{}/{}/mnt", ROOT_PATH, name_id);

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

    debug!("Deregister container: {:?}", &cr.name);
    RECORD_MANAGER.deregister(&cr.id);    
}
