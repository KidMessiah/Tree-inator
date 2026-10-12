//! Starts the background sprite bake and waits, to check it fills the disk cache.
fn main() {
    forest_core::treegen_sprite::prebake_in_background();
    std::thread::sleep(std::time::Duration::from_secs(std::env::args().nth(1).map_or(60, |v| v.parse().unwrap())));
}
