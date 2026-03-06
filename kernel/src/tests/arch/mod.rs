pub mod negative;
pub mod recovery;
pub mod smoke;

pub fn run() {
    smoke::run();
    negative::run();
    recovery::run();
}
