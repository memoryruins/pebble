initSidebarItems({"constant":[["BUDDY_ALLOCATOR_MAX_ORDER",""]],"mod":[["buddy_allocator","One of the allocators we use to manage physical memory is the buddy allocator. With this allocator, memory is broken up into a number of blocks, each of which is a power-of-2 in size. The allocator maintains a set of bins, each with an order `n`, where each bin contains blocks blocks of size `2^n`. When an allocation is requested, and a block of the correct size is not available to fulfil that allocation, a larger block is split into two buddy blocks of half the size, one of which is used to satisfy the allocation, or is split recursively until it's the correct size. When a block is freed, the buddy is queried, and if it's free, the blocks are coalesced again into a larger block."],["userspace_map",""]],"struct":[["LockedPhysicalMemoryManager",""],["PhysicalMemoryManager","The main physical memory manager. It tracks all conventional physical memory and is used by the rest of the kernel to allocate physical memory."]]});