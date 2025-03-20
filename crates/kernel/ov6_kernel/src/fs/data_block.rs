use safe_cast::{SafeInto as _, to_u32};

use super::{
    BlockNo, DeviceNo, SUPER_BLOCK, Tx,
    repr::{self, BITS_PER_BLOCK},
};
use crate::error::KernelError;

/// Zeros a block.
fn block_zero(tx: &Tx<false>, dev: DeviceNo, block_no: BlockNo) {
    tx.get_block(dev, block_no).lock().zeroed();
}

/// Allocates a zeroed data block.
///
/// Returns None if out of disk space.
pub fn alloc(tx: &Tx<false>, dev: DeviceNo) -> Result<BlockNo, KernelError> {
    let sb = SUPER_BLOCK.get();
    for bn0 in (0..sb.size).step_by(BITS_PER_BLOCK) {
        let mut br = tx.get_block(dev, sb.bmap_block(bn0));
        let Ok(mut bg) = br.lock().read();
        let Some(bni) = (0..to_u32!(BITS_PER_BLOCK))
            .take_while(|bni| bn0 + *bni < sb.size)
            .find(|&bni| {
                !bg.data::<repr::BmapBlock>().is_allocated(bni.safe_into()) // block is free (bit = 0)
            })
        else {
            continue;
        };
        bg.data_mut::<repr::BmapBlock>().allocate(bni.safe_into()); // mark block in use
        drop(bg);

        let bn = BlockNo::new(bn0 + bni);
        block_zero(tx, dev, bn);
        return Ok(bn);
    }
    crate::println!("out of blocks");
    Err(KernelError::StorageOutOfBlocks)
}

/// Frees a disk block.
pub fn free(tx: &Tx<false>, dev: DeviceNo, b: BlockNo) {
    let sb = SUPER_BLOCK.get();
    let mut br = tx.get_block(dev, sb.bmap_block(b.value()));
    let Ok(mut bg) = br.lock().read();
    let bi = b.value() as usize % BITS_PER_BLOCK;
    assert!(
        bg.data::<repr::BmapBlock>().is_allocated(bi),
        "freeing free block"
    );
    bg.data_mut::<repr::BmapBlock>().free(bi);
}
