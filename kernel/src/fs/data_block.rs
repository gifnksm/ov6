use super::{
    BlockNo, DeviceNo, SUPER_BLOCK, Tx,
    repr::{self, BITS_PER_BLOCK},
};

/// Zeros a block.
fn block_zero(tx: &Tx<false>, dev: DeviceNo, block_no: BlockNo) {
    tx.get_block(dev, block_no).lock().zeroed();
}

/// Allocates a zeroed data block.
///
/// Returns None if out of disk space.
pub fn alloc(tx: &Tx<false>, dev: DeviceNo) -> Option<BlockNo> {
    let sb = SUPER_BLOCK.get();
    let sb_size = sb.size as usize;
    for bn0 in (0..sb_size).step_by(BITS_PER_BLOCK) {
        let mut br = tx.get_block(dev, sb.bmap_block(bn0));
        let Ok(mut bg) = br.lock().read();
        let Some(bni) = (0..BITS_PER_BLOCK)
            .take_while(|bni| bn0 + *bni < sb_size)
            .find(|bni| {
                !bg.data::<repr::BmapBlock>().bit(*bni) // block is free (bit = 0)
            })
        else {
            continue;
        };
        bg.data_mut::<repr::BmapBlock>().set_bit(bni); // mark block in use
        drop(bg);

        let bn = BlockNo::new((bn0 + bni).try_into().unwrap());
        block_zero(tx, dev, bn);
        return Some(bn);
    }
    crate::println!("out of blocks");
    None
}

/// Frees a disk block.
pub fn free(tx: &Tx<false>, dev: DeviceNo, b: BlockNo) {
    let sb = SUPER_BLOCK.get();
    let mut br = tx.get_block(dev, sb.bmap_block(b.as_index()));
    let Ok(mut bg) = br.lock().read();
    let bi = b.value() as usize % BITS_PER_BLOCK;
    assert!(bg.data::<repr::BmapBlock>().bit(bi), "freeing free block");
    bg.data_mut::<repr::BmapBlock>().clear_bit(bi);
}
