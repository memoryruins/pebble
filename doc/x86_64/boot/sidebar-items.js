initSidebarItems({"constant":[["BOOT_INFO_MAGIC",""],["MAX_CAPABILITY_BYTES_PER_IMAGE",""],["NUM_IMAGES",""],["NUM_MEMORY_MAP_ENTRIES",""],["NUM_SEGMENTS_PER_IMAGE","Each initial image is expected to have a maximum of three segments: read-only, read+write, and read+execute."]],"enum":[["MemoryType",""],["PixelFormat",""]],"struct":[["BootInfo","This structure is placed in memory by the bootloader and a reference to it passed to the kernel. It allows the kernel to access information discovered by the bootloader, such as the graphics mode it switched to."],["ImageInfo","An image loaded from the filesystem by the bootloader. The kernel should turn this information into the correct representation and treat this image like a normal task."],["MemoryEntry",""],["MemoryObjectInfo","Describes a memory region that should be represented by the `MemoryObject` kernel object in the kernel."],["VideoInfo",""]]});