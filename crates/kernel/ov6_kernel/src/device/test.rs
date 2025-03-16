use core::ptr;

use once_init::OnceInit;
use vcell::VolatileCell;

use crate::memory::layout::VIRT_TEST;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Finisher {
    Fail(u16),
    Pass(u16),
    Reset,
}

impl Finisher {
    fn to_code(self) -> u32 {
        match self {
            Self::Fail(code) => 0x3333 | (u32::from(code) << 16),
            Self::Pass(code) => 0x5555 | (u32::from(code) << 16),
            Self::Reset => 0x7777,
        }
    }
}

struct TestDevice {
    finisher: VolatileCell<u32>,
}

unsafe impl Sync for TestDevice {}

static TEST: OnceInit<&TestDevice> = OnceInit::new();

pub fn init() {
    let test = unsafe {
        ptr::with_exposed_provenance::<TestDevice>(VIRT_TEST)
            .as_ref()
            .unwrap()
    };
    TEST.init(test);
}

pub fn finish(finisher: Finisher) -> ! {
    TEST.get().finisher.set(finisher.to_code());
    panic!("finished");
}
