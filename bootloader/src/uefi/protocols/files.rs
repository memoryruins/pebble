use crate::{
    memory::MemoryType,
    uefi::{
        boot_services::{self, utf16_to_str, OpenProtocolAttributes, Pool, Protocol, SearchType},
        system_table,
        Char16,
        Guid,
        Handle,
        Status,
    },
};
use bitflags::bitflags;
use core::{mem, ops::Drop, slice};

/// The label of the boot volume (the FAT filesystem that the bootloader reads files from). Needs to
/// match the volume label used when creating the FAT filesystem.
const BOOT_VOLUME_LABEL: &str = "BOOT";

/// Read a file from the `BOOT` volume using the UEFI file protocols.
pub fn read_file(path: &str, image_handle: Handle) -> Result<Pool<[u8]>, Status> {
    let volume_root = system_table()
        .boot_services
        .locate_handle(SearchType::ByProtocol, Some(SimpleFileSystem::guid()), None)?
        .iter()
        .filter_map(|handle| {
            system_table()
                .boot_services
                .open_protocol::<SimpleFileSystem>(
                    *handle,
                    image_handle,
                    0,
                    OpenProtocolAttributes::BY_HANDLE_PROTOCOL,
                )
                .and_then(|volume| volume.open_volume())
                .ok()
        })
        .find(|root| {
            root.get_info::<FileSystemInfo>()
                .and_then(|info| info.volume_label())
                .map(|label| label == BOOT_VOLUME_LABEL)
                .unwrap_or(false)
        })
        .ok_or(Status::NotFound)?;

    let path = boot_services::str_to_utf16(path)?;
    let file = volume_root.open(&path, FileMode::READ, FileAttributes::empty())?;

    let file_size = file.get_info::<FileInfo>()?.file_size as usize;
    let mut file_buf = system_table().boot_services.allocate_slice::<u8>(file_size)?;

    file.read(&mut file_buf)?;
    Ok(file_buf)
}

/// Provides file based access to supported file systems
#[repr(C)]
pub struct File {
    pub revision: u64,
    pub _open: extern "win64" fn(
        this: &File,
        new_handle: &mut *mut File,
        file_name: *const Char16,
        open_mode: FileMode,
        attributes: FileAttributes,
    ) -> Status,
    pub _close: extern "win64" fn(this: &File) -> Status,
    pub _delete: extern "win64" fn() -> Status,
    pub _read: extern "win64" fn(this: &File, buffer_size: &mut usize, buffer: *mut u8) -> Status,
    pub _write: extern "win64" fn() -> Status,
    pub _get_position: extern "win64" fn() -> Status,
    pub _set_position: extern "win64" fn() -> Status,
    pub _get_info: extern "win64" fn(
        this: &File,
        information_type: &Guid,
        buffer_size: &mut usize,
        buffer: *mut u8,
    ) -> Status,
    pub _set_info: extern "win64" fn() -> Status,
    pub _flush: extern "win64" fn() -> Status,
}

impl File {
    /// Opens a new file relative to this file's location
    pub fn open(
        &self,
        file_name: &[Char16],
        open_mode: FileMode,
        attributes: FileAttributes,
    ) -> Result<&mut File, Status> {
        let mut file = 0x0 as *mut _;
        (self._open)(self, &mut file, file_name.as_ptr(), open_mode, attributes).as_result()?;

        if file == 0x0 as *mut _ {
            Err(Status::NotFound)
        } else {
            Ok(unsafe { &mut *file })
        }
    }

    /// Reads data from this file
    pub fn read(&self, buf: &mut [u8]) -> Result<usize, Status> {
        let mut len = buf.len();
        (self._read)(self, &mut len, buf.as_mut_ptr()).as_result().map(|_| len)
    }

    /// Returns information about a file
    pub fn get_info<T>(&self) -> Result<Pool<T>, Status>
    where
        T: FileInformationType + Sized,
    {
        let mut buf_size = mem::size_of::<T>();
        let buf = system_table().boot_services.allocate_pool(MemoryType::LoaderData, buf_size)?;
        let res = (self._get_info)(self, T::guid(), &mut buf_size, buf);
        if res == Status::Success {
            // If the initial buffer happened to be large enough, return it
            // This should never happen, because the length of the file name or volume label should
            // always be greater than 1
            return Ok(unsafe { Pool::new_unchecked(buf as *mut T) });
        } else if res != Status::BufferTooSmall {
            return Err(res);
        }

        // Reallocate the buffer with the specified size
        system_table().boot_services.free_pool(buf)?;
        let buf = system_table().boot_services.allocate_pool(MemoryType::LoaderData, buf_size)?;
        (self._get_info)(self, T::guid(), &mut buf_size, buf)
            .as_result()
            .map(|_| unsafe { Pool::new_unchecked(buf as *mut T) })
    }
}

impl Drop for File {
    fn drop(&mut self) {
        let _ = (self._close)(self);
    }
}

bitflags! {
    /// Attribute bits for a file
    pub struct FileAttributes: u64 {
        const READ_ONLY = 0x0000_0000_0000_0001;
        const HIDDEN = 0x0000_0000_0000_0002;
        const SYSTEM = 0x0000_0000_0000_0004;
        const RESERVED = 0x0000_0000_0000_0008;
        const DIRECTORY = 0x0000_0000_0000_0010;
        const ARCHIVE = 0x0000_0000_0000_0020;
        const VALID_ATTR = 0x0000_0000_0000_0037;
    }
}

bitflags! {
    /// Mode to open a file
    pub struct FileMode: u64 {
        const READ = 0x0000_0000_0000_0001;
        const WRITE = 0x0000_0000_0000_0002;
        const CREATE = 0x8000_0000_0000_0000;
    }
}

/// Type of information that can be retrieved about a file
pub trait FileInformationType {
    fn guid() -> &'static Guid;
}

/// Generic information about a file
#[derive(Debug)]
#[repr(C)]
pub struct FileInfo {
    pub size: u64,
    pub file_size: u64,
    pub physical_size: u64,
    pub create_time: usize,       // TODO
    pub last_access_time: usize,  // TODO
    pub modification_time: usize, // TODO
    pub attribute: u64,           // TODO
    _file_name: Char16,
}

impl FileInformationType for FileInfo {
    fn guid() -> &'static Guid {
        &FILE_INFO_GUID
    }
}

/// Information about the system volume
#[derive(Debug)]
#[repr(C)]
pub struct FileSystemInfo {
    _size: usize,
    pub read_only: bool,
    pub volume_size: u64,
    pub free_space: u64,
    pub block_size: u32,
    _volume_label: Char16,
}

impl FileSystemInfo {
    pub fn volume_label(&self) -> Result<Pool<str>, Status> {
        let buf = unsafe {
            let buf_size = self._size - (mem::size_of::<FileSystemInfo>() - mem::size_of::<Char16>());
            slice::from_raw_parts(&(self._volume_label), buf_size)
        };

        utf16_to_str(buf)
    }
}

impl FileInformationType for FileSystemInfo {
    fn guid() -> &'static Guid {
        &FILE_SYSTEM_INFO_GUID
    }
}

static FILE_INFO_GUID: Guid =
    Guid { a: 0x09576e92, b: 0x6d3f, c: 0x11d2, d: [0x8e, 0x39, 0x00, 0xa0, 0xc9, 0x69, 0x72, 0x3b] };

static FILE_SYSTEM_INFO_GUID: Guid =
    Guid { a: 0x09576e93, b: 0x6d3f, c: 0x11d2, d: [0x8e, 0x39, 0x00, 0xa0, 0xc9, 0x69, 0x72, 0x3b] };

/// Provides a minimal interface for file-type access to a device
#[repr(C)]
pub struct SimpleFileSystem {
    pub revision: u64,
    pub _open_volume: extern "win64" fn(this: &SimpleFileSystem, root: &mut *mut File) -> Status,
}

impl SimpleFileSystem {
    /// Opens the root directory on a volume
    pub fn open_volume(&self) -> Result<&mut File, Status> {
        let mut file = 0x0 as *mut _;
        (self._open_volume)(self, &mut file).as_result()?;

        if file == 0x0 as *mut _ {
            Err(Status::NotFound)
        } else {
            Ok(unsafe { &mut *file })
        }
    }
}

impl Protocol for SimpleFileSystem {
    fn guid() -> &'static Guid {
        &SIMPLE_FILE_SYSTEM_GUID
    }
}

static SIMPLE_FILE_SYSTEM_GUID: Guid =
    Guid { a: 0x0964e5b22, b: 0x6459, c: 0x11d2, d: [0x8e, 0x39, 0x00, 0xa0, 0xc9, 0x69, 0x72, 0x3b] };
