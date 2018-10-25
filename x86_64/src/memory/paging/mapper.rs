use super::entry::EntryFlags;
use super::table::{Level4, Table};
use super::{Frame, FrameAllocator, Page};
use crate::hw::tlb;
use crate::memory::{PhysicalAddress, VirtualAddress};

/// A `Mapper` allows you to change the virtual to physical mappings in a set of page tables. It
/// relies on the set of page tables it represents being accessible through the **current**
/// recursive mapping, and so this structure can only safely exist for either the currently mapped
/// set of tables, or if the physical address of the P4 this `Mapper` refers to has been installed
/// into the recursive entry of the currently mapped P4.
///
/// We don't support page tables containing huge pages (P4s, P3s, or P2s that map whole contiguous
/// blocks of memory, rather than containing child tables). This shouldn't be a problem if you use
/// `Mapper` to create page tables as Pebble does, but may create problems if you try and interpret
/// existing page tables (set up by the UEFI, for example) with `Mapper`.
pub struct Mapper {
    p4: &'static mut Table<Level4>,
}

impl Mapper {
    pub unsafe fn new() -> Mapper {
        Mapper {
            p4: &mut *super::table::P4,
        }
    }

    /// Get the `PhysicalAddress` a given `VirtualAddress` is mapped to by these page tables, if
    /// it's mapped. If these page tables don't map it to any physical frame, this returns `None`.
    pub fn translate(&self, address: VirtualAddress) -> Option<PhysicalAddress> {
        self.translate_page(Page::contains(address))?
            .start_address()
            + address.offset_into_page()
    }

    /// Get the physical `Frame` that a given virtual `Page` is mapped to, if it's mapped.
    /// Returns `None` if the page is not mapped by these page tables.
    pub fn translate_page(&self, page: Page) -> Option<Frame> {
        self.p4
            .next_table(page.p4_index())
            .and_then(|p3| p3.next_table(page.p3_index()))
            .and_then(|p2| p2.next_table(page.p2_index()))
            .and_then(|p1| p1[page.p1_index()].pointed_frame())
    }

    /// Map the given `Page` somewhere in physical memory. Allocates a page using the given
    /// `FrameAllocator`.
    pub fn map<A>(&mut self, page: Page, flags: EntryFlags, allocator: &mut A)
    where
        A: FrameAllocator,
    {
        self.map_to(page, allocator.allocate().unwrap(), flags, allocator);
    }

    pub fn map_to<A>(&mut self, page: Page, frame: Frame, flags: EntryFlags, allocator: &mut A)
    where
        A: FrameAllocator,
    {
        /*
         * If the page should be accessible from userspace, all the parent paging structures need to
         * be marked user-accessible too, or we'll still page-fault. This doesn't alter permissions
         * for other pages in those structures.
         */
        let user_accessible = flags.contains(EntryFlags::USER_ACCESSIBLE);
        let p3 = self
            .p4
            .next_table_create(page.p4_index(), user_accessible, allocator);
        let p2 = p3.next_table_create(page.p3_index(), user_accessible, allocator);
        let p1 = p2.next_table_create(page.p2_index(), user_accessible, allocator);

        assert!(
            p1[page.p1_index()].is_unused(),
            "Tried to map a page that is already mapped: {:#x}",
            page.start_address()
        );

        p1[page.p1_index()].set(frame, flags | EntryFlags::default());
        tlb::invalidate_page(page.start_address());
    }

    pub fn unmap<A>(&mut self, page: Page, allocator: &mut A)
    where
        A: FrameAllocator,
    {
        assert!(self.translate_page(page).is_some());

        let p1 = self
            .p4
            .next_table_mut(page.p4_index())
            .and_then(|p3| p3.next_table_mut(page.p3_index()))
            .and_then(|p2| p2.next_table_mut(page.p2_index()))
            .expect("Page tables have been broken. Something has gone very wrong...");
        let frame = p1[page.p1_index()].pointed_frame().unwrap();
        p1[page.p1_index()].set_unused();
        tlb::invalidate_page(page.start_address());

        // TODO: should we care about freeing empty P1s, P2s and P3s?
        allocator.free(frame);
    }
}
