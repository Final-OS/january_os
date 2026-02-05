pub mod vmalloc_test;
pub mod vma_test;
pub mod buddy_test;
pub mod slub_test;
pub mod paging_test;

use crate::kprintln;

pub fn run_tests() {
    kprintln!("========================================");
    kprintln!("Starting Memory Management Unit Tests");
    kprintln!("========================================");

    buddy_test::test_buddy();
    slub_test::test_slub();
    paging_test::test_paging();
    vmalloc_test::test_vmalloc();
    vma_test::test_vma();

    kprintln!("========================================");
    kprintln!("All MM Tests Completed");
    kprintln!("========================================");
}
