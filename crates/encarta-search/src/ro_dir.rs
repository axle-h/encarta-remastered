//! A read-only tantivy directory: memory-mapped reads, no lock files, no writes. Lets the server
//! open `out/search/index` on a read-only volume (tantivy's reader otherwise creates
//! `.tantivy-meta.lock`).

use std::io;
use std::path::Path;
use std::sync::Arc;
use tantivy::directory::error::{DeleteError, LockError, OpenReadError, OpenWriteError};
use tantivy::directory::{Directory, DirectoryLock, FileHandle, Lock, MmapDirectory, WatchCallback, WatchHandle, WritePtr};

#[derive(Debug, Clone)]
pub struct ReadOnlyDir(MmapDirectory);

impl ReadOnlyDir {
    pub fn open(path: &Path) -> tantivy::Result<Self> {
        Ok(ReadOnlyDir(MmapDirectory::open(path)?))
    }
}

fn read_only(path: &Path) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, format!("read-only index: {}", path.display()))
}

impl Directory for ReadOnlyDir {
    fn get_file_handle(&self, path: &Path) -> Result<Arc<dyn FileHandle>, OpenReadError> {
        self.0.get_file_handle(path)
    }
    fn delete(&self, path: &Path) -> Result<(), DeleteError> {
        Err(DeleteError::IoError { io_error: Arc::new(read_only(path)), filepath: path.to_path_buf() })
    }
    fn exists(&self, path: &Path) -> Result<bool, OpenReadError> {
        self.0.exists(path)
    }
    fn open_write(&self, path: &Path) -> Result<WritePtr, OpenWriteError> {
        Err(OpenWriteError::IoError { io_error: Arc::new(read_only(path)), filepath: path.to_path_buf() })
    }
    fn atomic_read(&self, path: &Path) -> Result<Vec<u8>, OpenReadError> {
        self.0.atomic_read(path)
    }
    fn atomic_write(&self, path: &Path, _data: &[u8]) -> io::Result<()> {
        Err(read_only(path))
    }
    fn sync_directory(&self) -> io::Result<()> {
        Ok(())
    }
    fn acquire_lock(&self, _lock: &Lock) -> Result<DirectoryLock, LockError> {
        // Nothing writes this index while it is served; readers need no lock.
        Ok(DirectoryLock::from(Box::new(())))
    }
    fn watch(&self, _cb: WatchCallback) -> tantivy::Result<WatchHandle> {
        Ok(WatchHandle::empty())
    }
}
