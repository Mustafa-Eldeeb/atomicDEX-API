#[cfg(target_os = "windows")] extern crate winapi;

use std::fs;
use std::path::{Path, PathBuf};

#[cfg(not(target_os = "windows"))]
use std::os::unix::io::AsRawFd;

#[cfg(target_os = "windows")]
use {std::ffi::OsStr, std::os::windows::ffi::OsStrExt};

pub(crate) trait DiskWriteable {
    fn write_to_file(&self, writer: &mut fs::File) -> Result<(), std::io::Error>;
}

pub(crate) fn get_full_filepath(mut filepath: PathBuf, filename: String) -> String {
    filepath.push(filename);
    filepath.to_str().unwrap().to_string()
}

#[cfg(target_os = "windows")]
macro_rules! call {
    ($e: expr) => {
        if $e != 0 {
            return Ok(());
        } else {
            return Err(std::io::Error::last_os_error());
        }
    };
}

#[cfg(target_os = "windows")]
fn path_to_windows_str<T: AsRef<OsStr>>(path: T) -> Vec<winapi::shared::ntdef::WCHAR> {
    path.as_ref().encode_wide().chain(Some(0)).collect()
}

#[allow(bare_trait_objects)]
pub(crate) fn write_to_file<D: DiskWriteable>(path: PathBuf, filename: String, data: &D) -> std::io::Result<()> {
    fs::create_dir_all(path.clone())?;
    // Do a crazy dance with lots of fsync()s to be overly cautious here...
    // We never want to end up in a state where we've lost the old data, or end up using the
    // old data on power loss after we've returned.
    // The way to atomically write a file on Unix platforms is:
    // open(tmpname), write(tmpfile), fsync(tmpfile), close(tmpfile), rename(), fsync(dir)
    let filename_with_path = get_full_filepath(path, filename);
    let tmp_filename = format!("{}.tmp", filename_with_path);

    {
        // Note that going by rust-lang/rust@d602a6b, on MacOS it is only safe to use
        // rust stdlib 1.36 or higher.
        let mut f = fs::File::create(&tmp_filename)?;
        data.write_to_file(&mut f)?;
        f.sync_all()?;
    }
    // Fsync the parent directory on Unix.
    #[cfg(not(target_os = "windows"))]
    {
        fs::rename(&tmp_filename, &filename_with_path)?;
        let path = Path::new(&filename_with_path).parent().unwrap();
        let dir_file = fs::OpenOptions::new().read(true).open(path)?;
        unsafe {
            libc::fsync(dir_file.as_raw_fd());
        }
    }
    #[cfg(target_os = "windows")]
    {
        let src = PathBuf::from(tmp_filename);
        let dst = PathBuf::from(filename_with_path.clone());
        if Path::new(&filename_with_path).exists() {
            unsafe {
                winapi::um::winbase::ReplaceFileW(
                    path_to_windows_str(dst).as_ptr(),
                    path_to_windows_str(src).as_ptr(),
                    std::ptr::null(),
                    winapi::um::winbase::REPLACEFILE_IGNORE_MERGE_ERRORS,
                    std::ptr::null_mut() as *mut winapi::ctypes::c_void,
                    std::ptr::null_mut() as *mut winapi::ctypes::c_void,
                )
            };
        } else {
            call!(unsafe {
                winapi::um::winbase::MoveFileExW(
                    path_to_windows_str(src).as_ptr(),
                    path_to_windows_str(dst).as_ptr(),
                    winapi::um::winbase::MOVEFILE_WRITE_THROUGH | winapi::um::winbase::MOVEFILE_REPLACE_EXISTING,
                )
            });
        }
    }
    Ok(())
}
