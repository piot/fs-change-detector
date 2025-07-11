use tracing::{info, warn};
use fs_change_detector::FileWatcher;

#[test_log::test]
fn test() {

    let file_watcher = FileWatcher::new("./".as_ref()).unwrap();

    for _ in 0..10 {
        std::thread::sleep(std::time::Duration::from_millis(500));
        if file_watcher.has_changed() {
            warn!("fs has changed!!");
        } else {
            info!("...no change...");
        }
    }
}