use core::{
    iter::{Peekable, Zip},
    mem,
    ops::{RangeBounds, RangeInclusive},
    slice,
};

use arrayvec::ArrayVec;

use super::{PageTableEntries, PtEntry};
use crate::memory::VirtAddr;

type EntriesIter<'a> = Peekable<Zip<RangeInclusive<usize>, slice::Iter<'a, PtEntry>>>;
type EntriesStack<'a> = ArrayVec<(usize, VirtAddr, EntriesIter<'a>), 3>;

pub(super) struct Entries<'a> {
    state: Option<(RangeInclusive<VirtAddr>, EntriesStack<'a>)>,
    last_item_is_non_leaf: bool,
}

impl<'a> Entries<'a> {
    pub(super) fn new<R>(pt: &'a PageTableEntries, va_range: R) -> Self
    where
        R: RangeBounds<VirtAddr>,
    {
        let Some(va_range) = VirtAddr::range_inclusive(va_range) else {
            return Self {
                state: None,
                last_item_is_non_leaf: false,
            };
        };

        let min_va = *va_range.start();
        let max_va = *va_range.end();
        let level_min_idx = min_va.level_idx(2);
        let level_max_idx = max_va.level_idx(2);
        let mut stack = ArrayVec::<_, 3>::new();
        let it = (level_min_idx..=level_max_idx)
            .zip(&pt.0[level_min_idx..=level_max_idx])
            .peekable();
        stack.push((2, VirtAddr::ZERO, it));
        Self {
            state: Some((va_range, stack)),
            last_item_is_non_leaf: false,
        }
    }
}

impl<'a> Iterator for Entries<'a> {
    type Item = (usize, VirtAddr, &'a PtEntry);

    fn next(&mut self) -> Option<Self::Item> {
        let (va_range, stack) = self.state.as_mut()?;
        let min_va = *va_range.start();
        let max_va = *va_range.end();

        if mem::take(&mut self.last_item_is_non_leaf) {
            let (level, base_va, ptes) = stack.last_mut().unwrap();
            let (idx, pte) = ptes.next().unwrap();
            let level_va = base_va.with_level_idx(*level, idx);

            assert_eq!(level_va.level_idx(*level - 1), 0);
            let level_min_va = level_va.with_level_idx(*level - 1, 0);
            let leval_max_va = level_va.with_level_idx(*level - 1, 511);
            let level_min_idx = VirtAddr::max(min_va, level_min_va).level_idx(*level - 1);
            let level_max_idx = VirtAddr::min(max_va, leval_max_va).level_idx(*level - 1);

            let pt = pte.get_page_table().unwrap();
            let it = (level_min_idx..=level_max_idx)
                .zip(&pt.0[level_min_idx..=level_max_idx])
                .peekable();
            let elem = (*level - 1, level_min_va, it);
            stack.push(elem);
        }

        while let Some((level, base_va, ptes)) = stack.last_mut() {
            if let Some((idx, pte)) = ptes.next_if(|(_idx, pte)| !pte.is_valid() || pte.is_leaf()) {
                let level_min_va = base_va.with_level_idx(*level, idx);
                return Some((*level, level_min_va, pte));
            }

            let Some((idx, pte)) = ptes.peek() else {
                stack.pop();
                continue;
            };

            self.last_item_is_non_leaf = true;
            let level_min_va = base_va.with_level_idx(*level, *idx);
            return Some((*level, level_min_va, *pte));
        }
        None
    }
}

type LeavesMutIter<'a> = Zip<RangeInclusive<usize>, slice::IterMut<'a, PtEntry>>;
type LeavesMutStack<'a> = ArrayVec<(usize, VirtAddr, LeavesMutIter<'a>), 3>;

pub(super) struct LeavesMut<'a>(Option<(RangeInclusive<VirtAddr>, LeavesMutStack<'a>)>);

impl<'a> LeavesMut<'a> {
    pub(super) fn new<R>(pt: &'a mut PageTableEntries, va_range: R) -> Self
    where
        R: RangeBounds<VirtAddr>,
    {
        let Some(va_range) = VirtAddr::range_inclusive(va_range) else {
            return Self(None);
        };

        let min_va = *va_range.start();
        let max_va = *va_range.end();
        let level_min_idx = min_va.level_idx(2);
        let level_max_idx = max_va.level_idx(2);
        let mut stack = ArrayVec::<_, 3>::new();
        let it = (level_min_idx..=level_max_idx).zip(&mut pt.0[level_min_idx..=level_max_idx]);
        stack.push((2, VirtAddr::ZERO, it));
        Self(Some((va_range, stack)))
    }
}

impl<'a> Iterator for LeavesMut<'a> {
    type Item = (usize, VirtAddr, &'a mut PtEntry);

    fn next(&mut self) -> Option<Self::Item> {
        let (va_range, stack) = self.0.as_mut()?;
        let min_va = *va_range.start();
        let max_va = *va_range.end();
        while let Some((level, base_va, ptes)) = stack.last_mut() {
            let Some((idx, pte)) = ptes.next() else {
                stack.pop();
                continue;
            };

            let level_min_va = base_va.with_level_idx(*level, idx);
            assert!(!pte.is_leaf() || va_range.contains(&level_min_va));

            if !pte.is_valid() {
                continue;
            }

            if pte.is_leaf() {
                return Some((*level, level_min_va, pte));
            }

            let leval_max_va = level_min_va.with_level_idx(*level - 1, 511);
            let level_min_idx = VirtAddr::max(min_va, level_min_va).level_idx(*level - 1);
            let level_max_idx = VirtAddr::min(max_va, leval_max_va).level_idx(*level - 1);

            let pt = pte.get_page_table_mut().unwrap();
            let it = (level_min_idx..=level_max_idx).zip(&mut pt.0[level_min_idx..=level_max_idx]);
            let elem = (*level - 1, level_min_va, it);
            stack.push(elem);
        }
        None
    }
}
